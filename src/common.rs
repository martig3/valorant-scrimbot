use core::time::Duration as CoreDuration;
use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use async_std::task;
use chrono::{Datelike, DateTime, Duration as ChronoDuration, Local, TimeZone};
use rand::Rng;
use regex::Regex;
use serenity::Client;
use serenity::model::channel::Message;
use serenity::model::prelude::{Guild, ReactionType};
use serenity::model::user::User;
use serenity::prelude::{Context, TypeMapKey};
use serenity::utils::MessageBuilder;

use crate::common_utils::{admin_check, list_unpicked, move_user, populate_unicode_emojis, read_game_accounts, read_maps, read_teamnames, send_simple_msg, send_simple_tagged_msg, write_to_file};

#[derive(Serialize, Deserialize)]
pub struct Config {
    pub discord: DiscordConfig,
    pub autoclear_hour: Option<u32>,
    pub post_setup_msg: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct DiscordConfig {
    pub token: String,
    pub admin_role_id: Option<u64>,
    team_a_channel_id: Option<u64>,
    team_b_channel_id: Option<u64>,
    assign_role_id: Option<u64>,
}
#[derive(Clone)]
pub struct Draft {
    pub captain_a: Option<User>,
    pub captain_b: Option<User>,
    pub team_a: Vec<User>,
    pub team_b: Vec<User>,
    pub team_b_start_side: String,
    pub current_picker: Option<User>,
}

#[derive(PartialEq)]
pub enum State {
    Queue,
    MapPick,
    CaptainPick,
    Draft,
    SidePick,
    Ready,
}

struct ReactionResult {
    count: u64,
    map: String,
}

pub struct UserQueue;

pub struct GameAccountIds;

pub struct TeamNameCache;

struct QueueMessages;

pub struct BotState;

pub struct Maps;

pub struct DraftContainer;


impl TypeMapKey for UserQueue {
    type Value = Vec<User>;
}

impl TypeMapKey for Config {
    type Value = Config;
}

impl TypeMapKey for GameAccountIds {
    type Value = HashMap<u64, String>;
}

impl TypeMapKey for TeamNameCache {
    type Value = HashMap<u64, String>;
}

impl TypeMapKey for BotState {
    type Value = StateContainer;
}

impl TypeMapKey for Maps {
    type Value = Vec<String>;
}

impl TypeMapKey for DraftContainer {
    type Value = Draft;
}

impl TypeMapKey for QueueMessages {
    type Value = HashMap<u64, String>;
}

#[derive(PartialEq)]
pub struct StateContainer {
    pub  state: State,
}

pub async fn init_context(client: &Client, config: Config, game_acc_id_path: &str) {
    let mut data = client.data.write().await;
    data.insert::<UserQueue>(Vec::new());
    data.insert::<QueueMessages>(HashMap::new());
    data.insert::<Config>(config);
    data.insert::<GameAccountIds>(read_game_accounts(game_acc_id_path).await.unwrap());
    data.insert::<TeamNameCache>(read_teamnames().await.unwrap());
    data.insert::<BotState>(StateContainer { state: State::Queue });
    data.insert::<Maps>(read_maps().await.unwrap());
    data.insert::<DraftContainer>(Draft {
        captain_a: None,
        captain_b: None,
        current_picker: None,
        team_a: Vec::new(),
        team_b: Vec::new(),
        team_b_start_side: String::from(""),
    });
}

pub async fn queue_join(context: &Context, msg: &Message, author: &User) {
    let mut data = context.data.write().await;
    let user_queue: &mut Vec<User> = &mut data.get_mut::<UserQueue>().unwrap();
    if user_queue.contains(&author) {
        let response = MessageBuilder::new()
            .mention(author)
            .push(" is already in the queue.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    if user_queue.len() >= 10 {
        let response = MessageBuilder::new()
            .mention(author)
            .push(" sorry but the queue is full.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    user_queue.push(author.clone());
    let response = MessageBuilder::new()
        .mention(author)
        .push(" has been added to the queue. Queue size: ")
        .push(user_queue.len().to_string())
        .push("/10")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
    let queued_msgs: &mut HashMap<u64, String> = data.get_mut::<QueueMessages>().unwrap();
    let quote_regex = Regex::new("[\"”“](.*?)[\"”“]").unwrap();
    if let Some(mat) = quote_regex.find(&msg.content) {
        let start = mat.start();
        let mut end = mat.end();
        end = end.min(start + 50);
        queued_msgs.insert(*msg.author.id.as_u64(), String::from(msg.content[start..end].trim()));
    }
    let config: &Config = data.get::<Config>().unwrap();
    if let Some(role_id) = config.discord.assign_role_id {
        if let Ok(value) = msg.author.has_role(&context.http, msg.guild_id.unwrap(), role_id).await {
            if !value {
                let guild = Guild::get(&context.http, msg.guild_id.unwrap()).await.unwrap();
                if let Ok(mut member) = guild.member(&context.http, msg.author.id).await {
                    if let Err(err) = member.add_role(&context.http, role_id).await {
                        eprintln!("assign_role_id exists but cannot add role to user, check bot permissions");
                        eprintln!("{:?}", err);
                    }
                }
            }
        }
    }
}

pub async fn queue_leave(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let state: &mut StateContainer = data.get_mut::<BotState>().unwrap();
    if state.state != State::Queue {
        send_simple_tagged_msg(&context, &msg, " cannot `.leave` the queue after `.start`, use `.cancel` to start over if needed.", &msg.author).await;
        return;
    }
    let user_queue: &mut Vec<User> = data.get_mut::<UserQueue>().unwrap();
    if !user_queue.contains(&msg.author) {
        let response = MessageBuilder::new()
            .mention(&msg.author)
            .push(" is not in the queue. type `.join` to join the queue.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("error sending message: {:?}", why);
        }
        return;
    }
    let index = user_queue.iter().position(|r| r.id == msg.author.id).unwrap();
    user_queue.remove(index);
    let response = MessageBuilder::new()
        .mention(&msg.author)
        .push(" has left the queue. queue size: ")
        .push(user_queue.len().to_string())
        .push("/10")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("error sending message: {:?}", why);
    }
    let queued_msgs: &mut HashMap<u64, String> = data.get_mut::<QueueMessages>().unwrap();
    if queued_msgs.get(&msg.author.id.as_u64()).is_some() {
        queued_msgs.remove(&msg.author.id.as_u64());
    }
}

pub async fn queue_cancel(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let bot_state: &StateContainer = &data.get::<BotState>().unwrap();
    if bot_state.state == State::Queue {
        send_simple_tagged_msg(&context, &msg, " command only valid during `.start` process", &msg.author).await;
        return;
    }
    let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
    draft.team_a = vec![];
    draft.team_b = vec![];
    draft.captain_a = None;
    draft.captain_b = None;
    draft.current_picker = None;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    bot_state.state = State::Queue;
    send_simple_tagged_msg(&context, &msg, " `.start` process cancelled.", &msg.author).await;
}

pub async fn queue_list(context: &Context, msg: &Message) {
    let data = context.data.write().await;
    let user_queue: &Vec<User> = data.get::<UserQueue>().unwrap();
    let queue_msgs: &HashMap<u64, String> = data.get::<QueueMessages>().unwrap();
    let mut user_name = String::new();
    for u in user_queue {
        user_name.push_str(format!("\n- @{}", u.name).as_str());
        if let Some(value) = queue_msgs.get(u.id.as_u64()) {
            user_name.push_str(format!(": `{}`", value).as_str());
        }
    }
    let response = MessageBuilder::new()
        .push("Current queue size: ")
        .push(&user_queue.len())
        .push("/10")
        .push(user_name)
        .build();

    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub async fn queue_clear(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let user_queue: &mut Vec<User> = &mut data.get_mut::<UserQueue>().unwrap();
    user_queue.clear();
    let response = MessageBuilder::new()
        .mention(&msg.author)
        .push(" cleared queue")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub async fn queue_kick(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let state: &mut StateContainer = data.get_mut::<BotState>().unwrap();
    if state.state != State::Queue {
        send_simple_tagged_msg(&context, &msg, " cannot `.kick` the queue after `.start`, use `.cancel` to start over if needed.", &msg.author).await;
        return;
    }
    let user_queue: &mut Vec<User> = data.get_mut::<UserQueue>().unwrap();
    let user = &msg.mentions[0];
    if !user_queue.contains(&user) {
        let response = MessageBuilder::new()
            .mention(&msg.author)
            .push(" is not in the queue.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    let index = user_queue.iter().position(|r| r.id == user.id).unwrap();
    user_queue.remove(index);
    let response = MessageBuilder::new()
        .mention(user)
        .push(" has been kicked. Queue size: ")
        .push(user_queue.len().to_string())
        .push("/10")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub async fn assign_captain(context: &Context, msg: &Message) -> Option<Draft> {
    let mut data = context.data.write().await;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    if bot_state.state != State::CaptainPick {
        send_simple_tagged_msg(&context, &msg, " command ignored, not in the captain pick phase", &msg.author).await;
        return None;
    }
    let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
    if draft.captain_a != None && &msg.author == draft.captain_a.as_ref().unwrap() {
        send_simple_tagged_msg(&context, &msg, " you're already a captain!", &msg.author).await;
        return None;
    }
    if draft.captain_a == None {
        send_simple_tagged_msg(&context, &msg, " is set as captain.", &msg.author).await;
        draft.captain_a = Some(msg.author.clone());
    } else {
        send_simple_tagged_msg(&context, &msg, " is set as captain.", &msg.author).await;
        draft.captain_b = Some(msg.author.clone());
    }
    if draft.captain_a != None && draft.captain_b != None {
        send_simple_msg(&context, &msg, "Randomizing captain pick order...").await;
        // flip a coin, if 1 switch captains
        if rand::thread_rng().gen_range(0, 2) != 0 {
            let captain_a = draft.captain_a.clone();
            let captain_b = draft.captain_b.clone();
            draft.captain_a = captain_b;
            draft.captain_b = captain_a;
        }
        draft.team_a.push(draft.captain_a.clone().unwrap());
        draft.team_b.push(draft.captain_b.clone().unwrap());
        send_simple_tagged_msg(&context, &msg, " is set as the first pick captain (Team A)", &draft.captain_a.clone().unwrap()).await;
        send_simple_tagged_msg(&context, &msg, " is set as the second captain (Team B)", &draft.captain_b.clone().unwrap()).await;
        draft.current_picker = draft.captain_a.clone();
        return Some(draft.clone());
    } else {
        None
    }
}

pub async fn draft_pick(context: &Context, msg: &Message) -> Option<usize> {
    let mut data = context.data.write().await;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    if bot_state.state != State::Draft {
        send_simple_tagged_msg(&context, &msg, " it is not currently the draft phase", &msg.author).await;
        return None;
    }
    if msg.mentions.is_empty() {
        send_simple_tagged_msg(&context, &msg, " please mention a discord user in your message.", &msg.author).await;
        return None;
    }
    let picked = msg.mentions[0].clone();
    let user_queue: &Vec<User> = &data.get::<UserQueue>().unwrap().to_vec();
    if !user_queue.contains(&picked) {
        send_simple_tagged_msg(&context, &msg, " this user is not in the queue", &msg.author).await;
        return None;
    }
    let draft = data.get::<DraftContainer>().unwrap();
    let current_picker = draft.current_picker.clone().unwrap();
    if msg.author != *draft.captain_a.as_ref().unwrap() && msg.author != *draft.captain_b.as_ref().unwrap() {
        send_simple_tagged_msg(&context, &msg, " you are not a captain", &msg.author).await;
        return None;
    }
    if current_picker != msg.author {
        send_simple_tagged_msg(&context, &msg, " it is not your turn to pick", &msg.author).await;
        return None;
    }
    if draft.team_a.contains(&picked) || draft.team_b.contains(&picked) {
        send_simple_tagged_msg(&context, &msg, " this player is already on a team", &msg.author).await;
        return None;
    }

    let teamname_cache = data.get::<TeamNameCache>().unwrap();
    let team_a_name = String::from(teamname_cache.get(draft.captain_a.as_ref().unwrap().id.as_u64())
        .unwrap_or(&draft.captain_a.as_ref().unwrap().name));
    let team_b_name = String::from(teamname_cache.get(draft.captain_b.as_ref().unwrap().id.as_u64())
        .unwrap_or(&draft.captain_b.as_ref().unwrap().name));
    let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
    if draft.captain_a.as_ref().unwrap() == &current_picker {
        send_simple_tagged_msg(&context, &msg, &format!(" has been added to Team {}", team_a_name), &picked).await;
        draft.team_a.push(picked);
        draft.current_picker = draft.captain_b.clone();
        list_unpicked(&user_queue, &draft, &context, &msg, &team_a_name, &team_b_name).await;
    } else {
        send_simple_tagged_msg(&context, &msg, &format!(" has been added to Team {}", team_b_name), &picked).await;
        draft.team_b.push(picked);
        draft.current_picker = draft.captain_a.clone();
        list_unpicked(&user_queue, &draft, &context, &msg, &team_a_name, &team_b_name).await;
    }
    return Some(user_queue
        .iter()
        .filter(|user| !draft.team_a.contains(user) && !draft.team_b.contains(user))
        .count());
}

pub async fn map_vote(context: &Context, msg: &Message) {
    let admin_check = admin_check(&context, &msg, true).await;
    if !admin_check { return; }
    let mut data = context.data.write().await;
    let bot_state: &StateContainer = data.get::<BotState>().unwrap();
    if bot_state.state != State::Queue {
        send_simple_tagged_msg(&context, &msg, " `.start` command has already been entered", &msg.author).await;
        return;
    }
    let user_queue: &mut Vec<User> = data.get_mut::<UserQueue>().unwrap();
    if !user_queue.contains(&msg.author) && !admin_check {
        send_simple_tagged_msg(&context, &msg, " non-admin users that are not in the queue cannot start the match", &msg.author).await;
        return;
    }
    if user_queue.len() != 10 {
        let response = MessageBuilder::new()
            .mention(&msg.author)
            .push(" the queue is not full yet")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        // return;
    }
    let user_queue_mention: String = user_queue
        .iter()
        .map(|user| format!("- <@{}>\n", user.id))
        .collect();
    let response = MessageBuilder::new()
        .push(user_queue_mention)
        .push_bold_line("Scrim setup is starting...")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
    let bot_state: &mut StateContainer = data.get_mut::<BotState>().unwrap();
    bot_state.state = State::MapPick;
    let maps: &Vec<String> = &data.get::<Maps>().unwrap();
    let mut unicode_to_maps: HashMap<String, String> = HashMap::new();
    let a_to_z = ('a'..'z').collect::<Vec<_>>();
    let unicode_emoji_map = populate_unicode_emojis().await;
    for (i, map) in maps.iter().enumerate() {
        unicode_to_maps.insert(String::from(unicode_emoji_map.get(&a_to_z[i]).unwrap()), String::from(map));
    }
    let emoji_suffixes = a_to_z[..maps.len()].to_vec();
    let vote_text: String = emoji_suffixes
        .iter()
        .enumerate()
        .map(|(i, c)| format!(":regional_indicator_{}: `{}`\n", c, &maps[i]))
        .collect();
    let response = MessageBuilder::new()
        .push_bold_line("Map Vote:")
        .push(vote_text)
        .build();
    let vote_msg = msg.channel_id.say(&context.http, &response).await.unwrap();
    for c in emoji_suffixes {
        vote_msg.react(&context.http, ReactionType::Unicode(String::from(unicode_emoji_map.get(&c).unwrap()))).await.unwrap();
    }
    task::sleep(Duration::from_secs(50)).await;
    let response = MessageBuilder::new()
        .push("Voting will end in 10 seconds")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
    task::sleep(Duration::from_secs(10)).await;
    let updated_vote_msg = vote_msg.channel_id.message(&context.http, vote_msg.id).await.unwrap();
    let mut results: Vec<ReactionResult> = Vec::new();
    for reaction in updated_vote_msg.reactions {
        let react_as_map: Option<&String> = unicode_to_maps.get(reaction.reaction_type.to_string().as_str());
        if react_as_map != None {
            let map = String::from(react_as_map.unwrap());
            results.push(ReactionResult {
                count: reaction.count,
                map,
            });
        }
    }
    let max_count = results
        .iter()
        .max_by(|x, y| x.count.cmp(&y.count))
        .unwrap()
        .count;
    let final_results: Vec<ReactionResult> = results
        .into_iter()
        .filter(|m| m.count == max_count)
        .collect();
    if final_results.len() > 1 {
        let map = &final_results.get(rand::thread_rng().gen_range(0, final_results.len())).unwrap().map;
        let response = MessageBuilder::new()
            .push("Maps were tied, `")
            .push(&map)
            .push("` was selected at random")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
    } else {
        let map = &final_results[0].map;
        let response = MessageBuilder::new()
            .push("Map vote has concluded. `")
            .push(&map)
            .push("` will be played")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
    }
}

pub async fn add_map(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let maps: &mut Vec<String> = data.get_mut::<Maps>().unwrap();
    if maps.len() >= 26 {
        let response = MessageBuilder::new()
            .mention(&msg.author)
            .push(" unable to add map, max amount reached.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    let map_name: String = String::from(msg.content.trim().split(" ").take(2).collect::<Vec<_>>()[1]);
    if maps.contains(&map_name) {
        let response = MessageBuilder::new()
            .mention(&msg.author)
            .push(" unable to add map, already exists.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    maps.push(String::from(&map_name));
    write_to_file(String::from("maps.json"), serde_json::to_string(maps).unwrap()).await;
    let response = MessageBuilder::new()
        .mention(&msg.author)
        .push(" added map: `")
        .push(&map_name)
        .push("`")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub async fn remove_map(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let maps: &mut Vec<String> = data.get_mut::<Maps>().unwrap();
    let map_name: String = String::from(msg.content.trim().split(" ").take(2).collect::<Vec<_>>()[1]);
    if !maps.contains(&map_name) {
        let response = MessageBuilder::new()
            .mention(&msg.author)
            .push(" this map doesn't exist in the list.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    let index = maps.iter().position(|m| m == &map_name).unwrap();
    maps.remove(index);
    write_to_file(String::from("maps.json"), serde_json::to_string(maps).unwrap()).await;
    let response = MessageBuilder::new()
        .mention(&msg.author)
        .push(" removed map: `")
        .push(&map_name)
        .push("`")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub async fn finish_setup(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let draft: &Draft = &data.get::<DraftContainer>().unwrap().clone();
    let config: &Config = &data.get::<Config>().unwrap();
    if let Some(team_a_channel_id) = config.discord.team_a_channel_id {
        for user in &draft.team_a {
            move_user(msg, user, team_a_channel_id, &context).await;
        }
    }
    if let Some(team_b_channel_id) = config.discord.team_b_channel_id {
        for user in &draft.team_b {
            move_user(msg, user, team_b_channel_id, &context).await;
        }
    }
    if let Some(post_start_msg) = &config.post_setup_msg {
        if let Err(why) = msg.channel_id.say(&context.http, &post_start_msg).await {
            eprintln!("Error sending message: {:?}", why);
        }
    }
    // reset to queue state
    let user_queue: &mut Vec<User> = data.get_mut::<UserQueue>().unwrap();
    user_queue.clear();
    let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
    draft.team_a = vec![];
    draft.team_b = vec![];
    draft.captain_a = None;
    draft.captain_b = None;
    draft.current_picker = None;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    bot_state.state = State::Queue;
    let queue_msgs: &mut HashMap<u64, String> = &mut data.get_mut::<QueueMessages>().unwrap();
    queue_msgs.clear();
}

pub async fn add_teamname(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let teamname_cache: &mut HashMap<u64, String> = &mut data.get_mut::<TeamNameCache>().unwrap();
    let split_content = msg.content.trim().split(' ').collect::<Vec<_>>();
    if split_content.len() < 2 {
        send_simple_tagged_msg(&context, &msg, " invalid message formatting. Example: `.teamname TeamName`", &msg.author).await;
        return;
    }
    let teamname = String::from(&msg.content[10..msg.content.len()]);
    if teamname.len() > 25 {
        send_simple_tagged_msg(&context, &msg, &format!(" team name is over the character limit by {}.", teamname.len() - 25), &msg.author).await;
        return;
    }
    teamname_cache.insert(*msg.author.id.as_u64(), String::from(&teamname));
    write_to_file(String::from("teamnames.json"), serde_json::to_string(teamname_cache).unwrap()).await;
    send_simple_tagged_msg(&context, &msg, &format!(" custom team name successfully set to `{}`", &teamname), &msg.author).await;
}

pub async fn autoclear_queue(context: &Context) {
    let autoclear_hour_prop = get_autoclear_hour(context).await;
    if let Some(autoclear_hour) = autoclear_hour_prop {
        println!("Autoclear feature started");
        loop {
            let current: DateTime<Local> = Local::now();
            let mut autoclear: DateTime<Local> = Local.ymd(current.year(), current.month(), current.day())
                .and_hms(autoclear_hour, 0, 0);
            if autoclear.signed_duration_since(current).num_milliseconds() < 0 { autoclear = autoclear + ChronoDuration::days(1) }
            let time_between: ChronoDuration = autoclear.signed_duration_since(current);
            task::sleep(CoreDuration::from_millis(time_between.num_milliseconds() as u64)).await;
            {
                let mut data = context.data.write().await;
                let user_queue: &mut Vec<User> = &mut data.get_mut::<UserQueue>().unwrap();
                user_queue.clear();
                let queued_msgs: &mut HashMap<u64, String> = data.get_mut::<QueueMessages>().unwrap();
                queued_msgs.clear();
            }
        }
    }
}

async fn get_autoclear_hour(client: &Context) -> Option<u32> {
    let data = client.data.write().await;
    let config: &Config = &data.get::<Config>().unwrap();
    config.autoclear_hour
}

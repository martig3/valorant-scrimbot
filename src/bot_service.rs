use std::collections::HashMap;
use std::time::Duration;

use async_std::task;
use rand::Rng;
use regex::Regex;
use serenity::client::Context;
use serenity::model::channel::{Message, ReactionType};
use serenity::model::guild::{GuildContainer, Guild};
use serenity::model::user::User;
use serenity::prelude::TypeMap;
use serenity::utils::MessageBuilder;
use tokio::sync::RwLockWriteGuard;

use crate::{BotState, Config, Draft, Maps, QueueMessages, RiotIdCache, State, StateContainer, TeamNameCache, UserQueue};

struct ReactionResult {
    count: u64,
    map: String,
}

pub(crate) async fn handle_join(context: &Context, msg: &Message, author: &User) {
    let mut data = context.data.write().await;
    let riot_id_cache: &HashMap<u64, String> = &data.get::<RiotIdCache>().unwrap();
    if !riot_id_cache.contains_key(author.id.as_u64()) {
        let response = MessageBuilder::new()
            .mention(author)
            .push(" riotid not found for your discord user, \
                    please use `.riotid <your riotid>` to assign one. Example: `.riotid Martige#NA1`")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
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

pub(crate) async fn handle_leave(context: Context, msg: Message) {
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
            .push(" is not in the queue. Type `.join` to join the queue.")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        return;
    }
    let index = user_queue.iter().position(|r| r.id == msg.author.id).unwrap();
    user_queue.remove(index);
    let response = MessageBuilder::new()
        .mention(&msg.author)
        .push(" has left the queue. Queue size: ")
        .push(user_queue.len().to_string())
        .push("/10")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
    let queued_msgs: &mut HashMap<u64, String> = data.get_mut::<QueueMessages>().unwrap();
    if queued_msgs.get(&msg.author.id.as_u64()).is_some() {
        queued_msgs.remove(&msg.author.id.as_u64());
    }
}

pub(crate) async fn handle_list(context: Context, msg: Message) {
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

pub(crate) async fn handle_clear(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
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

pub(crate) async fn handle_help(context: Context, msg: Message) {
    let mut commands = String::from("
`.join` - Join the queue, add a message in quotes (max 50 char) i.e. `.join \"available at 9pm\"`
`.leave` - Leave the queue
`.queue` - List all users in the queue
`.riotid` - Set your riotid i.e. `.riotid Martige#NA1`
`.maps` - Lists all maps available for map vote
`.teamname` - Sets a custom team name when you are a captain i.e. `.teamname Your Team Name`
_These are commands used during the `.start` process:_
`.captain` - Add yourself as a captain.
`.pick` - If you are a captain, this is used to pick a player by tagging them i.e. `.pick @Martige`
");
    let admin_commands = String::from("
_These are privileged admin commands:_
`.start` - Start the match setup process
`.kick` - Kick a player by mentioning them i.e. `.kick @user`
`.addmap` - Add a map to the map vote i.e. `.addmap mapname`
`.removemap` - Remove a map from the map vote i.e. `.removemap mapname`
`.recoverqueue` - Manually set a queue, tag all users to add after the command
`.clear` - Clear the queue
`.cancel` - Cancels `.start` process & retains current queue
    ");
    if admin_check(&context, &msg, false).await {
        commands.push_str(&admin_commands)
    }
    let response = MessageBuilder::new()
        .push(commands)
        .build();
    if let Ok(channel) = &msg.author.create_dm_channel(&context.http).await {
        if let Err(why) = channel.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
    } else {
        eprintln!("Error sending .help dm");
    }
}

pub(crate) async fn handle_recover_queue(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    {
        let mut data = context.data.write().await;
        let user_queue: &mut Vec<User> = &mut data.get_mut::<UserQueue>().unwrap();
        user_queue.clear();
    }
    for mention in &msg.mentions {
        handle_join(&context, &msg, &mention).await
    }
}

pub(crate) async fn handle_start(context: Context, msg: Message) {
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
        return;
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
    let mut bot_state: &mut StateContainer = data.get_mut::<BotState>().unwrap();
    bot_state.state = State::CaptainPick;
    let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
    draft.captain_a = None;
    draft.captain_b = None;
    draft.team_a = Vec::new();
    draft.team_b = Vec::new();
    send_simple_msg(&context, &msg, "Starting captain pick phase. Two users type `.captain` to start picking teams.").await;
}


pub(crate) async fn handle_captain(context: Context, msg: Message) {
    let mut data = context.data.write().await;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    if bot_state.state != State::CaptainPick {
        send_simple_tagged_msg(&context, &msg, " command ignored, not in the captain pick phase", &msg.author).await;
        return;
    }
    let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
    if draft.captain_a != None && &msg.author == draft.captain_a.as_ref().unwrap() {
        send_simple_tagged_msg(&context, &msg, " you're already a captain!", &msg.author).await;
        return;
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
        let response = MessageBuilder::new()
            .push("Captain pick has concluded. Starting draft phase. ")
            .mention(&draft.current_picker.clone().unwrap())
            .push(" gets first `.pick @<user>`")
            .build();
        if let Err(why) = msg.channel_id.say(&context.http, &response).await {
            eprintln!("Error sending message: {:?}", why);
        }
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        bot_state.state = State::Draft;
        let user_queue: &Vec<User> = &mut data.get::<UserQueue>().unwrap();
        let draft: &Draft = &mut data.get::<Draft>().unwrap();
        let teamname_cache = data.get::<TeamNameCache>().unwrap();
        let team_a_name = teamname_cache.get(draft.captain_a.as_ref().unwrap().id.as_u64())
            .unwrap_or(&draft.captain_a.as_ref().unwrap().name);
        let team_b_name = teamname_cache.get(draft.captain_b.as_ref().unwrap().id.as_u64())
            .unwrap_or(&draft.captain_b.as_ref().unwrap().name);
        list_unpicked(&user_queue, &draft, &context, &msg, team_a_name, team_b_name).await;
    }
}

pub(crate) async fn handle_pick(context: Context, msg: Message) {
    let mut data = context.data.write().await;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    if bot_state.state != State::Draft {
        send_simple_tagged_msg(&context, &msg, " it is not currently the draft phase", &msg.author).await;
        return;
    }
    if msg.mentions.is_empty() {
        send_simple_tagged_msg(&context, &msg, " please mention a discord user in your message.", &msg.author).await;
        return;
    }
    let picked = msg.mentions[0].clone();
    let user_queue: &Vec<User> = &data.get::<UserQueue>().unwrap().to_vec();
    if !user_queue.contains(&picked) {
        send_simple_tagged_msg(&context, &msg, " this user is not in the queue", &msg.author).await;
        return;
    }
    let draft = data.get::<Draft>().unwrap();
    let current_picker = draft.current_picker.clone().unwrap();
    if msg.author != *draft.captain_a.as_ref().unwrap() && msg.author != *draft.captain_b.as_ref().unwrap() {
        send_simple_tagged_msg(&context, &msg, " you are not a captain", &msg.author).await;
        return;
    }
    if current_picker != msg.author {
        send_simple_tagged_msg(&context, &msg, " it is not your turn to pick", &msg.author).await;
        return;
    }
    if draft.team_a.contains(&picked) || draft.team_b.contains(&picked) {
        send_simple_tagged_msg(&context, &msg, " this player is already on a team", &msg.author).await;
        return;
    }

    let teamname_cache = data.get::<TeamNameCache>().unwrap();
    let team_a_name = String::from(teamname_cache.get(draft.captain_a.as_ref().unwrap().id.as_u64())
        .unwrap_or(&draft.captain_a.as_ref().unwrap().name));
    let team_b_name = String::from(teamname_cache.get(draft.captain_b.as_ref().unwrap().id.as_u64())
        .unwrap_or(&draft.captain_b.as_ref().unwrap().name));
    let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
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
    let remaining_users = user_queue
        .iter()
        .filter(|user| !draft.team_a.contains(user) && !draft.team_b.contains(user))
        .count();
    if remaining_users == 0 {
        let captain_b = draft.captain_b.clone().unwrap();
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        bot_state.state = State::SidePick;
        send_simple_tagged_msg(&context, &msg, " type `.defense` or `.attack` to pick a starting side.", &captain_b).await;
    }
}

pub(crate) async fn list_unpicked(user_queue: &Vec<User>, draft: &Draft, context: &Context, msg: &Message, team_a_name: &String, team_b_name: &String) {
    let remaining_users: String = user_queue
        .iter()
        .filter(|user| !draft.team_a.contains(user) && !draft.team_b.contains(user))
        .map(|user| format!("- @{}\n", &user.name))
        .collect();
    let team_a: String = draft.team_a
        .iter()
        .map(|user| format!("- @{}\n", &user.name))
        .collect();
    let team_b: String = draft.team_b
        .iter()
        .map(|user| format!("- @{}\n", &user.name))
        .collect();
    let response = MessageBuilder::new()
        .push_bold_line(format!("Team {}:", team_a_name))
        .push_line(team_a)
        .push_bold_line(format!("Team {}:", team_b_name))
        .push_line(team_b)
        .push_bold_line("Remaining players: ")
        .push_line(remaining_users)
        .build();

    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub(crate) async fn handle_defense_option(context: Context, msg: Message) {
    {
        let mut data: RwLockWriteGuard<TypeMap> = context.data.write().await;
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        if bot_state.state != State::SidePick {
            send_simple_tagged_msg(&context, &msg, " it is not currently the side pick phase", &msg.author).await;
            return;
        }
        let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
        if &msg.author != draft.captain_b.as_ref().unwrap() {
            send_simple_tagged_msg(&context, &msg, " you are not Captain B", &msg.author).await;
            return;
        }
        draft.team_b_start_side = String::from("ct");
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        bot_state.state = State::Ready;
        send_simple_msg(&context, &msg, "Setup is completed.").await;
    }
    handle_ready(&context, &msg).await;
}

pub(crate) async fn handle_attack_option(context: Context, msg: Message) {
    {
        let mut data = context.data.write().await;
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        if bot_state.state != State::SidePick {
            send_simple_tagged_msg(&context, &msg, " it is not currently the side pick phase", &msg.author).await;
            return;
        }
        let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
        if &msg.author != draft.captain_b.as_ref().unwrap() {
            send_simple_tagged_msg(&context, &msg, " you are not Captain B", &msg.author).await;
            return;
        }
        draft.team_b_start_side = String::from("t");
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        bot_state.state = State::Ready;
        send_simple_msg(&context, &msg, "Setup is completed.").await;
    }
    handle_ready(&context, &msg).await;
}

pub(crate) async fn handle_riotid(context: Context, msg: Message) {
    let mut data = context.data.write().await;
    let riot_id_cache: &mut HashMap<u64, String> = &mut data.get_mut::<RiotIdCache>().unwrap();
    let split_content = msg.content.trim().split(' ').take(2).collect::<Vec<_>>();
    if split_content.len() == 1 {
        send_simple_tagged_msg(&context, &msg, " please check the command formatting. There must be a space in between `.riotid` and your Riot id. \
        Example: `.riotid Martige#NA1`", &msg.author).await;
        return;
    }
    let riot_id_str: String = String::from(split_content[1]);
    let riot_id_regex = Regex::new("\\w+#\\w+").unwrap();
    if !riot_id_regex.is_match(&riot_id_str) {
        send_simple_tagged_msg(&context, &msg, " invalid Riot id formatting. Please follow this example: `.riotid Martige#NA1`", &msg.author).await;
        return;
    }
    riot_id_cache.insert(*msg.author.id.as_u64(), String::from(&riot_id_str));
    write_to_file(String::from("riot_ids.json"), serde_json::to_string(riot_id_cache).unwrap()).await;
    let response = MessageBuilder::new()
        .push("Updated Riot id for ")
        .mention(&msg.author)
        .push(" to `")
        .push(&riot_id_str)
        .push("`")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub(crate) async fn handle_map_list(context: Context, msg: Message) {
    let data = context.data.write().await;
    let maps: &Vec<String> = data.get::<Maps>().unwrap();
    let map_str: String = maps.iter().map(|map| format!("- `{}`\n", map)).collect();
    let response = MessageBuilder::new()
        .push_line("Current map pool:")
        .push(map_str)
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub(crate) async fn handle_kick(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
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

pub(crate) async fn handle_add_map(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
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

pub(crate) async fn handle_remove_map(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
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

pub(crate) async fn handle_unknown(context: Context, msg: Message) {
    let response = MessageBuilder::new()
        .push("Unknown command, type `.help` for list of commands.")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub(crate) async fn write_to_file(path: String, content: String) {
    let mut error_string = String::from("Error writing to ");
    error_string.push_str(&path);
    std::fs::write(path, content)
        .expect(&error_string);
}

pub(crate) async fn handle_ready(context: &Context, msg: &Message) {
    let mut data = context.data.write().await;
    let draft: &Draft = &data.get::<Draft>().unwrap().clone();
    let riot_id_cache: &HashMap<u64, String> = &data.get::<RiotIdCache>().unwrap().clone();
    let teamname_cache = data.get::<TeamNameCache>().unwrap();
    let team_a_name = teamname_cache.get(draft.captain_a.as_ref().unwrap().id.as_u64())
        .unwrap_or(&draft.captain_a.as_ref().unwrap().name);
    let team_b_name = teamname_cache.get(draft.captain_b.as_ref().unwrap().id.as_u64())
        .unwrap_or(&draft.captain_b.as_ref().unwrap().name);
    let team_a: String = draft.team_a
        .iter()
        .map(|user| format!("- @{}: `{}`\n", &user.name, riot_id_cache.get(user.id.as_u64()).unwrap()))
        .collect();
    let team_b: String = draft.team_b
        .iter()
        .map(|user| format!("- @{}: `{}`\n", &user.name, riot_id_cache.get(user.id.as_u64()).unwrap()))
        .collect();
    let response = MessageBuilder::new()
        .push_bold_line(format!("Team {}:", team_a_name))
        .push_line(team_a)
        .push_bold_line(format!("Team {}:", team_b_name))
        .push_line(team_b)
        .build();

    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
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
    let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
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

pub(crate) async fn handle_cancel(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    let mut data = context.data.write().await;
    let bot_state: &StateContainer = &data.get::<BotState>().unwrap();
    if bot_state.state == State::Queue {
        send_simple_tagged_msg(&context, &msg, " command only valid during `.start` process", &msg.author).await;
        return;
    }
    let draft: &mut Draft = &mut data.get_mut::<Draft>().unwrap();
    draft.team_a = vec![];
    draft.team_b = vec![];
    draft.captain_a = None;
    draft.captain_b = None;
    draft.current_picker = None;
    let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
    bot_state.state = State::Queue;
    send_simple_tagged_msg(&context, &msg, " `.start` process cancelled.", &msg.author).await;
}


pub(crate) async fn handle_teamname(context: Context, msg: Message) {
    let mut data = context.data.write().await;
    let teamname_cache: &mut HashMap<u64, String> = &mut data.get_mut::<TeamNameCache>().unwrap();
    let split_content = msg.content.trim().split(' ').collect::<Vec<_>>();
    if split_content.len() < 2 {
        send_simple_tagged_msg(&context, &msg, " invalid message formatting. Example: `.teamname TeamName`", &msg.author).await;
        return;
    }
    let teamname = String::from(&msg.content[10..msg.content.len()]);
    if teamname.len() > 18 {
        send_simple_tagged_msg(&context, &msg, &format!(" team name is over the character limit by {}.", teamname.len() - 18), &msg.author).await;
        return;
    }
    teamname_cache.insert(*msg.author.id.as_u64(), String::from(&teamname));
    write_to_file(String::from("teamnames.json"), serde_json::to_string(teamname_cache).unwrap()).await;
    send_simple_tagged_msg(&context, &msg, &format!(" custom team name successfully set to `{}`", &teamname), &msg.author).await;
}

pub(crate) async fn send_simple_msg(context: &Context, msg: &Message, text: &str) {
    let response = MessageBuilder::new()
        .push(text)
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub(crate) async fn send_simple_tagged_msg(context: &Context, msg: &Message, text: &str, mentioned: &User) -> Option<Message> {
    let response = MessageBuilder::new()
        .mention(mentioned)
        .push(text)
        .build();
    if let Ok(m) = msg.channel_id.say(&context.http, &response).await {
        Some(m)
    } else {
        eprintln!("Error sending message");
        None
    }
}

pub(crate) async fn admin_check(context: &Context, msg: &Message, print_msg: bool) -> bool {
    let data = context.data.write().await;
    let config: &Config = data.get::<Config>().unwrap();
    if let Some(admin_role_id) = config.discord.admin_role_id {
        let role_name = context.cache.role(msg.guild_id.unwrap(), admin_role_id).await.unwrap().name;
        return if msg.author.has_role(&context.http, GuildContainer::from(msg.guild_id.unwrap()), admin_role_id).await.unwrap_or_else(|_| false) {
            true
        } else {
            if print_msg {
                let response = MessageBuilder::new()
                    .mention(&msg.author)
                    .push(" this command requires the '")
                    .push(role_name)
                    .push("' role.")
                    .build();
                if let Err(why) = msg.channel_id.say(&context.http, &response).await {
                    eprintln!("Error sending message: {:?}", why);
                }
            }
            false
        };
    }
    true
}

pub(crate) async fn move_user(msg: &Message, user: &User, channel_id: u64, context: &Context) {
    if let Some(guild) = &msg.guild(&context.cache).await {
        if let Err(why) = guild.move_member(&context.http, user.id, channel_id).await {
            println!("Cannot move user: {:?}", why);
        }
    }
}

pub(crate) async fn populate_unicode_emojis() -> HashMap<char, String> {
// I hate this implementation and I deserve to be scolded
// in my defense however, you have to provide unicode emojis to the api
// if Discord's API allowed their shortcuts i.e. ":smile:" instead that would have been more intuitive
    let mut map = HashMap::new();
    map.insert('a', String::from("🇦"));
    map.insert('b', String::from("🇧"));
    map.insert('c', String::from("🇨"));
    map.insert('d', String::from("🇩"));
    map.insert('e', String::from("🇪"));
    map.insert('f', String::from("🇫"));
    map.insert('g', String::from("🇬"));
    map.insert('h', String::from("🇭"));
    map.insert('i', String::from("🇮"));
    map.insert('j', String::from("🇯"));
    map.insert('k', String::from("🇰"));
    map.insert('l', String::from("🇱"));
    map.insert('m', String::from("🇲"));
    map.insert('n', String::from("🇳"));
    map.insert('o', String::from("🇴"));
    map.insert('p', String::from("🇵"));
    map.insert('q', String::from("🇶"));
    map.insert('r', String::from("🇷"));
    map.insert('s', String::from("🇸"));
    map.insert('t', String::from("🇹"));
    map.insert('u', String::from("🇺"));
    map.insert('v', String::from("🇻"));
    map.insert('w', String::from("🇼"));
    map.insert('x', String::from("🇽"));
    map.insert('y', String::from("🇾"));
    map.insert('z', String::from("🇿"));
    map
}

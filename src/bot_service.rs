use std::collections::HashMap;

use regex::Regex;
use serenity::client::Context;
use serenity::model::channel::Message;
use serenity::model::user::User;
use serenity::prelude::TypeMap;
use serenity::utils::MessageBuilder;
use tokio::sync::RwLockWriteGuard;

use crate::common::{BotState, Draft, draft_pick, DraftContainer, GameAccountIds, State, StateContainer, TeamNameCache, UserQueue};
use crate::common;
use crate::common_utils::{admin_check, list_unpicked, print_map_pool, send_simple_msg, send_simple_tagged_msg, write_to_file};

pub async fn handle_join(context: &Context, msg: &Message, author: &User) {
    {
        let data = context.data.write().await;
        let riot_id_cache: &HashMap<u64, String> = &data.get::<GameAccountIds>().unwrap();
        if !riot_id_cache.contains_key(&author.id.as_u64()) {
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
    }
    common::queue_join(&context, &msg, &author).await;
}

pub(crate) async fn handle_leave(context: Context, msg: Message) {
    common::queue_leave(&context, &msg).await;
}

pub(crate) async fn handle_list(context: Context, msg: Message) {
    common::queue_list(&context, &msg).await;
}

pub(crate) async fn handle_clear(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    common::queue_clear(&context, &msg).await;
}

pub(crate) async fn handle_help(context: Context, msg: Message) {
    let mut commands = String::from("
`.join` - Join the queue, add a message in quotes (max 50 char) i.e. `.join \"available at 9pm\"`
`.leave` - Leave the queue
`.list` - List all users in the queue
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
    common::map_vote(&context, &msg).await;
    {
        let mut data = context.data.write().await;
        let mut bot_state: &mut StateContainer = data.get_mut::<BotState>().unwrap();
        bot_state.state = State::CaptainPick;
        let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
        draft.captain_a = None;
        draft.captain_b = None;
        draft.team_a = Vec::new();
        draft.team_b = Vec::new();
        send_simple_msg(&context, &msg, "Starting captain pick phase. Two users type `.captain` to start picking teams.").await;
    }
}


pub(crate) async fn handle_captain(context: Context, msg: Message) {
    if let Some(draft) = common::assign_captain(&context, &msg).await {
        let mut data = context.data.write().await;
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
        let draft: &Draft = &mut data.get::<DraftContainer>().unwrap();
        let teamname_cache = data.get::<TeamNameCache>().unwrap();
        let team_a_name = teamname_cache.get(draft.captain_a.as_ref().unwrap().id.as_u64())
            .unwrap_or(&draft.captain_a.as_ref().unwrap().name);
        let team_b_name = teamname_cache.get(draft.captain_b.as_ref().unwrap().id.as_u64())
            .unwrap_or(&draft.captain_b.as_ref().unwrap().name);
        list_unpicked(&user_queue, &draft, &context, &msg, team_a_name, team_b_name).await;
    }
}

pub(crate) async fn handle_pick(context: Context, msg: Message) {
    if let Some(remaining_users) = draft_pick(&context, &msg).await {
        let mut data = context.data.write().await;
        let draft: &Draft = data.get::<DraftContainer>().unwrap();
        if remaining_users == 0 {
            let captain_b = draft.captain_b.clone().unwrap();
            let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
            bot_state.state = State::SidePick;
            send_simple_tagged_msg(&context, &msg, " type `.defense` or `.attack` to pick a starting side.", &captain_b).await;
        }
    }
}


async fn handle_finish(context: &Context, msg: &Message) {
    {
        let data = context.data.write().await;
        let draft: &Draft = &data.get::<DraftContainer>().unwrap().clone();
        let riot_id_cache: &HashMap<u64, String> = &data.get::<GameAccountIds>().unwrap().clone();
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
    }
    common::finish_setup(&context, &msg).await;
}

pub(crate) async fn handle_defense_option(context: Context, msg: Message) {
    {
        let mut data: RwLockWriteGuard<TypeMap> = context.data.write().await;
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        if bot_state.state != State::SidePick {
            send_simple_tagged_msg(&context, &msg, " it is not currently the side pick phase", &msg.author).await;
            return;
        }
        let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
        if &msg.author != draft.captain_b.as_ref().unwrap() {
            send_simple_tagged_msg(&context, &msg, " you are not Captain B", &msg.author).await;
            return;
        }
        draft.team_b_start_side = String::from("ct");
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        bot_state.state = State::Ready;
        send_simple_msg(&context, &msg, "Setup is completed.").await;
    }
    handle_finish(&context, &msg).await;
}

pub(crate) async fn handle_attack_option(context: Context, msg: Message) {
    {
        let mut data = context.data.write().await;
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        if bot_state.state != State::SidePick {
            send_simple_tagged_msg(&context, &msg, " it is not currently the side pick phase", &msg.author).await;
            return;
        }
        let draft: &mut Draft = &mut data.get_mut::<DraftContainer>().unwrap();
        if &msg.author != draft.captain_b.as_ref().unwrap() {
            send_simple_tagged_msg(&context, &msg, " you are not Captain B", &msg.author).await;
            return;
        }
        draft.team_b_start_side = String::from("t");
        let bot_state: &mut StateContainer = &mut data.get_mut::<BotState>().unwrap();
        bot_state.state = State::Ready;
        send_simple_msg(&context, &msg, "Setup is completed.").await;
    }
    handle_finish(&context, &msg).await;
}

pub(crate) async fn handle_riotid(context: Context, msg: Message) {
    let mut data = context.data.write().await;
    let riot_id_cache: &mut HashMap<u64, String> = &mut data.get_mut::<GameAccountIds>().unwrap();
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
    print_map_pool(&context, &msg).await;
}

pub(crate) async fn handle_kick(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    common::queue_kick(&context, &msg).await;
}

pub(crate) async fn handle_add_map(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    common::add_map(&context, &msg).await;
}

pub(crate) async fn handle_remove_map(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    common::remove_map(&context, &msg).await;
}

pub(crate) async fn handle_unknown(context: Context, msg: Message) {
    let response = MessageBuilder::new()
        .push("Unknown command, type `.help` for list of commands.")
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}

pub(crate) async fn handle_cancel(context: Context, msg: Message) {
    if !admin_check(&context, &msg, true).await { return; }
    common::queue_cancel(&context, &msg).await;
}


pub(crate) async fn handle_teamname(context: Context, msg: Message) {
    common::add_teamname(&context, &msg).await;
}

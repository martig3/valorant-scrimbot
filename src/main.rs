use std::str::FromStr;

use serenity::async_trait;
use serenity::Client;
use serenity::client::Context;
use serenity::framework::standard::StandardFramework;
use serenity::model::channel::Message;
use serenity::model::prelude::Ready;
use serenity::prelude::{EventHandler};
use crate::common::autoclear_queue;
use crate::common_utils::read_config;

mod bot_service;
mod common;
mod common_utils;


enum Command {
    JOIN,
    LEAVE,
    LIST,
    START,
    RIOTID,
    MAPS,
    ADDMAP,
    CANCEL,
    REMOVEMAP,
    KICK,
    CAPTAIN,
    TEAMNAME,
    PICK,
    DEFENSE,
    ATTACK,
    RECOVERQUEUE,
    CLEAR,
    HELP,
    UNKNOWN,
}

struct Handler;

impl FromStr for Command {
    type Err = ();

    fn from_str(input: &str) -> Result<Command, Self::Err> {
        match input {
            ".join" => Ok(Command::JOIN),
            ".leave" => Ok(Command::LEAVE),
            ".list" => Ok(Command::LIST),
            ".start" => Ok(Command::START),
            ".riotid" => Ok(Command::RIOTID),
            ".maps" => Ok(Command::MAPS),
            ".kick" => Ok(Command::KICK),
            ".addmap" => Ok(Command::ADDMAP),
            ".cancel" => Ok(Command::CANCEL),
            ".captain" => Ok(Command::CAPTAIN),
            ".teamname" => Ok(Command::TEAMNAME),
            ".pick" => Ok(Command::PICK),
            ".defense" => Ok(Command::DEFENSE),
            ".attack" => Ok(Command::ATTACK),
            ".removemap" => Ok(Command::REMOVEMAP),
            ".recoverqueue" => Ok(Command::RECOVERQUEUE),
            ".clear" => Ok(Command::CLEAR),
            ".help" => Ok(Command::HELP),
            _ => Err(()),
        }
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, context: Context, msg: Message) {
        if msg.author.bot { return; }
        if !msg.content.starts_with('.') { return; }
        let command = Command::from_str(&msg.content.to_lowercase()
            .trim()
            .split(' ')
            .take(1)
            .collect::<Vec<_>>()[0])
            .unwrap_or(Command::UNKNOWN);
        match command {
            Command::JOIN => bot_service::handle_join(&context, &msg, &msg.author).await,
            Command::LEAVE => bot_service::handle_leave(context, msg).await,
            Command::LIST => bot_service::handle_list(context, msg).await,
            Command::START => bot_service::handle_start(context, msg).await,
            Command::RIOTID => bot_service::handle_riotid(context, msg).await,
            Command::MAPS => bot_service::handle_map_list(context, msg).await,
            Command::KICK => bot_service::handle_kick(context, msg).await,
            Command::CANCEL => bot_service::handle_cancel(context, msg).await,
            Command::ADDMAP => bot_service::handle_add_map(context, msg).await,
            Command::REMOVEMAP => bot_service::handle_remove_map(context, msg).await,
            Command::TEAMNAME => bot_service::handle_teamname(context, msg).await,
            Command::CAPTAIN => bot_service::handle_captain(context, msg).await,
            Command::PICK => bot_service::handle_pick(context, msg).await,
            Command::DEFENSE => bot_service::handle_defense_option(context, msg).await,
            Command::ATTACK => bot_service::handle_attack_option(context, msg).await,
            Command::RECOVERQUEUE => bot_service::handle_recover_queue(context, msg).await,
            Command::CLEAR => bot_service::handle_clear(context, msg).await,
            Command::HELP => bot_service::handle_help(context, msg).await,
            Command::UNKNOWN => bot_service::handle_unknown(context, msg).await,
        }
    }
    async fn ready(&self, context: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        autoclear_queue(&context).await;
    }
}

#[tokio::main]
async fn main() -> () {
    let config = read_config().await.unwrap();
    let token = &config.discord.token;
    let framework = StandardFramework::new();
    let mut client = Client::builder(&token)
        .event_handler(Handler {})
        .framework(framework)
        .await
        .expect("Error creating client");
    common::init_context(&client, config, "riot_ids.json").await;
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}





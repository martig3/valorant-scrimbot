use std::collections::HashMap;

use serenity::model::channel::Message;
use serenity::model::prelude::GuildContainer;
use serenity::model::user::User;
use serenity::prelude::Context;
use serenity::utils::MessageBuilder;

use crate::common::{Config, Draft, Maps};

pub async fn send_simple_tagged_msg(context: &Context, msg: &Message, text: &str, mentioned: &User) -> Option<Message> {
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

pub async fn print_map_pool(context: &Context, msg: &Message) {
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

pub async fn list_unpicked(user_queue: &Vec<User>, draft: &Draft, context: &Context, msg: &Message, team_a_name: &String, team_b_name: &String) {
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

pub async fn admin_check(context: &Context, msg: &Message, print_msg: bool) -> bool {
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

pub async fn move_user(msg: &Message, user: &User, channel_id: u64, context: &Context) {
    if let Some(guild) = &msg.guild(&context.cache).await {
        if let Err(why) = guild.move_member(&context.http, user.id, channel_id).await {
            println!("Cannot move user: {:?}", why);
        }
    }
}

pub async fn send_simple_msg(context: &Context, msg: &Message, text: &str) {
    let response = MessageBuilder::new()
        .push(text)
        .build();
    if let Err(why) = msg.channel_id.say(&context.http, &response).await {
        eprintln!("Error sending message: {:?}", why);
    }
}


pub async fn read_game_accounts(filename: &str) -> Result<HashMap<u64, String>, serde_json::Error> {
    if std::fs::read(filename).is_ok() {
        let json_str = std::fs::read_to_string("riot_ids.json").unwrap();
        let json = serde_json::from_str(&json_str).unwrap();
        Ok(json)
    } else {
        Ok(HashMap::new())
    }
}

pub async fn read_teamnames() -> Result<HashMap<u64, String>, serde_json::Error> {
    if std::fs::read("teamnames.json").is_ok() {
        let json_str = std::fs::read_to_string("teamnames.json").unwrap();
        let json = serde_json::from_str(&json_str).unwrap();
        Ok(json)
    } else {
        Ok(HashMap::new())
    }
}

pub async fn read_maps() -> Result<Vec<String>, serde_json::Error> {
    if std::fs::read("maps.json").is_ok() {
        let json_str = std::fs::read_to_string("maps.json").unwrap();
        let json = serde_json::from_str(&json_str).unwrap();
        Ok(json)
    } else {
        Ok(Vec::new())
    }
}

pub async fn read_config() -> Result<Config, serde_yaml::Error> {
    let yaml = std::fs::read_to_string("config.yaml").unwrap();
    let config: Config = serde_yaml::from_str(&yaml)?;
    Ok(config)
}

pub async fn write_to_file(path: String, content: String) {
    let mut error_string = String::from("Error writing to ");
    error_string.push_str(&path);
    std::fs::write(path, content)
        .expect(&error_string);
}

pub async fn populate_unicode_emojis() -> HashMap<char, String> {
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

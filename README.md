# valorant-scrimbot

Simple Discord bot for managing & organizing a queue for 10 man scrims in Valorant

## Features
Manages a 10 person queue, then starts a map vote followed by a draft. 
At the end of the setup, it prints out everyone's RiotId to help facilitate joining a custom lobby.
### Example Screenshots
#### `.join` the queue
![preview](https://i.imgur.com/8xsKCJh.png)
#### `.start` command will first initiate a map vote
![preview](https://i.imgur.com/YnhO0FA.png)
#### Draft Phase - Captains are volunteered and teams are picked
![preview](https://i.imgur.com/fx6aAWe.png)
#### After the draft is completed, Captain B chooses what side to start on
![preview](https://i.imgur.com/NNoFNf9.png)
## Setup

No CI/CD yet so clone the repo, create a `config.yaml` file (see example below) and run using standard `cargo run`

**Note:** Make sure to only allow the bot to listen/read messages in one channel only. 
### Example config.yaml

```yaml
autoclear_hour: <value between 0-24> -- optional
post-setup-msg: GLHF! Add any string here -- optional
discord:
  token: <your discord bot api token>
  admin_role_id: <a discord server role id> -- optional, but highly recommended!!!
  team_a_channel_id: <a discord channel id> -- optional
  team_b_channel_id: <a discord channel id> -- optional
  assign_role_id: <a dicord role id to assign for user on queue join> -- optional
```

## Commands

`.join` - Join the queue, add an optional message in quotes (max 50 characters) i.e. `.join "available at 9pm"`

`.leave` - Leave the queue

`.queue` - List all users in the queue

`.riotid` - Set your RiotId i.e. `.riotid Martige#NA1` (required before joining queue)

`.maps` - Lists all maps available for map vote

`.teamname` - Sets a custom team name when you are a captain i.e. `.teamname Your Team Name`

_These are commands used during the `.start` process:_

`.captain` - Add yourself as a captain.

`.pick` - If you are a captain, this is used to pick a player by tagging them i.e. `.pick @Martige`

`.defense` - An option to pick the defense side after the draft (if you are Captain B)

`.attack` - An option to pick the attack side after the draft (if you are Captain B)

### Admin Commands - restricted to an 'admin' role if provided in config

`.start` - Start the match setup process

`.kick` - Kick a player by mentioning them i.e. `.kick @user`

`.addmap` - Add a map to the map vote i.e. `.addmap mapname`

`.removemap` - Remove a map from the map vote i.e. `.removemap mapname`

`.recoverqueue` - Manually set a queue, tag all users to add after the command

`.clear` - Clear the queue

`.cancel` - Cancels `.start` process & retains current queue

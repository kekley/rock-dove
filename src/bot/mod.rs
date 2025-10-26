use serenity::{
    all::{ChannelId, Context, EventHandler, GuildId, Message, User, UserId},
    async_trait,
};

/*
 * Commands I want to do:
 * play now
 * quit
 * list
 * shuffle
 * add
 * clear queue
 * loop
 * remove from queue position
 * pause
 * resume
 * nowplaying
 * skip {first,number,user}
 * undo (maybe)
 * bot stats (most listened to, most skipped)
 * move (song position)
 * beep
 */

struct CommandContext {
    channel: ChannelId,
    author: UserId,
}

impl CommandContext {
    pub fn from_message(message: &Message) -> Self {
        let channel = message.channel_id;
        let author = message.author.id;
        Self { channel, author }
    }
}

struct QueueEntry {
    user: UserId,
}

struct GuildContext {
    start_pattern: String,
    playback_queue: Vec<QueueEntry>,
}

struct MusicBot {
    guild_datas: Vec<GuildContext>,
}

#[async_trait]
impl EventHandler for MusicBot {
    async fn message(&self, ctx: Context, msg: Message) {
        let Some(guild_id) = msg.guild_id else {
            return;
        };

        let Some(guild_context) = self.get_guild_context_for_id(guild_id) else {
            return;
        };

        if msg
            .content
            .starts_with(guild_context.start_pattern.as_str())
        {
            let command_string = msg
                .content
                .strip_prefix(guild_context.start_pattern.as_str())
                .expect("Message should always start with prefix at this point");
            let command_context: CommandContext = CommandContext::from_message(&msg);
            let Some(actions) = self.parse_user_command(command_string, command_context) else {
                return;
            };
            todo!()
        }
        /*
                if msg.content == "!ping"
                    && let Err(why) = msg.channel_id.say(&ctx.http, "Pong!").await
                {
                    println!("Error sending message: {why:?}");
                }
        */
    }
}

pub enum BotAction {
    JoinChannel(()),
    SendMessage {
        channel: ChannelId,
        contents: String,
    },
    AddToQueue(()),
    RemoveFromQueue(()),
    LeaveChannel(()),
}

impl MusicBot {
    fn parse_user_command(
        &self,
        command_string: &str,
        context: CommandContext,
    ) -> Option<Vec<BotAction>> {
        let Some(command) = command_string.split_whitespace().next() else {
            return None;
        };
        let mut actions = vec![];

        match command {
            "playnow" => {
                unimplemented!()
            }
            "leave" => {
                unimplemented!()
            }

            "skip" => {
                unimplemented!()
            }

            "list" => {
                unimplemented!()
            }

            "shuffle" => {
                unimplemented!()
            }

            "add" => {
                unimplemented!()
            }

            "clear" => {
                unimplemented!()
            }

            "loop" => {
                unimplemented!()
            }

            "remove" => {
                unimplemented!()
            }

            "pause" => {
                unimplemented!()
            }

            "resume" => {
                unimplemented!()
            }

            "nowplaying" => {
                unimplemented!()
            }

            "undo" => {
                unimplemented!()
            }

            "stats" => {
                unimplemented!()
            }

            "move" => {
                unimplemented!()
            }
            "beep" => {
                actions.push(BotAction::SendMessage {
                    channel: context.channel,
                    contents: format!("boop"),
                });
                unimplemented!()
            }

            _ => {
                return None;
            }
        }
    }

    fn get_guild_context_for_id(&self, id: GuildId) -> Option<&mut GuildContext> {
        todo!()
    }
}

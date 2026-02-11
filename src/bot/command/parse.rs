use compact_str::{CompactString, format_compact};
use serenity::all::{ChannelId, Context, GuildId, Message, UserId};

use crate::{
    bot::{
        command::remove::{RemoveArgParseError, RemoveArgument},
        guild_context::queue::LoopMode,
        util::{choose_insult, get_or_insert_guild_context_lock, str_closest_to},
    },
    yt_dlp::VideoQuery,
};

/// A command for the bot paired with all the necessary data for executing it
#[derive(Debug)]
pub enum PreparedCommand {
    Help {
        user: UserId,
        guild: GuildId,
    },
    Leave {
        guild: GuildId,
    },
    Join {
        voice_channel: ChannelId,
        text_channel: ChannelId,
        guild: GuildId,
    },
    Skip {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    List {
        guild: GuildId,
        text_channel: ChannelId,
    },
    Shuffle {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Play {
        guild: GuildId,
        query: VideoQuery,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },

    Add {
        guild: GuildId,
        query: VideoQuery,
        user: UserId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Clear {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Loop {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
        mode: LoopMode,
    },
    Remove {
        arg: RemoveArgument,
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Pause {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Resume {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    NowPlaying {
        guild: GuildId,
        text_channel: ChannelId,
    },
    Undo {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Redo {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Mute {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Unmute {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Beep {
        text_channel: ChannelId,
    },
    Ffmpeg {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
        url: String,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum CommandParseError {
    #[error("")]
    NoWhitespace,
    #[error("You need to specify a link or something to search for")]
    NoQueryArgument,
    #[error("The bot does not accept commands from a private message channel")]
    NoGuild,
    #[error("There was no start pattern in the message")]
    NoStartPattern,
    #[error("You need to be in a voice channel to use this command")]
    NoVoiceChannnel,
    #[error("Valid loop modes: single, queue, off")]
    InvalidLoopMode,
    #[error("{0}")]
    InvalidRemoveArg(#[from] RemoveArgParseError),
    #[error("{suggestion}")]
    UnrecognizedCommand { suggestion: CompactString },
}

impl CommandParseError {
    pub fn user_reply(&self) -> Option<String> {
        match self {
            CommandParseError::InvalidRemoveArg(_)
            | CommandParseError::NoQueryArgument
            | CommandParseError::InvalidLoopMode
            | CommandParseError::NoVoiceChannnel
            | CommandParseError::UnrecognizedCommand { suggestion: _ } => Some(self.to_string()),
            _ => None,
        }
    }
}

impl PreparedCommand {
    pub async fn parse_discord_message(
        message: Message,
        ctx: &Context,
    ) -> Result<Self, CommandParseError> {
        let guild = message.guild(&ctx.cache);
        let user = message.author.id;
        let text_channel = message.channel_id;

        let voice_channel = guild
            .and_then(|cache_ref| {
                cache_ref
                    .voice_states
                    .get(&user)
                    .map(|state| state.channel_id)
            })
            .flatten();

        let Some(guild) = message.guild_id else {
            return Err(CommandParseError::NoGuild);
        };
        let text = message.content.as_str();
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let start_pattern = {
            let guild_context_guard = guild_lock.read().await;
            guild_context_guard.start_pattern
        };

        let Some(start_pattern_stripped) = text.strip_prefix(start_pattern) else {
            return Err(CommandParseError::NoStartPattern);
        };

        let (command, remainder) = start_pattern_stripped
            .split_once(" ")
            .unwrap_or((start_pattern_stripped, ""));

        let mut command_lower = CompactString::from(command);
        command_lower.make_ascii_lowercase();
        match command_lower.as_str() {
            "help" => Ok(PreparedCommand::Help { user, guild }),

            "leave" => Ok(PreparedCommand::Leave { guild }),

            "join" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Join {
                    voice_channel,
                    text_channel,
                    guild,
                })
            }

            "skip" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Skip {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }

            "list" => Ok(PreparedCommand::List {
                guild,
                text_channel,
            }),
            "shuffle" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Shuffle {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "play" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                if remainder.trim().is_empty() {
                    return Err(CommandParseError::NoQueryArgument);
                }

                let query = VideoQuery::new_from_str(remainder);

                Ok(PreparedCommand::Play {
                    guild,
                    query,
                    voice_channel,
                    text_channel,
                })
            }
            "ffmpreg" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                if remainder.trim().is_empty() {
                    return Err(CommandParseError::NoQueryArgument);
                }

                Ok(PreparedCommand::Ffmpeg {
                    guild,
                    url: remainder.to_string(),
                    voice_channel,
                    text_channel,
                })
            }
            "add" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                if remainder.trim().is_empty() {
                    return Err(CommandParseError::NoQueryArgument);
                }

                let query = VideoQuery::new_from_str(remainder);

                Ok(PreparedCommand::Add {
                    guild,
                    query,
                    user,
                    voice_channel,
                    text_channel,
                })
            }
            "clear" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Clear {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "loop" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                let Some(mode) = LoopMode::parse(remainder) else {
                    return Err(CommandParseError::InvalidLoopMode);
                };

                Ok(PreparedCommand::Loop {
                    guild,
                    voice_channel,
                    text_channel,
                    mode,
                })
            }
            "remove" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                let arg = RemoveArgument::parse(remainder)?;

                Ok(PreparedCommand::Remove {
                    arg,
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "pause" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Pause {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "resume" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Resume {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "nowplaying" => Ok(PreparedCommand::NowPlaying {
                guild,
                text_channel,
            }),
            "undo" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Undo {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "redo" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Redo {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "mute" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Mute {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "unmute" => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Unmute {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "beep" => Ok(PreparedCommand::Beep {
                text_channel: message.channel_id,
            }),

            _ => {
                let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
                let guard = guild_lock.read().await;
                let aliases = guard.get_command_mappings().get_command_aliases();
                let closest = str_closest_to(&command_lower, aliases, 0.5);

                let msg = if let Some(closest) = closest {
                    let insult = choose_insult();
                    format_compact!(
                        "Unrecognized command `{start_pattern}{command}`, did you mean: `{start_pattern}{closest}`, {insult}? 🤦",
                    )
                } else {
                    format_compact!(
                        "Unrecognized command `{start_pattern}{command}`, try `{start_pattern}help`",
                    )
                };
                Err(CommandParseError::UnrecognizedCommand { suggestion: msg })
            }
        }
    }
}

use compact_str::{CompactString, format_compact};
use serenity::all::{ChannelId, Context, GuildId, Message, UserId};

use crate::{
    bot::{
        command::{
            Command,
            remove::{RemoveArgParseError, RemoveArgument},
        },
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
    Next {
        guild: GuildId,
        voice_channel: ChannelId,
        text_channel: ChannelId,
    },
    Back {
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
    Prefix {
        guild: GuildId,
        new_prefix: char,
        text_channel: ChannelId,
    },

    Alias {
        guild: GuildId,
        old_command: CompactString,
        new_command: CompactString,
        text_channel: ChannelId,
    },
}

#[derive(thiserror::Error, Debug)]
pub enum CommandParseError {
    #[error("")]
    NoWhitespace,
    #[error("You need to specify a link or something to search for")]
    NoQueryArgument,
    #[error("")]
    NoGuild,
    #[error("")]
    NoStartPattern,
    #[error("You need to be in a voice channel to use this command")]
    NoVoiceChannnel,
    #[error("Valid loop modes: single, queue, off")]
    InvalidLoopMode,
    #[error("{0}")]
    InvalidRemoveArg(#[from] RemoveArgParseError),
    #[error("You need to specify a new prefix character")]
    NoPrefixArgument,
    #[error("The new prefix must be a single ascii character")]
    InvalidPrefix,
    #[error("You need to specify both the current command and the new command")]
    MissingAliasArgument,
    #[error("{suggestion}")]
    UnrecognizedCommand { suggestion: CompactString },
}

impl CommandParseError {
    pub fn should_log(&self) -> bool {
        !matches!(
            self,
            CommandParseError::NoGuild
                | CommandParseError::NoWhitespace
                | CommandParseError::NoStartPattern
        )
    }
    pub fn user_reply(&self) -> Option<String> {
        match self {
            CommandParseError::InvalidRemoveArg(_)
            | CommandParseError::NoQueryArgument
            | CommandParseError::InvalidLoopMode
            | CommandParseError::NoVoiceChannnel
            | CommandParseError::NoPrefixArgument
            | CommandParseError::InvalidPrefix
            | CommandParseError::MissingAliasArgument
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

        let guild_context_guard = guild_lock.read().await;
        let start_pattern = guild_context_guard.start_pattern;
        let aliases = &guild_context_guard.command_aliases;

        let Some(prefix_stripped) = text.strip_prefix(start_pattern) else {
            return Err(CommandParseError::NoStartPattern);
        };

        let (command, remainder) = prefix_stripped
            .split_once(" ")
            .unwrap_or((prefix_stripped, ""));
        let Some(command) = aliases.get_command_for_alias(command) else {
            let aliases_iter = aliases.iter();
            let closest = str_closest_to(command, aliases_iter, 0.5);

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
            return Err(CommandParseError::UnrecognizedCommand { suggestion: msg });
        };

        match command {
            Command::Help => Ok(PreparedCommand::Help { user, guild }),
            Command::Leave => Ok(PreparedCommand::Leave { guild }),
            Command::Join => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Join {
                    voice_channel,
                    text_channel,
                    guild,
                })
            }
            Command::Next => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Next {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::List => Ok(PreparedCommand::List {
                guild,
                text_channel,
            }),
            Command::Shuffle => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Shuffle {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Play => {
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
            Command::Ffmpeg => {
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
            Command::Add => {
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
            Command::Clear => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Clear {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Loop => {
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
            Command::Remove => {
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
            Command::Pause => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Pause {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Resume => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Resume {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::NowPlaying => Ok(PreparedCommand::NowPlaying {
                guild,
                text_channel,
            }),
            Command::Undo => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Undo {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Redo => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Redo {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Mute => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };

                Ok(PreparedCommand::Mute {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Unmute => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Unmute {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Beep => Ok(PreparedCommand::Beep {
                text_channel: message.channel_id,
            }),
            Command::Back => {
                let Some(voice_channel) = voice_channel else {
                    return Err(CommandParseError::NoVoiceChannnel);
                };
                Ok(PreparedCommand::Next {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            Command::Alias => {
                let mut parts = remainder.split_whitespace();
                let old_command = parts.next();
                let new_command = parts.next();

                if let Some(old_command) = old_command
                    && let Some(new_command) = new_command
                {
                    Ok(PreparedCommand::Alias {
                        guild,
                        old_command: CompactString::from(old_command),
                        new_command: CompactString::from(new_command),
                        text_channel,
                    })
                } else {
                    Err(CommandParseError::MissingAliasArgument)
                }
            }
            Command::Prefix => {
                let trimmed = remainder.trim();
                if trimmed.is_empty() {
                    return Err(CommandParseError::NoPrefixArgument);
                }
                if !trimmed.is_ascii() {
                    return Err(CommandParseError::InvalidPrefix);
                }
                if trimmed.len() > 1 {
                    return Err(CommandParseError::InvalidPrefix);
                }
                let new_prefix = trimmed
                    .chars()
                    .next()
                    .expect("trimmed is not empty, so it should have a char");
                Ok(PreparedCommand::Prefix {
                    guild,
                    new_prefix,
                    text_channel,
                })
            }
        }
    }
}

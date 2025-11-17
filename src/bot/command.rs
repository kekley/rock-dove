use std::{num::ParseIntError, sync::Arc};

use compact_str::CompactString;

use serenity::{
    all::{ChannelId, Context, CreateMessage, EditMessage, GuildId, Message, UserId},
    model::guild,
};
use songbird::{Call, Songbird, TrackEvent, error::JoinError};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tracing::{Level, event};

use crate::{
    GuildContextKey,
    bot::{
        command_is_in_same_call,
        guild_context::{self, GuildContext, TrackControlError},
        queue::LoopMode,
        send_message,
        track_notifier::{TrackEndNotifier, TrackErrorNotifier},
    },
    yt_dlp::{VideoQuery, YtDlp, YtDlpKey, sidecar::YtDlpSidecar},
};

pub enum BotCommand {
    Help {
        user: UserId,
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
}

#[derive(Error, Debug)]
pub enum BotCommandError {
    #[error("Could not split the command string by whitespace once")]
    NoWhitespace,
    #[error("Command did not come from a guild")]
    MessageNotFromGuild,
    #[error("Command sent requires a voice channel but the issuer was not in one")]
    IssuerNotInVoiceChannel,
    #[error("Command not recognized")]
    UnrecognizedCommand,
    #[error("Invalid loop mode")]
    InvalidLoopMode,
    #[error("{0}")]
    InvalidRemoveArgument(#[from] RemoveArgParseError),
}

impl BotCommand {
    pub fn parse(message: Message, ctx: &Context) -> Result<Self, BotCommandError> {
        let guild = message.guild(&ctx.cache);
        let text = message.content;
        let Some((first, remainder)) = text.split_once(" ") else {
            return Err(BotCommandError::NoWhitespace);
        };
        let mut first_lower = CompactString::new("");
        for mut char in first.chars() {
            char.make_ascii_lowercase();
            first_lower.push(char);
        }
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

        let guild = message.guild_id;

        match first_lower.as_str() {
            "help" => Ok(BotCommand::Help { user }),

            "leave" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                Ok(BotCommand::Leave { guild })
            }

            "join" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                Ok(BotCommand::Join {
                    voice_channel,
                    text_channel,
                    guild,
                })
            }

            "skip" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };
                Ok(BotCommand::Skip {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }

            "list" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };

                Ok(BotCommand::List {
                    guild,
                    text_channel,
                })
            }
            "shuffle" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                Ok(BotCommand::Shuffle {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "add" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                let query = VideoQuery::new_from_str(remainder);

                Ok(BotCommand::Add {
                    guild,
                    query,
                    user,
                    voice_channel,
                    text_channel,
                })
            }
            "clear" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };
                Ok(BotCommand::Clear {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "loop" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                let Some(mode) = LoopMode::parse(remainder) else {
                    return Err(BotCommandError::InvalidLoopMode);
                };

                Ok(BotCommand::Loop {
                    guild,
                    voice_channel,
                    text_channel,
                    mode,
                })
            }
            "remove" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                let arg = RemoveArgument::parse(remainder)?;

                Ok(BotCommand::Remove {
                    arg,
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "pause" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                Ok(BotCommand::Pause {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "resume" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                Ok(BotCommand::Resume {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "nowplaying" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };

                Ok(BotCommand::NowPlaying {
                    guild,
                    text_channel,
                })
            }
            "undo" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };
                Ok(BotCommand::Undo {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "redo" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                Ok(BotCommand::Redo {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "mute" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };

                Ok(BotCommand::Mute {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "unmute" => {
                let Some(guild) = guild else {
                    return Err(BotCommandError::MessageNotFromGuild);
                };
                let Some(voice_channel) = voice_channel else {
                    return Err(BotCommandError::IssuerNotInVoiceChannel);
                };
                Ok(BotCommand::Unmute {
                    guild,
                    voice_channel,
                    text_channel,
                })
            }
            "beep" => Ok(BotCommand::Beep {
                text_channel: message.channel_id,
            }),

            _ => Err(BotCommandError::UnrecognizedCommand),
        }
    }
}

#[derive(Debug, Error)]
pub enum CommandExecutionError {
    #[error("Serenity Error: {0}")]
    SerenityError(#[from] serenity::Error),

    #[error("Guild not found")]
    GuildNotFound,

    #[error("{0}")]
    TrackControlError(#[from] TrackControlError),

    #[error("{0}")]
    JoinError(#[from] JoinError),

    #[error("Not in a voice channel")]
    VoiceChannelMismatch,

    #[error("Bot is not in a voice channel")]
    BotNotInVoiceChannel,
}

impl BotCommand {
    pub async fn execute(self, ctx: &Context) {
        match self {
            BotCommand::Help { user } => {
                help(ctx, user).await;
            }
            BotCommand::Leave { guild } => {
                leave(ctx, guild).await;
            }
            BotCommand::Join {
                voice_channel,
                text_channel,
                guild,
            } => {
                handle_voice_channel_joining(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Skip {
                guild,
                voice_channel,
                text_channel,
            } => {
                skip(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::List {
                guild,
                text_channel,
            } => {
                list(ctx, guild, text_channel).await;
            }
            BotCommand::Shuffle {
                guild,
                voice_channel,
                text_channel,
            } => {
                shuffle(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Add {
                guild,
                query,
                user,
                voice_channel,
                text_channel,
            } => {
                add(ctx, guild, query, user, voice_channel, text_channel).await;
            }
            BotCommand::Clear {
                guild,
                voice_channel,
                text_channel,
            } => {
                clear(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Loop {
                guild,
                voice_channel,
                text_channel,
                mode,
            } => {
                set_loop(ctx, guild, voice_channel, text_channel, mode).await;
            }
            BotCommand::Remove {
                arg,
                guild,
                voice_channel,
                text_channel,
            } => {
                remove(ctx, arg, guild, voice_channel, text_channel).await;
            }
            BotCommand::Pause {
                guild,
                voice_channel,
                text_channel,
            } => {
                pause(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Resume {
                guild,
                voice_channel,
                text_channel,
            } => {
                resume(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::NowPlaying {
                guild,
                text_channel,
            } => {
                now_playing(ctx, guild, text_channel).await;
            }
            BotCommand::Undo {
                guild,
                voice_channel,
                text_channel,
            } => {
                undo(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Redo {
                guild,
                voice_channel,
                text_channel,
            } => {
                redo(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Mute {
                guild,
                voice_channel,
                text_channel,
            } => {
                mute(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Unmute {
                guild,
                voice_channel,
                text_channel,
            } => {
                unmute(ctx, guild, voice_channel, text_channel).await;
            }
            BotCommand::Beep { text_channel } => {
                beep(ctx, text_channel).await;
            }
        }
    }
}

async fn mute(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |call| {
        match call.lock().await.mute(false).await {
            Ok(_) => todo!(),
            Err(_) => todo!(),
        }
    })
    .await;
}

async fn unmute(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |call| {
        match call.lock().await.mute(true).await {
            Ok(_) => todo!(),
            Err(_) => todo!(),
        }
    })
    .await;
}

async fn redo(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        guild_lock.write().await.redo();
    })
    .await;
}

async fn undo(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        guild_lock.write().await.undo();
    })
    .await;
}

async fn now_playing(ctx: &Context, guild: GuildId, text_channel: ChannelId) {
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
    let guild_context = guild_lock.read().await;
    #[cfg(feature = "tracing")]
    event!(Level::INFO, "nowplaying command issued");
    if let Some(track) = guild_context.get_current_track_info() {
        let handle = track.handle.clone();
        //TODO Print the current track position
        let _pos = handle
            .get_info()
            .await
            .map(|info| info.position.as_secs())
            .ok();

        let _ = send_message(
            text_channel,
            &ctx.http,
            &format!(
                "Currently playing: {track_name}",
                track_name = track.stream.name
            ),
        )
        .await;
    } else {
        let _ = send_message(text_channel, &ctx.http, "Not playing anything right now").await;
    }
}

async fn resume(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        match guild_context.resume_current_track().await {
            Ok(_) => {
                let _ = send_message(text_channel, &ctx.http, "Resumed track playback").await;
            }
            Err(err) => match err {
                TrackControlError::NoTrack => {
                    let _ = send_message(text_channel, &ctx.http, "No track to resume").await;
                }
                TrackControlError::Error(control_error) => {
                    let _ = send_message(
                        text_channel,
                        &ctx.http,
                        &format!("Error resuming track: {control_error}"),
                    )
                    .await;
                }
            },
        }
    })
    .await
}

async fn pause(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        match guild_context.pause_current_track().await {
            Ok(_) => {
                let _ = send_message(text_channel, &ctx.http, "Resumed track playback").await;
            }
            Err(err) => match err {
                TrackControlError::NoTrack => {
                    let _ = send_message(text_channel, &ctx.http, "No track to resume").await;
                }
                TrackControlError::Error(control_error) => {
                    let _ = send_message(
                        text_channel,
                        &ctx.http,
                        &format!("Error resuming track: {control_error}"),
                    )
                    .await;
                }
            },
        }
    })
    .await
}

async fn remove(
    ctx: &Context,
    arg: RemoveArgument,
    guild: GuildId,
    voice_channel: ChannelId,
    text_channel: ChannelId,
) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        match arg {
            RemoveArgument::From(user) => {
                match guild_context.remove_tracks_from(guild, &user, ctx).await {
                    Ok(tracks_removed) => todo!(),
                    Err(err) => todo!(),
                }
            }
            RemoveArgument::At(pos) => {
                let range = (pos as usize - 1)..pos as usize;
                let tracks_removed = guild_context.remove_tracks_in_range(range);
            }
            RemoveArgument::Until(pos) => {
                let max = guild_context.queue_length();
                let range = 0..(pos as usize - 1).min(max);
                let tracks_removed = guild_context.remove_tracks_in_range(range);
            }
            RemoveArgument::Past(pos) => {
                let max = guild_context.queue_length();
                let range = (pos as usize)..max;
                let tracks_removed = guild_context.remove_tracks_in_range(range);
            }
        }
    })
    .await;
}

async fn set_loop(
    ctx: &Context,
    guild: GuildId,
    voice_channel: ChannelId,
    text_channel: ChannelId,
    mode: LoopMode,
) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        guild_context.set_loop_mode(mode).await;
        //TODO Send chat message here
        todo!()
    })
    .await;
}

async fn clear(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        guild_context.clear_queue().await;
        //TODO Send chat message here
        todo!()
    })
    .await;
}

async fn add(
    ctx: &Context,
    guild: GuildId,
    query: VideoQuery,
    user: UserId,
    voice_channel: ChannelId,
    text_channel: ChannelId,
) {
    handle_voice_channel_joining(ctx, guild, voice_channel, text_channel).await;
    let mut message_sent = send_message(text_channel, &ctx.http, " Searching...").await;
    let yt_dlp = get_ytdlp(ctx).await;
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;

    if query.is_playlist() {
        if let Some(message) = message_sent.as_mut() {
            let builder = EditMessage::new().content("Searching... (Playlists can take a while)");
            let _ = message.edit(&ctx.http, builder).await;
            let VideoQuery::Url(url) = query else {
                //is_playlist ensures we have the url enum
                unreachable!();
            };
            let Ok(streams) = yt_dlp.search_for_playlist(&url).await else {
                if let Some(mut message) = message_sent {
                    let builder =
                        EditMessage::new().content("I couldn't find anything :(".to_string());
                    let _ = message.edit(&ctx.http, builder).await;
                };

                return;
            };
            let len = streams.len();
            if let Some(mut message) = message_sent {
                let builder =
                    EditMessage::new().content(format!("Adding {len} tracks to the queue",));
                let _ = message.edit(&ctx.http, builder).await;
            };
            let streams = streams.into_iter().map(Arc::new).collect::<Vec<_>>();

            guild_lock
                .write()
                .await
                .add_many_to_queue(user, &streams, &ctx, guild)
                .await;
        };
    } else {
        let Ok(video) = yt_dlp.search_for_video(&query).await else {
            if let Some(mut message) = message_sent {
                let builder = EditMessage::new().content("I couldn't find anything :(".to_string());
                let _ = message.edit(&ctx.http, builder).await;
            };

            return;
        };
        let video_info_arc = Arc::new(video);

        let queue_length = guild_lock.read().await.playback_queue.num_tracks();

        if let Some(mut message) = message_sent {
            let builder = EditMessage::new().content(format!(
                "Adding {track_name} to the queue at position {pos}",
                pos = queue_length + 1,
                track_name = video_info_arc.title()
            ));
            let _ = message.edit(&ctx.http, builder).await;
        };

        guild_lock
            .write()
            .await
            .add_to_queue(user, video_info_arc, &ctx, guild)
            .await;
    }
}

async fn shuffle(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        if guild_context.queue_length() == 0 {
            let _ = send_message(ctx, text_channel, "Queue is empty").await;
        } else if guild_context.queue_length() == 1 {
            guild_context.shuffle_queue().await;
            let _ = send_message(ctx, text_channel, "shuffled.... one song");
        } else {
            guild_context.shuffle_queue().await;
            let _ = send_message(ctx, text_channel, "Shuffled the queue");
        }
    })
    .await
}

async fn list(ctx: &Context, guild: GuildId, text_channel: ChannelId) {
    todo!()
}

async fn skip(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        match guild_lock.write().await.next_track(ctx, guild).await {
            Ok(_) => todo!(),
            Err(_) => todo!(),
        }
    })
    .await
}

async fn leave(ctx: &Context, guild: GuildId) {
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
    let songbird_manager = get_songbird(ctx).await;

    if let Some(call) = songbird_manager.get(guild) {
        let _ = call.lock().await.leave().await;
    }
    let mut write_lock = guild_lock.write().await;
    write_lock.pause_current_track().await;
}

async fn help(ctx: &Context, user: UserId) {
    const COMMAND_SYNTAX: [&str; 16] = [
        "help",
        "add { url | playlist url | search text }",
        "join",
        "leave",
        "list",
        "clear",
        "loop { off | single | queue }",
        "remove {  at | past | until | from } ",
        "pause",
        "resume",
        "shuffle",
        "nowplaying",
        "skip",
        "undo",
        "redo",
        "beep",
    ];
    const COMMAND_EXPLANATION: [&str; 16] = [
        "Show this list.",
        "Add a song or playlist to the queue from a url or youtube search.",
        "Join the voice channel you're in.",
        "Remove the bot from any voice channels.",
        "List the current contents of the queue.",
        "Clear the queue.",
        "Set the loop mode.\noff = No looping\nsingle = Loop the current song indefinitely\nqueue = Loop the queue when it ends",
        "Remove one or more tracks from the queue.\nremove at (track position) = Remove the track at (track position)\nremove past (track position) = Remove all tracks after (track position)\nremove until (track position) = Remove all tracks up to (track position)\nremove from (username) = Remove all tracks added by (username)",
        "Pause the current track.",
        "Resume the current track.",
        "Shuffle the contents of the queue.",
        "See the name of the current track.",
        "End the current track.",
        "Undo the last change made to the queue.",
        "Undo the last undo..?",
        "Say hi",
    ];
    let mut help_message = String::new();
    help_message.push_str("COMMANDS:\n");
    COMMAND_SYNTAX
        .iter()
        .zip(COMMAND_EXPLANATION)
        .for_each(|(syntax, explanation)| {
            help_message.push_str(syntax);
            help_message.push_str(" : ");
            help_message.push_str(explanation);
            help_message.push('\n');
            help_message.push('\n');
        });
    //Remove the two trailing newlines
    help_message.pop();
    help_message.pop();

    user.direct_message(&ctx.http, CreateMessage::new().content(help_message))
        .await;
}

async fn handle_voice_channel_joining(
    ctx: &Context,
    guild: GuildId,
    voice_channel: ChannelId,
    text_channel: ChannelId,
) {
    let songbird_manager = get_songbird(ctx).await;
    if let Some(call) = songbird_manager.get(guild) {
        let call_lock = call.lock().await;
        if let Some(current_channel) = call_lock.current_channel()
            && current_channel == voice_channel.into()
        {
            //Already in the correct call wheeeeeeee
            return;
        }
    }

    match songbird_manager.join(guild, voice_channel).await {
        Ok(call) => {
            let mut call_lock = call.lock().await;
            call_lock.add_global_event(
                TrackEvent::Error.into(),
                TrackErrorNotifier {
                    context: ctx.clone(),
                    guild_id: guild,
                },
            );
            call_lock.add_global_event(
                TrackEvent::End.into(),
                TrackEndNotifier {
                    context: ctx.clone(),
                    guild_id: guild,
                },
            );
        }
        Err(err) => {
            #[cfg(feature = "tracing")]
            event!(Level::ERROR, "Failed to join voice call. Error: {err}");
            //TODO send error message here
            todo!();
        }
    }
}

async fn beep(ctx: &Context, channel: ChannelId) {
    channel.say(&ctx.http, "Boop").await;
}

pub enum RemoveArgument {
    From(CompactString),
    At(u32),
    Until(u32),
    Past(u32),
}

#[derive(Debug, Error)]
pub enum RemoveArgParseError {
    #[error("")]
    InvalidUsage,
    #[error("")]
    InvalidArg(#[from] ParseIntError),
}

impl RemoveArgument {
    pub(crate) fn parse(str: &str) -> Result<Self, RemoveArgParseError> {
        let Some((kind, arg)) = str.split_once(" ") else {
            return Err(RemoveArgParseError::InvalidUsage);
        };
        let mut kind_lower = CompactString::new("");
        for mut char in kind.chars() {
            char.make_ascii_lowercase();
            kind_lower.push(char);
        }

        match kind_lower.as_str() {
            "from" => Ok(RemoveArgument::From(CompactString::from(arg))),
            "at" => {
                let arg = arg.trim().parse::<u32>()?;
                Ok(RemoveArgument::At(arg))
            }
            "until" => {
                let arg = arg.trim().parse::<u32>()?;
                Ok(RemoveArgument::Until(arg))
            }
            "past" => {
                let arg = arg.trim().parse::<u32>()?;
                Ok(RemoveArgument::Past(arg))
            }
            _ => Err(RemoveArgParseError::InvalidUsage),
        }
    }
}
async fn get_or_insert_guild_lock(ctx: &Context, guild: GuildId) -> Arc<RwLock<GuildContext>> {
    let data = ctx.data.read().await;
    let guilds = data
        .get::<GuildContextKey>()
        .expect("Guild Contexts should have been created at startup");
    if let Some((_, arc)) = guilds.iter().find(|(id, _)| *id == guild) {
        return arc.clone();
    } else {
        drop(guilds);
        drop(data);

        let mut data = ctx.data.write().await;
        let guilds = data
            .get_mut::<GuildContextKey>()
            .expect("Guild Contexts should have been created at startup");
        let guild_context = Arc::new(RwLock::new(GuildContext::default()));

        guilds.push((guild, guild_context.clone()));

        return guild_context.clone();
    }
}

pub async fn get_songbird(ctx: &Context) -> Arc<Songbird> {
    let songbird = songbird::get(ctx)
        .await
        .expect("Songbird should have been inserted at startup");
    songbird
}
pub async fn get_ytdlp(ctx: &Context) -> Arc<YtDlpSidecar> {
    ctx.data
        .read()
        .await
        .get::<YtDlpKey>()
        .expect("YtDlp should have been inserted at startup")
        .clone()
}

async fn run_if_same_call<F: Future<Output = ()>>(
    ctx: &Context,
    guild: GuildId,
    voice_channel: ChannelId,
    f: impl FnOnce(Arc<Mutex<Call>>) -> F,
) {
    let songbird = get_songbird(ctx).await;
    if let Some(call) = songbird.get(guild)
        && call
            .lock()
            .await
            .current_channel()
            .is_some_and(|c| c.eq(&voice_channel.into()))
    {
        return f(call).await;
    } else {
        //print a message about bot not being in a voice channel
        todo!()
    }
}

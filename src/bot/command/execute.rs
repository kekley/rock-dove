use std::{fmt::Write as _, sync::Arc};

use serenity::all::{ChannelId, Context, CreateMessage, EditMessage, GuildId, UserId};
use songbird::{error::JoinError, input::HttpRequest};
use strum::IntoEnumIterator;
use tracing::{Level, event};

use crate::{
    HTTPClientKey,
    bot::{
        command::{Command, parse::PreparedCommand, remove::RemoveArgument},
        guild_context::{RemoveTracksFromError, TrackControlError, queue::LoopMode},
        util::{
            ensure_same_call, get_or_insert_guild_context_lock, get_songbird,
            get_ytdlp_from_global_context, handle_voice_channel_joining, send_message,
        },
    },
    yt_dlp::{VideoQuery, YtDlp as _},
};

#[derive(Debug, thiserror::Error)]
pub enum CommandExecutionError {
    #[error("Serenity Error: {0}")]
    SerenityError(#[from] serenity::Error),
    #[error("Guild not found")]
    GuildNotFound,
    #[error("Error controlling track: {0}")]
    TrackControlError(#[from] TrackControlError),
    #[error("{0}")]
    VoiceChannelJoinError(#[from] JoinError),
    #[error("Not in the correct voice channel")]
    VoiceChannelMismatch,
    #[error("Bot is not in a voice channel")]
    BotNotInVoiceChannel,
}

impl PreparedCommand {
    pub async fn execute(self, ctx: Context) {
        match self {
            PreparedCommand::Help { user, guild } => {
                help(&ctx, user, guild).await;
            }
            PreparedCommand::Leave { guild } => {
                leave(&ctx, guild).await;
            }
            PreparedCommand::Join {
                voice_channel,
                text_channel,
                guild,
            } => {
                handle_voice_channel_joining(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Next {
                guild,
                voice_channel,
                text_channel,
            } => {
                next(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::List {
                guild,
                text_channel,
            } => {
                list_queue_contents(&ctx, guild, text_channel).await;
            }
            PreparedCommand::Shuffle {
                guild,
                voice_channel,
                text_channel,
            } => {
                shuffle(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Add {
                guild,
                query,
                user,
                voice_channel,
                text_channel,
            } => {
                add(&ctx, guild, query, user, voice_channel, text_channel).await;
            }
            PreparedCommand::Clear {
                guild,
                voice_channel,
                text_channel,
            } => {
                clear(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Loop {
                guild,
                voice_channel,
                text_channel,
                mode,
            } => {
                set_loop(&ctx, guild, voice_channel, text_channel, mode).await;
            }
            PreparedCommand::Remove {
                arg,
                guild,
                voice_channel,
                text_channel,
            } => {
                remove(&ctx, arg, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Pause {
                guild,
                voice_channel,
                text_channel,
            } => {
                pause(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Resume {
                guild,
                voice_channel,
                text_channel,
            } => {
                resume(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::NowPlaying {
                guild,
                text_channel,
            } => {
                now_playing(&ctx, guild, text_channel).await;
            }
            PreparedCommand::Undo {
                guild,
                voice_channel,
                text_channel,
            } => {
                undo(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Redo {
                guild,
                voice_channel,
                text_channel,
            } => {
                redo(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Mute {
                guild,
                voice_channel,
                text_channel,
            } => {
                mute(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Unmute {
                guild,
                voice_channel,
                text_channel,
            } => {
                unmute(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::Beep { text_channel } => {
                beep(&ctx, text_channel).await;
            }
            PreparedCommand::Play {
                guild,
                query,
                voice_channel,
                text_channel,
            } => {
                play(&ctx, guild, query, voice_channel, text_channel).await;
            }
            PreparedCommand::Ffmpeg {
                guild,
                voice_channel,
                text_channel,
                url,
            } => ffmpeg(&ctx, guild, url, text_channel, voice_channel).await,
            PreparedCommand::Back {
                guild,
                voice_channel,
                text_channel,
            } => back(&ctx, guild, voice_channel, text_channel).await,
            PreparedCommand::Prefix {
                guild,
                new_prefix,
                text_channel,
            } => prefix(&ctx, guild, text_channel, new_prefix).await,
            PreparedCommand::Alias {
                guild,
                old_command,
                new_command,
                text_channel,
            } => alias(&ctx, guild, text_channel, &old_command, &new_command).await,
        }
    }
}

async fn play(
    ctx: &Context,
    guild: GuildId,
    query: VideoQuery,
    voice_channel: ChannelId,
    text_channel: ChannelId,
) {
    handle_voice_channel_joining(ctx, guild, voice_channel, text_channel).await;
    let mut message_sent = send_message(ctx, text_channel, " Searching...").await;
    let yt_dlp = get_ytdlp_from_global_context(ctx).await;
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;

    if query.is_playlist() {
        if let Some(message) = message_sent.as_mut() {
            let builder =
                EditMessage::new().content("Playlists must be added with the add command");
            let _ = message.edit(&ctx.http, builder).await;
        }
    } else {
        let video = match yt_dlp.search_for_video(&query).await {
            Ok(video) => video,
            Err(err) => {
                if let Some(mut message) = message_sent {
                    let builder = EditMessage::new().content(err.to_string());
                    let _ = message.edit(&ctx.http, builder).await;
                };

                return;
            }
        };

        let stream = match yt_dlp.get_audio_streams(&video).await {
            Ok(v) => v,
            Err(err) => {
                if let Some(mut message) = message_sent {
                    let builder = EditMessage::new().content("Error playing track");
                    let _ = message.edit(&ctx.http, builder).await;
                };
                event!(Level::ERROR, "{err}");
                return;
            }
        };

        let http = ctx
            .data
            .read()
            .await
            .get::<HTTPClientKey>()
            .expect("")
            .clone();

        let audio = stream.clone().to_audio_stream(http);

        let Some(audio_arc) = audio.map(Arc::new) else {
            if let Some(mut message) = message_sent {
                let builder = EditMessage::new().content("Error playing track");
                let _ = message.edit(&ctx.http, builder).await;
            };

            return;
        };

        if let Some(mut message) = message_sent {
            let builder = EditMessage::new().content(format!(
                "Playing {track_name} [{duration}]",
                track_name = stream.title,
                duration = stream.duration_string,
            ));
            let _ = message.edit(&ctx.http, builder).await;
        };

        guild_lock
            .write()
            .await
            .play_now(ctx.clone(), guild, voice_channel, audio_arc)
            .await;
    }
}

async fn mute(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(
        ctx,
        guild,
        voice_channel,
        text_channel,
        async move |call| match call.lock().await.mute(false).await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Muted!").await;
            }
            Err(err) => {
                event!(Level::WARN, "Error muting bot: {err}");
            }
        },
    )
    .await;
}

async fn unmute(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(
        ctx,
        guild,
        voice_channel,
        text_channel,
        async move |call| match call.lock().await.mute(true).await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Unmuted!").await;
            }
            Err(err) => {
                event!(Level::WARN, "Error unmuting bot: {err}");
            }
        },
    )
    .await;
}

async fn redo(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        if guild_lock.write().await.redo().await {
            let _ = send_message(ctx, text_channel, "Undid last undo").await;
        } else {
            let _ = send_message(ctx, text_channel, "Nothing to redo").await;
        }
    })
    .await;
}

async fn undo(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        if guild_lock.write().await.undo().await {
            let _ = send_message(ctx, text_channel, "Undid last queue change").await;
        } else {
            let _ = send_message(ctx, text_channel, "Nothing to undo").await;
        }
    })
    .await;
}

async fn now_playing(ctx: &Context, guild: GuildId, text_channel: ChannelId) {
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let guild_context = guild_lock.read().await;
    if let Some(track) = guild_context.get_current_track_info() {
        let handle = track.handle.clone();
        //TODO Print the current track position
        let _pos = handle
            .get_info()
            .await
            .map(|info| info.position.as_secs())
            .ok();

        let _ = send_message(
            ctx,
            text_channel,
            &format!(
                "Currently playing: {track_name} [{duration}]",
                track_name = track.stream.name,
                duration = track.stream.duration_string
            ),
        )
        .await;
    } else {
        let _ = send_message(ctx, text_channel, "Not playing anything right now").await;
    }
}

async fn resume(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        match guild_context.resume_current_track().await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Resumed track playback").await;
            }
            Err(err) => match err {
                TrackControlError::NoTrack => {
                    let _ = send_message(ctx, text_channel, "No track to resume").await;
                }
                TrackControlError::Error(control_error) => {
                    let _ = send_message(
                        ctx,
                        text_channel,
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
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        match guild_context.pause_current_track().await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Resumed track playback").await;
            }
            Err(err) => match err {
                TrackControlError::NoTrack => {
                    let _ = send_message(ctx, text_channel, "No track to resume").await;
                }
                TrackControlError::Error(control_error) => {
                    let _ = send_message(
                        ctx,
                        text_channel,
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
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        match arg {
            RemoveArgument::From(user) => {
                match guild_context.remove_tracks_from(guild, &user, ctx).await {
                    Ok(tracks_removed) => {
                        let _ = send_message(
                            ctx,
                            text_channel,
                            &format!("Removed {tracks_removed} tracks"),
                        )
                        .await;
                    }
                    Err(err) => match err {
                        RemoveTracksFromError::ErrorFetchingMembers => {
                            event!(Level::WARN, "{}", err);
                        }
                        RemoveTracksFromError::NoUsersFound => {
                            let _ = send_message(
                                ctx,
                                text_channel,
                                "Couldn't find a matching user to remove tracks from",
                            )
                            .await;
                        }
                        RemoveTracksFromError::MultipleUsersFound => {
                            let _ =
                                send_message(ctx, text_channel, "Multiple matching users found")
                                    .await;
                        }
                    },
                }
            }
            RemoveArgument::At(pos) => {
                let range = (pos as usize - 1)..pos as usize;
                let tracks_removed = guild_context.remove_tracks_in_range(range);
                let _ = send_message(
                    ctx,
                    text_channel,
                    &format!("Removed {tracks_removed} tracks"),
                )
                .await;
            }
            RemoveArgument::Until(pos) => {
                let max = guild_context.get_total_queue_length();
                let range = 0..(pos as usize - 1).min(max);
                let tracks_removed = guild_context.remove_tracks_in_range(range);
                let _ = send_message(
                    ctx,
                    text_channel,
                    &format!("Removed {tracks_removed} tracks"),
                )
                .await;
            }
            RemoveArgument::Past(pos) => {
                let max = guild_context.get_total_queue_length();
                let range = (pos as usize)..max;
                let tracks_removed = guild_context.remove_tracks_in_range(range);
                let _ = send_message(
                    ctx,
                    text_channel,
                    &format!("Removed {tracks_removed} tracks"),
                )
                .await;
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
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        guild_context.set_loop_mode(mode).await;
        let _ = send_message(ctx, text_channel, &format!("Set loop mode to {}", mode)).await;
    })
    .await;
}

async fn clear(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        guild_context.clear_queue().await;
        let _ = send_message(ctx, text_channel, "Cleared the queue").await;
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
    let mut message_sent = send_message(ctx, text_channel, " Searching...").await;
    let yt_dlp = get_ytdlp_from_global_context(ctx).await;
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;

    if query.is_playlist() {
        if let Some(message) = message_sent.as_mut() {
            let builder = EditMessage::new().content("Searching... (Playlists can take a while)");
            let _ = message.edit(&ctx.http, builder).await;
            let VideoQuery::Url(url) = query else {
                //is_playlist ensures we have the url enum
                unreachable!();
            };
            let streams = match yt_dlp.search_for_playlist(&url).await {
                Ok(streams) => streams,
                Err(err) => {
                    if let Some(mut message) = message_sent {
                        let builder = EditMessage::new().content(err.to_string());
                        let _ = message.edit(&ctx.http, builder).await;
                    };

                    return;
                }
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
                .add_many_to_queue(user, &streams, ctx, guild)
                .await;
        };
    } else {
        let video = match yt_dlp.search_for_video(&query).await {
            Ok(video) => video,
            Err(err) => {
                if let Some(mut message) = message_sent {
                    let builder = EditMessage::new().content(err.to_string());
                    let _ = message.edit(&ctx.http, builder).await;
                };

                return;
            }
        };
        let video_info_arc = Arc::new(video);

        let queue_length = guild_lock.read().await.playback_queue.len();

        if let Some(mut message) = message_sent {
            let builder = EditMessage::new().content(format!(
                "Adding {track_name} [{duration}] to the queue at position {pos}",
                pos = queue_length + 1,
                track_name = video_info_arc.title(),
                duration = video_info_arc.duration(),
            ));
            let _ = message.edit(&ctx.http, builder).await;
        };

        guild_lock
            .write()
            .await
            .add_to_queue(user, video_info_arc, ctx, guild)
            .await;
    }
}

async fn shuffle(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        if guild_context.get_total_queue_length() == 0 {
            let _ = send_message(ctx, text_channel, "Queue is empty").await;
        } else if guild_context.get_total_queue_length() == 1 {
            guild_context.shuffle_queue().await;
            let _ = send_message(ctx, text_channel, "shuffled.... one song").await;
        } else {
            guild_context.shuffle_queue().await;
            let _ = send_message(ctx, text_channel, "Shuffled the queue").await;
        }
    })
    .await
}

async fn list_queue_contents(ctx: &Context, guild: GuildId, text_channel: ChannelId) {
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let guild_context = guild_lock.read().await;

    if guild_context.playback_queue.is_empty() {
        send_message(ctx, text_channel, "Queue is currently empty!").await;
        return;
    }

    let mut message = String::new();
    let _ = writeln!(&mut message, "Current Queue:");
    guild_context
        .playback_queue
        .iter()
        .enumerate()
        .for_each(|(i, entry)| {
            let _ = write!(
                &mut message,
                "Position #{pos}: {name} [{duration}]",
                pos = i + 1,
                name = entry.info.title(),
                duration = entry.info.duration(),
            );
            if i == guild_context.queue_position() {
                let _ = write!(&mut message, " <- Next up");
            }
            let _ = writeln!(&mut message);
        });
    send_message(ctx, text_channel, &message).await;
}

async fn next(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        match guild_lock.write().await.next_track(ctx, guild).await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Went forward in the queue").await;
            }
            Err(err) => {
                event!(Level::WARN, "{err}");
            }
        }
    })
    .await
}
async fn back(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    ensure_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
        match guild_lock.write().await.last_track(ctx, guild).await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Went back in the queue").await;
            }
            Err(err) => {
                event!(Level::WARN, "{err}");
            }
        }
    })
    .await
}

async fn leave(ctx: &Context, guild: GuildId) {
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let songbird_manager = get_songbird(ctx).await;

    if let Some(call) = songbird_manager.get(guild) {
        let _ = call.lock().await.leave().await;
    }
    let mut write_lock = guild_lock.write().await;
    let _ = write_lock.pause_current_track().await;
}

async fn help(ctx: &Context, user: UserId, guild: GuildId) {
    let mut help_message = String::new();
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let guard = guild_lock.read().await;
    let prefix = guard.start_pattern;
    let command_mappings = &guard.command_aliases;
    help_message.push_str("COMMANDS:\n");

    Command::iter().for_each(|c| {
        let Some(alias) = command_mappings.get_alias_for_command(c) else {
            event!(
                Level::ERROR,
                "Could not get mapping for command. mappings: {command_mappings:?}"
            );
            return;
        };
        help_message.push(prefix);
        help_message.push_str(alias);
        help_message.push(' ');
        help_message.push_str(c.syntax());
        help_message.push_str(": ");
        help_message.push_str(c.description());
        help_message.push('\n');
    });
    let _ = user
        .direct_message(&ctx.http, CreateMessage::new().content(help_message))
        .await;
}

async fn beep(ctx: &Context, channel: ChannelId) {
    let _ = send_message(ctx, channel, "Boop!").await;
}
async fn ffmpeg(
    ctx: &Context,
    guild: GuildId,
    url: String,
    text_channel: ChannelId,
    voice_channel: ChannelId,
) {
    handle_voice_channel_joining(ctx, guild, voice_channel, text_channel).await;
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let http = ctx
        .data
        .read()
        .await
        .get::<HTTPClientKey>()
        .expect("")
        .clone();

    let request = HttpRequest::new(http, url);

    guild_lock
        .write()
        .await
        .play_stream(ctx.clone(), guild, voice_channel, request)
        .await;
}

async fn alias(
    ctx: &Context,
    guild: GuildId,
    text_channel: ChannelId,
    old_alias: &str,
    new_alias: &str,
) {
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let mut write_guard = guild_lock.write().await;
    if let Some(old_command) = write_guard.command_aliases.get_command_for_alias(old_alias) {
        match write_guard
            .command_aliases
            .set_command_alias(new_alias, old_command)
        {
            Ok(_) => {
                let _ = send_message(
                    ctx,
                    text_channel,
                    &format!("Changed the command for `{old_alias}` to `{new_alias}`"),
                )
                .await;
            }
            Err(err) => {
                let _ = send_message(
                    ctx,
                    text_channel,
                    &format!("Error changing the command: {err}"),
                )
                .await;
            }
        }
    } else {
        let _ = send_message(ctx, text_channel, "I couldn't find that command").await;
    }
}

async fn prefix(ctx: &Context, guild: GuildId, text_channel: ChannelId, new_prefix: char) {
    let guild_lock = get_or_insert_guild_context_lock(ctx, guild).await;
    let mut write_guard = guild_lock.write().await;
    if write_guard.start_pattern == new_prefix {
        let _ = send_message(ctx, text_channel, "That's already the current prefix").await;
    } else {
        write_guard.start_pattern = new_prefix;
        let _ = send_message(
            ctx,
            text_channel,
            &format!("Changed the command prefix to `{new_prefix}`"),
        )
        .await;
    }
}

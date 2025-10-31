use std::{fmt::Write, sync::Arc};
pub mod guild_context;
pub mod tracks;
pub mod undo_stack;

use reqwest::Client;
use serenity::{
    all::{CacheHttp, ChannelId, Context, EditMessage, GuildId, Message},
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as SongBirdEventHandler, Songbird};

use tokio::sync::{RwLock, RwLockReadGuard};
use tracing::{Level, event};

use crate::{
    bot::guild_context::{GuildContext, LoopMode, RemoveMode, StreamData},
    commands::Executable,
    yt_dlp::video::Video,
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

pub struct MusicBot {
    client: Client,
    guild_datas: tokio::sync::RwLock<Vec<(GuildId, Arc<tokio::sync::RwLock<GuildContext>>)>>,
    yt_dlp: Executable,
}

#[async_trait]
impl serenity::all::EventHandler for MusicBot {
    async fn message(&self, ctx: Context, msg: Message) {
        let Some(guild_id) = msg.guild_id else {
            return;
        };

        let guild_rw_lock = self.get_or_insert_guild_context(guild_id).await;
        let songbird_manager = songbird::get(&ctx)
            .await
            .expect("Songbird manager should be inserted at startup");

        let read_guard = guild_rw_lock.read().await;

        if msg.content.starts_with(read_guard.start_pattern.as_str()) {
            let command_string = msg
                .content
                .strip_prefix(read_guard.start_pattern.as_str())
                .expect("Message should always start with prefix at this point");

            //Drop the read_guard here
            drop(read_guard);

            let Some(command) = command_string.split_whitespace().next() else {
                return;
            };
            let request_text_channel = msg.channel_id;

            let Some(guild) = msg.guild_id else {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Could not get guild for message");

                return;
            };

            let Some(request_voice_channel) = guild
                .to_guild_cached(&ctx.cache)
                .and_then(|cache_ref| {
                    cache_ref
                        .voice_states
                        .get(&msg.author.id)
                        .map(|state| state.channel_id)
                })
                .flatten()
            else {
                send_message(
                    request_text_channel,
                    ctx.http.clone(),
                    "You must be in a voice channel to send bot commands",
                )
                .await;
                return;
            };

            match command {
                "playnow" => {
                    GuildContext::handle_voice_channel_joining(
                        guild_id,
                        request_text_channel,
                        request_voice_channel,
                        guild_rw_lock.clone(),
                        &ctx,
                    )
                    .await;

                    let mut guild_context = guild_rw_lock.write().await;

                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "playnow command issued");

                    let rest = command_string.trim().strip_prefix("playnow ").unwrap();
                    let query = VideoQuery::from_str(rest);

                    let message =
                        send_message(request_text_channel, &ctx.http, " Searching...").await;
                    let Some(audio) =
                        Self::get_stream_data(query, self.yt_dlp.clone(), self.client.clone())
                            .await
                    else {
                        if let Some(mut message) = message {
                            let builder = EditMessage::new()
                                .content("I couldn't find anything :(".to_string());
                            let _ = message.edit(&ctx.http, builder).await;
                        };

                        return;
                    };

                    guild_context
                        .play_now(
                            &ctx,
                            guild_id,
                            request_voice_channel,
                            request_text_channel,
                            audio.clone(),
                        )
                        .await;
                    if let Some(mut message) = message {
                        let builder = EditMessage::new().content(format!(
                            "Started Playing: {track_name}",
                            track_name = audio.name
                        ));
                        let _ = message.edit(&ctx.http, builder).await;
                    };
                }
                //Suspends the currently playing track and leaves the call
                "leave" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "leave command issued");

                    if let Some(call) = songbird_manager.get(guild_id) {
                        let _ = call.lock().await.leave().await;
                    }
                }
                "join" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "join command issued");

                    GuildContext::handle_voice_channel_joining(
                        guild_id,
                        request_text_channel,
                        request_voice_channel,
                        guild_rw_lock.clone(),
                        &ctx,
                    )
                    .await;
                    let _ = guild_rw_lock.write().await.resume().await;
                }
                //Ends the currently playing track
                "skip" => {
                    if command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        #[cfg(feature = "tracing")]
                        event!(Level::INFO, "skip command issued");

                        let mut guild_context = guild_rw_lock.write().await;
                        guild_context
                            .skip_track(request_text_channel, &ctx, guild_id)
                            .await;
                    }
                }
                //Lists the contents of the queue
                "list" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "list command issued");

                    let guild_context = guild_rw_lock.read().await;
                    if guild_context.queue_is_empty() {
                        send_message(request_text_channel, ctx.http, "Queue is currently empty!")
                            .await;
                        return;
                    }
                    let mut message = String::new();
                    let _ = writeln!(&mut message, "Current Queue:");
                    guild_context
                        .iter_queue()
                        .enumerate()
                        .for_each(|(i, entry)| {
                            let _ = writeln!(
                                &mut message,
                                "Position #{pos}: {name} (added by {user})",
                                pos = i + 1,
                                name = entry.stream.name,
                                user = entry.user
                            );
                        });
                    send_message(request_text_channel, ctx.http, &message).await;
                }
                //Shuffles the tracks in the queue and resets the queue index to 0
                "shuffle" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    if guild_context.queue_is_empty() {
                        send_message(request_text_channel, ctx.http, "Queue is currently empty!")
                            .await;
                        return;
                    }
                    let mut message = String::new();
                    let _ = writeln!(&mut message, "Shuffled Queue:");
                    guild_context
                        .iter_queue()
                        .enumerate()
                        .for_each(|(i, entry)| {
                            let _ = writeln!(
                                &mut message,
                                "Position #{pos}: {name} (added by {user})",
                                pos = i + 1,
                                name = entry.stream.name,
                                user = entry.user
                            );
                        });
                    send_message(request_text_channel, ctx.http, &message).await;

                    guild_context.shuffle_queue().await;
                }

                "add" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    let rest = command_string.trim().strip_prefix("add ").unwrap();
                    let query = VideoQuery::from_str(rest);
                    let message =
                        send_message(request_text_channel, &ctx.http, " Searching...").await;

                    let Some(audio) =
                        Self::get_stream_data(query, self.yt_dlp.clone(), self.client.clone())
                            .await
                    else {
                        if let Some(mut message) = message {
                            let builder = EditMessage::new()
                                .content("I couldn't find anything :(".to_string());
                            let _ = message.edit(&ctx.http, builder).await;
                        };

                        return;
                    };
                    let queue_length = guild_context.iter_queue().count();
                    send_message(
                        request_text_channel,
                        &ctx.http,
                        &format!(
                            "Adding {track_name} to the queue at position: {pos}",
                            pos = queue_length + 1,
                            track_name = &audio.name
                        ),
                    )
                    .await;
                    guild_context.add_to_queue(msg.author.id, audio).await;
                    if guild_context.get_current_track_info().is_none() {
                        guild_context.handle_next_track(&ctx, guild_id).await;
                    }
                }

                "clear" => {
                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.clear_queue().await;
                    let _ =
                        send_message(request_text_channel, &ctx.http, "Cleared the queue").await;
                }

                "loop" => {
                    let mut guild_context = guild_rw_lock.write().await;
                    let Some(mode) = command_string.split_whitespace().nth(1) else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "A loop mode must be specified: Off, Single, Queue",
                        )
                        .await;
                        return;
                    };
                    let new_loop_mode = match mode {
                        "off" | "Off" => LoopMode::Off,
                        "single" | "Single" => LoopMode::Track,
                        "queue" | "Queue" => LoopMode::Queue,
                        _ => {
                            send_message(
                                request_text_channel,
                                ctx.http.clone(),
                                "Valid loop modes: Off, Single, Queue",
                            )
                            .await;
                            return;
                        }
                    };
                    guild_context.set_loop_mode(new_loop_mode).await;
                }

                "remove" => {
                    let mut guild_context = guild_rw_lock.write().await;
                    let Some(mode) = command_string.split_whitespace().nth(1) else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "A remove mode must be specified: From, At, Until, Past",
                        )
                        .await;
                        return;
                    };
                    let remove_mode = match mode {
                        "from" | "From" => RemoveMode::From,
                        "at" | "At" => RemoveMode::At,
                        "until" | "Until" => RemoveMode::Until,
                        "past" | "Past" => RemoveMode::Past,
                        _ => {
                            send_message(request_text_channel, ctx.http.clone(), &format!("{mode} is not a valid remove mode. Valid options: From, At, Until, Past",)).await;
                            return;
                        }
                    };
                    let Some(command_removed) = command_string.trim().strip_prefix("remove") else {
                        #[cfg(feature = "tracing")]
                        event!(Level::ERROR, "Error parsing: {command_string}");
                        return;
                    };
                    let Some(rest) = command_removed.trim().strip_prefix(mode) else {
                        #[cfg(feature = "tracing")]
                        event!(Level::ERROR, "Error parsing: {command_string}");
                        return;
                    };
                    let removed_track_count;
                    match remove_mode {
                        RemoveMode::From => {
                            removed_track_count = guild_context
                                .remove_tracks_from(guild_id, rest.trim(), &ctx)
                                .await;
                        }
                        RemoveMode::Past => {
                            let Some(count) = rest.trim().split_ascii_whitespace().next() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The remove past command needs a queue position to remove tracks after",
                                )
                                .await;
                                return;
                            };
                            let Ok(count) = count.parse::<usize>() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The queue position for the remove past command should be a number",
                                )
                                .await;
                                return;
                            };
                            //Both remove past and remove from should prooobably be exclusive
                            removed_track_count =
                                guild_context.remove_tracks_in_range((count + 1)..);
                        }
                        RemoveMode::At => {
                            let Some(count) = rest.trim().split_ascii_whitespace().next() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The remove at command needs a queue position to remove",
                                )
                                .await;
                                return;
                            };
                            let Ok(count) = count.parse::<usize>() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The queue position for the remove at command should be a number",
                                )
                                .await;
                                return;
                            };
                            removed_track_count =
                                guild_context.remove_tracks_in_range(count..(count + 1));
                        }
                        RemoveMode::Until => {
                            let Some(count) = rest.trim().split_ascii_whitespace().next() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The remove until command needs a queue position to remove tracks up to",
                                )
                                .await;
                                return;
                            };
                            let Ok(count) = count.parse::<usize>() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The queue position for the remove until command should be a number",
                                )
                                .await;
                                return;
                            };
                            removed_track_count = guild_context.remove_tracks_in_range(..count);
                        }
                    }
                    send_message(
                        request_text_channel,
                        &ctx.http,
                        &format!("Removed {removed_track_count} tracks"),
                    )
                    .await;
                }

                "pause" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "pause command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.pause().await;
                }

                "resume" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "resume command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.resume().await;
                }

                "nowplaying" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "nowplaying command issued");

                    let guild_context = guild_rw_lock.read().await;
                    if let Some(track) = guild_context.get_current_track_info() {
                        let handle = track.handle.clone();
                        //TODO Print the current track position
                        let _pos = handle
                            .get_info()
                            .await
                            .map(|info| info.position.as_secs())
                            .ok();

                        let _ = send_message(
                            request_text_channel,
                            &ctx.http,
                            &format!(
                                "Currently playing: {track_name}",
                                track_name = track.stream.name
                            ),
                        )
                        .await;
                    }
                }

                "undo" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "undo command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    if guild_context.undo(guild_id, &ctx).await {
                        let _ = send_message(request_text_channel, &ctx.http, "Undid last action")
                            .await;
                    } else {
                        let _ =
                            send_message(request_text_channel, &ctx.http, "Nothing to undo").await;
                    }
                }
                "redo" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "redo command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    if guild_context.redo(guild_id, &ctx).await {
                        let _ =
                            send_message(request_text_channel, &ctx.http, "Redid last undo").await;
                    } else {
                        let _ =
                            send_message(request_text_channel, &ctx.http, "Nothing to redo").await;
                    }
                }

                "stats" => {
                    //TODO
                    let _guild_context = guild_rw_lock.read().await;
                }

                "move" => {
                    //TODO
                    let mut _guild_context = guild_rw_lock.write().await;
                }
                "mute" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.mute(&ctx, guild_id).await;
                }
                "unmute" => {
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.unmute(&ctx, guild_id).await;
                }
                "beep" | "beep!" => {
                    send_message(request_text_channel, ctx.http, "boop!").await;
                }

                _ => {}
            }
        }
    }
}

pub async fn command_is_in_same_call(
    songbird_manager: Arc<Songbird>,
    guild_id: GuildId,
    voice_call: ChannelId,
) -> bool {
    if let Some(call) = songbird_manager.get(guild_id)
        && call
            .lock()
            .await
            .current_channel()
            .is_some_and(|id| id == voice_call.into())
    {
        true
    } else {
        false
    }
}

#[derive(Debug, Clone, Copy)]
enum VideoQuery<'a> {
    Url(&'a str),
    SearchTerm(&'a str),
}

impl<'a> VideoQuery<'a> {
    pub fn from_str(str: &'a str) -> Self {
        if str.trim().starts_with("https://") {
            VideoQuery::Url(str)
        } else {
            VideoQuery::SearchTerm(str)
        }
    }
}

impl MusicBot {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            guild_datas: RwLock::new(Vec::new()),
            yt_dlp: Executable::new("./binaries/yt-dlp_linux"),
        }
    }
    async fn get_stream_data(
        query: VideoQuery<'_>,
        yt_dlp: Executable,
        client: Client,
    ) -> Option<StreamData> {
        let query_arg = match query {
            VideoQuery::Url(url) => url,
            VideoQuery::SearchTerm(term) => &format!("ytsearch:{term}"),
        };
        let args = [
            "-j",
            query_arg,
            "-f",
            "ba[abr>0][vcodec=none]/best",
            "--no-playlist",
        ];

        let child_process = match yt_dlp.execute_with_temp_args(args) {
            Ok(child) => child,
            Err(err) => {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Error fetching track {err}");
                return None;
            }
        };

        match child_process.wait_with_output().await {
            Ok(output) => {
                if !output.status.success() {
                    let err =
                        std::str::from_utf8(&output.stderr[..]).unwrap_or("<no error message>");

                    #[cfg(feature = "tracing")]
                    event!(Level::ERROR, "Error running yt_dlp {err}");

                    return None;
                }
                let out = match serde_json::from_str::<Video>(
                    str::from_utf8(&output.stdout).unwrap().trim(),
                ) {
                    Ok(out) => out,
                    Err(err) => {
                        eprintln!("Error!:{err}");
                        return None;
                    }
                };
                out.to_audio_stream(client.clone())
            }
            Err(err) => {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Error running yt_dlp {err}");
                None
            }
        }
    }

    async fn get_or_insert_guild_context(&self, id: GuildId) -> Arc<RwLock<GuildContext>> {
        let read_lock = self.guild_datas.read().await;
        if let Ok(guild_context) = RwLockReadGuard::try_map(read_lock, |vec| {
            vec.iter().find_map(|(stored_id, context)| {
                if *stored_id == id {
                    Some(context)
                } else {
                    None
                }
            })
        }) {
            guild_context.clone()
        } else {
            let mut write_lock = self.guild_datas.write().await;
            write_lock.push((id, Arc::new(RwLock::new(GuildContext::new()))));
            write_lock
                .last()
                .as_ref()
                .expect("We just pushed to the vec")
                .1
                .clone()
        }
    }
}
pub async fn send_message(
    channel: ChannelId,
    http: impl CacheHttp,
    message: &str,
) -> Option<Message> {
    match channel.say(http, message).await {
        Ok(message) => Some(message),
        Err(err) => {
            #[cfg(feature = "tracing")]
            event!(Level::ERROR, "Error sending message: {err}");
            None
        }
    }
}

impl Default for MusicBot {
    fn default() -> Self {
        Self::new()
    }
}

struct TrackErrorNotifier {
    guild: Arc<RwLock<GuildContext>>,
    guild_id: GuildId,
    context: Context,
}

struct TrackEndNotifier {
    guild: Arc<RwLock<GuildContext>>,
    guild_id: GuildId,
    context: Context,
}

#[async_trait]
impl SongBirdEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let mut lock = self.guild.write().await;
        if let EventContext::Track(track_list) = ctx {
            for (_state, handle) in *track_list {
                if let Some(current_track) = &lock.get_current_track_info()
                    && current_track.handle.uuid() == handle.uuid()
                {
                    lock.undo_stack.clear();
                    lock.handle_next_track(&self.context, self.guild_id).await;
                }
                println!("Track {:?}  ended", handle.uuid());
            }
        }

        None
    }
}

#[async_trait]
impl SongBirdEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let mut lock = self.guild.write().await;
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                if let Some(current_track) = &lock.get_current_track_info()
                    && current_track.handle.uuid() == handle.uuid()
                {
                    lock.undo_stack.clear();
                    lock.handle_next_track(&self.context, self.guild_id).await;
                }
                println!(
                    "Track {:?} encountered an error: {:?}",
                    handle.uuid(),
                    state.playing
                );
            }
        }

        None
    }
}

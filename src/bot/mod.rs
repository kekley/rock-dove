use std::{fmt::Write, num::NonZeroUsize, sync::Arc};
pub mod guild_context;
pub mod queue;
pub mod tracks;
pub mod undo_stack;

use reqwest::Client;
use serenity::{
    all::{CacheHttp, ChannelId, Context, CreateMessage, EditMessage, GuildId, Message},
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

            let request_voice_channel = guild
                .to_guild_cached(&ctx.cache)
                .and_then(|cache_ref| {
                    cache_ref
                        .voice_states
                        .get(&msg.author.id)
                        .map(|state| state.channel_id)
                })
                .flatten();

            match command {
                "help" => {
                    const COMMAND_SYNTAX: [&str; 16] = [
                        "help",
                        "play { url | search text }",
                        "add { url | search text }",
                        "join",
                        "leave",
                        "list",
                        "clear",
                        "loop { off | single | queue }",
                        "remove { ( at | past | until ) | from } { track position | username }",
                        "pause",
                        "resume",
                        "nowplaying",
                        "skip",
                        "undo",
                        "redo",
                        "beep",
                    ];
                    const COMMAND_EXPLANATION: [&str; 16] = [
                        "Show this list.",
                        "Bypass the queue and play a song from a url or youtube search.",
                        "Add a song to the queue from a url or youtube search.",
                        "Join the voice channel you're in.",
                        "Remove the bot from any voice channels.",
                        "List the current contents of the queue.",
                        "Clear the queue.",
                        "Set the loop mode.\noff = No looping\nsingle = Loop the current song indefinitely\nqueue = Loop the queue when it ends",
                        "Remove one or more tracks from the queue.\nat = Remove the track at (track position)\npast = Remove all tracks after (track position)\nuntil = Remove all tracks up to (track position)\nfrom = Remove all tracks added by (username)",
                        "Pause the current track.",
                        "Resume the current track.",
                        "See the name of the current track.",
                        "End the current track.",
                        "Undo the last change made to the queue.",
                        "Undo the last undo..?",
                        "Say hi",
                    ];
                    let mut message = String::new();
                    message.push_str("COMMANDS:\n");
                    COMMAND_SYNTAX.iter().zip(COMMAND_EXPLANATION).for_each(
                        |(syntax, explanation)| {
                            message.push_str(syntax);
                            message.push_str(" : ");
                            message.push_str(explanation);
                            message.push('\n');
                            message.push('\n');
                        },
                    );
                    //Remove the two trailing newlines
                    message.pop();
                    message.pop();

                    let _ = msg
                        .author
                        .id
                        .direct_message(&ctx.http, CreateMessage::new().content(message))
                        .await;
                }
                "play" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };

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

                    let rest = command_string.trim().strip_prefix("play ").unwrap();
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
                    let audio_arc = Arc::new(audio);

                    guild_context
                        .play_now(&ctx, guild_id, request_voice_channel, audio_arc.clone())
                        .await;
                    if let Some(mut message) = message {
                        let builder = EditMessage::new().content(format!(
                            "Started Playing: {track_name}",
                            track_name = audio_arc.name
                        ));
                        let _ = message.edit(&ctx.http, builder).await;
                    };
                }
                //Suspends the currently playing track and leaves the call
                "leave" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "leave command issued");

                    let _ = guild_rw_lock.write().await.pause().await;
                    if let Some(call) = songbird_manager.get(guild_id) {
                        let _ = call.lock().await.leave().await;
                    }
                }
                //
                "join" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
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
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        #[cfg(feature = "tracing")]
                        event!(Level::INFO, "skip command issued");

                        let mut guild_context = guild_rw_lock.write().await;
                        guild_context
                            .next_track(request_text_channel, &ctx, guild_id)
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
                    println!("{:?}", guild_context.playback_queue);
                    let _ = writeln!(&mut message, "Current Queue:");
                    guild_context
                        .iter_queue()
                        .enumerate()
                        .for_each(|(i, entry)| {
                            let _ = write!(
                                &mut message,
                                "Position #{pos}: {name}",
                                pos = i + 1,
                                name = entry.audio.name,
                            );
                            if i + 1 == guild_context.queue_position() {
                                let _ = write!(&mut message, " <- Current position");
                            }
                            let _ = writeln!(&mut message);
                        });
                    send_message(request_text_channel, ctx.http, &message).await;
                }
                //Shuffles the tracks in the queue and resets the queue index to 0
                "shuffle" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "shuffle command issued");

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
                                "Position {pos}: {name}",
                                pos = i + 1,
                                name = entry.audio.name,
                            );
                        });
                    send_message(request_text_channel, ctx.http, &message).await;

                    guild_context.shuffle_queue().await;
                }

                "add" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if songbird_manager.get(guild_id).is_none() {
                        GuildContext::handle_voice_channel_joining(
                            guild_id,
                            request_text_channel,
                            request_voice_channel,
                            guild_rw_lock.clone(),
                            &ctx,
                        )
                        .await;
                    }
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "add command issued");

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
                    let audio_arc = Arc::new(audio);
                    let queue_length = guild_context.iter_queue().count();

                    if let Some(mut message) = message {
                        let builder = EditMessage::new().content(format!(
                            "Adding {track_name} to the queue at position {pos}",
                            pos = queue_length + 1,
                            track_name = &audio_arc.name
                        ));
                        let _ = message.edit(&ctx.http, builder).await;
                    };

                    guild_context
                        .add_to_queue(msg.author.id, audio_arc, &ctx, guild_id)
                        .await;
                }

                "clear" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }

                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "clear command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.clear_queue().await;
                    let _ =
                        send_message(request_text_channel, &ctx.http, "Cleared the queue").await;
                }

                "loop" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }

                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "loop command issued");

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
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "remove command issued");

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
                        "from" | "From" => RemoveMode::FromUser,
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
                        RemoveMode::FromUser => {
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
                            let Ok(start) = count.parse::<NonZeroUsize>() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The queue position for the remove past command should be a number greater than 0",
                                )
                                .await;
                                return;
                            };
                            let end = guild_context.queue_len();
                            //Both remove past and remove from should prooobably be exclusive
                            println!(
                                "past. start= {start}, end = {end}",
                                start = start.get(),
                                end = end
                            );

                            removed_track_count =
                                guild_context.remove_tracks_in_range(start.get()..end);
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
                            let Ok(count) = count.parse::<NonZeroUsize>() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The queue position for the remove past command should be a number greater than 0",
                                )
                                .await;
                                return;
                            };

                            println!(
                                "at. start= {start}, end = {end}",
                                start = (count.get() - 1),
                                end = count.get()
                            );
                            removed_track_count = guild_context
                                .remove_tracks_in_range((count.get() - 1)..count.get());
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
                            let Ok(count) = count.parse::<NonZeroUsize>() else {
                                send_message(
                                    request_text_channel,
                                    &ctx.http,
                                    "The queue position for the remove past command should be a number greater than 0",
                                )
                                .await;
                                return;
                            };
                            let end = guild_context.queue_len();
                            println!("until. start= 0, end = {end}", end = count.get().min(end));
                            removed_track_count = guild_context
                                .remove_tracks_in_range(0..((count.get() - 1).min(end)));
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
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
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
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
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
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "undo command issued");

                    let mut guild_context = guild_rw_lock.write().await;

                    if guild_context.undo().await {
                        let _ = send_message(request_text_channel, &ctx.http, "Undid last action")
                            .await;
                    } else {
                        let _ =
                            send_message(request_text_channel, &ctx.http, "Nothing to undo").await;
                    }
                }
                "redo" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "redo command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    if guild_context.redo().await {
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
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    guild_context.mute(&ctx, guild_id).await;
                }
                "unmute" => {
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
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
        println!("f");
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
    guild_context: Arc<RwLock<GuildContext>>,
    guild_id: GuildId,
    context: Context,
}

#[async_trait]
impl SongBirdEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let mut lock = self.guild_context.write().await;
        if let EventContext::Track(track_list) = ctx {
            for (_state, handle) in *track_list {
                if let Some(current_track) = &lock.get_current_track_info()
                    && current_track.handle.uuid() == handle.uuid()
                {
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

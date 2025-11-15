pub mod command;
pub mod guild_context;
pub mod queue;
pub mod track_notifier;
pub mod tracks;
pub mod undo_stack;
pub mod work_queue;

use serenity::{
    all::{CacheHttp, ChannelId, Context, CreateMessage, EditMessage, GuildId, Message, UserId},
    async_trait,
};
use songbird::Songbird;
use std::{fmt::Write, num::NonZeroUsize, sync::Arc};

use tokio::sync::{RwLock, RwLockReadGuard};
use tracing::{Level, event};

use crate::{
    HTTPClientKey,
    bot::{
        guild_context::GuildContext,
        queue::{LoopMode, RemoveMode},
    },
    yt_dlp::{VideoQuery, YtDlp, YtDlpKey},
};

pub struct MusicBot {
    guild_datas: tokio::sync::RwLock<Vec<(GuildId, Arc<tokio::sync::RwLock<GuildContext>>)>>,
}

#[async_trait]
impl serenity::all::EventHandler for MusicBot {
    async fn message(&self, ctx: Context, user_message: Message) {
        let Some(guild_id) = user_message.guild_id else {
            return;
        };

        let guild_rw_lock = self.get_or_insert_guild_context(guild_id).await;
        let songbird_manager = songbird::get(&ctx)
            .await
            .expect("Songbird manager should be inserted at startup");

        let read_guard = guild_rw_lock.read().await;

        if user_message
            .content
            .starts_with(read_guard.start_pattern.as_str())
        {
            let command_string = user_message
                .content
                .strip_prefix(read_guard.start_pattern.as_str())
                .expect("Message should always start with prefix at this point");

            //Drop the read_guard here
            drop(read_guard);

            let Some(command) = command_string.split_whitespace().next() else {
                return;
            };
            let request_text_channel = user_message.channel_id;

            let Some(guild) = user_message.guild_id else {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Could not get guild for message");

                return;
            };

            let request_voice_channel = guild
                .to_guild_cached(&ctx.cache)
                .and_then(|cache_ref| {
                    cache_ref
                        .voice_states
                        .get(&user_message.author.id)
                        .map(|state| state.channel_id)
                })
                .flatten();

            match command {
                "help" => {
                    send_help_dm(user_message.author.id, ctx.clone()).await;
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

                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "playnow command issued");
                    play_now(
                        command_string.into(),
                        request_text_channel,
                        request_voice_channel,
                        guild_id,
                        ctx.clone(),
                        guild_rw_lock.clone(),
                    )
                    .await;
                }
                //pauses the currently playing track and leaves the call
                "leave" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "leave command issued");

                    let _ = guild_rw_lock.write().await.pause_current_track().await;
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
                    //Attempt to resume any tracks we have
                    let _ = guild_rw_lock.write().await.resume_current_track().await;
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
                    list_queue(request_text_channel, &ctx, &guild_context).await;
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
                    if guild_context.playback_queue.is_empty() {
                        send_message(request_text_channel, ctx.http, "Queue is currently empty!")
                            .await;
                        return;
                    }
                    let mut message = String::new();

                    guild_context.shuffle_queue().await;

                    let _ = writeln!(&mut message, "Shuffled Queue:");
                    guild_context
                        .playback_queue
                        .iter()
                        .enumerate()
                        .for_each(|(i, entry)| {
                            let _ = writeln!(
                                &mut message,
                                "Position {pos}: {title}",
                                pos = i + 1,
                                title = entry.info.title(),
                            );
                        });
                    send_message(request_text_channel, ctx.http, &message).await;
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

                    GuildContext::handle_voice_channel_joining(
                        guild_id,
                        request_text_channel,
                        request_voice_channel,
                        guild_rw_lock.clone(),
                        &ctx,
                    )
                    .await;

                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "add command issued");

                    let message_sent =
                        send_message(request_text_channel, &ctx.http, " Searching...").await;

                    add_to_queue(
                        command_string.into(),
                        user_message.author.id,
                        guild_id,
                        ctx.clone(),
                        guild_rw_lock.clone(),
                        message_sent,
                    )
                    .await;
                }

                "clear" => {
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in the same voice channel as the bot
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
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in the same voice channel as the bot
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
                        "single" | "Single" => LoopMode::Single,
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
                    let _ = send_message(
                        request_text_channel,
                        &ctx.http,
                        &format!("Set loop mode to {new_loop_mode}"),
                    )
                    .await;
                }

                "remove" => {
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in the same voice channel as the bot
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "remove command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    remove_tracks(
                        command_string,
                        guild_id,
                        request_text_channel,
                        &ctx,
                        &mut guild_context,
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
                    pause_track(request_text_channel, &ctx, &mut guild_context).await;
                }

                "resume" => {
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in same voice channel as the bot
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "resume command issued");

                    let mut guild_context = guild_rw_lock.write().await;
                    resume_track(request_text_channel, &ctx, &mut guild_context).await;
                }

                "nowplaying" => {
                    //No requirement on being in a voice channel
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
                    } else {
                        let _ = send_message(
                            request_text_channel,
                            &ctx.http,
                            "Not playing anything right now",
                        )
                        .await;
                    }
                }

                "undo" => {
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in same voice channel as the bot
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
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in same voice channel as the bot
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
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in same voice channel as the bot
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    match guild_context.mute(&ctx, guild_id).await {
                        Ok(_) => {
                            let _ = send_message(request_text_channel, &ctx.http, "Muted").await;
                        }
                        Err(err) => {
                            #[cfg(feature = "tracing")]
                            event!(Level::WARN, "Error when muting: {err}");
                        }
                    }
                }
                "unmute" => {
                    //Require user to be in a voice channel
                    let Some(request_voice_channel) = request_voice_channel else {
                        send_message(
                            request_text_channel,
                            ctx.http.clone(),
                            "You must be in a voice channel to use this command",
                        )
                        .await;
                        return;
                    };
                    //Require user to be in the same voice channel as the bot
                    if !command_is_in_same_call(songbird_manager, guild_id, request_voice_channel)
                        .await
                    {
                        return;
                    }
                    let mut guild_context = guild_rw_lock.write().await;
                    match guild_context.unmute(&ctx, guild_id).await {
                        Ok(_) => {
                            let _ = send_message(request_text_channel, &ctx.http, "Unmuted").await;
                        }
                        Err(err) => {
                            #[cfg(feature = "tracing")]
                            event!(Level::WARN, "Error when unmuting: {err}");
                        }
                    }
                }
                "beep" | "beep!" => {
                    let _ = send_message(request_text_channel, ctx.http, "boop!").await;
                }

                _ => {}
            }
        }
    }
}

async fn add_to_queue(
    command_string: Box<str>,
    command_issuer: UserId,
    guild_id: GuildId,
    ctx: Context,
    guild_context: Arc<RwLock<GuildContext>>,
    mut message_sent: Option<Message>,
) {
    let read_lock = ctx.data.read().await;
    let yt_dlp = read_lock.get::<YtDlpKey>().unwrap();

    let rest_of_command = command_string.trim().strip_prefix("add ").unwrap();
    let query = VideoQuery::new_from_str(rest_of_command);

    if query.is_playlist() {
        if let Some(message) = message_sent.as_mut() {
            let builder = EditMessage::new().content("Searching... (Playlists can take a while)");
            let _ = message.edit(&ctx.http, builder).await;
        };
        let VideoQuery::Url(url) = query else {
            //is_playlist ensures we have the url enum
            unreachable!();
        };

        let Ok(streams) = yt_dlp.search_for_playlist(url).await else {
            if let Some(mut message) = message_sent {
                let builder = EditMessage::new().content("I couldn't find anything :(".to_string());
                let _ = message.edit(&ctx.http, builder).await;
            };

            return;
        };
        let len = streams.len();
        if let Some(mut message) = message_sent {
            let builder = EditMessage::new().content(format!("Adding {len} tracks to the queue",));
            let _ = message.edit(&ctx.http, builder).await;
        };
        let streams = streams.into_iter().map(Arc::new).collect::<Vec<_>>();

        guild_context
            .write()
            .await
            .add_many_to_queue(command_issuer, &streams, &ctx, guild_id)
            .await;
    } else {
        let Ok(video) = yt_dlp.search_for_video(query).await else {
            if let Some(mut message) = message_sent {
                let builder = EditMessage::new().content("I couldn't find anything :(".to_string());
                let _ = message.edit(&ctx.http, builder).await;
            };

            return;
        };
        let video_info_arc = Arc::new(video);
        let queue_length = guild_context.read().await.playback_queue.num_tracks();

        if let Some(mut message) = message_sent {
            let builder = EditMessage::new().content(format!(
                "Adding {track_name} to the queue at position {pos}",
                pos = queue_length + 1,
                track_name = video_info_arc.title()
            ));
            let _ = message.edit(&ctx.http, builder).await;
        };

        guild_context
            .write()
            .await
            .add_to_queue(command_issuer, video_info_arc, &ctx, guild_id)
            .await;
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

impl MusicBot {
    pub fn new() -> Self {
        Self {
            guild_datas: RwLock::new(Vec::new()),
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
    #[cfg(feature = "tracing")]
    event!(Level::INFO, "Sending chat message: {message}");

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

async fn send_help_dm(user: UserId, ctx: Context) {
    const COMMAND_SYNTAX: [&str; 17] = [
        "help",
        "play { url | search text }",
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
    const COMMAND_EXPLANATION: [&str; 17] = [
        "Show this list.",
        "Bypass the queue and play a song from a url or youtube search.",
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

    let _ = user
        .direct_message(&ctx.http, CreateMessage::new().content(help_message))
        .await;
}

async fn play_now(
    command_string: Box<str>,
    request_text_channel: ChannelId,
    request_voice_channel: ChannelId,
    guild_id: GuildId,
    ctx: Context,
    guild_context: Arc<RwLock<GuildContext>>,
) {
    let read_lock = ctx.data.read().await;

    let yt_dlp = read_lock.get::<YtDlpKey>().unwrap();

    let http_client = read_lock.get::<HTTPClientKey>().unwrap();

    let rest_of_command = command_string.trim().strip_prefix("play ").unwrap();
    let query = VideoQuery::new_from_str(rest_of_command);

    let message = send_message(request_text_channel, &ctx.http, " Searching...").await;

    let video_info = yt_dlp.search_for_video(query).await.unwrap();

    let stream_info = yt_dlp.get_audio_streams(&video_info).await.unwrap();

    let Some(audio) = stream_info.to_audio_stream(http_client.clone()) else {
        if let Some(mut message) = message {
            let builder = EditMessage::new().content("I couldn't find anything :(".to_string());
            let _ = message.edit(&ctx.http, builder).await;
        };

        return;
    };
    let audio_arc = Arc::new(audio);
    let mut guild_context = guild_context.write().await;

    guild_context
        .play_now(
            ctx.clone(),
            guild_id,
            request_voice_channel,
            audio_arc.clone(),
        )
        .await;
    if let Some(mut message) = message {
        let builder = EditMessage::new().content(format!(
            "Started Playing: {track_name}",
            track_name = audio_arc.name
        ));
        let _ = message.edit(&ctx.http, builder).await;
    };
}
async fn list_queue(request_text_channel: ChannelId, ctx: &Context, guild_context: &GuildContext) {
    if guild_context.playback_queue.is_empty() {
        send_message(request_text_channel, &ctx.http, "Queue is currently empty!").await;
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
                "Position #{pos}: {name}",
                pos = i + 1,
                name = entry.info.title(),
            );
            if i == guild_context.queue_position() {
                let _ = write!(&mut message, " <- Next up");
            }
            let _ = writeln!(&mut message);
        });
    send_message(request_text_channel, &ctx.http, &message).await;
}

async fn pause_track(
    request_text_channel: ChannelId,
    ctx: &Context,
    guild_context: &mut GuildContext,
) {
    match guild_context.pause_current_track().await {
        Ok(_) => {
            let _ = send_message(request_text_channel, &ctx.http, "Paused track").await;
        }
        Err(err) => match err {
            guild_context::TrackControlError::NoTrack => {
                let _ = send_message(request_text_channel, &ctx.http, "No track to pause").await;
            }
            guild_context::TrackControlError::Error(control_error) => {
                #[cfg(feature = "tracing")]
                event!(
                    Level::WARN,
                    "Track control error when pausing track. Error: {control_error:?}"
                );
                let _ = send_message(
                    request_text_channel,
                    &ctx.http,
                    &format!("Error pausing track: {control_error:?}"),
                )
                .await;
            }
        },
    }
}

async fn resume_track(
    request_text_channel: ChannelId,
    ctx: &Context,
    guild_context: &mut GuildContext,
) {
    match guild_context.resume_current_track().await {
        Ok(_) => {
            let _ = send_message(request_text_channel, &ctx.http, "Resumed track playback").await;
        }
        Err(err) => match err {
            guild_context::TrackControlError::NoTrack => {
                let _ = send_message(request_text_channel, &ctx.http, "No track to resume").await;
            }
            guild_context::TrackControlError::Error(control_error) => {
                let _ = send_message(
                    request_text_channel,
                    &ctx.http,
                    &format!("Error resuming track: {control_error}"),
                )
                .await;
            }
        },
    }
}

async fn remove_tracks(
    command_string: &str,
    guild_id: GuildId,
    request_text_channel: ChannelId,
    ctx: &Context,
    guild_context: &mut GuildContext,
) {
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
            removed_track_count = match guild_context
                .remove_tracks_from(guild_id, rest.trim(), ctx)
                .await
            {
                Ok(tracks_removed) => tracks_removed,
                Err(err) => {
                    //TODO Send message to chat
                    #[cfg(feature = "tracing")]
                    event!(Level::WARN, "Error removing tracks from user: {err:?}");
                    0
                }
            }
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
            let end = guild_context.queue_length();
            //Both remove past and remove from should prooobably be exclusive
            println!(
                "past. start= {start}, end = {end}",
                start = start.get(),
                end = end
            );

            removed_track_count = guild_context
                .playback_queue
                .remove_tracks_in_range(start.get()..end);
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
                .playback_queue
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
            let end = guild_context.queue_length();
            println!("until. start= 0, end = {end}", end = count.get().min(end));
            removed_track_count = guild_context
                .playback_queue
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

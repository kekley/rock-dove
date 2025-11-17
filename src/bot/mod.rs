pub mod command;
pub mod guild_context;
pub mod queue;
pub mod track_notifier;
pub mod tracks;
pub mod undo_stack;
pub mod work_queue;

use serenity::{
    all::{CacheHttp, ChannelId, Context, EditMessage, GuildId, Message, UserId},
    async_trait,
};
use songbird::{Call, Songbird};
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

pub struct MusicBot {}

#[async_trait]
impl serenity::all::EventHandler for MusicBot {
    async fn message(&self, ctx: Context, user_message: Message) {
        let Some(guild_id) = user_message.guild_id else {
            return;
        };

        let guild_rw_lock = self.get_or_insert_guild_context(guild_id).await;
        let read_guard = guild_rw_lock.read().await;

        let songbird_manager = songbird::get(&ctx).await.unwrap();

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
                "skip" => {}
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

impl MusicBot {
    pub fn new() -> Self {
        Self {}
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
pub async fn send_message(ctx: &Context, channel: ChannelId, message: &str) -> Option<Message> {
    #[cfg(feature = "tracing")]
    event!(Level::INFO, "Sending chat message: {message}");

    match channel.say(&ctx.http, message).await {
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

    let video_info = yt_dlp.search_for_video(&query).await.unwrap();

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

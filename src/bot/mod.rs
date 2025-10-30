use std::{fmt::Write, sync::Arc};
pub mod guild_context;
pub mod tracks;
pub mod undo_stack;

use reqwest::Client;
use serenity::{
    all::{CacheHttp, ChannelId, Context, GuildId, Http, Message, UserId},
    async_trait,
};
use songbird::{CoreEvent, Event, EventContext, EventHandler as SongBirdEventHandler, TrackEvent};
use thiserror::Error;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::{Level, event};

use crate::{
    bot::guild_context::{GuildContext, LoopMode, RemoveMode, StreamData},
    commands::{CommandError, Executable},
    yt_dlp::{self, video::Video},
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

        let context_lock = self.get_or_insert_guild_context(guild_id).await;

        let read_guard = context_lock.read().await;

        if msg.content.starts_with(read_guard.start_pattern.as_str()) {
            let command_string = msg
                .content
                .strip_prefix(read_guard.start_pattern.as_str())
                .expect("Message should always start with prefix at this point");
            drop(read_guard);
            let Some(command) = command_string.split_whitespace().next() else {
                return;
            };
            let text_channel = msg.channel_id;

            let Some(guild) = msg.guild_id else {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Could not get guild for message");

                return;
            };

            let Some(voice_channel) = guild
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
                    text_channel,
                    ctx.http.clone(),
                    "You must be in a voice channel to send bot commands",
                )
                .await;
                return;
            };

            match command {
                "playnow" => {
                    #[cfg(feature = "tracing")]
                    event!(Level::INFO, "Playnow");

                    let rest = command_string.trim().strip_prefix("playnow ").unwrap();
                    let query = VideoQuery::from_str(rest);
                    let Some(audio) =
                        Self::get_stream_data(query, self.yt_dlp.clone(), self.client.clone())
                            .await
                    else {
                        return;
                    };

                    let mut guild_context = context_lock.write().await;

                    guild_context
                        .play_now(&ctx, guild_id, voice_channel, text_channel, audio)
                        .await
                }
                "leave" => {
                    unimplemented!()
                }

                "skip" => {
                    let mut guild_context = context_lock.write().await;
                    guild_context.skip_track().await;
                }

                "list" => {
                    let guild_context = context_lock.read().await;
                    if guild_context.queue_is_empty() {
                        send_message(text_channel, ctx.http, "Queue is currently empty!").await;
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
                    send_message(text_channel, ctx.http, &message).await
                }

                "shuffle" => {
                    let mut guild_context = context_lock.write().await;

                    guild_context.shuffle_queue();
                }

                "add" => {
                    let mut guild_context = context_lock.write().await;
                    let rest = command_string.trim().strip_prefix("add ").unwrap();
                    let query = VideoQuery::from_str(rest);
                    let Some(audio) =
                        Self::get_stream_data(query, self.yt_dlp.clone(), self.client.clone())
                            .await
                    else {
                        send_message(
                            text_channel,
                            &ctx.http,
                            "COUGH WHEEEZE I'M FUCKING DEAD (that didn't work for some reason, sorry)",
                        )
                        .await;

                        return;
                    };
                    let queue_length = guild_context.iter_queue().count();
                    send_message(
                        text_channel,
                        &ctx.http,
                        &format!(
                            "Adding {track_name} to the queue at position: {queue_length}",
                            track_name = &audio.name
                        ),
                    )
                    .await;
                    guild_context.add_to_queue(msg.author.id, audio);
                }

                "clear" => {
                    let mut guild_context = context_lock.write().await;
                    send_message(text_channel, &ctx.http, "Clearing the queue").await;
                    guild_context.clear_queue();
                }

                "loop" => {
                    let mut guild_context = context_lock.write().await;
                    let Some(mode) = command_string.split_whitespace().nth(1) else {
                        send_message(
                            text_channel,
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
                                text_channel,
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
                    let mut guild_context = context_lock.write().await;
                    let Some(mode) = command_string.split_whitespace().nth(1) else {
                        send_message(
                            text_channel,
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
                            send_message(text_channel, ctx.http.clone(), &format!("{mode} is not a valid remove mode. Valid options: From, At, Until, Past",)).await;
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
                                    text_channel,
                                    &ctx.http,
                                    "The remove past command needs a queue position to remove tracks after",
                                )
                                .await;
                                return;
                            };
                            let Ok(count) = count.parse::<usize>() else {
                                send_message(
                                    text_channel,
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
                                    text_channel,
                                    &ctx.http,
                                    "The remove at command needs a queue position to remove",
                                )
                                .await;
                                return;
                            };
                            let Ok(count) = count.parse::<usize>() else {
                                send_message(
                                    text_channel,
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
                                    text_channel,
                                    &ctx.http,
                                    "The remove until command needs a queue position to remove tracks up to",
                                )
                                .await;
                                return;
                            };
                            let Ok(count) = count.parse::<usize>() else {
                                send_message(
                                    text_channel,
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
                        text_channel,
                        &ctx.http,
                        &format!("Removed {removed_track_count} tracks"),
                    )
                    .await;
                }

                "pause" => {
                    let mut guild_context = context_lock.write().await;
                    guild_context.pause().await;
                }

                "resume" => {
                    let mut guild_context = context_lock.write().await;
                    guild_context.resume().await;
                }

                "nowplaying" => {
                    let guild_context = context_lock.read().await;
                }

                "undo" => {
                    unimplemented!()
                }

                "stats" => {
                    let guild_context = context_lock.read().await;
                }

                "move" => {
                    let mut guild_context = context_lock.write().await;
                }
                "mute" => {
                    let mut guild_context = context_lock.write().await;
                }
                "unmute" => {
                    let mut guild_context = context_lock.write().await;
                }
                "beep" | "beep!" => send_message(text_channel, ctx.http, "boop!").await,

                _ => {}
            }
        }
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
#[derive(Error, Debug)]
enum AudioFetchError {
    #[error("Error running ytp-dlp: {0}")]
    CommandError(#[from] CommandError),
    #[error("")]
    OtherError(),
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
pub async fn send_message(channel: ChannelId, http: impl CacheHttp, message: &str) {
    if let Err(err) = channel.say(http, message).await {
        event!(Level::ERROR, "Error sending message: {err}");
    }
}

impl Default for MusicBot {
    fn default() -> Self {
        Self::new()
    }
}

struct TrackErrorNotifier {
    guild: Arc<RwLock<GuildContext>>,
}

#[async_trait]
impl SongBirdEventHandler for TrackErrorNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
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

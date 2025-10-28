use std::fmt::Write;
pub mod guild_context;

use serenity::{
    all::{CacheHttp, ChannelId, Context, GuildId, Message, UserId},
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as SongBirdEventHandler, TrackEvent};
use thiserror::Error;
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};
use tracing::{Level, event};

use crate::{
    bot::guild_context::{GuildContext, StreamData},
    commands::{CommandError, Executable},
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

struct CommandContext {
    text_channel: ChannelId,
    voice_channel: Option<ChannelId>,
    author: UserId,
    ctx: Context,
    guild_id: GuildId,
}

impl CommandContext {
    pub fn new(message: &Message, ctx: Context) -> Self {
        let channel = message.channel_id;
        let author = message.author.id;
        let guild = message
            .guild(&ctx.cache)
            .expect("A command should always be issued through guild messages");
        let voice_channel = guild
            .voice_states
            .get(&author)
            .and_then(|voicestate| voicestate.channel_id);
        let guild_id = guild.id;
        drop(guild);
        Self {
            text_channel: channel,
            author,
            ctx,
            guild_id,
            voice_channel,
        }
    }
}

pub struct MusicBot {
    guild_datas: tokio::sync::RwLock<Vec<(GuildId, tokio::sync::RwLock<GuildContext>)>>,
    yt_dlp: Executable,
}

#[async_trait]
impl serenity::all::EventHandler for MusicBot {
    async fn message(&self, ctx: Context, msg: Message) {
        let Some(guild_id) = msg.guild_id else {
            return;
        };

        let guild_context = self.get_or_insert_guild_context(guild_id).await;

        let read_guard = guild_context.read().await;

        if msg.content.starts_with(read_guard.start_pattern.as_str()) {
            let command_string = msg
                .content
                .strip_prefix(read_guard.start_pattern.as_str())
                .expect("Message should always start with prefix at this point");
            let command_context: CommandContext = CommandContext::new(&msg, ctx);
            drop(read_guard);
            Self::run_user_command(command_string, command_context, &self.yt_dlp, guild_context)
                .await;
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
        todo!()
    }
    async fn run_user_command(
        command_string: &str,
        command_context: CommandContext,
        yt_dlp: &Executable,
        context_lock: RwLockReadGuard<'_, RwLock<GuildContext>>,
    ) {
        let Some(command) = command_string.split_whitespace().next() else {
            return;
        };
        match command {
            "playnow" => {
                let rest = command_string.trim().strip_prefix("playnow ").unwrap();
                let query = VideoQuery::from_str(rest);
                let Some(audio) = Self::get_stream_data(query, yt_dlp).await else {
                    return;
                };
                let write_lock = context_lock.write().await;

                let manager = songbird::get(&command_context.ctx)
                    .await
                    .expect("Songbird manager should have been registered");

                if let Some(call_manager_mutex) = manager.get(command_context.guild_id) {
                    let mut call_manager = call_manager_mutex.lock().await;
                    let track_handle = call_manager.play_input(audio.into());
                } else {
                    eprintln!("songbird handler lock");
                    return;
                }
                todo!()
            }
            "leave" => {
                unimplemented!()
            }

            "skip" => {
                unimplemented!()
            }

            "list" => {
                let guild_context = context_lock.read().await;
                if guild_context.queue_is_empty() {
                    Self::send_message(
                        command_context.text_channel,
                        command_context.ctx.http,
                        "Queue is currently empty!",
                    )
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
                Self::send_message(
                    command_context.text_channel,
                    command_context.ctx.http,
                    &message,
                )
                .await
            }

            "shuffle" => {
                unimplemented!()
            }

            "add" => {
                let rest = command_string.trim().strip_prefix("add ").unwrap();
                let query = VideoQuery::from_str(rest);
                let Some(audio) = Self::get_stream_data(query, yt_dlp).await else {
                    return;
                };
                todo!()
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
            "mute" => {}
            "beep" | "beep!" => {
                Self::send_message(
                    command_context.text_channel,
                    command_context.ctx.http,
                    "boop!",
                )
                .await
            }

            _ => {}
        }
    }
    async fn get_stream_data(query: VideoQuery<'_>, yt_dlp: &Executable) -> Option<StreamData> {
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
                let Some(out) = output
                    .stdout
                    .split(|&b| b == b'\n')
                    .filter(|&x| !x.is_empty())
                    .flat_map(serde_json::from_slice::<Video>)
                    .next()
                else {
                    #[cfg(feature = "tracing")]
                    event!(Level::ERROR, "ytp-dlp had no valid output");
                    return None;
                };
            }
            Err(err) => {
                #[cfg(feature = "tracing")]
                event!(Level::ERROR, "Error running yt_dlp {err}");
                None
            }
        }
    }

    async fn send_message(channel: ChannelId, http: impl CacheHttp, message: &str) {
        if let Err(err) = channel.say(http, message).await {
            event!(Level::ERROR, "Error sending message: {err}");
        }
    }

    async fn get_or_insert_guild_context(
        &self,
        id: GuildId,
    ) -> RwLockReadGuard<'_, RwLock<GuildContext>> {
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
            guild_context
        } else {
            let mut write_lock = self.guild_datas.write().await;
            write_lock.push((id, RwLock::new(GuildContext::new())));
            RwLockWriteGuard::downgrade_map(write_lock, |vec| {
                vec.last().map(|(_, context)| context).unwrap()
            })
        }
    }

    async fn join_voice_channel(command_context: CommandContext) {
        let Some(connect_to) = command_context.voice_channel else {
            Self::send_message(
                command_context.text_channel,
                command_context.ctx.http(),
                "Must be in a voice channel to use this command",
            )
            .await;
            return;
        };

        let manager = songbird::get(&command_context.ctx)
            .await
            .expect("Songbird Voice client placed in at initialisation.")
            .clone();

        if let Ok(handler_lock) = manager.join(command_context.guild_id, connect_to).await {
            let mut handler = handler_lock.lock().await;
            handler.add_global_event(TrackEvent::Error.into(), TrackErrorNotifier);
        }
    }
}

impl Default for MusicBot {
    fn default() -> Self {
        Self::new()
    }
}

struct TrackErrorNotifier;

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

        if let EventContext::DriverConnect(c) = ctx {
            todo!()
        }

        if let EventContext::DriverReconnect(c) = ctx {
            todo!()
        }

        if let EventContext::DriverDisconnect(c) = ctx {
            todo!()
        }
        None
    }
}

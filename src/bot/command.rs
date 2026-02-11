use std::{fmt::Write, num::ParseIntError, sync::Arc};

use compact_str::{CompactString, format_compact};

use rand::{rng, seq::IndexedRandom};
use serenity::all::{ChannelId, Context, CreateMessage, EditMessage, GuildId, Message, UserId};
use songbird::{Call, CoreEvent, Songbird, TrackEvent, error::JoinError, input::HttpRequest};
use strum::IntoEnumIterator as _;
use strum_macros::EnumIter;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};
use tracing::{Level, event, instrument};

use crate::{
    GuildContextKey, HTTPClientKey,
    bot::{
        guild_context::{GuildContext, RemoveTracksFromError, TrackControlError},
        queue::LoopMode,
        send_message,
        track_notifier::{TrackEndNotifier, TrackErrorNotifier, UserDisconnectNotifier},
    },
    yt_dlp::{VideoQuery, YtDlp, YtDlpKey, sidecar::YtDlpSidecar},
};

#[derive(Debug, EnumIter, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Command {
    Help,
    Leave,
    Join,
    Skip,
    List,
    Shuffle,
    Play,
    Add,
    Clear,
    Loop,
    Remove,
    Pause,
    Resume,
    NowPlaying,
    Undo,
    Redo,
    Mute,
    Unmute,
    Beep,
    Ffmpeg,
}

impl Command {
    ///Get the syntax for using the command
    pub fn syntax(self) -> &'static str {
        match self {
            Command::Help => "",
            Command::Leave => "",
            Command::Join => "",
            Command::Skip => "",
            Command::List => "",
            Command::Shuffle => "",
            Command::Play => "{ url | playlist url | search text }",
            Command::Add => "{ url | playlist url | search text }",
            Command::Clear => "",
            Command::Loop => "{ off | single | queue }",
            Command::Remove => "{  at | past | until | from }",
            Command::Pause => "",
            Command::Resume => "",
            Command::NowPlaying => "",
            Command::Undo => "",
            Command::Redo => "",
            Command::Mute => "",
            Command::Unmute => "",
            Command::Beep => "",
            Command::Ffmpeg => "",
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Command::Help => "Show this list.",
            Command::Leave => "Remove the bot from any voice channels.",
            Command::Join => "Join the voice channel you're in.",
            Command::Skip => "End the current track.",
            Command::List => "List the current contents of the queue.",
            Command::Shuffle => "Shuffle the contents of the queue.",
            Command::Play => "Bypass the queue and play a song from a url or youtube search",
            Command::Add => "Add a song or playlist to the queue from a url or youtube search.",
            Command::Clear => "Clear the queue.",
            Command::Loop => {
                "Set the loop mode.\noff = No looping\nsingle = Loop the current song indefinitely\nqueue = Loop the queue when it ends"
            }

            Command::Remove => {
                "Remove one or more tracks from the queue.\nremove at (track position) = Remove the track at (track position)\nremove past (track position) = Remove all tracks after (track position)\nremove until (track position) = Remove all tracks up to (track position)\nremove from (username) = Remove all tracks added by (username)"
            }
            Command::Pause => "Pause the current track.",
            Command::Resume => "Resume the current track.",
            Command::NowPlaying => "See the name of the current track.",
            Command::Undo => "Undo the last change made to the queue.",
            Command::Redo => "Undo the last undo..?",
            Command::Mute => "Mute the bot",
            Command::Unmute => "Unmute",
            Command::Beep => "Say hi",
            Command::Ffmpeg => "Play back a raw audio stream from the web",
        }
    }
}

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
use strsim::normalized_damerau_levenshtein;

pub fn closest_to<'a>(input: &str, commands: impl Iterator<Item = &'a str>) -> Option<&'a str> {
    const THRESHOLD: f64 = 0.5;

    let mut best_command: Option<&'a str> = None;
    let mut best_score: f64 = 0.0;

    for command in commands {
        let score = normalized_damerau_levenshtein(input, command);

        if score > best_score {
            best_score = score;
            best_command = Some(command);
        }
    }

    if best_score >= THRESHOLD {
        best_command
    } else {
        None
    }
}

#[derive(Error, Debug)]
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
    #[error("{error_message}")]
    UnrecognizedCommand { error_message: CompactString },
}

impl CommandParseError {
    pub fn to_reply(&self) -> Option<String> {
        match self {
            CommandParseError::InvalidRemoveArg(_)
            | CommandParseError::NoQueryArgument
            | CommandParseError::InvalidLoopMode
            | CommandParseError::NoVoiceChannnel
            | CommandParseError::UnrecognizedCommand { error_message: _ } => Some(self.to_string()),
            _ => None,
        }
    }
}

impl PreparedCommand {
    pub async fn parse(message: Message, ctx: &Context) -> Result<Self, CommandParseError> {
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
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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
                let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
                let guard = guild_lock.read().await;
                let commands = guard.get_command_mappings().keys();
                let commands_as_strs = commands.map(CompactString::as_str);
                let closest = closest_to(&command_lower, commands_as_strs);

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
                Err(CommandParseError::UnrecognizedCommand { error_message: msg })
            }
        }
    }
}

fn choose_insult() -> &'static str {
    pub const INSULTS: &[&str] = &[
        "cretin",
        "bungler",
        "clod",
        "bonobo",
        "feckless",
        "imbecile",
        "numbskull",
        "peasant",
        "spleen",
        "troglodyte",
        "whelp",
        "wretch",
        "bozo",
        "dunghead",
        "cretin",
        "loathesome dung eater",
    ];
    INSULTS.choose(&mut rng()).unwrap()
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

impl PreparedCommand {
    #[instrument]
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
            PreparedCommand::Skip {
                guild,
                voice_channel,
                text_channel,
            } => {
                skip(&ctx, guild, voice_channel, text_channel).await;
            }
            PreparedCommand::List {
                guild,
                text_channel,
            } => {
                list(&ctx, guild, text_channel).await;
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
    let yt_dlp = get_ytdlp(ctx).await;
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;

    if query.is_playlist() {
        if let Some(message) = message_sent.as_mut() {
            let builder =
                EditMessage::new().content("Playlists must be added with the add command");
            let _ = message.edit(&ctx.http, builder).await;
        }
    } else {
        let Ok(video) = yt_dlp.search_for_video(&query).await else {
            if let Some(mut message) = message_sent {
                let builder = EditMessage::new().content("I couldn't find anything :(".to_string());
                let _ = message.edit(&ctx.http, builder).await;
            };

            return;
        };

        let streams = match yt_dlp.get_audio_streams(&video).await {
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

        let audio = streams.clone().to_audio_stream(http);

        let Some(audio_arc) = audio.map(Arc::new) else {
            if let Some(mut message) = message_sent {
                let builder = EditMessage::new().content("Error playing track");
                let _ = message.edit(&ctx.http, builder).await;
            };

            return;
        };

        if let Some(mut message) = message_sent {
            let builder = EditMessage::new()
                .content(format!("Playing {track_name}", track_name = &streams.title));
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
    run_if_same_call(
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
    run_if_same_call(
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
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        if guild_lock.write().await.redo().await {
            let _ = send_message(ctx, text_channel, "Undid last undo").await;
        } else {
            let _ = send_message(ctx, text_channel, "Nothing to redo").await;
        }
    })
    .await;
}

async fn undo(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        if guild_lock.write().await.undo().await {
            let _ = send_message(ctx, text_channel, "Undid last queue change").await;
        } else {
            let _ = send_message(ctx, text_channel, "Nothing to undo").await;
        }
    })
    .await;
}

async fn now_playing(ctx: &Context, guild: GuildId, text_channel: ChannelId) {
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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
                let max = guild_context.queue_length();
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
                let max = guild_context.queue_length();
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
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        guild_context.set_loop_mode(mode).await;
        let _ = send_message(ctx, text_channel, &format!("Set loop mode to {}", mode)).await;
    })
    .await;
}

async fn clear(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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
                .add_many_to_queue(user, &streams, ctx, guild)
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
            .add_to_queue(user, video_info_arc, ctx, guild)
            .await;
    }
}

async fn shuffle(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        let mut guild_context = guild_lock.write().await;
        if guild_context.queue_length() == 0 {
            let _ = send_message(ctx, text_channel, "Queue is empty").await;
        } else if guild_context.queue_length() == 1 {
            guild_context.shuffle_queue().await;
            let _ = send_message(ctx, text_channel, "shuffled.... one song").await;
        } else {
            guild_context.shuffle_queue().await;
            let _ = send_message(ctx, text_channel, "Shuffled the queue").await;
        }
    })
    .await
}

async fn list(ctx: &Context, guild: GuildId, text_channel: ChannelId) {
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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

async fn skip(ctx: &Context, guild: GuildId, voice_channel: ChannelId, text_channel: ChannelId) {
    run_if_same_call(ctx, guild, voice_channel, text_channel, async move |_| {
        let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
        match guild_lock.write().await.next_track(ctx, guild).await {
            Ok(_) => {
                let _ = send_message(ctx, text_channel, "Skipped track").await;
            }
            Err(err) => {
                event!(Level::WARN, "{err}");
            }
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
    let _ = write_lock.pause_current_track().await;
}

async fn help(ctx: &Context, user: UserId, guild: GuildId) {
    let mut help_message = String::new();
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
    let guard = guild_lock.read().await;
    guard.help_message.push_str("COMMANDS:\n");
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
            call_lock.add_global_event(
                CoreEvent::ClientDisconnect.into(),
                UserDisconnectNotifier {
                    guild_id: guild,
                    context: ctx.clone(),
                },
            );
        }
        Err(err) => {
            event!(Level::ERROR, "Failed to join voice call. Error: {err}");

            send_message(ctx, text_channel, "Failed to join voice channel").await;
        }
    }
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
    let guild_lock = get_or_insert_guild_lock(ctx, guild).await;
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

#[derive(Debug, Clone)]
pub enum RemoveArgument {
    From(CompactString),
    At(u32),
    Until(u32),
    Past(u32),
}

#[derive(Debug, Error)]
pub enum RemoveArgParseError {
    #[error(
        "You need to specify a remove mode: from (user), at (position), until (position), or past (position)"
    )]
    NoModeSpecified,
    #[error(
        "Valid remove arguments: from (user), at (position), until (position), or past (position)"
    )]
    InvalidModeSpecified,

    #[error("The positional argument should be a number")]
    InvalidArg(#[from] ParseIntError),
}

impl RemoveArgument {
    pub(crate) fn parse(str: &str) -> Result<Self, RemoveArgParseError> {
        let Some((kind, arg)) = str.split_once(" ") else {
            return Err(RemoveArgParseError::NoModeSpecified);
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
            _ => Err(RemoveArgParseError::InvalidModeSpecified),
        }
    }
}

pub async fn get_or_insert_guild_lock(ctx: &Context, guild: GuildId) -> Arc<RwLock<GuildContext>> {
    let data = ctx.data.read().await;
    let guilds = data
        .get::<GuildContextKey>()
        .expect("Guild Contexts should have been created at startup");
    if let Some((_, arc)) = guilds.iter().find(|(id, _)| *id == guild) {
        arc.clone()
    } else {
        drop(data);

        let mut data = ctx.data.write().await;
        let guilds = data
            .get_mut::<GuildContextKey>()
            .expect("Guild Contexts should have been created at startup");
        let guild_context = Arc::new(RwLock::new(GuildContext::default()));

        guilds.push((guild, guild_context.clone()));

        guild_context.clone()
    }
}

pub async fn get_songbird(ctx: &Context) -> Arc<Songbird> {
    songbird::get(ctx)
        .await
        .expect("Songbird should have been inserted at startup")
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
    text_channel: ChannelId,
    f: impl FnOnce(Arc<Mutex<Call>>) -> F,
) {
    let songbird = get_songbird(ctx).await;
    if let Some(call) = songbird.get(guild) {
        if call
            .lock()
            .await
            .current_channel()
            .is_some_and(|c| c.eq(&voice_channel.into()))
        {
            f(call).await
        } else {
            let _ = send_message(
                ctx,
                text_channel,
                "You need to be in the same channel as the bot to use this command",
            )
            .await;
        }
    } else {
        //print a message about bot not being in a voice channel
    }
}

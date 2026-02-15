use std::sync::Arc;

use rand::seq::IndexedRandom as _;
use serenity::all::{ChannelId, Context, CreateAttachment, CreateMessage, GuildId, Message};
use songbird::{Call, CoreEvent, Songbird, TrackEvent};
use strsim::normalized_damerau_levenshtein;
use tokio::sync::{Mutex, RwLock};
use tracing::{Level, event};

use crate::{
    GuildContextKey,
    bot::guild_context::{
        GuildContext,
        notifiers::{TrackEndNotifier, TrackErrorNotifier, UserDisconnectNotifier},
    },
    yt_dlp::{YtDlpKey, sidecar::YtDlpSidecar},
};

///Returns the string most similar to `input` with a similarity above `threshold`
pub fn str_closest_to<'a>(
    input: &str,
    haystack: impl Iterator<Item = &'a str>,
    threshold: f64,
) -> Option<&'a str> {
    let mut best_command: Option<&'a str> = None;
    let mut best_score = 0.0;

    for command in haystack {
        let score = normalized_damerau_levenshtein(input, command);

        if score > best_score {
            best_score = score;
            best_command = Some(command);
        }
    }

    if best_score >= threshold {
        best_command
    } else {
        None
    }
}

pub async fn get_or_insert_guild_context_lock(
    ctx: &Context,
    guild: GuildId,
) -> Arc<RwLock<GuildContext>> {
    let read_guard = ctx.data.read().await;
    let guilds = read_guard
        .get::<GuildContextKey>()
        .expect("Guild Contexts should have been created at startup");
    if let Some((_, arc)) = guilds.iter().find(|(id, _)| *id == guild) {
        arc.clone()
    } else {
        drop(read_guard);

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

pub async fn get_ytdlp_from_global_context(ctx: &Context) -> Arc<YtDlpSidecar> {
    ctx.data
        .read()
        .await
        .get::<YtDlpKey>()
        .expect("YtDlp should have been inserted at startup")
        .clone()
}

pub async fn ensure_same_call<F: Future<Output = ()>>(
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

pub async fn send_message(ctx: &Context, channel: ChannelId, message: &str) -> Option<Message> {
    event!(Level::INFO, "Sending chat message: {message}");
    if message.len() > 2000 {
        let attachment = CreateAttachment::bytes(message.as_bytes(), "message.txt");
        let message = CreateMessage::new().add_file(attachment).content(
            "Message is above discord's 2000 character limit. Attaching file with message instead:",
        );
        match channel.send_message(&ctx.http, message).await {
            Ok(message) => Some(message),
            Err(err) => {
                event!(Level::ERROR, "Error sending message: {err}");
                None
            }
        }
    } else {
        match channel.say(&ctx.http, message).await {
            Ok(message) => Some(message),
            Err(err) => {
                event!(Level::ERROR, "Error sending message: {err}");
                None
            }
        }
    }
}

pub fn choose_insult() -> &'static str {
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
    INSULTS.choose(&mut rand::rng()).unwrap()
}

pub async fn handle_voice_channel_joining(
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

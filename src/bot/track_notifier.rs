use serenity::{
    all::{Context, GuildId},
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as SongBirdEventHandler};

use crate::bot::command::{get_or_insert_guild_lock, get_songbird};

pub struct TrackErrorNotifier {
    pub guild_id: GuildId,
    pub context: Context,
}

pub struct TrackEndNotifier {
    pub guild_id: GuildId,
    pub context: Context,
}

pub struct UserDisconnectNotifier {
    pub guild_id: GuildId,
    pub context: Context,
}

#[async_trait]
impl SongBirdEventHandler for UserDisconnectNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        if let EventContext::ClientDisconnect(_) = ctx {
            let songbird_manager = get_songbird(&self.context).await;
            let call = songbird_manager.get(self.guild_id);
            if let Some(call) = call {
                //lock is dropped at the semicolon
                let channel = call
                    .lock()
                    .await
                    .current_channel()
                    .map(|c| serenity::all::ChannelId::new(c.0.get()));

                let should_leave = channel
                    .and_then(|c| {
                        self.guild_id
                            .to_guild_cached(&self.context.cache)
                            .and_then(|g| {
                                g.channels
                                    .get(&c)
                                    .and_then(|c| c.members(&self.context.cache).ok())
                            })
                    })
                    .is_none_or(|members| members.len() <= 1);

                if should_leave {
                    let _ = call.lock().await.leave().await;
                }
            }
        }

        None
    }
}

#[async_trait]
impl SongBirdEventHandler for TrackEndNotifier {
    async fn act(&self, ctx: &EventContext<'_>) -> Option<Event> {
        let guild_lock = get_or_insert_guild_lock(&self.context, self.guild_id).await;
        let mut guild_context = guild_lock.write().await;

        if let EventContext::Track(track_list) = ctx {
            for (_state, handle) in *track_list {
                if let Some(current_track) = &guild_context.get_current_track_info()
                    && current_track.handle.uuid() == handle.uuid()
                {
                    guild_context
                        .handle_next_track(&self.context, self.guild_id)
                        .await;
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
        let guild_lock = get_or_insert_guild_lock(&self.context, self.guild_id).await;
        let mut guild_context = guild_lock.write().await;

        if let EventContext::Track(track_list) = ctx {
            for (state, handle) in *track_list {
                if let Some(current_track) = &guild_context.get_current_track_info()
                    && current_track.handle.uuid() == handle.uuid()
                {
                    guild_context
                        .handle_next_track(&self.context, self.guild_id)
                        .await;
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

use std::sync::Arc;

use serenity::{
    all::{Context, GuildId},
    async_trait,
};
use songbird::{Event, EventContext, EventHandler as SongBirdEventHandler};
use tokio::sync::RwLock;

use crate::bot::guild_context::GuildContext;

pub struct TrackErrorNotifier {
    pub guild_context: Arc<RwLock<GuildContext>>,
    pub guild_id: GuildId,
    pub context: Context,
}

pub struct TrackEndNotifier {
    pub guild_context: Arc<RwLock<GuildContext>>,
    pub guild_id: GuildId,
    pub context: Context,
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
        let mut lock = self.guild_context.write().await;
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

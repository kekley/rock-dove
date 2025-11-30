use serenity::{
    all::{CacheHttp, Context, GuildId},
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
        if let EventContext::ClientDisconnect(c) = ctx {
            let guild_lock = get_or_insert_guild_lock(&self.context, self.guild_id).await;
            let songbird_manager = get_songbird(&self.ctx).await;
            let call = songbird_manager.get(self.guild_id);
            if let Some(call) = call {
                let lock = call.lock().await;
                if let Some(channel) = lock.current_channel() {
                    if let Some(guild) = self.guild_id.to_guild_cached(&self.context.cache) {
                        if let Some(channel) = guild.channels.get(channel) {
                            let members = channel
                                .guild_id
                                .members(self.context.http(), None, None)
                                .await;
                        }
                    }
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

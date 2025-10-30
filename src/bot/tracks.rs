use std::sync::Arc;

use serenity::all::UserId;
use songbird::tracks::TrackHandle;
use symphonia::core::units::Duration;

use crate::bot::guild_context::StreamData;

#[derive(Debug, Clone)]
pub struct SuspendedTrack {
    stream_data: Arc<StreamData>,
    position: Duration,
}

#[derive(Debug, Clone)]
pub struct PlayingTrack {
    pub handle: TrackHandle,
    pub stream: Arc<StreamData>,
}

impl PlayingTrack {
    pub fn pause(&mut self) {
        let _ = self.handle.pause();
    }
    pub fn resume(&mut self) {
        let _ = self.handle.play();
    }
}

#[derive(Clone, Debug)]
pub struct QueuedTrack {
    pub user: UserId,
    pub stream: Arc<StreamData>,
}

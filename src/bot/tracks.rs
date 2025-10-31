use std::{sync::Arc, time::Duration};

use serenity::all::UserId;
use songbird::tracks::TrackHandle;
use tracing::Level;
#[cfg(feature = "tracing")]
use tracing::event;

use crate::bot::guild_context::StreamData;

#[derive(Debug, Clone)]
pub struct SuspendedTrack {
    pub stream_data: Arc<StreamData>,
    pub position: Duration,
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
    pub async fn suspend(&self) -> SuspendedTrack {
        let pos = match self.handle.get_info().await {
            Ok(info) => info.position,
            Err(err) => {
                #[cfg(feature = "tracing")]
                event!(
                    Level::ERROR,
                    "Could not save stream position for suspend Error: {err}"
                );
                std::time::Duration::from_secs(0)
            }
        };
        SuspendedTrack {
            stream_data: self.stream.clone(),
            position: pos,
        }
    }

    pub fn new(track_handle: TrackHandle, stream: Arc<StreamData>) -> Self {
        Self {
            handle: track_handle,
            stream,
        }
    }
}

#[derive(Clone, Debug)]
pub struct QueuedTrack {
    pub user: UserId,
    pub stream: Arc<StreamData>,
}

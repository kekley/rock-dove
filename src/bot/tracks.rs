use std::sync::Arc;

use serenity::all::UserId;
use songbird::{error::ControlError, tracks::TrackHandle};

use crate::bot::guild_context::StreamData;

#[derive(Debug, Clone)]
pub struct PlayingTrack {
    pub handle: TrackHandle,
    pub stream: Arc<StreamData>,
}

impl PlayingTrack {
    pub fn pause(&mut self) -> Result<(), ControlError> {
        self.handle.pause()
    }

    pub fn resume(&mut self) -> Result<(), ControlError> {
        self.handle.play()
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
    pub audio: Arc<StreamData>,
}

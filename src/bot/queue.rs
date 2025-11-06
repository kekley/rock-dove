use std::collections::VecDeque;

use rand::{rng, seq::SliceRandom};

use crate::bot::tracks::QueuedTrack;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoopMode {
    Track,
    Queue,
    #[default]
    Off,
}

pub enum RemoveMode {
    FromUser,
    At,
    Until,
    Past,
}

#[derive(Debug, Clone, Default)]
pub struct PlaybackQueue {
    data: VecDeque<QueuedTrack>,
    queue_position: usize,
}

impl PlaybackQueue {
    pub fn add_to_back(&mut self, track: QueuedTrack) {
        self.data.push_back(track);
    }
    pub fn clear(&mut self) {
        self.data.clear();
        self.queue_position = 0;
    }
    pub fn next_track(&mut self) -> Option<QueuedTrack> {
        if let Some(track) = self.data.get(self.queue_position) {
            self.queue_position += 1;
            Some(track.clone())
        } else {
            None
        }
    }
    pub fn shuffle(&mut self) {
        self.queue_position = 0;
        let mut rng = rng();
        let slice = self.data.make_contiguous();
        slice.shuffle(&mut rng);
    }
    fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

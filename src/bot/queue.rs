use std::{
    collections::VecDeque,
    fmt::{Debug, Display},
    ops::RangeBounds,
};

use rand::{rng, seq::SliceRandom};
use serenity::all::UserId;

use crate::bot::tracks::QueuedTrack;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LoopMode {
    Single,
    Queue,
    #[default]
    Off,
}

impl Display for LoopMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            LoopMode::Single => "single",
            LoopMode::Queue => "queue",
            LoopMode::Off => "off",
        };

        f.write_str(str)?;
        Ok(())
    }
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
    queue_index: usize,
}

impl PlaybackQueue {
    pub fn num_tracks(&self) -> usize {
        self.data.len()
    }

    pub fn tracks_left(&self) -> usize {
        self.data.len() - self.queue_position()
    }

    pub fn queue_position(&self) -> usize {
        self.queue_index
    }

    pub fn remove_tracks_in_range<R>(&mut self, range: R) -> usize
    where
        R: RangeBounds<usize> + Debug,
    {
        let drain = self.data.drain(range);
        drain.len()
    }

    pub fn remove_tracks_from_user(&mut self, user_id: UserId) -> usize {
        let starting_len = self.num_tracks();

        //If we remove any tracks before the current queue position, shift back by the number of
        //tracks removed before the queue position
        let mut shift_amount = 0;
        let mut i = 0;

        self.data.retain(|track| {
            let track_index = i;
            i += 1;
            if track.added_by == user_id {
                if track_index < self.queue_index {
                    shift_amount += 1;
                }
                true
            } else {
                false
            }
        });
        self.queue_index -= shift_amount;

        let ending_len = self.data.len();

        starting_len - ending_len
    }

    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, QueuedTrack> {
        self.data.iter()
    }

    pub fn add_to_back(&mut self, track: QueuedTrack) {
        self.data.push_back(track);
    }

    pub fn clear(&mut self) {
        self.data.clear();
        self.queue_index = 0;
    }

    pub fn next_track(&mut self) -> Option<QueuedTrack> {
        if let Some(track) = self.data.get(self.queue_index) {
            self.queue_index += 1;
            Some(track.clone())
        } else {
            None
        }
    }

    pub fn shuffle(&mut self) {
        self.queue_index = 0;
        let mut rng = rng();
        let slice = self.data.make_contiguous();
        slice.shuffle(&mut rng);
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

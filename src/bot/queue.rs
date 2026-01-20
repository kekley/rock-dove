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
    Track,
    Queue,
    #[default]
    Off,
}
impl LoopMode {
    #[inline]
    pub(crate) fn parse(str: &str) -> Option<Self> {
        match str.trim() {
            "track" | "Track" => Some(LoopMode::Track),
            "queue" | "Queue" => Some(LoopMode::Queue),
            "off" | "Off" => Some(LoopMode::Off),
            _ => None,
        }
    }
}

impl Display for LoopMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            LoopMode::Track => "single",
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
        let queue_pos = self.queue_index;
        let range_start = match range.start_bound() {
            std::ops::Bound::Included(s) => *s,
            std::ops::Bound::Excluded(s) => *s + 1,
            std::ops::Bound::Unbounded => 0,
        };
        let range_end = match range.end_bound() {
            std::ops::Bound::Included(e) => *e + 1,
            std::ops::Bound::Excluded(e) => *e,
            std::ops::Bound::Unbounded => self.data.len(),
        };
        let shift_amt = if range_start <= queue_pos {
            if range_end < queue_pos {
                range_end - range_start
            } else {
                queue_pos - range_start
            }
        } else {
            0
        };

        self.queue_index -= shift_amt;

        let drain = self.data.drain(range);
        drain.len()
    }

    pub fn remove_tracks_from_user(&mut self, user_id: UserId) -> usize {
        let starting_len = self.num_tracks();

        //If we remove any tracks before the current queue position, shift back by the number of
        //tracks removed before the queue position
        let mut shift_amount = 0;
        let mut i = 0;

        dbg!(user_id);
        self.data.retain(|track| {
            dbg!(track.added_by);
            let track_index = i;
            i += 1;
            if track.added_by == user_id {
                if track_index < self.queue_index {
                    shift_amount += 1;
                }
                false
            } else {
                true
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

#[test]
fn remove_shift() {
    let data_len = 10;
    let range = 5..5usize;
    let mut queue_pos = 5;
    let range_start = match range.start_bound() {
        std::ops::Bound::Included(s) => *s,
        std::ops::Bound::Excluded(s) => *s + 1,
        std::ops::Bound::Unbounded => 0,
    };
    let range_end = match range.end_bound() {
        std::ops::Bound::Included(e) => *e + 1,
        std::ops::Bound::Excluded(e) => *e,
        std::ops::Bound::Unbounded => data_len,
    };

    let shift_amt = if range_start <= queue_pos {
        if range_end < queue_pos {
            range_end - range_start
        } else {
            queue_pos - range_start
        }
    } else {
        0
    };
    queue_pos -= shift_amt;
    dbg!(queue_pos);

    dbg!(shift_amt);
}

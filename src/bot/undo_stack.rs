use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

use crate::bot::{guild_context::PlaybackQueue, tracks::SuspendedTrack};

#[derive(Default, Clone)]
pub struct UndoStack {
    buf: ConstGenericRingBuffer<UndoData, 10>,
    redo_index: usize,
}

impl UndoStack {
    pub fn push_undo(&mut self, state: UndoData) {
        //if the undo history forks here, remove all the redo states (we just copy over all the
        //valid undo states)
        if self.redo_index != 0 {
            let mut new_buf = ConstGenericRingBuffer::<UndoData, 10>::new();
            let current_len = self.buf.len();
            let items_to_keep = current_len - self.redo_index;
            for _ in 0..items_to_keep {
                let value = self.buf.dequeue();
                new_buf.enqueue(value.expect("buf should have at least this many elements"));
            }

            self.buf = new_buf;
        }
        //finally push the new undo
        self.buf.enqueue(state);
        self.redo_index = 0;
    }
    pub fn pop_undo(&mut self) -> Option<UndoData> {
        //Subtract from 10 because we want the newest item in the buffer
        let a = self.buf.get(10 - self.redo_index).cloned();
        if a.is_some() {
            self.redo_index += 1;
        }
        a
    }
    pub fn clear(&mut self) {
        self.redo_index = 0;
        self.buf.clear();
    }
}

#[derive(Clone)]
pub struct UndoData {
    pub current_track: Option<SuspendedTrack>,
    pub queue: PlaybackQueue,
}

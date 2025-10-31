use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

use crate::bot::{guild_context::PlaybackQueue, tracks::SuspendedTrack};

#[derive(Default, Clone)]
pub struct UndoStack {
    buf: ConstGenericRingBuffer<UndoData, 10>,
    redo_index: usize,
}

impl UndoStack {
    pub fn push_undo(&mut self, state: UndoData) {
        let _ = self.buf.enqueue(state);
        self.redo_index = 0;
    }
    pub fn pop_undo(&mut self) -> Option<UndoData> {
        let a = self.buf.get(self.redo_index).cloned();
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

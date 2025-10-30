use ringbuffer::{ConstGenericRingBuffer, RingBuffer};

use crate::bot::{guild_context::PlaybackQueue, tracks::SuspendedTrack};

#[derive(Default)]
pub struct UndoStack {
    buf: ConstGenericRingBuffer<UndoData, 10>,
    redo_index: usize,
}

impl UndoStack {
    pub fn undo(&mut self, state: UndoData) {
        let _ = self.buf.enqueue(state);
        self.redo_index = 0;
    }
    pub fn redo(&mut self) -> Option<UndoData> {
        self.buf.get(self.redo_index).cloned()
    }
}

#[derive(Clone)]
pub struct UndoData {
    pub current_track: Option<SuspendedTrack>,
    pub queue: PlaybackQueue,
}

use std::collections::VecDeque;

use crate::bot::guild_context::PlaybackQueue;

const UNDO_CAPACITY: usize = 10;

#[derive(Clone)]
pub struct UndoStack {
    buf: VecDeque<UndoData>,
    redo_index: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        let mut buf = VecDeque::with_capacity(UNDO_CAPACITY);
        buf.push_back(UndoData {
            queue: Default::default(),
        });
        Self { buf, redo_index: 0 }
    }
}

impl UndoStack {
    pub fn push_undo(&mut self, state: UndoData) {
        println!("push undo");

        if self.redo_index != 0 {
            let new_len = self.buf.len() - self.redo_index;
            self.buf.truncate(new_len);
        }

        self.buf.push_back(state);

        if self.buf.len() > UNDO_CAPACITY {
            self.buf.pop_front();
        }

        self.redo_index = 0;
    }

    pub fn pop_undo(&mut self) -> Option<UndoData> {
        println!("pop_undo");

        let max_undo_steps = self.buf.len() - 1;
        if self.redo_index >= max_undo_steps {
            return None;
        }

        self.redo_index += 1;

        let index = self.buf.len() - 1 - self.redo_index;

        self.buf.get(index).cloned()
    }

    pub fn pop_redo(&mut self) -> Option<UndoData> {
        println!("pop redo");

        if self.redo_index == 0 {
            return None;
        }

        self.redo_index -= 1;

        let index = self.buf.len() - 1 - self.redo_index;

        self.buf.get(index).cloned()
    }

    pub fn clear(&mut self) {
        self.redo_index = 0;
        self.buf.clear();
    }
}

#[derive(Clone, Debug)]
pub struct UndoData {
    pub queue: PlaybackQueue,
}

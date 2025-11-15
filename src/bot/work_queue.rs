use std::{collections::VecDeque, pin::Pin};

use tracing::{Level, event};

pub async fn dispatch_work(mut receiver: tokio::sync::mpsc::UnboundedReceiver<WorkQueueMessage>) {
    let mut current_task: Option<tokio::task::JoinHandle<()>> = None;
    let mut queued_tasks = VecDeque::new();
    loop {
        let Some(message) = receiver.recv().await else {
            #[cfg(feature = "tracing")]
            event!(Level::INFO, "Task dispatch channel closed");
            return;
        };

        match message {
            WorkQueueMessage::Task(future) => {
                queued_tasks.push_back(future);
            }
            WorkQueueMessage::Cancellation(_) => {
                if let Some(handle) = current_task.take() {
                    handle.abort();
                }
            }
        }

        if current_task.is_none() && !queued_tasks.is_empty() {
            let popped = queued_tasks
                .pop_front()
                .expect("We just checked if the queue was empty");
            let handle = tokio::spawn(popped);
            current_task = Some(handle);
        }
    }
}

pub enum WorkQueueMessage {
    Task(Pin<Box<dyn Future<Output = ()> + Send>>),
    Cancellation(()),
}

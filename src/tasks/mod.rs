pub mod runner;
use std::{
    sync::{
        mpsc::{Sender},
    },
};

use serde::Serialize;

use crate::application::http::route::ServerContext;

use self::runner::{TaskError, TaskRequest};

/// A manager for asynchronous tasks (worker tasks queued for processing,
/// and returned a result through a callback)
///
/// * A Task may need resources from the server, so we need a way to inject
/// ServerResources / Data / Providers during processing.
///
/// * The task runner will be responible for establishing a worker environment,
/// and running the task inside that environment. e.g. waiting for a worker thread.
///
pub trait Task<T> {
    type Input: Serialize;
    type Output: Serialize;

    fn run(input: Self::Input) -> Self::Output;
}

struct Worker<T> {
    task: T,
}

#[derive(Default)]
// TODO: Shouldn't be able to default - only new (to generate new ID)
pub struct Ticket {
    id: uuid::Uuid,
}

impl Ticket {
    fn new() -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
        }
    }
    pub fn id(&self) -> &uuid::Uuid {
        &self.id
    }
}

enum ScheduleError {
    UnknownTask,
    BadRequest,
    Unknown,
}

#[derive(Clone)]
pub struct TaskScheduler {
    task_queue: Sender<TaskRequest>,
}
impl TaskScheduler {
    pub fn enqueue<T: Serialize + 'static>(
        &mut self,
        request_data: T,
    ) -> Result<Ticket, TaskError> {
        let task_request = TaskRequest::new(request_data);
        let ticket = task_request.get_ticket();
        // First: Store request with status "NOT_STARTED"
        self.task_queue.send(task_request)?;
        // .push_back((TypeId::of::<T>(), serde_json::to_string(&task_request)?));
        log::debug!("[TICKET {}] Sent task to handler", &ticket.id);
        Ok(ticket)
    } // TODO: Return a handle to the job ID
}

// impl

impl From<ServerContext> for TaskScheduler {
    fn from(value: ServerContext) -> Self {
        value.server_data.get::<Self>().unwrap().clone()
    }
}

use std::{
    any::TypeId,
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
    thread::JoinHandle,
};

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::http::route::ServerContext;

use super::{TaskScheduler, Ticket};

#[derive(Default, Eq, PartialEq, PartialOrd, Ord, serde::Deserialize, serde::Serialize)]
pub enum TaskStatus {
    #[default]
    NotStarted,
    Started,
    Completed,
    Canceled,
    Failed,
}

impl<T: ToString> From<T> for TaskError {
    fn from(value: T) -> Self {
        Self::Unknown(value.to_string())
    }
}

// TODO: Move this to the DB for persistence. Need to do some better type/ignore mapping in the ORM first.
pub struct TaskRequest {
    id: uuid::Uuid,
    create_date: NaiveDateTime,
    // status: TaskStatus,
    type_id: TypeId,
    data: Vec<u8>,
}

pub struct TaskContext {}

impl TaskRequest {
    pub fn new<T: Serialize + 'static>(data: T) -> Self {
        Self {
            id: Uuid::new_v4(),
            create_date: chrono::Utc::now().naive_utc(),
            type_id: TypeId::of::<T>(), // TODO: Not a reliable way to pass this around. Need to enum-ize it, or otherwise use a static representation.
            data: serde_json::to_vec(&data).unwrap(),
        }
    }
    pub fn get_ticket(&self) -> Ticket {
        Ticket {
            id: self.id,
        }
    }
    pub fn ticket(&self) -> Ticket {
        self.get_ticket()
    }
}

pub type TaskResult = Result<(), TaskError>;
// struct Runnable = dyn Fn() -> String;
#[derive(Debug)]
pub enum TaskError {
    TaskNotFound,
    Unknown(String),
}

type Handler<Req, Res> = Box<dyn Send + 'static + Sync + Fn(Req) -> Res>;
type TaskHandler = Handler<TaskRequest, TaskResult>;
// Going to use the same pattern as [IntoRouteHandler<F, Tag, IO>].
// Start thinking of an abstraction approach. Maybe add a <Req, Res> to the TaskHandler type?
pub trait IntoTaskHandler<F, Tag, IO> {
    fn into(self) -> TaskHandler;
}

pub struct TaskExecutor {
    handlers: HashMap<TypeId, TaskHandler>,
    task_queue: Receiver<TaskRequest>,
    task_sender: Sender<TaskRequest>,
}
impl Default for TaskExecutor {
    fn default() -> Self {
        let (task_sender, task_queue) = channel::<TaskRequest>();
        Self {
            handlers: Default::default(),
            task_queue,
            task_sender,
        }
    }
}

impl<F, I, O> IntoTaskHandler<F, I, O> for F
where
    F: Send + Sync + 'static + Fn(I) -> O,
    I: FromTaskRequest + Sized + Send,
    O: IntoTaskResult + Sized + Send,
{
    fn into(self) -> Handler<TaskRequest, TaskResult> {
        Box::new(move |req| self(I::from(req)).into())
    }
}

/// From / To implementations - required in order to make this generic over foreign types.
/// Either it can't be done without creating custom versions of the From/To traits,
/// or I'm missing something obvious. I spent a long time trying to navigate this
/// with the compiler.
mod from_to_impl {
    use serde::Deserialize;

    use super::{TaskRequest, TaskResult};

    pub trait FromTaskRequest {
        fn from(req: TaskRequest) -> Self;
    }

    impl<T: for<'a> Deserialize<'a>> FromTaskRequest for T {
        fn from(req: TaskRequest) -> Self {
            serde_json::from_slice(&req.data).unwrap()
        }
    }
    pub trait IntoTaskResult {
        fn into(self) -> TaskResult;
    }
    impl<T> IntoTaskResult for T {
        fn into(self) -> TaskResult {
            Ok(())
        }
    }
}
pub use from_to_impl::*;

// impl<F: From<TaskRequest>> IntoTaskHandler<F,  for F {
//     fn into(self) -> TaskHandler {
//         todo!()
//     }
// }
enum TaskExecutorState {}

impl TaskExecutor {
    pub fn add_handler<F, T, O, Req, Res>(
        &mut self,
        f: F,
    ) where
        F: IntoTaskHandler<F, T, O> + Sized + Sync + Send + 'static + Fn(Req) -> Res,
        Req: 'static,
    {
        log::debug!("Adding Request Type: {:?}", TypeId::of::<Req>());
        self.handlers.insert(TypeId::of::<Req>(), f.into());
    }

    pub fn run_in_new_thread(
        self,
        context: ServerContext,
    ) -> JoinHandle<()> {
        std::thread::spawn(|| self.run())
    }
    pub fn run(self) {
        while let Ok(task) = self.task_queue.recv() {
            let id = task.id.clone();
            match self.handle_task(task) {
                Ok(_) => {
                    log::info!("[TASK {id}] COMPLETED TASK");
                },
                Err(e) => {
                    log::error!("[TASK {id}] Error while processing task: {:?}", e);
                },
            };
        }
    }
    pub fn scheduler(&self) -> TaskScheduler {
        TaskScheduler {
            task_queue: self.task_sender.clone(),
        }
    }
    fn handle_task(
        &self,
        task: TaskRequest,
    ) -> TaskResult {
        log::debug!("Retrieving Request Type: {:?}", &task.type_id);
        let handler = self.handlers.get(&task.type_id).ok_or(TaskError::TaskNotFound)?;
        handler(task)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn handles_tasks_in_thread() {
        todo!()
    }
}

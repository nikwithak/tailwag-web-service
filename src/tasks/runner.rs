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

type Handler<Req, Ctx, Res> = Box<dyn Send + 'static + Sync + Fn(Req, Ctx) -> Res>;
type TaskHandler = Handler<TaskRequest, ServerContext, TaskResult>;
// Going to use the same pattern as [IntoRouteHandler<F, Tag, IO>].
// Start thinking of an abstraction approach. Maybe add a <Req, Res> to the TaskHandler type?
pub trait IntoTaskHandler<F, Tag, Req> {
    fn into(self) -> TaskHandler;
}

pub struct TaskExecutor {
    is_running: bool,
    handlers: HashMap<TypeId, TaskHandler>,
    task_queue: Receiver<TaskRequest>,
    task_sender: Sender<TaskRequest>,
}

impl Default for TaskExecutor {
    fn default() -> Self {
        let (task_sender, task_queue) = channel::<TaskRequest>();
        Self {
            is_running: false,
            handlers: Default::default(),
            task_queue,
            task_sender,
        }
    }
}

// impl<F, I, O> IntoTaskHandler<F, I, O> for F
// where
//     F: Send + Sync + 'static + Fn(I, O) -> O,
//     I: FromTaskRequest + Sized + Send,
//     O: IntoTaskResult + Sized + Send,
// {
//     fn into(self) -> Handler<TaskRequest, ServerContext, TaskResult> {
//         Box::new(move |req, ctx| self(I::from(req)).into())
//     }
// }

struct Tag;
impl<F, I, C, O> IntoTaskHandler<F, (I, C), O> for F
where
    F: Send + Sync + 'static + Fn(I, C) -> O,
    I: FromTaskRequest + Sized + Send + 'static,
    O: IntoTaskResult + Sized + Send + 'static,
    C: From<ServerContext> + Sized + Send + 'static,
{
    fn into(self) -> Handler<TaskRequest, ServerContext, TaskResult> {
        Box::new(move |req, ctx| self(I::from(req), C::from(ctx)).into())
    }
}

// macro_rules! generate_trait_impl {
//     (R1, $($context_id:ident),*) => {
//         // impl<F, I, $($context_id,)* O, Fut>
//         //     IntoTaskHandler<F, (Fut, $($context_id,)*), (($($context_id),*), I, (O, Fut))> for F
//         // where
//         //     F: Fn(I, $($context_id),*) -> Fut + Send + Copy + 'static + Sync,
//         //     I: FromTaskRequest + Sized + 'static,
//         //     $($context_id: From<ServerContext> + Sized + 'static,)*
//         //     O: IntoTaskResult + Sized + Send + 'static,
//         //     Fut: std::future::Future<Output = O> + 'static + Send,
//         // {
//         //     fn into(self) -> TaskHandler {
//         //         TaskHandler {
//         //             handler: Box::new(move |req, ctx| {
//         //                 Box::pin(async move {
//         //                     self(I::from(req), $($context_id::from(ctx.clone())),*)
//         //                         .await
//         //                         .into_response()
//         //                 })
//         //             }),
//         //         }
//         //     }
//         // }

//         impl<F, I, $($context_id,)* O>
//             IntoTaskHandler<F, ($($context_id,)*), (($($context_id),*), I, (O))> for F
//         where
//             F: Fn(I, $($context_id),*) -> O + Send + Copy + 'static + Sync,
//             I: FromTaskRequest + Sized + 'static,
//             $($context_id: From<ServerContext> + Sized + 'static,)*
//             O: IntoTaskResult + Sized + Send + 'static,
//         {
//             fn into(self) -> TaskHandler {
//                 TaskHandler {
//                     handler: Box::new(move |req, ctx| {
//                         Box::pin(async move {
//                             self(I::from(req), $($context_id::from(ctx.clone())),*)
//                                 .into_response()
//                         })
//                     }),
//                 }
//             }
//         }
//     };
// }
// generate_trait_impl!(R1, C1);

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

enum Signal {
    Kill,
}

impl TaskExecutor {
    pub fn add_handler<F, T, O, Req, Ctx>(
        &mut self,
        f: F,
    ) where
        F: IntoTaskHandler<F, T, O> + Sized + Sync + Send + 'static + Fn(Req, Ctx) -> O,
        Req: 'static,
    {
        log::debug!("Adding Request Type: {:?}", TypeId::of::<Req>());
        self.handlers.insert(TypeId::of::<Req>(), f.into());
    }

    pub fn run_in_new_thread(
        self,
        context: ServerContext,
    ) -> JoinHandle<()> {
        std::thread::spawn(move || self.run(context))
    }
    pub fn run(
        self,
        context: ServerContext,
    ) {
        while let Ok(task) = self.task_queue.recv() {
            let id = task.id;
            if TypeId::of::<Signal>() == task.type_id {
                // Any "Signal" is treated as kill for time being
                break;
            }
            match self.handle_task(task, context.clone()) {
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
        context: ServerContext,
    ) -> TaskResult {
        log::debug!("Retrieving Request Type: {:?}", &task.type_id);
        let handler = self.handlers.get(&task.type_id).ok_or(TaskError::TaskNotFound)?;
        handler(task, context)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn handles_tasks_in_thread() {
        todo!()
    }
}

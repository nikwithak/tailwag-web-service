use serde::{Deserialize, Serialize};

use crate::{
    application::WebServiceBuilder,
    tasks::{runner::TaskError, TaskScheduler, Ticket},
};

#[derive(Serialize, Deserialize, Debug)]
pub struct SendEmailEvent {
    pub subject: String,
    pub body: String,
    pub recipient: String,
}

pub async fn send_email(event: SendEmailEvent) {
    let SendEmailEvent {
        subject,
        body,
        recipient,
    } = event;
    let client = tailwag_utils::email::sendgrid::SendGridEmailClient::from_env().unwrap();
    client.send_email(&recipient, &subject, &body).await.unwrap();
}

trait Locked {}
#[allow(private_bounds)]
pub trait WithEmailQueueTask
where
    Self: Locked,
{
    fn with_email_queue_task(self) -> Self;
}
impl Locked for WebServiceBuilder {}
impl WithEmailQueueTask for WebServiceBuilder {
    fn with_email_queue_task(self) -> Self {
        self.with_task(send_email)
    }
}

#[allow(private_bounds)]
pub trait SendEmail
where
    Self: Locked,
{
    fn send_email(
        &mut self,
        subject: String,
        body: String,
        recipient: String,
    ) -> Result<Ticket, TaskError>;
}
impl Locked for TaskScheduler {}
impl SendEmail for TaskScheduler {
    fn send_email(
        &mut self,
        subject: String,
        body: String,
        recipient: String,
    ) -> Result<Ticket, TaskError> {
        self.enqueue(SendEmailEvent {
            subject,
            body,
            recipient,
        })
    }
}

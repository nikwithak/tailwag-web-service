use std::collections::BTreeSet;

use tailwag_web_service::{
    application::http::{
        response::HttpResponse,
        route::{HttpBody, IntoResponse, Response},
    },
    auth::gateway,
};
use uuid::Uuid;

mod tailwag {
    pub use tailwag_forms as forms;
    pub use tailwag_macros as macros;
    pub use tailwag_orm as orm;
    pub use tailwag_web_service as web;
}

#[derive(
    Clone,
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    sqlx::FromRow,
    tailwag::macros::GetTableDefinition,
    tailwag::macros::Insertable,
    tailwag::macros::Updateable,
    tailwag::macros::Deleteable,
    tailwag::macros::Filterable,
    tailwag::macros::BuildRoutes,
    tailwag::macros::Id,
    tailwag::macros::Display,
    tailwag::forms::macros::GetForm,
)]
#[actions(display)]
pub struct Event {
    id: Uuid,
    start_time: chrono::NaiveDateTime,
    // end_time: chrono::NaiveDateTime,
    name: String,
    #[no_filter]
    description: Option<String>,
    // attendees: Vec<String>,
}

pub struct EventGroup {
    id: Uuid,
    events: BTreeSet<Event>,
}

#[tokio::main]
async fn main() {
    tailwag_web_service::application::WebService::builder("My Events Service")
        .with_before(gateway::AuthorizationGateway)
        .post_public("login", gateway::login)
        .post_public("register", gateway::register)
        .with_resource::<Event>()
        .build_service()
        .run()
        .await
        .unwrap();
}

async fn display(id: String) -> Response {
    let html_template = include_bytes!("form.html");

    let mut res = Response::ok();
    res.body = html_template.to_vec();
    res
}

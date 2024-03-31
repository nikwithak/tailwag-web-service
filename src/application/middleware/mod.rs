// TODO

// Middleware to include as defaults:
// -- Logging,
// -- Detect / Block abusers
// -- Rate Limiting
// -- API Key
// -- Content-Size checking
// -- AuthZ

use std::pin::Pin;

use super::http::route::{Context, IntoResponse, Request, Response};

pub enum MiddlewareResult {
    Continue(Request, Context),
    Respond(Response),
}

pub struct Middleware {
    pub handle_request: Box<
        dyn Send
            + Sync
            + Fn(
                Request,
                Context,
                // Box<dyn FnOnce(Request, Context) -> Response>,
            ) -> Pin<Box<dyn std::future::Future<Output = MiddlewareResult>>>,
    >,
}

impl Into<MiddlewareResult> for Response {
    fn into(self) -> MiddlewareResult {
        MiddlewareResult::Respond(self)
    }
}

impl<T: IntoResponse> From<Option<T>> for MiddlewareResult {
    fn from(t: Option<T>) -> Self {
        match t {
            Some(t) => t.into_response().into(),
            None => MiddlewareResult::Respond(Response::not_found()),
        }
    }
}

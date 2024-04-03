pub mod cors;
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

type MiddlewareHandler = Box<
    dyn Send
        + Sync
        + Fn(
            Request,
            Context,
            // Box<dyn FnOnce(Request, Context) -> Response>,
        ) -> Pin<Box<dyn std::future::Future<Output = MiddlewareResult>>>,
>;
pub struct Middleware {
    pub handle_request: MiddlewareHandler,
}

impl From<Response> for MiddlewareResult {
    fn from(val: Response) -> Self {
        MiddlewareResult::Respond(val)
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

impl From<(Request, Context)> for MiddlewareResult {
    fn from(val: (Request, Context)) -> Self {
        let (req, ctx): (Request, Context) = val;
        MiddlewareResult::Continue(req, ctx)
    }
}

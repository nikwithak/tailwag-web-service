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

use super::http::route::{IntoResponse, Request, RequestContext, Response};

pub enum MiddlewareResult {
    Continue(Request, RequestContext),
    Respond(Response),
}

type MiddlewareHandler = Box<
    dyn Send
        + Sync
        + Fn(
            Request,
            RequestContext,
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

impl From<(Request, RequestContext)> for MiddlewareResult {
    fn from(val: (Request, RequestContext)) -> Self {
        let (req, ctx): (Request, RequestContext) = val;
        MiddlewareResult::Continue(req, ctx)
    }
}

// TODO

// Middleware to include as defaults:
// -- Logging,
// -- Detect / Block abusers
// -- Rate Limiting
// -- API Key
// -- Content-Size checking
// -- AuthZ

use std::pin::Pin;

use super::http::route::{Context, Request, Response};

pub enum MiddlewareResult {
    Continue(Request, Context),
    ShortCircuit(Response),
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

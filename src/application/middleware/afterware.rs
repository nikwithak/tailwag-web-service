// TODO

// Middleware to include as defaults:
// -- Logging,
// -- Detect / Block abusers
// -- Rate Limiting
// -- API Key
// -- Content-Size checking
// -- AuthZ

use std::pin::Pin;

use crate::application::http::route::{IntoResponse, Request, RequestContext, Response};

type AfterwareHandler = Box<
    dyn Send
        + Sync
        + Fn(
            Response,
            RequestContext,
            // Box<dyn FnOnce(Request, Context) -> Response>,
        ) -> Pin<Box<dyn std::future::Future<Output = (Response, RequestContext)>>>,
>;
pub struct Afterware {
    pub handle_request: AfterwareHandler,
}

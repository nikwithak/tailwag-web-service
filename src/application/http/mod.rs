pub mod header;
pub mod middleware;
pub mod request;
pub mod response;
pub mod routes;
pub mod state;

use serde::Serialize;

use self::{request::HttpRequest, response::HttpResponse};
// use tailwag_macros::Deref;

pub struct HttpHeader {
    name: String,
    data: String,
}

pub trait HttpMiddleware {
    fn before_request(request: HttpRequest) -> HttpRequest {
        todo!();
        request
    }
    fn after_request(response: HttpResponse) -> HttpResponse {
        todo!();
        response
    }
}

// TODO: Betterize this
pub trait ToJsonString {
    fn to_json_string(&self) -> String;
}

impl<T: Serialize> ToJsonString for T {
    fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap() // TODO: Un-unwrap this
    }
}

pub mod headers;
pub mod multipart;
pub mod request;
pub mod response;
pub mod route;
pub mod into_route_handler;

use serde::Serialize;

use self::{request::HttpRequest, response::HttpResponse};
// use tailwag_macros::Deref;

#[allow(dead_code)]
pub struct HttpHeader {
    name: String,
    data: String,
}

pub trait HttpMiddleware {
    fn before_request(_request: HttpRequest) -> HttpRequest {
        todo!()
    }
    fn after_request(_response: HttpResponse) -> HttpResponse {
        todo!()
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

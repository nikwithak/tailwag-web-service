pub mod middleware;
pub mod request;
pub mod response;
pub mod routes;

use std::marker::PhantomData;

use serde::Serialize;
use tailwag_macros::Deref;

use self::{response::HttpResponse, request::HttpRequest};
// use tailwag_macros::Deref;

pub struct HttpHeader {
    name: String,
    data: String,
}

pub trait HttpMiddleware {
    fn before_request(request: HttpRequest) -> HttpRequest {
        request
    }
    fn after_request(response: HttpResponse) -> HttpResponse {
        response
    }
}

pub trait ToJsonString {
    fn to_json_string(&self) -> String;
}

impl<T: Serialize> ToJsonString for T {
    fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap() // TODO: Un-unwrap this
    }
}

pub mod headers;
pub mod into_route_handler;
pub mod multipart;
pub mod response;
pub mod route;

use serde::Serialize;

use self::response::HttpResponse;
// use tailwag_macros::Deref;

#[allow(dead_code)]
pub struct HttpHeader {
    name: String,
    data: String,
}

// TODO: Betterize this
pub trait ToJsonString {
    fn to_json_string_unsafe(&self) -> String;
}

impl<T: Serialize> ToJsonString for T {
    fn to_json_string_unsafe(&self) -> String {
        serde_json::to_string(self).unwrap() // TODO: Un-unwrap this
    }
}

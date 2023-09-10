use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use tailwag_macros::Deref;
// use tailwag_macros::Deref;

#[derive(Deref)]
pub struct RoutePath {
    path: String,
}

pub struct HttpHeader {
    name: String,
    data: String,
}

pub struct HttpResponse {
    body: HttpResponseBody,
}

type HttpResponseBody = String;
type HttpRequestBody = String;
pub struct HttpRequest {
    body: HttpRequestBody,
    method: HttpMethod,
    headers: HttpHeader,
    // .. add here as needed
}

pub trait HttpMiddleware {
    fn before_request(request: HttpRequest) -> HttpRequest {
        request
    }
    fn after_request(response: HttpResponse) -> HttpResponse {
        response
    }
}

pub trait HttpRequestHandler<S> {
    fn handle_request(
        &self,
        request: HttpRequest,
    ) -> HttpResponse;
}

pub trait ToJsonString {
    fn to_json_string(&self) -> String;
}

impl<T: Serialize> ToJsonString for T {
    fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap() // TODO: Un-unwrap this
    }
}

// TODO: Finish this - the whole mod is kinda WIP right now.
impl<Function, Req, Res> HttpRequestHandler<Req> for Function
where
    Function: Fn(Req) -> Res,
    Req: Send + From<String>,
    Res: ToJsonString, // Really just make
{
    fn handle_request(
        &self,
        request: HttpRequest,
    ) -> HttpResponse {
        let request_body: Req = request.body.into();
        let response = self(request_body);
        HttpResponse {
            body: response.to_json_string(),
        }
    }
}

impl RoutePath {
    // TODO: Impl the RoutePath / ValidatedString trait
    pub fn new<T: Into<String>>(path: T) -> Result<Self, String> {
        // TODO: Verify route allowed syntax
        Ok(Self {
            path: path.into(),
        })
    }

    // Creates a new RoutePath with the given path - Panics if path is invalid.
    pub fn new_unchecked<T: Into<String>>(path: T) -> Self {
        Self::new(path).expect("Path input is not valid")
    }
}

pub enum HttpMethod {
    Get,
    Post,
    Patch,
    Put,
    Delete,
    // Options,
    // ..
    // TODO: Add the rest (no pun intended)
}

pub struct HttpRoute<T, S>
where
    T: HttpRequestHandler<S>,
{
    path: RoutePath,
    method: HttpMethod,
    handler: T,
    // policy: HttpRoutePolicy,
    _s: PhantomData<S>,
}

macro_rules! build_route_fn {
    ($method:ident, $enum:path) => {
        pub fn $method(
            path: RoutePath,
            handler: T,
        ) -> Self {
            Self {
                path,
                method: $enum,
                handler,
                _s: Default::default(),
            }
        }
    };
}

impl<T: HttpRequestHandler<R>, R> HttpRoute<T, R>
where
    T: HttpRequestHandler<R>,
    R: Send + From<String>,
{
    build_route_fn!(get, HttpMethod::Get);
    build_route_fn!(post, HttpMethod::Post);
    build_route_fn!(put, HttpMethod::Put);
    build_route_fn!(delete, HttpMethod::Delete);
    build_route_fn!(patch, HttpMethod::Patch);
}

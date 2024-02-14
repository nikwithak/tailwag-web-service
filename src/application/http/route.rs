use regex::Regex;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::{
    cell::OnceCell,
    collections::HashMap,
    io::{BufRead, Read},
    marker::PhantomData,
    net::TcpStream,
    ops::Deref,
    pin::Pin,
};
use tailwag_orm::{
    data_definition::exp_data_system::DataSystem,
    data_manager::{traits::DataProvider, PostgresDataProvider},
    queries::Insertable,
};

use crate::application::http::headers::Headers;

type RoutePath = String;

// TODO: This is to replace "RoutePath"
enum RoutePathE {
    Static(String),
    Param(String),
}
impl From<&str> for RoutePathE {
    fn from(value: &str) -> Self {
        todo!()
    }
}

#[allow(unused)]
pub struct Route {
    path: RoutePath,
    handlers: HashMap<HttpMethod, Box<RouteHandler>>,
    children: HashMap<RoutePath, Box<Route>>,
    // TODO: Should do one of htese merkel tree things / Trie
}
impl std::fmt::Debug for Route {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        f.debug_struct("Route")
            .field("path", &self.path)
            .field("handlers", &self.handlers.keys())
            .field("children", &self.children)
            .finish()
    }
}

const REGEX: OnceCell<Regex> = OnceCell::new();
impl Route {
    pub fn find_handler(
        &self,
        path: &RoutePath,
        method: &HttpMethod,
    ) -> Option<&RouteHandler> {
        let mut route = Some(dbg!(self));

        let segments = path.split("/");
        for segment in segments {
            // If empty segment, then we aren't routing anywhere - keep the current segment.
            if !segment.is_empty() {
                // TODO: Better way to split/parse the route string, instead of stripping and readding the /
                route = route
                    .and_then(|route| route.children.get(&format!("{}", segment)).map(|r| &**r));

                if let Some(route) = &route {
                    println!("SEGMENT: {} ROUTE: {}", &segment, route.path);
                } else {
                    println!("SEGMENT: {} NOT FOUND", &segment);
                }
            }
        }

        // if path == "" {
        //     return self.handlers.get(&method);
        // }
        // let binding = REGEX;
        // let regex = binding.get_or_init(|| {
        //     Regex::new(r"^(/[a-zA-Z0-9_-]*)+$").expect("Failed to compile route-parsing regex.")
        // });
        // let path_segments = regex.captures(&path)?;
        // for segment in path_segments.iter() {}
        // let mut route = self;

        route.and_then(|r| r.handlers.get(method).map(|handler| &**handler))
    }
}

impl Default for Route {
    fn default() -> Self {
        Self {
            path: "/".into(),
            handlers: Default::default(),
            children: Default::default(),
        }
    }
}
macro_rules! impl_method {
    ($method:ident:$variant:ident) => {
        pub fn $method<F, I, O>(
            mut self,
            handler: impl IntoRouteHandler<F, I, O>,
        ) -> Self {
            self.handlers.insert(HttpMethod::$variant, Box::new(handler.into()));
            self
        }
    };
}
impl Route {
    impl_method!(get:Get);
    impl_method!(post:Post);
    impl_method!(delete:Delete);
    impl_method!(patch:Patch);
}

#[allow(unused)]
impl Route {
    // TODO: Impl the RoutePath / ValidatedString trait
    pub fn new(path: &str) -> Result<Self, String> {
        // TODO: Verify route allowed syntax
        Ok(Self {
            path: path.to_string(),
            handlers: Default::default(),
            children: Default::default(),
        })
    }

    /// Creates a new RoutePath with the given path.
    /// Panics if the path is invalid.
    pub fn new_unchecked(path: &str) -> Self {
        Self::new(path).expect("Path input is not valid")
    }

    pub fn with_route(
        mut self,
        path: RoutePath,
        route: Route,
    ) -> Self {
        self.route(path, route);
        self
    }

    pub fn route(
        &mut self,
        path: RoutePath,
        route: Route,
    ) {
        self.children.insert(path, Box::new(route));
    }
}

#[derive(Eq, PartialEq, Hash, Debug)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
    Patch,
}

impl TryFrom<&str> for HttpMethod {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        type E = HttpMethod;
        let method = match value.to_uppercase().as_str() {
            "GET" => E::Get,
            "POST" => E::Post,
            "PATCH" => E::Patch,
            "DELETE" => E::Delete,
            _ => Err(format!("Unsupported HTTP method: {}", value))?,
        };
        Ok(method)
    }
}

#[derive(Eq, PartialEq, Hash, Clone)]
#[repr(usize)]
#[derive(Debug)]
pub enum HttpStatus {
    Ok = 200,
    Accepted = 201,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    IAmATeapot = 418,
    InternalServerError = 503,
}

pub struct RouteHandler {
    handler: Box<
        dyn Send
            + Sync
            + 'static
            + Fn(
                Request,
                Context,
            ) -> Pin<Box<dyn Send + 'static + std::future::Future<Output = Response>>>,
    >,
}
impl RouteHandler {
    pub async fn call(
        &self,
        request: Request,
        context: Context,
    ) -> Response {
        (self.handler)(request, context).await
    }
}

#[derive(Debug)]
pub enum HttpVersion {
    V1_1,
    V2_0,
    V3_0,
}
impl TryFrom<&str> for HttpVersion {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        type E = HttpVersion;

        Ok(match value {
            "HTTP/1.1" => E::V1_1,
            "HTTP/2.0" => E::V2_0,
            "HTTP/3.0" => E::V3_0,
            _ => Err(format!("Unsupported HTTP version: {}", value))?,
        })
    }
}
impl Deref for HttpVersion {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            HttpVersion::V1_1 => "HTTP/1.1",
            HttpVersion::V2_0 => "HTTP/2.0",
            HttpVersion::V3_0 => "HTTP/3.0",
        }
    }
}

#[derive(Debug)]
pub struct Request {
    // TODO: The rest of this
    // TODO: This could be a u8 in the future, sinc eit won't always be text.
    pub method: HttpMethod,
    pub path: String,
    pub http_version: HttpVersion,
    pub headers: Headers,
    pub body: HttpBody,
}

impl<T: for<'a> Deserialize<'a>> FromRequest for T {
    fn from(req: Request) -> Self {
        // TODO: Return this as a Result so we can route based on it later
        // ^^^ that didn't work,
        serde_json::from_slice(&req.body.bytes).unwrap()
    }
}

impl Request {
    fn parse(raw_request: &TcpStream) -> Self {
        todo!()
    }
}

// Trying to find a way to use both a request AND the context / state
// pub struct ContextualRequest {
//     request: Request,
//     context: Context,
// }

// pub trait FromContextualRequest {
//     fn from(ctx_req: ContextualRequest) -> Self;
// }

// impl<T: FromRequest> FromContextualRequest for T {
//     fn from(ctx_req: ContextualRequest) -> Self {
//         <Self as FromRequest>::from(ctx_req.request)
//     }
// }

#[derive(Debug)]
pub struct HttpBody {
    pub bytes: Vec<u8>,
}

impl TryFrom<&std::net::TcpStream> for Request {
    fn try_from(stream: &std::net::TcpStream) -> Result<Self, Self::Error> {
        let mut stream = std::io::BufReader::new(stream);
        let mut headers = Headers::default();
        let mut line = String::new();
        stream.read_line(&mut line)?;
        let mut routing_line = line.split_whitespace();
        let (Some(method), Some(path), Some(http_version)) =
            (routing_line.next(), routing_line.next(), routing_line.next())
        else {
            todo!("Handle the 400 BAD REQUEST case")
        };
        let mut line = String::new();

        // 2 is the size of the line break indicating end of headers, and is too small to fit anything else in a well-formed request.
        while stream.read_line(&mut line)? > 2 {
            println!("LINE: {}", &line);
            headers.insert_parsed(&line)?;
            println!("{}", &line);
            line = String::new();
        }
        dbg!(&headers);
        let content_length: usize =
            headers.get("content-length").and_then(|c| c.parse().ok()).unwrap_or(0);

        println!("Content length is {}", content_length);
        let mut body = HttpBody {
            bytes: Vec::<u8>::with_capacity(content_length),
        };
        // TODO: Need to limit request size.
        // Also need to handle laaaarge files, e.g. pass the stream itself instead of just the content.
        if content_length > 0 {
            println!("Reading {} bytes", content_length);
            let mut buf = vec![0; content_length];
            stream.read_exact(&mut buf)?;
            body.bytes = buf;
            println!("READ: {}", String::from_utf8(body.bytes.clone()).unwrap());
        }

        dbg!(Ok(Request {
            method: method.try_into()?,
            path: path.to_string(), // TODO: Validate it
            http_version: http_version.try_into()?,
            headers,
            body,
        }))
    }

    type Error = crate::Error;
}

#[derive(Debug)]
pub struct Response {
    pub http_version: HttpVersion,
    pub status: HttpStatus,
    pub headers: Headers,
    pub body: Vec<u8>,
}

impl Response {
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            format!("{} {} {}", (&self.http_version as &str), self.status.clone() as usize, "OK")
                .as_bytes(),
        );
        bytes.extend_from_slice(b"\r\n");
        for header in (&self.headers) as &HashMap<_, _> {
            bytes.extend_from_slice(format!("{}: {}", &header.0, &header.1).as_bytes());
            bytes.extend_from_slice(b"\r\n");
        }
        bytes.extend_from_slice(format!("{}: {}", "Content-Length", self.body.len()).as_bytes());
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(b"\r\n");
        bytes.extend_from_slice(&self.body);

        bytes
    }
}

#[derive(Clone)]
pub struct Context {
    pub data_providers: DataSystem,
}

impl FromContext for DataSystem {
    fn from(ctx: Context) -> Self {
        ctx.data_providers.clone()
    }
}

impl<T: Insertable + Clone + Send + 'static> FromContext for PostgresDataProvider<T> {
    fn from(ctx: Context) -> Self {
        ctx.data_providers
            .get::<T>()
            .clone()
            .expect("Attempted to use DataProvider that does not exist.")
    }
}

/// This is used as an intermediary step to get from a generic Fn to a RouteHandler.
/// `impl<F,I,O> IntoRouteHandler for F where F: Fn(I) -> O` is not allowed, since
/// the generics I and O have to be a part of F.
/// I'm really working some wonky type magic here
///
/// If you want to create a custom handler type, implement [IntoTypedRouteHandler] or [IntoRouteHandler]
pub struct TypedRouteHandler<F, I, O>
where
    F: Fn(I) -> O,
    I: FromRequest + Sized,
    O: IntoResponse + Sized,
{
    handler_fn: Box<F>,
    _i: PhantomData<I>,
    _o: PhantomData<O>,
}

pub trait IntoResponse
where
    Self: Sized,
{
    fn into_response(self) -> Response;
}

impl<T: Serialize> IntoResponse for T {
    fn into_response(self) -> Response {
        match serde_json::to_string(&self) {
            Ok(body) => crate::application::http::route::Response {
                status: crate::application::http::route::HttpStatus::Ok,
                headers: Headers::from(vec![]),
                http_version: crate::application::http::route::HttpVersion::V1_1,
                body: body.into_bytes(),
            },
            Err(_) => crate::application::http::route::Response {
                status: crate::application::http::route::HttpStatus::InternalServerError,
                headers: Headers::from(vec![]),
                http_version: crate::application::http::route::HttpVersion::V1_1,
                body: Vec::new(),
            },
        }
    }
}

pub trait FromRequest
where
    Self: Sized,
{
    fn from(req: Request) -> Self;
}

pub trait FromContext
where
    Self: Sized,
{
    fn from(ctx: Context) -> Self;
}

pub struct Nothing;
impl<F, O> IntoRouteHandler<F, Nothing, O> for F
where
    F: Fn() -> O + Send + Copy + 'static + Sync,
    O: IntoResponse + Sized + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, ctx| Box::pin(async move { self().into_response() })),
        }
    }
}

pub trait IntoRouteHandler<F, I, O> {
    fn into(self) -> RouteHandler;
}
impl<F, I, O> IntoRouteHandler<F, I, O> for F
where
    F: Fn(I) -> Pin<Box<dyn Send + 'static + std::future::Future<Output = O>>>
        + Send
        + Sync
        + Copy
        + 'static,
    I: FromRequest + Sized + Send,
    O: IntoResponse + Sized + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, ctx| {
                Box::pin(async move { self(I::from(req)).await.into_response() })
            }),
        }
    }
}

pub struct Nothing2;

impl<F, I, C, O, Fut> IntoRouteHandler<F, Nothing2, (C, I, (O, Fut))> for F
where
    F: Fn(I, C) -> Fut + Send + Copy + 'static + Sync,
    I: FromRequest + Sized + 'static,
    C: FromContext + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
    Fut: Future<Output = O> + 'static + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, ctx| {
                Box::pin(async move { self(I::from(req), C::from(ctx)).await.into_response() })
            }),
        }
    }
}

pub struct Nothing3;
impl<F, C, O, Fut> IntoRouteHandler<F, Nothing3, (C, O, Fut)> for F
where
    F: Fn(C) -> Fut + Send + Copy + 'static + Sync,
    C: FromContext + Sized + 'static,
    O: IntoResponse + Sized + Send + 'static,
    Fut: Future<Output = O> + 'static + Send,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |req, ctx| {
                Box::pin(async move { self(C::from(ctx)).await.into_response() })
            }),
        }
    }
}

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{
    any::TypeId,
    collections::HashMap,
    fmt::Display,
    io::{BufRead, Read},
    ops::Deref,
    pin::Pin,
};
use tailwag_macros::Deref;
use tailwag_orm::{
    data_definition::exp_data_system::DataSystem, data_manager::PostgresDataProvider,
    queries::Insertable,
};
use tailwag_utils::{
    data_strutures::hashmap_utils::{GetOrDefault, GetOrInsert},
    types::generic_type_map::{CloneableTypeInstanceMap, TypeInstanceMap},
};

use crate::application::http::{headers::Headers, multipart::parse_multipart_request};

pub type RoutePath = String;

enum RoutePolicy {
    Public,
    Protected,
}

/// An extractor used to specify that the inputs should come from a PathVariable.
/// This wrapper is needed to prevent the extraction coming from the RequestBody
pub struct PathVariable<T>(pub T);
impl<T> Deref for PathVariable<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.0
    }
}

/// Alias type for a path variable string extractor
pub type PathString = PathVariable<String>;
/// Alias type for path variable extractor shorthand
pub type PathVar<T> = PathVariable<T>;

#[derive(Deref)]
struct PoliciedRouteHandler {
    #[deref]
    handler: Box<RouteHandler>,
    _policy: RoutePolicy,
}

// I'll probably end up ditching this for something... better.
// I haven't quite wrapped my head around how I want to structure policies.
impl PoliciedRouteHandler {
    pub fn public(handler: RouteHandler) -> Self {
        Self {
            handler: Box::new(handler),
            _policy: RoutePolicy::Public,
        }
    }
    pub fn protected(handler: RouteHandler) -> Self {
        Self {
            handler: Box::new(handler),
            _policy: RoutePolicy::Protected,
        }
    }
}

#[allow(unused)]
#[derive(Default)]
pub struct Route {
    handlers: HashMap<HttpMethod, PoliciedRouteHandler>,
    children: HashMap<RoutePath, Route>,
    dynamic_child: Option<(String, Box<Route>)>, // String = the given name
}
impl std::fmt::Debug for Route {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        f.debug_struct("Route")
            .field("handlers", &self.handlers.keys())
            .field("children", &self.children)
            .finish()
    }
}

impl Route {
    pub async fn handle(
        &self,
        mut request: Request,
        context: RequestContext,
    ) -> Response {
        let path = &request.path;
        // let mut route = Some(self);
        // for segment in path.split('/').filter(|s| !s.is_empty()) {
        //     route = route.and_then(|r| r.children.get(&segment.to_string()));
        // }
        let mut route = self;
        for segment in path.split('/').filter(|s| !s.is_empty()) {
            match route.children.get(&segment.to_string()) {
                Some(new_route) => route = new_route,
                None => {
                    if let Some((name, new_route)) = &route.dynamic_child {
                        println!("{}: route found", name);
                        request.path_params.push(segment.to_owned());
                        route = new_route
                    } else {
                        return Default::default();
                    }
                },
            }
        }

        if let Some(future) = route
            .handlers
            .get(&request.method)
            .map(|handler| handler.call(request, context))
        {
            future.await
        } else {
            Response::default()
        }
    }
}

macro_rules! impl_method {
    ($method:ident:$variant:ident) => {
        pub fn $method<F, I, O>(
            mut self,
            handler: impl IntoRouteHandler<F, I, O>,
        ) -> Self {
            self.with_handler(HttpMethod::$variant, "", handler.into())
        }
    };
    ($method:ident:$variant:ident, public) => {
        pub fn $method<F, I, O>(
            self,
            handler: impl IntoRouteHandler<F, I, O>,
        ) -> Self {
            self.with_handler(HttpMethod::$variant, "", handler.into())
        }
    };
}
impl Route {
    impl_method!(get:Get);
    impl_method!(post:Post);
    impl_method!(delete:Delete);
    impl_method!(patch:Patch);
    impl_method!(get_public:Get, public);
    impl_method!(post_public:Post, public);
    impl_method!(delete_public:Delete, public);
    impl_method!(patch_public:Patch, public);
}

#[allow(unused)]
impl Route {
    // TODO: Impl the RoutePath / ValidatedString trait
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_handler<F, I, O>(
        mut self,
        method: HttpMethod,
        path: &str,
        handler: impl IntoRouteHandler<F, I, O>,
    ) -> Self {
        self.add_handler(method, path, handler);
        self
    }
    pub fn add_handler<F, I, O>(
        &mut self,
        method: HttpMethod,
        path: &str,
        handler: impl IntoRouteHandler<F, I, O>,
    ) {
        let parts = path.split('/');
        let mut route = self;
        for part in parts.filter(|p| !p.is_empty()) {
            if Regex::new("^\\{[a-zA-Z0-9_-]*\\}$") // /route/{this_part_gets_matched}/
                .expect("Something wrong with regex")
                .is_match(part)
            {
                // For now, only one dynamic route is allowed per route.
                // Reduces ambiguity (and lets me get away with this silly hack)
                // In the future, I'll add some regex support (maybe?) or at least a basic extraction syntax
                if route.dynamic_child.is_none() {
                    route.dynamic_child =
                        Some(("unnamed".to_string(), Box::new(Default::default())));
                }
                let (name, child_route) =
                    route.dynamic_child.as_mut().expect("Missing route that was just added.");
                route = &mut *child_route;
            } else if Regex::new("^[a-zA-Z0-9_]+$").expect("Regex is invalid").is_match(part) {
                route = route.children.get_or_default_mut(&part.to_string());
            } else if part.matches("...").next().is_some() {
                // TODO:
                todo!("... = \"the rest of the input\"");
            } else {
                println!("part: {} doesn't match regex", &part);
                panic!("Invalid route");
            }
        }

        // TODO: NEed to indicate that it's extracting something.
        // Static vs Dynamic routes
        if route
            .handlers
            .insert(method, PoliciedRouteHandler::public(handler.into()))
            .is_some()
        {
            panic!("This route already has a handler");
        }
    }

    pub fn with_route(
        mut self,
        path: RoutePath,
        route: Route,
    ) -> Self {
        self.route(path, route);
        self
    }

    /// Nest another Route inside this one.
    /// ```
    pub fn route(
        &mut self,
        path: impl Into<RoutePath>,
        route: Route,
    ) {
        // 1. Parse path (as str) into the parts
        // 2. Go down the request tree to find the Route node where this route would be handled, creating new Route nodes when necessary.
        // 3. Add the Route handler
        self.children.insert(path.into(), route);
    }
}

#[derive(Eq, PartialEq, Hash, Debug)]
pub enum HttpMethod {
    Get,
    Post,
    Delete,
    Patch,
    Options,
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
            "OPTIONS" => E::Options,
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

impl Display for HttpStatus {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let var_name = format!(
            "{} {}",
            self.clone() as usize,
            match self {
                HttpStatus::Ok => "OK",
                HttpStatus::Accepted => "Accepted",
                HttpStatus::BadRequest => "Bad Request",
                HttpStatus::Unauthorized => "Unauthorized",
                HttpStatus::Forbidden => "Forbidden",
                HttpStatus::NotFound => "Not Found",
                HttpStatus::IAmATeapot => "I Am A Teapot",
                HttpStatus::InternalServerError => "Internal Server Error",
                // _ => "Unknown",
            }
        );
        f.write_str(&var_name)
    }
}

type RouteHandlerInner = Box<
    dyn Send
        + Sync
        + 'static
        + Fn(
            Request,
            ServerContext,
        ) -> Pin<Box<dyn Send + 'static + std::future::Future<Output = Response>>>,
>;
pub struct RouteHandler {
    handler: RouteHandlerInner,
}
impl RouteHandler {
    pub async fn call(
        &self,
        request: Request,
        context: RequestContext,
    ) -> Response {
        (self.handler)(request, context.server_context).await
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
    pub path_params: Vec<String>,
    pub http_version: HttpVersion,
    pub headers: Headers,
    pub body: HttpBody,
}

impl<T: for<'a> Deserialize<'a>> FromRequest for T {
    fn from(req: Request) -> Self {
        // TODO: Return this as a Result so we can route based on it later
        // ^^^ that didn't work,
        match &req.body {
            HttpBody::Json(body) => serde_json::from_slice(body.as_bytes()).unwrap(),
            HttpBody::Bytes(_) => todo!(),
            HttpBody::Stream(_) => todo!(),
            HttpBody::Multipart(_) => todo!(),
            // HttpBody::Plaintext(String) => todo!(),
            HttpBody::None => todo!(),
            HttpBody::Html(_) => todo!(),
        }
    }
}

#[derive(Debug)]
pub enum HttpBody {
    // pub bytes: Vec<u8>,
    Json(String),
    Bytes(Vec<u8>),
    Multipart(Vec<MultipartPart>),
    Stream(std::io::BufReader<std::net::TcpStream>),
    Html(String),
    None,
}

const DEFAULT_CONTENT_TYPE: &str = "application/json";

impl TryFrom<&std::net::TcpStream> for Request {
    fn try_from(stream: &std::net::TcpStream) -> Result<Self, Self::Error> {
        let mut stream = std::io::BufReader::new(stream);

        let mut line = String::new();
        stream.read_line(&mut line)?;
        let mut routing_line = line.split_whitespace();
        let (Some(method), Some(path), Some(http_version)) =
            (routing_line.next(), routing_line.next(), routing_line.next())
        else {
            Err(Self::Error::BadRequest(format!("Invalid routing header found: {}", &line)))?
        };
        let headers = Headers::parse_headers(&mut stream)?;
        let content_length: usize =
            headers.get("content-length").and_then(|c| c.parse().ok()).unwrap_or(0);
        let content_type_header =
            headers.get("content-type").map(|s| s.as_str()).unwrap_or(DEFAULT_CONTENT_TYPE);

        let (content_type, content_type_params) =
            content_type_header.split_once(';').unwrap_or((content_type_header, ""));
        type E = HttpBody;

        let body = if content_length > 0 {
            let mut bytes = vec![0; content_length];
            log::info!("Reading {} bytes", content_length);
            stream.read_exact(&mut bytes)?;
            match content_type.to_lowercase().as_str() {
                "application/json" => {
                    dbg!(E::Json(String::from_utf8(bytes)?))
                },
                "multipart/form-data" => parse_multipart_request(content_type_params, bytes)?,
                _ => todo!("Unsupported content-type"),
            }
        } else {
            HttpBody::None
        };

        Ok(Request {
            method: method.try_into()?,
            path: path.to_string(), // TODO: Validate it
            path_params: Default::default(),
            http_version: http_version.try_into()?,
            headers,
            body,
        })
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

macro_rules! default_response {
    ($fnname:ident, $enumname:ident) => {
        /// Creates a default request with the given status codes.
        pub fn $fnname() -> Self {
            Self {
                http_version: HttpVersion::V1_1,
                status: HttpStatus::$enumname,
                headers: Headers::default(),
                body: Vec::new(),
            }
            .with_header("access-control-allow-origin", "http://localhost:3000")
        }
    };
}

/// Factory Methods
impl Response {
    default_response!(bad_request, BadRequest);
    default_response!(not_found, NotFound);
    default_response!(internal_server_error, InternalServerError);
    default_response!(unauthorized, Unauthorized);
    default_response!(ok, Ok);
}

impl Default for Response {
    fn default() -> Self {
        Self::not_found()
    }
}

impl Response {
    pub fn with_body(
        mut self,
        bytes: Vec<u8>,
    ) -> Self {
        self.body = bytes;
        self
    }
    pub fn with_header(
        mut self,
        name: impl Into<String>,
        val: impl Into<String>,
    ) -> Self {
        self.headers.insert(name.into().to_lowercase(), val.into());
        self
    }
}

impl Response {
    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(
            format!("{} {}", (&self.http_version as &str), self.status.to_string()).as_bytes(),
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

#[derive(Clone, Deref)]
pub struct ServerContext {
    pub data_providers: DataSystem,
}

impl ServerContext {
    pub fn from(data_providers: DataSystem) -> Self {
        ServerContext {
            data_providers,
        }
    }
}

#[derive(Deref)]
pub struct RequestContext {
    #[deref]
    server_context: ServerContext,
    request_data: TypeInstanceMap,
}

impl RequestContext {
    pub fn from_server_context(server_context: ServerContext) -> Self {
        Self {
            server_context,
            request_data: Default::default(),
        }
    }
}

impl RequestContext {
    /// Gets the requested data type from the request context, if it exists.
    /// Useful for maintaining data state between beforeware & afterware (e.g. wrapping with middleware)
    pub fn get_request_data<T: 'static + Sync + Send>(&self) -> Option<&T> {
        self.request_data.get::<T>()
    }
    pub fn get_request_data_mut<T: 'static + Sync + Send>(&mut self) -> Option<&mut T> {
        self.request_data.get_mut::<T>()
    }
    pub fn insert_request_data<T: 'static + Sync + Send>(
        &mut self,
        t: T,
    ) {
        self.request_data.insert(t);
    }
}

impl Into<ServerContext> for &RequestContext {
    fn into(self) -> ServerContext {
        self.server_context.clone()
    }
}

impl From<ServerContext> for DataSystem {
    fn from(ctx: ServerContext) -> Self {
        ctx.data_providers.clone()
    }
}

impl<T: Insertable + Clone + Send + Sync + 'static> From<ServerContext>
    for PostgresDataProvider<T>
{
    fn from(ctx: ServerContext) -> Self {
        ctx.data_providers
            .get::<T>()
            .clone()
            .expect("Attempted to use DataProvider that does not exist.")
    }
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
                headers: Headers::from(vec![("Content-Type", "application/json")]), // TODO: Make this dynamic
                http_version: crate::application::http::route::HttpVersion::V1_1,
                body: body.into_bytes(),
            }
            // TODO: HAAAAAAACK need to fix middleware so I can actually wrap this properly
            .with_header("access-control-allow-origin", "http://localhost:3000")
            .with_header("access-control-allow-credentials", "true"),
            Err(_) => crate::application::http::route::Response {
                status: crate::application::http::route::HttpStatus::InternalServerError,
                headers: Headers::from(vec![]),
                http_version: crate::application::http::route::HttpVersion::V1_1,
                body: Vec::new(),
            },
        }
    }
}

impl IntoResponse for Response {
    fn into_response(self) -> Response {
        self
    }
}

pub trait FromRequest
where
    Self: Sized,
{
    fn from(req: Request) -> Self;
}

impl FromRequest for Request {
    fn from(req: Request) -> Self {
        req
    }
}
impl<T> FromRequest for PathVariable<T>
where
    T: From<String>,
{
    fn from(req: Request) -> Self {
        // TODO: Not robust
        PathVariable(req.path_params.first().unwrap().to_owned().into())
    }
}

pub struct RouteArgsNone;
impl<F, O> IntoRouteHandler<F, RouteArgsNone, O> for F
where
    F: Fn() -> O + Send + Copy + 'static + Sync,
    O: IntoResponse + Sized + Send + Sync,
{
    fn into(self) -> RouteHandler {
        RouteHandler {
            handler: Box::new(move |_, _| Box::pin(async move { self().into_response() })),
        }
    }
}

/// This mod contains all the logic / trait impls for automatically converting functions into a RouteHandler.
/// The goal is to enable ergonomic and intuitive route handling.
/// At the moment, it supports exactly one Request input type, and one that reads from the Context (which currently only contains data providers).
mod into_route_handler {
    use std::pin::Pin;

    use std::future::Future;

    use super::{FromRequest, IntoResponse, RouteHandler, ServerContext};

    impl IntoRouteHandler<(), (), ()> for RouteHandler {
        fn into(self) -> RouteHandler {
            self
        }
    }

    pub struct RouteArgsStaticRequest;

    /// The generics are merely here for tagging / distinguishing implementations.
    /// F: represents the function signature for the different implementations. This is the one that really matters.
    /// Tag: Merely tag structs, to disambiguate implementations when there is trait overlap.
    /// IO: The function input / output types. They must be a part of the trait declaration in order to be used in the impl,
    ///     i.e. these exist only so that we can use them to define `F`
    pub trait IntoRouteHandler<F, Tag, IO> {
        fn into(self) -> RouteHandler;
    }
    impl<F, I, O> IntoRouteHandler<F, RouteArgsStaticRequest, (I, O)> for F
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
                handler: Box::new(move |req, _ctx| {
                    Box::pin(async move { self(I::from(req)).await.into_response() })
                }),
            }
        }
    }

    // Let's break that down.

    // We define the following Generics:
    //     GENERICS: F, I, an O.

    // For a breakdown of the

    // F is the function type, and the main type that we are implementing the IntoRH for. I is the input type of F, O is the output type of F.

    // We define IntoRouteHanlder in terms of F (The function type we want to use as a handler),
    // Tag (RouteArgsStaticRequest, in this case), and the input/output types.

    // So... why do we need so many generics, to all do the same thing? We need I/O to be generic in order to define them
    // in terms of the FromRequest and IntoResponse trait.
    // Because of a restriction imposed by the compiler, we can't use a generic in the implemnetation unles sit's also a generic in
    // either the trait, or the struct implementing the trait.

    // Unfortunately... this doesn't flow into the `where` clauses - which is to say, we can't do a generic implemnetation *over* a generic struct. That's why we have
    // to define IntoRouteHandler (and not getting the benefits of Into<RouteHandler>)

    // So where does RouteArgsStaticRequest (the Tag) come in? The tag was to get around a restriction of multiple implementations
    // using the same or similar generics, which adds ambiguity. As the developer, I can reasonably assume
    // that the implementations are unique, at least for my specific use cases, but the compiler doesn't
    // know how to cope with the other cases, since it is possible for the generics of `F(I) -> O` to overlap both.

    // The Tag ensures that the compiler will magically choose the right implementation, if only one applies.
    // In the event that a class overlaps in actual usage, then the user will have to disambiuate using these tags.

    // As a user, you shouldn't have to ever worry or care about these weird generics - this
    // abstraction is intended ot make coding with this library more ergonomic over closures
    // and simple function types. This explanation is only here for those curious enough to look under the hood.

    pub struct RouteArgsNoContextAsync;
    impl<F, I, O, Fut> IntoRouteHandler<F, RouteArgsNoContextAsync, (F, I, (O, Fut))> for F
    where
        F: Fn(I) -> Fut + Send + Copy + 'static + Sync,
        I: FromRequest + Sized + 'static,
        O: IntoResponse + Sized + Send + 'static,
        Fut: Future<Output = O> + 'static + Send,
    {
        fn into(self) -> RouteHandler {
            RouteHandler {
                handler: Box::new(move |req, _ctx| {
                    Box::pin(async move { self(I::from(req)).await.into_response() })
                }),
            }
        }
    }

    pub struct RouteArgsNoContextSync;
    impl<F, I, O> IntoRouteHandler<F, RouteArgsNoContextSync, (F, I, O)> for F
    where
        F: Send + Sync + Copy + 'static + Fn(I) -> O,
        I: FromRequest + Sized + 'static,
        O: IntoResponse + Sized + Send + 'static,
    {
        fn into(self) -> RouteHandler {
            RouteHandler {
                handler: Box::new(move |req, _ctx| {
                    Box::pin(async move { self(I::from(req)).into_response() })
                }),
            }
        }
    }

    pub struct RouteArgsRequestContextAsync;
    impl<F, I, C, O, Fut> IntoRouteHandler<F, RouteArgsRequestContextAsync, (C, I, (O, Fut))> for F
    where
        F: Fn(I, C) -> Fut + Send + Copy + 'static + Sync,
        I: FromRequest + Sized + 'static,
        C: From<ServerContext> + Sized + 'static,
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
    pub struct RouteArgsRequestContextSync;
    impl<F, I, C, O> IntoRouteHandler<F, RouteArgsRequestContextSync, (C, I, O)> for F
    where
        F: Send + Copy + 'static + Sync + Fn(I, C) -> O,
        I: FromRequest + Sized + 'static,
        C: From<ServerContext> + Sized + 'static,
        O: IntoResponse + Sized + Send + 'static,
    {
        fn into(self) -> RouteHandler {
            RouteHandler {
                handler: Box::new(move |req, ctx| {
                    Box::pin(async move { self(I::from(req), C::from(ctx)).into_response() })
                }),
            }
        }
    }

    pub struct Nothing3Async;
    impl<F, C, O, Fut> IntoRouteHandler<F, Nothing3Async, (C, O, Fut)> for F
    where
        F: Fn(C) -> Fut + Send + Copy + 'static + Sync,
        C: From<ServerContext> + Sized + 'static,
        O: IntoResponse + Sized + Send + 'static,
        Fut: Future<Output = O> + 'static + Send,
    {
        fn into(self) -> RouteHandler {
            RouteHandler {
                handler: Box::new(move |_req, ctx| {
                    Box::pin(async move { self(C::from(ctx)).await.into_response() })
                }),
            }
        }
    }
    pub struct Nothing3Sync;
    impl<F, C, O, Fut> IntoRouteHandler<F, Nothing3Sync, (C, O, Fut)> for F
    where
        F: Fn(C) -> O + Send + Copy + 'static + Sync,
        C: From<ServerContext> + Sized + 'static,
        O: IntoResponse + Sized + Send + 'static,
    {
        fn into(self) -> RouteHandler {
            RouteHandler {
                handler: Box::new(move |_req, ctx| {
                    Box::pin(async move { self(C::from(ctx)).into_response() })
                }),
            }
        }
    }
}
pub use into_route_handler::*;

use super::multipart::MultipartPart;

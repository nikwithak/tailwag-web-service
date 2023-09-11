use std::marker::PhantomData;

use tailwag_macros::Deref;

use super::request::{HttpMethod, HttpRequestHandler};

#[derive(Deref)]
pub struct RoutePath {
    path: String,
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

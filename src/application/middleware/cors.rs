use std::fmt::Display;

use crate::application::http::route::{Context, HttpMethod, Request, Response};

use super::{Middleware, MiddlewareResult};

#[derive(Default)]
pub struct CorsMiddleware {
    policy: CorsPolicy,
}

#[derive(Default)]
enum CorsPolicy {
    Wildcard,
    #[default]
    EchoOrigin,
    AllowOnly(Vec<String>),
}

mod headers {
    // Ref: https://fetch.spec.whatwg.org/#http-requests 3.2.2 & 3.2.3
    use std::fmt::Display;

    pub enum CorsHeader {
        Origin,
        AccessControlRequestMethod,
        AccessControlRequestHeaders,
        AccessControlAllowOrigin,
        AccessControlAllowCredentials,
        AccessControlAllowMethods,
        AccessControlAllowHeaders,
        AccessControlMaxAge,
        AccessControlExposeHeaders,
    }

    impl Display for CorsHeader {
        fn fmt(
            &self,
            f: &mut std::fmt::Formatter<'_>,
        ) -> std::fmt::Result {
            f.write_str(match self {
                CorsHeader::Origin => "Origin",
                CorsHeader::AccessControlRequestMethod => "Access-Control-Request-Method",
                CorsHeader::AccessControlRequestHeaders => "Access-Control-Request-Headers",
                CorsHeader::AccessControlAllowOrigin => "Access-Control-Allow-Origin",
                CorsHeader::AccessControlAllowCredentials => "Access-Control-Allow-Credentials",
                CorsHeader::AccessControlAllowMethods => "Access-Control-Allow-Methods",
                CorsHeader::AccessControlAllowHeaders => "Access-Control-Allow-Headers",
                CorsHeader::AccessControlMaxAge => "Access-Control-Max-Age",
                CorsHeader::AccessControlExposeHeaders => "Access-Control-Expose-Headers",
            })
        }
    }

    impl From<CorsHeader> for String {
        fn from(val: CorsHeader) -> Self {
            val.to_string()
        }
    }
}
pub use headers::*;

impl From<CorsMiddleware> for Middleware {
    fn from(val: CorsMiddleware) -> Self {
        Self {
            handle_request: Box::new(|req, ctx| Box::pin(async move { handle_cors(req, ctx) })),
        }
    }
}

/// Implements the CORS specification, as defined by the fetch spec.
/// It is not currently fully compliant.
/// Goal is "common case" with some flexibility
/// ref: https://fetch.spec.whatwg.org/#cors-protocol
fn handle_cors(
    req: Request,
    ctx: Context,
) -> MiddlewareResult {
    // TODO: Not the proper way to check, but "good enough" to unblock.
    // THIS CORS MIDDLEWARE IS WIDE OPEN RIGHT NOW don't rely on it for actual security
    if matches!(req.method, HttpMethod::Options) {
        MiddlewareResult::Respond(
            Response::ok()
                .with_header(
                    CorsHeader::AccessControlAllowOrigin.to_string(),
                    req.headers
                        .get(&CorsHeader::Origin.to_string())
                        .map_or("null".to_string(), |s| s.to_owned()),
                )
                .with_header(CorsHeader::AccessControlAllowCredentials, "true")
                .with_header(CorsHeader::AccessControlAllowHeaders, "origin, content-type, accept"),
        )
    } else {
        MiddlewareResult::Continue(req, ctx)
    }
}

fn handle_preflight() {}

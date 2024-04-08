use std::{collections::HashMap, fmt::Display, pin::Pin};

use crate::application::http::{
    headers::Headers,
    route::{HttpMethod, Request, RequestContext, Response, ServerContext},
};

use super::{Beforeware, MiddlewareResult};

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

#[derive(Debug)]
pub struct CorsHeaders(pub HashMap<String, String>);

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
use tailwag_macros::Deref;

impl From<CorsMiddleware> for Beforeware {
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
    mut ctx: RequestContext,
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
    } else if req.headers.contains_key("origin") {
        // Stash them for later - afterware will have to pull this
        ctx.insert_request_data(CorsHeaders(
            vec![
                (
                    CorsHeader::AccessControlAllowOrigin.to_string(),
                    req.headers
                        .get(&CorsHeader::Origin.to_string())
                        .map_or("null".to_string(), |s| s.to_owned()),
                ),
                (CorsHeader::AccessControlAllowCredentials.to_string(), "true".to_string()),
                (
                    CorsHeader::AccessControlAllowHeaders.to_string(),
                    "origin, content-type, accept".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
        ));
        MiddlewareResult::Continue(req, ctx)
    } else {
        MiddlewareResult::Continue(req, ctx)
    }
}

pub fn inject_cors_headers(
    mut res: Response,
    ctx: RequestContext,
) -> Pin<Box<dyn std::future::Future<Output = (Response, RequestContext)>>> {
    Box::pin(async move {
        if let Some(cors_headers) =
            ctx.get_request_data::<crate::application::middleware::cors::CorsHeaders>()
        {
            for (name, val) in &cors_headers.0 {
                res = res.with_header(name.clone(), val.clone());
            }
        }
        (res, ctx)
    })
}

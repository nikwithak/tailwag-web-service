use axum::Router;
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

use crate::auth::gateway::{self, Account, Session};

use super::WebService;

pub async fn run_axum(
    svc: WebService,
    pool: sqlx::Pool<sqlx::Postgres>,
) {
    let router = Router::new();
    let bind_addr = format!("{}:{}", &svc.config.socket_addr, svc.config.port);
    let data_providers = &svc.resources.connect(pool).await;

    // router = router.with_state(*data_providers);

    axum::Server::bind(
        &bind_addr
            .parse()
            .unwrap_or_else(|_| panic!("Unable to bind to address: {}", &bind_addr)),
    )
    .serve(
        router
            // .route(
            //     "/brewery/{id]/fetch",
            //     post(
            //         temp_webhook
            //     ),
            // )
            // TODO: Refactor this out - all the auth code here for now
            .layer(axum::middleware::from_fn_with_state(
                data_providers.get::<Session>().unwrap(),
                crate::auth::gateway::AuthorizationGateway::add_session_to_request,
            ))
            .nest(
                // TODO: This needs to be an entire closed system - no other part of the system should have direct read/write access to `Account` or `Session` (except by calling this service)
                // That, or Read-Only Access. How could I enforce this typeily?
                "/auth",
                axum::Router::new()
                    .route("/login", axum::routing::post(gateway::login))
                    .route("/register", axum::routing::post(gateway::register))
                    .with_state((
                        data_providers.get::<Account>().unwrap(),
                        data_providers.get::<Session>().unwrap(),
                    )),
            )
            // .nest("/brewery", Brewery::build_routes)
            // Allow CORS - TODO: Fix this to be configurable / safe. Currently allows everything
            .layer(
                CorsLayer::new()
                    .allow_methods([
                        hyper::Method::GET,
                        hyper::Method::POST,
                        hyper::Method::PATCH,
                        hyper::Method::OPTIONS,
                        hyper::Method::DELETE,
                    ])
                    .allow_origin(AllowOrigin::predicate(|origin, _| {
                        origin.as_bytes().starts_with(b"http://localhost")
                    }))
                    .allow_credentials(true)
                    .allow_headers(AllowHeaders::mirror_request()),
            )
            // .with_state(*data_providers)
            .into_make_service(),
    )
    .await
    .unwrap();
}

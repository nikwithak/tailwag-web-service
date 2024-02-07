use std::{
    any::{Any, TypeId},
    collections::HashMap,
    future::Future,
    marker::PhantomData,
    path::Path,
    pin::Pin,
};

use axum::{
    extract::State,
    response::{IntoResponse, Response},
    routing::post,
    Form, Router,
};
use env_logger::Env;
use hyper::StatusCode;
use log;
use sqlx::{
    postgres::{PgPoolOptions, PgRow},
    FromRow, Pool, Postgres,
};
use tailwag_forms::GetForm;
use tailwag_orm::{
    data_manager::{
        traits::{get_data_provider::ConnectPostgres, DataProvider},
        GetTableDefinition, PostgresDataProvider,
    },
    queries::{Deleteable, Insertable, Updateable},
};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

use crate::{
    auth::gateway::{self, Account, Session},
    traits::rest_api::BuildRoutes,
};

use super::stats::RunResult;

#[derive(thiserror::Error, Debug)]
pub enum ApplicationError {
    #[error("Something went wrong.")]
    Error,
}

// TODO: Separate definition from config
#[derive(Debug)]
pub struct WebServiceConfig {
    application_name: String,
    socket_addr: String,
    port: i32,
    migrate_on_init: bool,
    database_conn_string: String,
}

trait WebServiceState {}
pub struct Ready;
pub struct Building;
impl WebServiceState for Ready {}
impl WebServiceState for Building {}

type ResourceConfigurator = (
    // TODO: Actually struct this out the next time you touch it.
    Box<
        dyn Fn(
                Pool<Postgres>,
            ) -> (
                TypeId,
                Box<dyn Any + std::marker::Send + 'static>,
                Router,
                Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), String>>>>>,
            ) + Send,
    >,
    String,
);

#[derive(Default)]
pub struct DataProviders {
    map: HashMap<TypeId, Box<dyn Any + Send>>,
}
impl DataProviders {
    pub fn get<T>(&self) -> Option<PostgresDataProvider<T>>
    where
        T: tailwag_orm::queries::Insertable
            + Deleteable
            + tailwag_orm::queries::Updateable
            + Sync
            + Send
            + for<'r> FromRow<'r, PgRow>
            + Clone
            + 'static,
    {
        self.map
            .get(&TypeId::of::<T>())
            .map(|any| {
                (*any)
                    .downcast_ref::<PostgresDataProvider<T>>()
                    .expect("FATAL ERROR: Unable to downcast data provider type. This is either a bug,
                            or you are using this library in a way that is currently unsupported (Avoid calling
                            `put_any_unchecked`). Please file an issue at https://github.com/nikwithak/tailwag")
            })
            .map(|boxed| (*boxed).clone())
    }

    fn put<T>(
        &mut self,
        provider: PostgresDataProvider<T>,
    ) where
        T: Insertable + Send + 'static + Sync,
    {
        self.map.insert(TypeId::of::<T>(), Box::new(provider));
    }

    #[deprecated(note = "
        This function exists only as a workaround to the current approach for building an application.
        Once I've refactored that to be more maintainable and less spaghettiy, this function can be removed.
        Improper use of this function may cause difficulty debugging or other errors down the line.

        Instead, use [DataProviders::put]
    ")]
    fn put_any_unchecked(
        &mut self,
        type_id: TypeId,
        provider: Box<dyn Any + Send>,
    ) {
        self.map.insert(type_id, provider);
    }
}

// TODO: Separate definition from config
#[allow(private_bounds)]
pub struct WebService<State: WebServiceState> {
    config: WebServiceConfig,
    _state: PhantomData<State>,
    /// This should ONLY hold DataProvider<T> types - using Any because there is no
    /// other way I've found to have multiple of the same generic trait but using different
    /// underlying types.
    resource_configurators: Vec<ResourceConfigurator>, // TODO: This should only exist pre-Ready state
    router: Router,
    data_providers: DataProviders,
}

#[cfg(feature = "development")]
impl Default for WebService<Building> {
    fn default() -> Self {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        dotenv::dotenv().ok();
        Self {
            config: WebServiceConfig {
                socket_addr: "127.0.0.1".to_owned(),
                port: 8081,
                application_name: "Tailwag Default Application".into(),
                migrate_on_init: true,
                database_conn_string: "postgres://postgres:postgres@127.0.0.1:5432/postgres".into(),
            },
            router: Router::new(),
            resource_configurators: Vec::new(),
            data_providers: Default::default(),
            _state: PhantomData,
        }
        .with_resource::<Account>()
        .with_resource::<Session>()
    }
}

impl WebService<Building> {
    // Builder functions
    pub fn new(app_name: &str) -> Self {
        let mut builder = Self::default();
        builder.config.application_name = app_name.to_string();
        builder
    }

    pub fn with_resource<T>(mut self) -> Self
    where
        T: BuildRoutes<T>
            + GetTableDefinition
            + tailwag_orm::queries::Insertable
            + 'static
            + Send
            + Clone
            + Sync
            + std::fmt::Debug
            + Unpin
            + Updateable
            + GetForm
            + for<'r> sqlx::FromRow<'r, PgRow>
            + Deleteable,
    {
        self.resource_configurators.push((
            // This closure is run when the application starts to connect all the things
            // TODO: This got really messy with async and closures and generic-over-generics.
            // need to clean this up - try factoring the async out everywhere you can (data provider setup, etc), and
            // we can change the ::with_resource<T>()... to actually create the types.
            Box::new(|pool: Pool<Postgres>| {
                let provider = T::connect_postgres(pool);
                let resource_name = &T::get_table_definition().table_name;
                let mut providers = DataProviders::default();
                providers.put(provider.clone());

                fn save_form_def<F: GetForm>(filepath: &str) -> Result<(), std::io::Error> {
                    let form_def = serde_json::to_string(&F::get_form())?;
                    let dir = Path::new(filepath).parent().unwrap_or(Path::new(filepath));
                    std::fs::create_dir_all(dir).expect("Failed to create directories.");
                    std::fs::write(filepath, form_def.as_bytes())?;
                    Ok(())
                }
                save_form_def::<T>(&format!("./out/forms/{}.json", resource_name))
                    .expect("Failed to save form definition to disk, aborting.");
                (
                    TypeId::of::<T>(),
                    Box::new(provider.clone()),
                    // Uggggh... changing it to accept a generic "DataProviders" kinda screwed this.
                    // I really need to let each type give its "to_service" function and do all the prep
                    // work when it's added.
                    T::build_routes(provider.clone()),
                    // TODO: Document this and maybe macro it down the line
                    Box::new(move || {
                        Box::pin(async move { provider.clone().run_migrations().await })
                    }),
                )
            }),
            format!("/{}", &T::get_table_definition().table_name),
        ));
        self
    }

    pub async fn build_service(mut self) -> WebService<Ready> {
        let mut resources = self.resource_configurators;
        self.resource_configurators = Vec::new();
        let db_pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.config.database_conn_string)
            .await
            .expect("[DATABASE] Unable to obtain connection to database. Is postgres running?");

        while let Some((configure_resource, route_name)) = resources.pop() {
            let (type_id, provider, router, run_migrations) = configure_resource(db_pool.clone());
            // let (type_id, provider, router) = configure_resource(db_pool.clone());
            self.data_providers.put_any_unchecked(type_id, provider);
            if self.config.migrate_on_init {
                run_migrations().await.expect("Failed to run migrations - aborting");
            }
            self.router = self.router.nest(&route_name, router);
        }

        WebService::<Ready> {
            config: self.config,
            resource_configurators: Vec::new(),
            data_providers: self.data_providers,
            router: self.router,
            _state: PhantomData,
        }
    }
}

impl WebService<Ready> {
    /// Wrapper for `self.data_providers.get_data_provider<T>(&self)`
    pub fn get_data_provider<T>(&self) -> Option<PostgresDataProvider<T>>
    where
        T: tailwag_orm::queries::Insertable
            + Deleteable
            + tailwag_orm::queries::Updateable
            + Sync
            + Send
            + for<'r> FromRow<'r, PgRow>
            + Clone
            + 'static,
    {
        self.data_providers.get::<T>()
    }
}

impl WebService<Ready> {
    // TODO: Clean this up a bit
    fn print_welcome_message(&self) {
        log::info!(
            r#"
=============================================
   Starting Web Application {}
=============================================
"#,
            &self.config.application_name,
        );
        log::debug!("CONFIGURED ENVIRONMENT: {:?}", &self.config);
        #[cfg(feature = "development")]
        {
            log::warn!(
                r#"
============================================================================="
    ++ {} IS RUNNING IN DEVELOPMENT MODE ++
    ++      DO NOT USE IN PRODUCTION YET ++
============================================================================="
"#,
                self.config.application_name
            );
        }
    }

    /// Start the Application. By default, starts an HTTP server bound to `127.0.0.1::8081`.
    pub async fn run(self) -> RunResult {
        /////////////////////////////
        // Axum Web implementation //
        /////////////////////////////
        self.print_welcome_message();
        let bind_addr = format!("{}:{}", &self.config.socket_addr, self.config.port);
        log::info!("Starting service on {}", &bind_addr);
        let router = self.router.clone();

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
                    self.get_data_provider::<Session>().unwrap(),
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
                            self.get_data_provider::<Account>().unwrap(),
                            self.get_data_provider::<Session>().unwrap(),
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
                .into_make_service(),
        )
        .await
        .unwrap();

        RunResult::default()
    }
}

use axum::{routing::get, Router};
use log;
use sqlx::postgres::PgPoolOptions;
use sqlx::Postgres;
use tailwag_orm::{migration::*, AsSql}; 
use tower_http::cors::{Any, CorsLayer};


use crate::auth::gateway;

use super::{http::request::HttpRequestHandler, stats::RunResult};

#[derive(Debug)]
// TODO: Separate definition from config
pub struct WebServiceApplication {
    application_name: String,
    pub router: Router,
    socket_addr: String,
    port: i32,
    migrate_on_init: bool,
    database_conn_string: String,
}

// TODO: PReferences for thigns like:
//  * trailling slash / no trailing slash
//  * Plural resource names or singular

pub async fn hello(t: String) -> String {
    format!("Hello, {}", &t)
}

#[cfg(feature = "development")]
impl Default for WebServiceApplication {
    fn default() -> Self {
        let router = Router::new().route("/", get(hello));
        Self {
            socket_addr: "127.0.0.1".to_owned(),
            port: 8081,
            application_name: "Tailwag Default Application".into(),
            migrate_on_init: true,
            database_conn_string: "postgres://postgres:postgres@127.0.0.1:5432/postgres".into(),
            router,
        }
    }
}

/// The base class for building an application. See examples for usage.
impl WebServiceApplication {
    pub fn new(app_name: &str) -> Self {
        Self {
            application_name: app_name.into(),
            ..Default::default()
        }
    }

    // TODO: This is just for dev, refactor once
    pub async fn new_with_auth(app_name: &str) -> Self {
        let mut app = Self::new(app_name);
        // app.add_routes("/account", <crate::middleware::gateway::account as crate::traits::rest_api::BuildRoutes<crate::middleware::gateway::account>>::build_routes(provider).await);
        let session_provider = {
            let provider = 
            tailwag_orm::data_manager::PostgresDataProvider {
                table_definition: <crate::auth::gateway::Session as tailwag_orm::data_manager::GetTableDefinition>::get_table_definition(),
                db_pool: PgPoolOptions::new()
            .max_connections(1)
            // TODO: Unhardcode things
            .connect("postgres://postgres:postgres@127.0.0.1:5432/postgres")
            .await
            .expect("[DATABASE] Unable to obtain connection to database. Is postgres running?")
                ,
                _t: Default::default(),
            };
            // self.add_routes("/session", crate::middleware::gateway::account::build_routes(provider.clone()).await);
            provider.run_migrations().await.expect("Migrations failed. Aborting.");

            provider
        };
        app = app.add_routes("/session", <crate::auth::gateway::Session as crate::traits::rest_api::BuildRoutes<crate::auth::gateway::Session>>::build_routes(session_provider).await);
        app
    }

    // pub fn with_resource<T: BuildRoutes<T>>() -> Self {
    //     self.add_routes('/', T::build_routes());
    //     self
    // }

    #[allow(unused)]
    pub fn add_routes(
        mut self,
        path: &str,
        routes: Router,
    ) -> Self {
        self.router = self.router.nest(path, routes);
        self
    }

    #[allow(unused)]
    pub fn route<S, T: HttpRequestHandler<S>>(
        self,
        path: &str,
        // function: T,
        handler: T,
    ) -> Self {
        self
    }
}

impl WebServiceApplication {
    // TODO: Clean this up a bit
    fn print_welcome_message(&self) {
        log::info!(
            r#"
=============================================
   Starting Web Application {}
=============================================
"#,
            &self.application_name,
        );
        log::debug!("CONFIGURED ENVIRONMENT: {:?}", &self);
        #[cfg(feature = "development")]
        {
            log::warn!(
                r#"
============================================================================="
    ++ {} IS RUNNING IN DEVELOPMENT MODE ++
    ++      DO NOT USE IN PRODUCTION     ++
============================================================================="
"#,
                self.application_name
            );
        }
    }

    /// Start the Application. By default, starts an HTTP server bound to `128.0.0.1::8081`.
    async fn run(self) -> RunResult {
        use env_logger::Env;

        // dotenv::dotenv().expect("Failed to load config from .env"); // Load environment variables.
        // TODO
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();

        // TODO: This doesn't belong here
        let session_provider = {
            let provider = 
            tailwag_orm::data_manager::PostgresDataProvider {
                table_definition: <crate::auth::gateway::Session as tailwag_orm::data_manager::GetTableDefinition>::get_table_definition(),
                db_pool: PgPoolOptions::new()
            .max_connections(1)
            // TODO: Unhardcode things
            .connect("postgres://postgres:postgres@127.0.0.1:5432/postgres")
            .await
            .expect("[DATABASE] Unable to obtain connection to database. Is postgres running?")
                ,
                _t: Default::default(),
            };
            // self.add_routes("/session", crate::middleware::gateway::account::build_routes(provider.clone()).await);

            provider
        };
        // TODO: This doesn't belong here
        let account_provider: tailwag_orm::data_manager::PostgresDataProvider<crate::auth::gateway::Account> = {
            let provider = 
            tailwag_orm::data_manager::PostgresDataProvider {
                table_definition: <crate::auth::gateway::Account as tailwag_orm::data_manager::GetTableDefinition>::get_table_definition(),
                db_pool: PgPoolOptions::new()
            .max_connections(1)
            // TODO: Unhardcode things
            .connect("postgres://postgres:postgres@127.0.0.1:5432/postgres")
            .await
            .expect("[DATABASE] Unable to obtain connection to database. Is postgres running?")
                ,
                _t: Default::default(),
            };
            // self.add_routes("/session", crate::middleware::gateway::account::build_routes(provider.clone()).await);
            let result = provider.run_migrations().await;
            result.expect("Failed to run migrations.");

            provider
        };


        /////////////////////////////
        // Axum Web implementation //
        /////////////////////////////
        self.print_welcome_message();
        let bind_addr = format!("{}:{}", &self.socket_addr, self.port);
        log::info!("Starting service on {}", &bind_addr);
        axum::Server::bind(
            &bind_addr.parse().expect(&format!("Unable to bind to address: {}", &bind_addr)),
        )
        .serve(
            self.router
                // TODO: Refactor this out - all the auth code here for now
                .layer(
                    axum::middleware::from_fn_with_state(
                        session_provider.clone(),
                        crate::auth::gateway::AuthorizationGateway::add_session_to_request,
                    )
                )
                .nest(
                    "/auth",
                    axum::Router::new()
                        .route("/login", axum::routing::post(gateway::login))
                        .route("/register", axum::routing::post(gateway::register))
                        .with_state((account_provider.clone(), session_provider.clone()))
                )
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
                        .allow_origin(Any)
                        .allow_headers(Any),
                )
                .into_make_service(),
        )
        .await
        .unwrap();

        RunResult::default()
    }
}

impl WebServiceApplication {
    #[cfg(feature = "development")]
    pub async fn init_dev() -> Self {
        use env_logger::Env;

        // dotenv::dotenv().expect("Failed to load config from .env"); // Load environment variables.
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        let app = WebServiceApplication::default();
        app
    }

    pub async fn run_app(self) {
        /////////////////////////////////////
        // Boilerplate for DB connectivity //
        /////////////////////////////////////
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&self.database_conn_string)
            .await
            .expect("[DATABASE] Unable to obtain connection to database");

        ////////////////////////////////////////////////////////
        // HANDLES MIGRATIONS - NEED DB DEFINED AT THIS POINT //
        ////////////////////////////////////////////////////////
        // TODO: Migrate this to the DataProvider / DataManager itself.
        // TODO: Macro this?
        if self.migrate_on_init {
            if let Some(migration) = Migration::compare(
                // TODO: Actually do a compare here
                None,
                &tailwag_orm::data_definition::database_definition::DatabaseDefinition::new_unchecked(
                    "postgres",
                )
                .into(),
            ) {
                log::info!("[DATABASE] Running Migrations");
                // TODO: Refactor the migrations to return statements as a vec
                sqlx::query::<Postgres>(&migration.as_sql())
                    .execute(&pool)
                    .await
                    .expect("[DATABASE] Failed to run migrations");

                let migration = Migration::compare(
                    None,
                    &tailwag_orm::data_definition::database_definition::DatabaseDefinition::new(
                        "postgres",
                    )
                    .expect("Failed to initialize database definition")
                    .into(),
                )
                .unwrap();

                // TODO: Refactor the migrations to return statements as a vec
                sqlx::query::<Postgres>(&migration.as_sql())
                    .execute(&pool)
                    .await
                    .expect("[DATABASE] Failed to run migrations");
            } else {
                log::info!("[DATABASE] Database is up-to-data - no migrations needed!")
            }
        } else {
            log::info!("[DATABASE] Skipping Migrations");
            // TODO: Verify that DB is up to date - panic if incompatible with current DB schema
        }

        //////////////////////////////////////////////////////////////////////////////////////////////////////////////
        // Set up Data Types - This part should be as simple as possible, since it's where custom type must change. //
        //////////////////////////////////////////////////////////////////////////////////////////////////////////////
        // Lots of similar TODOs here, guess this is the next spot
        // TODO: Macro this?
        // TODO: Move this to a separate function for easy REMOVAL of template / boilerplate from the files. Clean up visually, make files small & simple <3
        // TODO: Migrate this to a module build_routes() call
        // self.router =Router::new()
        // .route("/", get(data_fetcher::get_food_truck_events))
        // .nest("/food_truck", food_truck::create_routes(pool.clone()))
        // .nest("/brewery", brewery::create_routes(pool.clone()));
        // ;
        let result = self.run().await;
        log::info!("{:?}", result);
    }
}

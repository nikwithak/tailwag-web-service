use axum::{routing::get, Json, Router};
use log;
use sqlx::postgres::PgPoolOptions;
use sqlx::Postgres;
use tailwag_orm::{
    database_definition::database_definition::DatabaseDefinition, migration::*, AsSql,
};

use crate::traits::rest_api::BuildRoutes;

use super::stats::RunResult;

#[derive(Debug)]
// TODO: Separate definition from config
pub struct WebServiceApplication {
    application_name: String,
    router: Router,
    socket_addr: String,
    port: i32,
    migrate_on_init: bool,
    database_conn_string: String,
}

impl From<DatabaseDefinition> for WebServiceApplication {
    fn from(value: DatabaseDefinition) -> Self {
        fn build_router(value: DatabaseDefinition) -> Router {
            for table in &value.tables {
                // table.
            }
            todo!()
        }

        let mut service =
            WebServiceApplication::new(&format!("Generated Tailwag Application {}", value.name));
        // .router(build_router(value));

        // let router = build_router(value);

        service
    }
}

pub async fn hello() -> Json<String> {
    Json("Hello".into())
}

#[cfg(feature = "development")]
impl Default for WebServiceApplication {
    fn default() -> Self {
        Self {
            socket_addr: "127.0.0.1".to_owned(),
            port: 3001,
            application_name: "Tailwag Default Application".into(),
            migrate_on_init: true,
            database_conn_string: "postgres://postgres:postgres@127.0.0.1:5432/postgres".into(),
            router: Router::new().route("/", get(hello)),
        }
    }
}

impl WebServiceApplication {
    pub fn new(application_name: &str) -> Self {
        Self {
            application_name: application_name.into(),
            ..Default::default()
        }
    }

    /// Adds a resource's route to the Path.
    pub fn with_resource<T: BuildRoutes>(
        mut self,
        route: &str,
    ) -> Self {
        self.router = self.router.nest(route, <T as BuildRoutes>::build_routes());
        self
    }
}

impl WebServiceApplication {
    fn print_welcome_message(&self) {
        log::info!("=============================================");
        log::info!("  Starting Web Application {}", &self.application_name);
        log::info!("=============================================");
        log::info!("");
        log::debug!("CONFIGURED ENVIRONMENT: {:?}", &self);
        #[cfg(feature = "development")]
        {
            log::warn!(
                "============================================================================="
            );
            log::warn!("     ++ {} IS RUNNING IN DEVELOPMENT MODE ++", &self.application_name);
            log::warn!("     ++      DO NOT USE IN PRODUCTION     ++",);
            log::warn!(
                "============================================================================="
            );
        }
    }

    /// Start the Application. By default, starts an HTTP server bound to `127.0.0.1::3001`.
    pub async fn run(self) -> RunResult {
        /////////////////////////////
        // Axum Web implementation //
        /////////////////////////////
        self.print_welcome_message();
        let bind_addr = format!("{}:{}", &self.socket_addr, self.port);
        log::info!("Starting service on {}", &bind_addr);
        axum::Server::bind(
            &bind_addr.parse().expect(&format!("Unable to bind to address: {}", &bind_addr)),
        )
        .serve(self.router.into_make_service())
        .await
        .unwrap();

        RunResult::default()
    }
}

impl WebServiceApplication {
    #[cfg(feature = "development")]
    pub async fn init_dev() -> Self {
        use env_logger::Env;

        dotenv::dotenv().expect("Failed to load config from .env"); // Load environment variables.
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
            log::info!("[DATABASE] Running Migrations");
            let migration = Migration::compare(
                None,
                &tailwag_orm::database_definition::database_definition::DatabaseDefinition::new_unchecked(
                    "postgres",
                )
                // .table(FoodTruck::get_table_definition())
                .into(),
            )
            .unwrap();

            // TODO: Refactor the migrations to return statements as a vec
            sqlx::query::<Postgres>(&migration.as_sql())
                .execute(&pool)
                .await
                .expect("[DATABASE] Failed to run migrations");

            let migration = Migration::compare(
                None,
                &tailwag_orm::database_definition::database_definition::DatabaseDefinition::new(
                    "postgres",
                )
                .expect("Failed to initialize database definition")
                // .table(Brewery::get_table_definition())
                .into(),
            )
            .unwrap();

            // TODO: Refactor the migrations to return statements as a vec
            sqlx::query::<Postgres>(&migration.as_sql())
                .execute(&pool)
                .await
                .expect("[DATABASE] Failed to run migrations");
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
        log::info!("{:?}", self.run().await);
    }
}

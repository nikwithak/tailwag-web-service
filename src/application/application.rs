use axum::{routing::get, Router};
use log;
use sqlx::postgres::PgPoolOptions;
use sqlx::Postgres;
use tailwag_orm::data_manager::GetTableDefinition;
use tailwag_orm::{migration::*, AsSql};

#[derive(Debug)]
pub struct WebServiceApplication {
    socket_addr: String,
    port: i32,
    application_name: String,
    migrate_on_init: bool,
    database_conn_string: String,
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
        }
    }
}

impl HelloApplication {
    #[cfg(feature = "development")]
    pub async fn init_dev() -> Self {
        use env_logger::Env;

        dotenv::dotenv().expect("Failed to load config from .env"); // Load environment variables.
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        let app = WebServiceApplication::default();
        app
    }

    fn print_welcome_message(&self) {
        log::info!("=============================================");
        log::info!("  Starting Web Application {}", &self.application_name);
        log::info!("=============================================");

        log::debug!("CONFIGURED ENVIRONMENT: {:?}", &self);
    }

    pub async fn run_app(&self) {
        self.print_welcome_message();

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
                &tailwag::orm::database_definition::database_definition::DatabaseDefinition::new(
                    "postgres",
                )
                .expect("Failed to initialize database definition")
                .table(FoodTruck::get_table_definition())
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
                &tailwag::orm::database_definition::database_definition::DatabaseDefinition::new(
                    "postgres",
                )
                .expect("Failed to initialize database definition")
                .table(Brewery::get_table_definition())
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
        let router = Router::new()
            .route("/", get(data_fetcher::get_food_truck_events))
            .nest("/food_truck", food_truck::create_routes(pool.clone()))
            .nest("/brewery", brewery::create_routes(pool.clone()));

        //////////////////////////////////////////////////////
        // Actually start the server - back to boilerplate! //
        //////////////////////////////////////////////////////
        let bind_addr = format!("{}:{}", &self.socket_addr, self.port);
        log::info!("Starting service on {}", &bind_addr);
        axum::Server::bind(
            &bind_addr.parse().expect(&format!("Unable to bind to address: {}", &bind_addr)),
        )
        .serve(router.into_make_service())
        .await
        .unwrap()
    }
}

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, future::Future, net::TcpListener, pin::Pin};

use crate::application::http::route::RoutePath;
use env_logger::Env;
use log;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPoolOptions, PgRow};
use tailwag_forms::{Form, GetForm};
use tailwag_orm::data_manager::rest_api::Id;
use tailwag_orm::queries::filterable_types::Filterable;
use tailwag_orm::{
    data_definition::{
        exp_data_system::{DataSystem, DataSystemBuilder, UnconnectedDataSystem},
        table::Identifier,
    },
    data_manager::GetTableDefinition,
    queries::{Deleteable, Updateable},
};

use crate::application::http::headers::Headers;
use crate::application::http::route::Context;
// use crate::application::threads::ThreadPool;
use crate::{
    auth::gateway::{Account, Session},
    traits::rest_api::BuildRoutes,
};

use super::http::route::{IntoRouteHandler, Request, Response};
use super::middleware::Middleware;
use super::{http::route::Route, stats::RunResult};

#[derive(thiserror::Error, Debug)]
pub enum ApplicationError {
    #[error("Something went wrong.")]
    Error,
}

// TODO: Separate definition from config
#[derive(Debug)]
#[allow(unused)]
pub struct WebServiceConfig {
    application_name: String,
    socket_addr: String,
    max_threads: usize,
    request_timeout_seconds: u64,
    port: i32,
    migrate_on_init: bool,
    database_conn_string: String,
}
// What if I do something like
// ```rust
// state: AppState<> and anything wrapped in an Arc<T: Clone> is FromAppState<>
// ````

#[allow(private_bounds)]
pub struct WebServiceInner {
    config: WebServiceConfig,
    middleware: Vec<Middleware>,
    routes: Route,
    resources: UnconnectedDataSystem,
    _migrations: Arc<Mutex<MigrationRunners>>, // Wrapped in a Mutex to work around some Arc issues - these only need to be run once.
}

#[derive(tailwag_macros::Deref, Clone)]
pub struct WebService {
    inner: std::sync::Arc<WebServiceInner>,
}

type MigrationRunners = Vec<
    Box<
        dyn FnOnce(DataSystem) -> Pin<Box<dyn Future<Output = Result<(), String>> + Sync + Send>>
            + Send
            + Sync,
    >,
>;

// TODO: Separate definition from config
#[allow(private_bounds)]
pub struct WebServiceBuilder {
    config: WebServiceConfig,
    root_route: Route,
    migrations: MigrationRunners,
    forms: HashMap<Identifier, Form>,
    middleware: Vec<Middleware>,
    resources: DataSystemBuilder,
}

#[cfg(feature = "development")]
impl Default for WebServiceBuilder {
    fn default() -> Self {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        dotenv::dotenv().ok();
        Self {
            config: WebServiceConfig {
                socket_addr: "127.0.0.1".to_owned(),
                port: 8081,
                max_threads: 4,
                application_name: "Tailwag Default Application".into(),
                migrate_on_init: true,
                database_conn_string: "postgres://postgres:postgres@127.0.0.1:5432/postgres".into(),
                request_timeout_seconds: 30,
            },
            resources: DataSystem::builder(),
            middleware: Vec::new(),
            migrations: Vec::new(),
            root_route: Route::default(),
            forms: HashMap::new(),
        }
        // .with_middleware(|| println!("I'm middling the ware!")) // TODO
        .with_resource::<Account>()
        .with_resource::<Session>()
    }
}

macro_rules! build_route_method {
    ($method:ident) => {
        pub fn $method<F, I, O>(
            mut self,
            path: impl Into<RoutePath>,
            handler: impl IntoRouteHandler<F, I, O>,
        ) -> Self {
            // TODO: Refactor the path definitions here
            self.root_route.route(path.into(), Route::new_unchecked("/").$method(handler));
            self
        }
    };
}

impl WebServiceBuilder {
    build_route_method!(get);
    build_route_method!(post);
    build_route_method!(delete);
    build_route_method!(patch);
}

type MiddlewareFunction = dyn Send
    + Sync
    + FnMut(
        Request,
        Context,
        Box<dyn Fn(Request, Context) -> Pin<Box<dyn Future<Output = Response>>>>,
    );

impl WebServiceBuilder {
    // Builder functions
    pub fn new(app_name: &str) -> Self {
        let mut builder = Self::default();
        builder.config.application_name = app_name.to_string();
        builder
    }

    pub fn with_resource<T>(mut self) -> Self
    where
        // Gross collection of required traits. Need to clean this up.
        T: GetTableDefinition
            + BuildRoutes<T>
            + tailwag_orm::queries::Insertable
            + 'static
            + Send
            + Id
            + Filterable
            + Clone
            + Sync
            + std::fmt::Debug
            + Unpin
            + Updateable
            + Default
            + GetForm
            + for<'a> Deserialize<'a>
            + Serialize
            + for<'r> sqlx::FromRow<'r, PgRow>
            + Deleteable,
    {
        let resource_name = T::get_table_definition().table_name.clone();
        self.resources.add_resource::<T>();
        self.forms.insert(resource_name.clone(), T::get_form());

        // TODO: I've fucked up the mgirations :(
        // // self.
        // let migration = Migration::<T>::compare(
        //     None, // TODO: Need to get the old migration
        //     &DatabaseDefinition::new_unchecked("postgres")
        //         .table(T::get_table_definition())
        //         .into(),
        // );
        // // self.migrations.push();

        /************************************************************************************/
        //  BUILD THE ROUTES

        {
            let route = T::build_routes();
            self.root_route.route(format!("{}", &resource_name), route);
        }

        //
        /************************************************************************************/

        self
    }

    pub fn with_middleware(
        mut self,
        // TODO: Go the route I went with RouteHandler, to automagic some type conversion
        func: impl Fn(
                Request,
                Context,
                Box<dyn Fn(Request, Context) -> Response>,
            ) -> Pin<Box<dyn Future<Output = Response>>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        // TODO+ Send + SPin<Box<tatic
        // + + Send + Sync + 'static
        // 1. Middleware is a function. It is essentially just a Handler that calls the next handler
        let middleware = Middleware {
            handle_request: Box::new(func),
        };
        self.middleware.push(middleware);

        self
    }

    pub fn build_service(self) -> WebService {
        let middleware = self.middleware.into_iter().rev();
        let middleware_handler: Box<
            dyn Send
                + Sync
                + Fn(
                    Request,
                    Context,
                    // Box<dyn Fn(Request, Context) -> Response>,
                ) -> Pin<Box<dyn std::future::Future<Output = Response>>>,
        > = middleware.fold(
            Box::new(|req: Request, res: Context| {
                Box::pin(async move {
                    // todo!(),
                    Response::default()
                    // todo!()
                })
            }),
            |acc, middleware_fn| acc,
        );
        // Box::new(middleware.fold(
        //     |req: Request, res: Response| Box::pin(async move {
        //         todo!()
        //         // next(req, res)
        //     },
        //     |next, middleware_fn| next,
        // ));
        WebService {
            inner: std::sync::Arc::new(WebServiceInner {
                config: self.config,
                // data_providers: self.data_providers,
                resources: self.resources.build(),
                // router: self.router,
                routes: self.root_route,
                _migrations: Arc::new(Mutex::new(self.migrations)),
                middleware: todo!(),
                // middleware: vec![middleware_handler],
            })
            .clone(),
        }
    }
}

type RequestMetrics = ();
impl WebService {
    fn print_welcome_message(&self) {
        // TODO: Bring this into a template file (.txt or .md)
        log::info!(
            r#"
=============================================
   Starting Web Application {}
=============================================
"#,
            &self.config.application_name,
        );
        log::debug!("CONFIGURED ENVIRONMENT: {:?}", &self.config);
        #[cfg(debug_assertions)]
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

    pub fn builder(name: &str) -> WebServiceBuilder {
        WebServiceBuilder::new(name)
    }

    /// Start the Application. By default, starts an HTTP server bound to `127.0.0.1::8081`.
    pub async fn run(self) -> Result<RunResult, crate::Error> {
        /////////////////////////////
        // Axum Web implementation //
        /////////////////////////////
        self.print_welcome_message();

        let db_pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&self.config.database_conn_string)
            .await
            .expect("[DATABASE] Unable to obtain connection to database. Is postgres running?");
        let data_providers = &self.resources.connect(db_pool).await;

        // // TODO: Run migrations
        // let migrations: MigrationRunners;
        // {
        //     let mut mutex_guard = self.migrations.lock()?;
        //     migrations = mutex_guard.drain(0..).collect();
        // }
        // for migration in migrations {
        //     migration(data_providers.clone()).await?
        // }

        let bind_addr = format!("{}:{}", &self.config.socket_addr, self.config.port);
        log::info!("Starting service on {}", &bind_addr);
        let listener = TcpListener::bind(&bind_addr).unwrap();
        println!("Waiting for connection....");
        while let Ok((stream, _addr)) = listener.accept() {
            println!("Received connection from {}!", _addr.ip());
            // TODO: Rate-limiting / failtoban stuff
            let svc = self.clone();

            svc.handle_request(
                stream,
                Context {
                    data_providers: data_providers.clone(),
                },
            )
            .await?;

            println!("Waiting for connection....");
        }
        Ok(RunResult::default())
    }

    pub async fn handle_request(
        self,
        mut stream: std::net::TcpStream,
        context: Context,
    ) -> Result<RequestMetrics, crate::Error> {
        // THis is very much ina  "debugging" state - need to clean up once it's working.
        log::info!("Connection received from {}", stream.peer_addr()?);
        // TODO: Reject requests where Content-Length > MAX_REQUEST_SIZE
        let request = crate::application::http::route::Request::try_from(&stream)?;
        let path = &request.path;
        println!("FULL PATH: {}", &path);
        let request_handler = self.routes.find_handler(path, &request.method);

        // TODO: Move this to the build_service step, for quick reuse. Middleware will always stay the same.
        // If you want selectiive middleware, you must wrap *services* behind a route
        let mut route_handler = Box::new(|req, ctx| async move {
            match request_handler {
                Some(handler) => handler.call(req, ctx).await,
                None => crate::application::http::route::Response {
                    status: crate::application::http::route::HttpStatus::NotFound,
                    headers: Headers::from(vec![]), // TODO: Default response headers
                    http_version: crate::application::http::route::HttpVersion::V1_1,
                    body: Vec::with_capacity(0),
                },
            }
        });
        for middleware in self.middleware.iter().rev() {
            // route_handler = Box::new(|req, ctx| async move {
            //     (middleware.handle_request)(req, ctx, route_handler).await
            // })
            // *(middleware.before_request)()
            // (middleware.before_request)(request, context)
        }

        let response = route_handler(request, context).await;
        stream.write_all(&dbg!(response).as_bytes())?;

        Ok(())
    }

    // fn handle_request(&)
}

#[test]
fn test_handle_request_multipart() {
    let request = r#""#;
}

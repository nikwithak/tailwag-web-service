use std::io::Write;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, future::Future, net::TcpListener, pin::Pin};

use env_logger::Env;
use log;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPoolOptions, PgRow};
use tailwag_forms::{Form, GetForm};
use tailwag_macros::Deref;
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

use crate::application::http::route::{RequestContext, ServerContext};
// use crate::application::threads::ThreadPool;
use crate::{
    auth::gateway::{Account, Session},
    traits::rest_api::BuildRoutes,
};

use super::http::route::{HttpMethod, IntoRouteHandler, Request, Response};
use super::middleware::cors::{inject_cors_headers, CorsMiddleware};
use super::middleware::{Afterware, Beforeware, MiddlewareResult};
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
pub enum AdminActions {
    KillServer,
}

#[allow(private_bounds)]
pub struct WebServiceInner {
    config: WebServiceConfig,
    middleware_before: Vec<Beforeware>,
    middleware_after: Vec<Afterware>,
    routes: Route,
    resources: UnconnectedDataSystem,
    _migrations: Arc<Mutex<MigrationRunners>>, // Wrapped in a Mutex to work around some Arc issues - these only need to be run once.
    admin_rx: Receiver<AdminActions>,
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
    middleware_before: Vec<Beforeware>,
    middleware_after: Vec<Afterware>,
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
            middleware_before: Vec::new(),
            middleware_after: Vec::new(),
            migrations: Vec::new(),
            root_route: Route::default(),
            forms: HashMap::new(),
        }
        .with_resource::<Account>()
        .with_resource::<Session>()
        // TODO: Make these consistent, by adding an Into / From pattern for these functions
        // so I don't have to wrap them in a Box<Pin<Future<_>>> every time
        .with_before(CorsMiddleware::default())
        .with_afterware(inject_cors_headers)
    }
}

macro_rules! build_route_method {
    ($method:ident:$variant:ident) => {
        pub fn $method<F, I, O>(
            mut self,
            path: &str,
            handler: impl IntoRouteHandler<F, I, O>,
        ) -> Self {
            self.root_route.add_handler(HttpMethod::$variant, path.into(), handler);
            self
        }
    };
}

/// Builder methods for easy route building. These allow you to chain together .get("/path", || ()) calls,
/// quickly building a service.
///
/// NOTE: The methods ending in `_public` are identical (for now) to the others.
///       In a future iteration these will allow you to restrict all routes by default,
///       and explicitly allow certain routes for public access.
impl WebServiceBuilder {
    build_route_method!(get:Get);
    build_route_method!(post:Post);
    build_route_method!(delete:Delete);
    build_route_method!(patch:Patch);
    build_route_method!(get_public:Get);
    build_route_method!(post_public:Post);
    build_route_method!(delete_public:Delete);
    build_route_method!(patch_public:Patch);
}

type MiddlewareFunction = dyn Send
    + Sync
    + FnMut(
        Request,
        ServerContext,
        Box<dyn Fn(Request, ServerContext) -> Pin<Box<dyn Future<Output = Response>>>>,
    );

impl WebServiceBuilder {
    // Builder functions
    pub fn new(app_name: &str) -> Self {
        let mut builder = Self::default();
        builder.config.application_name = app_name.to_string();
        builder
    }

    pub fn with_static_files(mut self) -> Self {
        async fn echo(req: Request) -> Option<String> {
            // value
            // Hacky implementation
            // let path = req.path.split('/');
            // if path.pop().filter(|p| p).is_none() {
            //     return None;
            // } else {
            //     None
            // }
            format!("Path: {}", &req.path).into()
        }
        self.get("/static", echo)
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

    pub fn with_before<F: Into<Beforeware>>(
        mut self,
        // TODO: Go the route I went with RouteHandler, to automagic some type conversion
        middleware: F,
    ) -> Self {
        // TODO+ Send + SPin<Box<tatic
        // + + Send + Sync + 'static
        // 1. Middleware is a function. It is essentially just a Handler that calls the next handler
        self.middleware_before.push(middleware.into());

        self
    }

    pub fn with_beforeware(
        mut self,
        // TODO: Go the route I went with RouteHandler, to automagic some type conversion
        func: impl Fn(
                Request,
                RequestContext,
                // Box<dyn FnOnce(Request, Context) -> Response>,
            ) -> Pin<Box<dyn Future<Output = MiddlewareResult>>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        // TODO+ Send + SPin<Box<tatic
        // + + Send + Sync + 'static
        // 1. Middleware is a function. It is essentially just a Handler that calls the next handler
        let middleware = Beforeware {
            handle_request: Box::new(func),
        };
        self.middleware_before.push(middleware);

        self
    }

    pub fn with_afterware(
        mut self,
        // TODO: Go the route I went with RouteHandler, to automagic some type conversion
        func: impl Fn(
                Response,
                RequestContext,
            ) -> Pin<Box<dyn Future<Output = (Response, RequestContext)>>>
            + Send
            + Sync
            + 'static,
    ) -> Self {
        // TODO+ Send + SPin<Box<tatic
        // + + Send + Sync + 'static
        // 1. Middleware is a function. It is essentially just a Handler that calls the next handler
        let afterware = Afterware {
            handle_request: Box::new(func),
        };
        self.middleware_after.push(afterware);

        self
    }

    pub fn build_service(self) -> WebServiceBuildResponse {
        let (admin_tx, admin_rx) = channel();
        let service = WebService {
            inner: std::sync::Arc::new(WebServiceInner {
                config: self.config,
                // data_providers: self.data_providers,
                resources: self.resources.build(),
                // router: self.router,
                routes: self.root_route,
                _migrations: Arc::new(Mutex::new(self.migrations)),
                middleware_before: self.middleware_before,
                middleware_after: self.middleware_after,
                admin_rx,
                // middleware: vec![middleware_handler],
            })
            .clone(),
        };
        WebServiceBuildResponse {
            service,
            sender: admin_tx,
        }
    }
}
#[derive(Deref)]
pub struct WebServiceBuildResponse {
    #[deref]
    pub service: WebService,
    pub sender: Sender<AdminActions>,
}

impl WebServiceBuildResponse {
    pub async fn run(self) -> Result<RunResult, crate::Error> {
        self.service.run().await
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
            if let Ok(AdminActions::KillServer) = self.admin_rx.try_recv() {
                // If we've gotten a kill signal, then stop the server.
                break;
            }

            println!("Received connection from {}!", _addr.ip());
            // TODO: Rate-limiting / failtoban stuff
            let svc = self.clone();

            svc.handle_request(stream, ServerContext::from(data_providers.clone())).await?;

            println!("Waiting for connection....");
        }
        Ok(RunResult::default())
    }

    pub async fn handle_request(
        self,
        mut stream: std::net::TcpStream,
        server_context: ServerContext,
    ) -> Result<RequestMetrics, crate::Error> {
        log::info!("Connection received from {}", stream.peer_addr()?);
        // TODO: Reject requests where Content-Length > MAX_REQUEST_SIZE
        // And other validity checks.
        let request = crate::application::http::route::Request::try_from(&stream)?;
        let context = RequestContext::from_server_context(server_context);

        // PREPROCESSING
        let before_ware = self.middleware_before.iter();
        let (mut req, mut ctx) = (request, context);
        for middleware in before_ware {
            match (middleware.handle_request)(req, ctx).await {
                MiddlewareResult::Continue(new_req, new_ctx) => {
                    req = new_req;
                    ctx = new_ctx;
                },
                MiddlewareResult::Respond(res) => {
                    stream.write_all(&dbg!(res).as_bytes())?;
                    return Ok(());
                },
            }
        }

        // let response = route_handler(req, ctx).await;
        let mut response = self.routes.handle(req, &ctx).await;

        // // POSTPROCESSIING
        let afterware = self.middleware_after.iter();
        for after_fn in afterware {
            (response, ctx) = (after_fn.handle_request)(response, ctx).await;
        }

        stream.write_all(&dbg!(response).as_bytes())?;

        Ok(())
    }
}

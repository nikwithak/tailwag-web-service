use std::io::Write;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::{collections::HashMap, future::Future, net::TcpListener, pin::Pin};

use crate::application::http::into_route_handler::IntoRouteHandler;
use crate::tasks::runner::{IntoTaskHandler, TaskExecutor};
use env_logger::Env;
use log;
use serde::{Deserialize, Serialize};
use sqlx::postgres::{PgPoolOptions, PgRow};
use sqlx::PgPool;
use tailwag_forms::{Form, GetForm};
use tailwag_macros::{time_exec, Deref};
use tailwag_orm::data_definition::database_definition::DatabaseDefinition;
use tailwag_orm::data_manager::rest_api::Id;
use tailwag_orm::migration::Migration;
use tailwag_orm::queries::filterable_types::Filterable;
use tailwag_orm::{
    data_definition::{
        exp_data_system::{DataSystem, DataSystemBuilder, UnconnectedDataSystem},
        table::Identifier,
    },
    data_manager::GetTableDefinition,
    queries::{Deleteable, Updateable},
};
use tailwag_utils::types::generic_type_map::TypeInstanceMap;

use crate::application::http::route::{RequestContext, ServerContext};
// use crate::application::threads::ThreadPool;
use crate::{
    auth::gateway::{Account, Session},
    traits::rest_api::BuildRoutes,
};

use super::http::route::{HttpMethod, Request, Response};
use super::middleware::cors::{self, inject_cors_headers, CorsMiddleware};
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
    server_data: Arc<TypeInstanceMap>,
    migrations: Arc<Mutex<MigrationRunners>>, // Wrapped in a Mutex to work around some Arc issues - these only need to be run once.
    admin_rx: Receiver<AdminActions>,
}

#[derive(tailwag_macros::Deref)]
pub struct WebService {
    #[deref]
    inner: std::sync::Arc<WebServiceInner>,
    task_executor: Option<TaskExecutor>,
}

type MigrationRunners = Vec<
    Box<
        dyn FnOnce(
            sqlx::Pool<sqlx::Postgres>,
        ) -> Pin<Box<dyn Future<Output = Result<(), tailwag_orm::Error>>>>,
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
    server_data: TypeInstanceMap,
    task_executor: TaskExecutor,
}

#[cfg(debug_assertions)]
impl Default for WebServiceBuilder {
    fn default() -> Self {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        // Load in the current `.env` file, if it exists. If it fails, who cares, the rest of the ENV should be set.
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
            server_data: Default::default(),
            task_executor: Default::default(),
        }
        .with_resource::<Account>()
        .with_resource::<Session>()
        .with_before(cors::CorsMiddleware::default())
        .with_afterware(cors::inject_cors_headers)
    }
}

macro_rules! build_route_method {
    ($method:ident:$variant:ident) => {
        pub fn $method<F, I, O>(
            mut self,
            path: &str,
            handler: impl IntoRouteHandler<F, I, O>,
        ) -> Self {
            self.root_route = self.root_route.$method(path, handler);
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

async fn run_migration<T: GetTableDefinition + std::fmt::Debug + Clone>(
    pool: &sqlx::Pool<sqlx::Postgres>
) -> Result<(), tailwag_orm::Error> {
    let table_def = T::get_table_definition();
    let mig = Migration::<T>::compare(
        None, // TODO: Need to get the old migration
        &DatabaseDefinition::new_unchecked("postgres").table(table_def.clone()).into(),
    );

    if let Some(mig) = mig {
        mig.run(pool).await?;
        log::info!("Running migration for {}", &table_def.table_name);
        Ok(())
    } else {
        log::info!("Skipping migration for {} - table is up to date!", &table_def.table_name);
        Ok(())
    }
}

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
            format!("TODO: Static Files. Hit Path: {}", &req.path).into()
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
            // + for<'r> sqlx::FromRow<'r, PgRow>
            + Deleteable,
    {
        let resource_name = T::get_table_definition().table_name.clone();
        self.resources.add_resource::<T>();
        self.forms.insert(resource_name.clone(), T::get_form());

        {
            //  MIGRATIONS
            self.migrations
                .push(Box::new(|pool| Box::pin(async move { run_migration::<T>(&pool).await })));
        }

        {
            //  BUILD THE ROUTES
            let route = T::build_routes();
            self.root_route.route(format!("{}", &resource_name), route);
        }

        {
            //  EXPORT THE FORMS
            let form = T::get_form();
            form.save_json(&format!("out/forms/{resource_name}.json")).unwrap();
        }

        self
    }

    pub fn with_task<F, T, Req>(
        mut self,
        task_handler: F,
    ) -> Self
    where
        F: IntoTaskHandler<F, T, Req> + Sized + Sync + Send + 'static,
        Req: 'static,
    {
        self.task_executor.add_handler(task_handler);
        self
    }

    pub fn with_before<F: Into<Beforeware>>(
        mut self,
        middleware: F,
    ) -> Self {
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

    pub fn with_server_data<T: Clone + Send + Sync + 'static>(
        mut self,
        data: T,
    ) -> Self {
        self.server_data.insert(data);
        self
    }

    pub fn build_service(self) -> WebServiceBuildResponse {
        let (admin_tx, admin_rx) = channel();
        // let WebServiceBuilder { config, root_route, migrations, forms, middleware_before, middleware_after, resources, server_data, task_executor } = self;
        let mut server_data = self.server_data;
        server_data.insert(self.task_executor.scheduler());
        let service = WebService {
            inner: std::sync::Arc::new(WebServiceInner {
                config: self.config,
                resources: self.resources.build(),
                routes: self.root_route,
                migrations: Arc::new(Mutex::new(self.migrations)),
                middleware_before: self.middleware_before,
                middleware_after: self.middleware_after,
                admin_rx,
                server_data: Arc::new(server_data),
            }),
            task_executor: Some(self.task_executor),
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
        let WebServiceConfig {
            application_name,
            ..
        } = &self.config;
        // TODO: Bring this into a template file (.txt or .md)
        log::info!(
            r#"
=============================================
   Starting Web Application {application_name}
=============================================
"#,
        );
        log::debug!("CONFIGURED ENVIRONMENT: {:?}", &self.config);
        #[cfg(debug_assertions)]
        {
            log::warn!(
                r#"
============================================================================="
    ++ {application_name} IS RUNNING IN DEVELOPMENT MODE ++
    ++      DO NOT USE IN PRODUCTION YET ++
============================================================================="
"#
            );
        }
    }

    pub fn builder(name: &str) -> WebServiceBuilder {
        WebServiceBuilder::new(name)
    }

    async fn build_context(
        &self,
        db_pool: &PgPool,
    ) -> ServerContext {
        let data_providers = self.resources.connect(db_pool.clone()).await;
        let server_data = self.server_data.clone();

        ServerContext {
            data_providers,
            server_data,
        }
    }

    async fn run_migrations(
        &self,
        db_pool: &PgPool,
    ) -> Result<(), crate::Error> {
        // Run migrations
        let mut mutex_guard = self.migrations.lock()?;
        let migrations: MigrationRunners = mutex_guard.drain(0..).collect();
        for migration in migrations {
            migration(db_pool.clone()).await?;
        }
        Ok(())
    }

    async fn connect_postgres(&self) -> Result<PgPool, crate::Error> {
        Ok(PgPoolOptions::new()
            .max_connections(4)
            .connect(&self.config.database_conn_string)
            .await?)
    }

    async fn start_service(
        &self,
        context: ServerContext,
    ) -> Result<RunResult, crate::Error> {
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

            time_exec!("ENTIRE REQUEST", self.handle_request(stream, context.clone()).await?);

            println!("Waiting for connection....");
        }
        Ok(RunResult::default())
    }

    fn start_task_executor(
        &mut self,
        context: ServerContext,
    ) -> Option<JoinHandle<()>> {
        self.task_executor.take().map(|exec| std::thread::spawn(|| exec.run(context)))
    }

    pub async fn run(mut self) -> Result<RunResult, crate::Error> {
        self.print_welcome_message();

        let db_pool = self.connect_postgres().await?;
        self.run_migrations(&db_pool).await?;
        let context = self.build_context(&db_pool).await;

        let tasks_thread = self.start_task_executor(context.clone());
        let result = self.start_service(context.clone()).await;

        // Let the tasks_thread die
        tasks_thread.map(|thread| thread.join());
        result
    }
}
impl WebServiceInner {
    pub async fn handle_request(
        &self,
        mut stream: std::net::TcpStream,
        server_context: ServerContext,
    ) -> Result<RequestMetrics, crate::Error> {
        log::info!("Connection received from {}", stream.peer_addr()?);
        // TODO: Reject requests where Content-Length > MAX_REQUEST_SIZE
        // And other validity checks.

        let request = time_exec!(
            "Request Destructuring",
            crate::application::http::route::Request::try_from(&stream)
        )?;
        let context =
            time_exec!("Build Context", RequestContext::from_server_context(server_context));

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

/// This mod adds QueuedTask support to the WebApplication, running in a separate thread.
/// #[cfg(feature = "tasks")]
impl WebService {}

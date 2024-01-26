use axum::Router;
use tailwag_macros::Deref;
use tailwag_orm::{
    data_definition::database_definition::{DatabaseDefinition, DatabaseDefinitionBuilder},
    queries::Insertable,
};

use crate::traits::rest_api::BuildRoutes;

use super::{
    http::{request::HttpRequestHandler, routes::RoutePath},
    WebServiceApplication,
};

#[derive(Debug, Deref)]
pub struct RestWebService {
    _resources: DatabaseDefinitionBuilder,
    #[deref]
    application: WebServiceApplication,
}

impl RestWebService {
    #[allow(dead_code)]
    fn new(name: &str) -> Self {
        Self {
            _resources: DatabaseDefinition::new_unchecked("_"),
            application: WebServiceApplication::new(name),
        }
    }

    #[allow(dead_code)]
    fn with_route<P, H, T>(
        self,
        _path: P,
        _handler: H,
    ) -> Self
    where
        P: Into<RoutePath>,
        H: HttpRequestHandler<T>,
    {
        // let Route
        self
    }

    #[allow(dead_code)]
    #[allow(unused_mut)]
    fn with_crud_resource<T>(mut self) -> Self
    where
        T: BuildRoutes<T> + Insertable + Send + Unpin + 'static,
    {
        // let routes = T::build_routes();
        // self.application.router.nest("/", T::build_routes(data_manager));
        // self.application.router = T::build_routes(todo!());
        // self.application.router.route("/", get());
        self
    }
}

// impl Default for DataModelRestServiceDefinition {
// fn default() -> Self {
//     let db: DatabaseDefinition = DatabaseDefinition::new_unchecked("db").into();

//     DataModelRestServiceDefinition {
//         resources: db,
//         application: todo!(),
//     }
// }

pub trait BuildCrudRoutes {
    fn build_crud_routes(&self) -> Router;
}

impl Into<WebServiceApplication> for RestWebService {
    fn into(self) -> WebServiceApplication {
        todo!()
    }
}

// impl<T> BuildCrudRoutes for T: impl {}

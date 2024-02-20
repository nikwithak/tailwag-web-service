use serde::Deserialize;
use tailwag_orm::{data_manager::GetTableDefinition, queries::Insertable};

use crate::application::http::route::Route;

pub trait BuildRoutes<T> {
    fn build_routes() -> Route;
}

// pub trait BuildCreateRoute<'a>
// where
//     Self: Sized + Insertable,
// {
//     type Request: Into<Self> + Deserialize<'a>;
//     fn build_create_route() -> Route;
// }

// pub trait BuildGetItemRoute<'a>
// where
//     Self: Sized,
// {
//     type Request: Into<Self> + Deserialize<'a>;
//     fn build_get_item_route() -> Route;
// }

// pub trait BuildListGetRoute
// where
//     Self: Sized,
// {
//     fn build_list_get_route() -> Route;
// }

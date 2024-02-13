use axum::{async_trait, Router};
use serde::Deserialize;
use tailwag_orm::queries::Insertable;

pub trait BuildRoutes<T> {
    fn build_routes() -> Router;
}

pub trait BuildCreateRoute<'a>
where
    Self: Sized + Insertable,
{
    type Request: Into<Self> + Deserialize<'a>;
    fn build_create_route() -> Router;
}

pub trait BuildGetItemRoute<'a>
where
    Self: Sized,
{
    type Request: Into<Self> + Deserialize<'a>;
    fn build_get_item_route() -> Router;
}

pub trait BuildListGetRoute
where
    Self: Sized,
{
    fn build_list_get_route() -> Router;
}

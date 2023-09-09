use axum::Router;
use tailwag_orm::{
    data_manager::PostgresDataProvider,
    queries::{Insertable, Queryable},
};
use uuid::Uuid;

pub trait GetById<T: Queryable> {
    fn get_by_id(id: &Uuid);
}
pub trait ListResource<T: Queryable> {
    fn list() -> Vec<T>;
}
pub trait CreateResource<T: Insertable> {
    fn create() -> Result<T, String>;
}
// trait DeleteResource<T: Deleteable> {
//     fn delete() -> Result<(), String>;
// }

pub trait BuildRoutes {
    fn build_routes() -> Router;
}

impl<T: Insertable + Queryable> GetById<T> for PostgresDataProvider<T> {
    fn get_by_id(id: &Uuid) {}
}

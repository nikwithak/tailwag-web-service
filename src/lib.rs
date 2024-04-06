pub mod application;
pub mod auth;
mod components;
pub mod errors;
pub mod traits;

#[derive(Debug)]
pub enum Error {
    BadRequest(String),
    NotFound,
}
impl<T: ToString> From<T> for Error {
    fn from(value: T) -> Self {
        Error::BadRequest(value.to_string())
    }
}

use application::http::route::{IntoResponse, Response};

pub mod application;
pub mod auth;
mod components;
pub mod errors;
pub mod extras;
pub mod tasks;
pub mod traits;

#[derive(Debug)]
pub enum Error {
    BadRequest(String),
    InternalServerError(String),
    NotFound,
}

pub type HttpResult<T> = Result<T, Error>;
impl<T: ToString> From<T> for Error {
    fn from(value: T) -> Self {
        Error::BadRequest(value.to_string())
    }
}

// impl From<tailwag_orm::Error> for Error {
//     fn from(value: tailwag_orm::Error) -> Self {
//         todo!()
//     }
// }

impl IntoResponse for crate::Error {
    fn into_response(self) -> Response {
        match self {
            Error::BadRequest(msg) => {
                log::warn!("[BAD REQUEST] {}", &msg);
                Response::bad_request()
            },
            Error::NotFound => Response::not_found(),
            Error::InternalServerError(msg) => {
                log::error!("[INTERNAL SERVER ERROR]: {}", &msg);
                Response::internal_server_error()
            },
        }
    }
}

pub type ResponseResult<T> = core::result::Result<T, crate::Error>;

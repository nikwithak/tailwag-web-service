pub mod application;
pub mod auth;
mod components;
pub mod errors;
pub mod traits;

#[derive(Debug)]
pub enum Error {
    BadRequest(String),
}
impl<T: ToString> From<T> for Error {
    fn from(value: T) -> Self {
        Error::BadRequest(value.to_string())
    }
}

pub fn add(
    left: usize,
    right: usize,
) -> usize {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}

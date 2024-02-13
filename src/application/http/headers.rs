use std::collections::HashMap;

use tailwag_macros::Deref;

use crate::Error;

type HeaderName = String;
type HeaderValue = String;

#[derive(Debug, Default, Deref)]
pub struct Headers {
    headers: HashMap<HeaderName, HeaderValue>,
}

impl Headers {
    pub fn insert_parsed(
        &mut self,
        header: &str,
    ) -> Result<(), Error> {
        let mut split = dbg!(header).split(":");
        let (Some(name), Some(value)) = (split.next(), split.next()) else {
            return Err(Error::BadRequest(format!("Failed to parse header: {}", header)));
        };
        self.headers.insert(name.to_string().to_lowercase(), value.trim().to_string());
        Ok(())
    }
    pub fn get(
        &self,
        header_name: &str,
    ) -> Option<&HeaderValue> {
        self.headers.get(&header_name.to_lowercase())
    }
}

impl From<Vec<(&str, &str)>> for Headers {
    fn from(value: Vec<(&str, &str)>) -> Self {
        Headers {
            headers: value
                .into_iter()
                .map(|(name, val)| (name.to_string(), val.to_string()))
                .collect(),
        }
    }
}

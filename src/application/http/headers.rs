use std::collections::HashMap;

use tailwag_macros::Deref;

use crate::Error;

type HeaderName = String;
type HeaderValue = String;

#[derive(Debug, Deref)]
pub struct Headers {
    headers: HashMap<HeaderName, HeaderValue>,
}

impl Default for Headers {
    fn default() -> Self {
        // Creates a sensible-defaults header set based on OWASP recommendations.
        // Ref: https://cheatsheetseries.owasp.org/cheatsheets/HTTP_Headers_Cheat_Sheet.html
        Self {
            headers: vec![
                ("X-Frame-Options", "DENY"),
                ("X-Content-Type-Options", "nosniff"),
                ("Referrer-Policy", "strict-origin-when-cross-origin"),
            ]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        }
    }
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

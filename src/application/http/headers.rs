use std::{
    collections::HashMap,
    io::{BufRead, BufReader},
    ops::DerefMut,
};

use tailwag_macros::Deref;

use crate::Error;

type HeaderName = String;
type HeaderValue = String;

#[derive(Debug, Deref)]
pub struct Headers {
    headers: HashMap<HeaderName, HeaderValue>,
}
impl DerefMut for Headers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.headers
    }
}

impl Headers {
    pub fn parse_params(params_str: &str) -> HashMap<String, String> {
        HashMap::from_iter(params_str.split(';').filter_map(|param| {
            param
                .trim()
                .split_once('=')
                .map(|(a, b)| (a.trim().to_lowercase(), b.trim().to_string()))
        }))
    }
    pub fn parse_headers<T: std::io::Read>(stream: &mut BufReader<T>) -> Result<Self, Error> {
        let mut headers = Headers::default();
        let mut line = String::new();

        // 2 is the size of the line break indicating end of headers, and is too small to fit anything else in a well-formed request. Technically speaking I should be checking for CRLF specifically (or at least LF)
        while stream.read_line(&mut line)? > 2 {
            println!("LINE: {}", &line);
            headers.insert_parsed(&line)?;

            println!("{}", &line);
            line = String::new();
        }
        Ok(dbg!(headers))
    }
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
        header_line: &str,
    ) -> Result<(), Error> {
        let Some((name, value)) = dbg!(header_line).split_once(':') else {
            return Err(Error::BadRequest(format!("Failed to parse header: {}", header_line)));
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

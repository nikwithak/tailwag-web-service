use std::io::{BufRead, BufReader};

use super::{
    headers::Headers,
    route::{FromRequest, HttpBody},
};

#[derive(Debug, Default)]
pub struct MultipartPart {
    pub headers: Headers,
    pub content: Vec<u8>,
}

enum BoundaryMatch {
    None,
    Boundary,
    EndBoundary,
}

#[derive(Default)]
enum MultipartParserState {
    #[default]
    Start,
    Headers,
    Body,
    Done,
}

pub type MulitpartParts = Vec<MultipartPart>;
#[derive(Default)]
struct MultipartParser {
    parts: Vec<MultipartPart>,
    // boundary: String,
    state: MultipartParserState,
}

fn check_boundary(
    bytes: &[u8],
    boundary: &str,
) -> BoundaryMatch {
    let is_boundary = bytes.len() >= boundary.len() + 4 // To avoid out of bounds panic. 4 = len("----") AND len("--\r\n"). It's a bit of a hack.
        && bytes[0..2] == *b"--" // Must start with --
        && bytes[2..boundary.len() + 2] == *boundary.as_bytes() // Check the boundary
    ;
    if is_boundary {
        let is_end_boundary = bytes[boundary.len() + 2..boundary.len() + 4] == *b"--";
        if is_end_boundary {
            BoundaryMatch::EndBoundary
        } else {
            BoundaryMatch::Boundary
        }
    } else {
        BoundaryMatch::None
    }
}

//Parses the body of a multi_part_request into parts.
pub fn parse_multipart_request(
    content_type_params: &str,
    bytes: Vec<u8>,
) -> Result<HttpBody, crate::Error> {
    let parsed = Headers::parse_params(content_type_params);
    let boundary = parsed
        .get("boundary")
        .ok_or("No boundary defined. Required for multipart requests.")?;

    let mut parser: MultipartParser = MultipartParser::default();
    let mut stream = BufReader::new(&*bytes);

    let mut chunk: Vec<u8> = Vec::new();
    let mut part = MultipartPart::default();

    while stream.read_until(b'\n', &mut chunk)? > 0 {
        match parser.state {
            MultipartParserState::Start => {
                if !matches!(check_boundary(&chunk, boundary), BoundaryMatch::Boundary) {
                    Err("Failed to find boundary at start of message.")?
                }
                parser.state = MultipartParserState::Headers;
            },
            MultipartParserState::Headers => {
                // Read headers
                let header = String::from_utf8_lossy(&chunk);
                let header = header.trim();
                if header.is_empty() {
                    parser.state = MultipartParserState::Body;
                } else {
                    part.headers.insert_parsed(header)?;
                }
            },
            MultipartParserState::Body => match check_boundary(&chunk, boundary) {
                BoundaryMatch::None => part.content.append(&mut chunk),
                BoundaryMatch::Boundary => {
                    parser.parts.push(part);
                    parser.state = MultipartParserState::Headers;
                    part = MultipartPart::default();
                },
                BoundaryMatch::EndBoundary => {
                    parser.parts.push(part);
                    part = MultipartPart::default();
                    parser.state = MultipartParserState::Done;
                },
            },
            MultipartParserState::Done => panic!("Continued receiving data after end of parser."),
        }
        chunk = Vec::new();
    }
    log::debug!("Finished parsing multipart request!");

    Ok(HttpBody::Multipart(parser.parts))
}

pub trait FromMultipartPart {
    fn from_multipart_part(part: MultipartPart) -> Option<Self>
    where
        Self: Sized;
}

// impl<T> FromRequest for T
// where
//     T: FromMultipartPart,
// {
//     fn from(req: super::route::Request) -> Result<Self, crate::Error> {
//         todo!()
//     }
// }

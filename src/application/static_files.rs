use std::{collections::HashMap, path::Path};

use super::http::route::Response;

struct StaticFiles {
    files: HashMap<String, Vec<u8>>,
    templates: HashMap<String, Vec<u8>>,
}

// TODO: Pre-load the templates /statics

pub fn load_template<T: serde::Serialize>(
    filename: &str,
    obj: T,
) -> Response {
    // TODO: This is gross and inefficient.
    let data = serde_json::to_value(obj)
        .unwrap()
        .as_object()
        .unwrap()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let Ok(mut filled) = templater::templatize_file(
        // TODO: Pre-load the templates /statics
        Path::new(&format!("static/{}", filename)),
        &data,
    ) else {
        return Response::bad_request();
    };
    let Some(filename) = filename.strip_suffix(".template") else {
        return Response::not_found();
    };
    let mime_type = get_content_type(filename);
    let mime_type = if mime_type == "text/markdown" {
        let mut rendered_html = String::new();
        pulldown_cmark::html::push_html(&mut rendered_html, pulldown_cmark::Parser::new(&filled));
        filled = rendered_html;
        "text/html"
    } else {
        mime_type
    };

    Response::ok()
        .with_body(filled.bytes().collect())
        .with_header("content-type", mime_type)
}

fn get_content_type(filename: &str) -> &'static str {
    filename
        .split('.')
        .last()
        .map(|ext| match ext {
            // TODO: Do I need to enum this out?
            // Probably should at some point.
            "html" => "text/html",
            "css" => "text/css",
            "json" => "application/json",
            "pdf" => "application/pdf",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "png" => "image/png",
            "webp" => "image/webp",
            "md" => "text/markdown",
            _ => "application/octet-stream",
        })
        .unwrap_or("application/octet-stream")
}

fn load_static(filename: &str) -> Response {
    let Ok(mut body) = std::fs::read(format!("static/{}", filename)) else {
        return Response::bad_request();
    };
    // TODO: DRY out to MimeType type
    let mime_type = get_content_type(filename);

    let mime_type = if mime_type == "text/markdown" {
        let mut rendered_html = String::new();
        pulldown_cmark::html::push_html(
            &mut rendered_html,
            pulldown_cmark::Parser::new(&String::from_utf8(body).unwrap()),
        );
        body = rendered_html.into_bytes();
        "text/html"
    } else {
        mime_type
    };

    Response::ok()
        .with_body(body)
        // TODO: Parse the file extension into a content-type MIME-Type
        .with_header("content-type", mime_type)
}

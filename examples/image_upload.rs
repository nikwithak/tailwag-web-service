use std::{fmt::Display, str::FromStr};

use tailwag_web_service::application::{
    http::{
        headers::Headers,
        multipart::{FromMultipartPart, MultipartPart},
        route::FromRequest,
    },
    WebService,
};

#[tokio::main]
pub async fn main() {
    WebService::builder("My Multipart Web Service")
        // .get("/", |image: Image| "Testing")
        .post("image", save_image)
        .build_service()
        .run()
        .await
        .unwrap();

    async fn save_image(image: Image) -> String {
        let filename = format!("./downloaded_images/{}", &image.filename);
        std::fs::write(filename, image.bytes).unwrap();
        "Saved file!".to_string()
    }
}

#[derive(Clone)]
pub struct Image {
    filename: String,
    #[allow(unused)]
    mime_type: ImageMimeType,
    bytes: Vec<u8>,
}

#[derive(Clone)]
pub enum ImageMimeType {
    Jpeg,
    Gif,
    Png,
    Webp,
}
impl Display for ImageMimeType {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        f.write_str(match self {
            ImageMimeType::Jpeg => "image/jpeg",
            ImageMimeType::Gif => "image/gif",
            ImageMimeType::Png => "image/png",
            ImageMimeType::Webp => "image/webp",
        })
    }
}
impl FromStr for ImageMimeType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "image/jpeg" => Ok(Self::Jpeg),
            "image/gif" => Ok(Self::Gif),
            "image/png" => Ok(Self::Png),
            "image/webp" => Ok(Self::Webp),
            _ => Err(format!("Not a valid ImageMimeType: {}", s)),
        }
    }
}

impl FromMultipartPart for Image {
    fn from_multipart_part(part: MultipartPart) -> Option<Self> {
        let mime_type = part.get_image_mime_type()?;
        let filename = part.get_filename()?.to_string();

        Some(Image {
            bytes: part.content,
            filename,
            mime_type,
        })
    }
}

impl FromRequest for Image {
    fn from(req: tailwag_web_service::application::http::route::Request) -> Self {
        match req.body {
            tailwag_web_service::application::http::route::HttpBody::Multipart(parts) => {
                parts.into_iter().find_map(Image::from_multipart_part).unwrap()
            },
            _ => todo!("Need to refactor FromRequest to return an error (for bad requests / etc)"),
        }
    }
}

trait GetFileDetails {
    fn get_image_mime_type(&self) -> Option<ImageMimeType>;
    fn get_filename(&self) -> Option<String>;
}

impl GetFileDetails for MultipartPart {
    fn get_image_mime_type(&self) -> Option<ImageMimeType> {
        self.headers
            .get("content-type")
            .and_then(|mime| ImageMimeType::from_str(mime).ok())
    }

    fn get_filename(&self) -> Option<String> {
        // TODO: DRY this out
        Headers::parse_params(
            self.headers.get("content-disposition").unwrap().split_once(';').unwrap().1,
        )
        .get("filename")
        .map(|s| s.trim_matches('"').to_owned())
    }
}

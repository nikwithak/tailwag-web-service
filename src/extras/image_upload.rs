use std::{fmt::Display, str::FromStr};

use crate::{
    application::http::{
        headers::Headers,
        multipart::MultipartPart,
        route::{FromRequest, IntoResponse, PathString, Response},
    },
    Error,
};
use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use uuid::Uuid;

/// A custom implementation of FromRequest allows us to parse the Multipart bits for Image.
/// This is a result of the choice to base a lot of things off of Serde, and generically implement FromRequest for T: Serialize.
impl FromRequest for Image {
    fn from(req: crate::application::http::route::Request) -> Result<Self, Error> {
        let result = match req.body {
            crate::application::http::route::HttpBody::Multipart(mut parts) => {
                let id = Uuid::new_v4();
                let file = parts.remove("file").ok_or(Error::BadRequest("Missing file".into()))?;

                let mime_type = file
                    .get_image_mime_type()
                    .ok_or(Error::BadRequest("File is not a supported type.".into()))?;
                let filename = file
                    .get_filename()
                    .ok_or(Error::BadRequest("no filename for file".into()))?
                    .to_string();
                let key = format!("{id}_{filename}");
                let url = format!("http://localhost:8081/image/{key}"); // TODO: Unhardcode this. Only for localhost right now. Do a find/replace on hardcoded URLs & localhost specifically
                let title = parts
                    .remove("title")
                    .map(|title| String::from_utf8(title.content))
                    .unwrap_or(Ok("Untitled Image".into()))?;
                let description = parts
                    .remove("description")
                    .map(|description| String::from_utf8(description.content))
                    .unwrap_or(Ok("".into()))?;

                Image {
                    metadata: ImageMetadata {
                        id,
                        namespace: "static".into(), // TODO: Move this to a config / provider.
                        key,
                        url,
                        title,
                        description,
                    },
                    bytes: file.content,
                    mime_type,
                }
            },
            _ => Err(crate::Error::BadRequest(
                "This endpoint requires multipart/form_data.".to_owned(),
            ))?,
        };
        Ok(result)
    }
}

mod tailwag {
    pub use crate as web;
    pub use tailwag_forms as forms;
    pub use tailwag_macros as macros;
    pub use tailwag_orm as orm;
}
#[derive(
    Clone,
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    tailwag::macros::GetTableDefinition,
    tailwag::macros::Insertable,
    tailwag::macros::Updateable,
    tailwag::macros::Deleteable,
    tailwag::macros::Filterable,
    tailwag::macros::BuildRoutes,
    tailwag::macros::Id,
    tailwag::macros::Display,
    tailwag::forms::macros::GetForm,
)]
#[create_type(ImageMetadata)]
#[post(save_image)]
#[patch(update_image_md)]
// #[views(("/image/{id}", load_image))]
pub struct ImageMetadata {
    pub id: Uuid,
    pub namespace: String,
    pub key: String,
    pub url: String,
    pub title: String,
    pub description: String,
}

pub async fn load_image(filename: PathString) -> Response {
    let filename = &*filename;
    let Ok(bytes) = std::fs::read(format!("./downloaded_images/{filename}")) else {
        return Response::not_found();
    };
    Response::ok().with_body(bytes).with_header(
        "content-type",
        ImageMimeType::try_from_filename(filename)
            .map(|mt| mt.to_string())
            .unwrap_or("application/octet-stream".to_string()),
    )
}

pub async fn save_image(
    image: Image,
    db_images: PostgresDataProvider<ImageMetadata>,
) -> Response {
    let filename = format!("./downloaded_images/{}", &image.metadata.key);
    let result = match db_images.create(image.metadata).await {
        Ok(result) => result,
        Err(e) => {
            log::error!("Error saving image to DB: {:?}", e);
            return Response::internal_server_error();
        },
    };
    std::fs::write(filename, image.bytes).unwrap();
    result.into_response()
}
pub fn update_image_md() -> impl IntoResponse {
    Response::not_implemented()
}

#[derive(Clone)]
pub struct Image {
    pub metadata: ImageMetadata,
    #[allow(unused)]
    pub mime_type: ImageMimeType,
    pub bytes: Vec<u8>,
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

impl ImageMimeType {
    pub fn try_from_filename(filename: &str) -> Result<Self, crate::Error> {
        let ext = filename
            .split('.')
            .last()
            .ok_or(crate::Error::BadRequest("Invalid filename provided.".into()))?;
        let mime_type = match ext {
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "png" => Self::Png,
            "webp" => Self::Webp,
            _ => Err(crate::Error::BadRequest("Invalid file format requested".into()))?,
        };
        Ok(mime_type)
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

#[allow(unused)]
trait GetFileDetails {
    fn get_image_mime_type(&self) -> Option<ImageMimeType>;
    fn get_content_type(&self) -> Option<&str>;
    fn get_filename(&self) -> Option<String>;
}

impl GetFileDetails for MultipartPart {
    fn get_content_type(&self) -> Option<&str> {
        self.headers.get("content-type").map(|s| s.as_str())
    }
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

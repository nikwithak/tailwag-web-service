use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use tailwag_web_service::{
    application::{
        http::route::{FromRequest, IntoResponse, PathString, Response},
        WebService,
    },
    extras::image_upload::{GetFileDetails, MimeType},
    Error,
};
use uuid::Uuid;

#[tokio::main]
pub async fn main() {
    WebService::builder("My Multipart Web Service")
        // .get("/", |image: Image| "Testing")
        .with_resource::<ImageMetadata>()
        .post("/image/upload", save_image)
        .get("/image/{filename}", load_image)
        .build_service()
        .run()
        .await
        .unwrap();

    async fn load_image(filename: PathString) -> Response {
        let filename = &*filename;
        let Ok(bytes) = std::fs::read(format!("./downloaded_images/{filename}")) else {
            return Response::not_found();
        };
        Response::ok().with_body(bytes).with_header(
            "content-type",
            MimeType::try_from_filename(filename)
                .map(|mt| mt.to_string())
                .unwrap_or("application/octet-stream".to_string()),
        )
    }

    async fn save_image(
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
}

/// A custom implementation of FromRequest allows us to parse the Multipart bits for Image.
/// This is a result of the choice to base a lot of things off of Serde, and generically implement FromRequest for T: Serialize.
impl FromRequest for Image {
    fn from(req: tailwag_web_service::application::http::route::Request) -> Result<Self, Error> {
        let result = match req.body {
            tailwag_web_service::application::http::route::HttpBody::Multipart(mut parts) => {
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
                    .unwrap_or(Ok("".into()))?;
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
            _ => Err(tailwag_web_service::Error::BadRequest(
                "This endpoint requires multipart/form_data.".to_owned(),
            ))?,
        };
        Ok(result)
    }
}

mod tailwag {
    pub use tailwag_forms as forms;
    pub use tailwag_macros as macros;
    pub use tailwag_orm as orm;
    pub use tailwag_web_service as web;
}
#[derive(
    Clone,
    Debug,
    Default,
    serde::Deserialize,
    serde::Serialize,
    sqlx::FromRow,
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
#[post(upload_new_image)]
pub struct ImageMetadata {
    id: Uuid,
    namespace: String,
    key: String,
    url: String,
    title: String,
    description: String,
}

fn upload_new_image() {}

#[derive(Clone)]
pub struct Image {
    metadata: ImageMetadata,
    #[allow(unused)]
    mime_type: MimeType,
    bytes: Vec<u8>,
}

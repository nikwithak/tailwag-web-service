use tailwag_web_service::application::{
    http::{headers::Headers, route::FromRequest},
    WebService,
};

#[derive(Clone)]
pub struct Image {
    filename: String,
    bytes: Vec<u8>,
}

const ALLOWED_MIME_TYPES: [&str; 3] = ["image/jpeg", "image/png", "image/webp"];

impl FromRequest for Image {
    fn from(req: tailwag_web_service::application::http::route::Request) -> Self {
        match req.body {
            // tailwag_web_service::application::http::route::HttpBody::Json(_) => todo!(),
            // tailwag_web_service::application::http::route::HttpBody::Bytes(_) => todo!(),
            tailwag_web_service::application::http::route::HttpBody::Multipart(parts) => {
                let image = parts
                    .into_iter()
                    .find(|part| {
                        part.headers
                            .get("content-type")
                            .map_or(false, |val| ALLOWED_MIME_TYPES.contains(&val.as_str()))
                    })
                    .unwrap();
                let filename = Headers::parse_params(
                    image.headers.get("content-disposition").unwrap().split_once(';').unwrap().1,
                )
                .get("name")
                .map(|filename| filename.trim_matches('"'))
                .unwrap()
                .to_string();
                Image {
                    bytes: image.content,
                    filename,
                }
            },
            // tailwag_web_service::application::http::route::HttpBody::Stream(_) => todo!(),
            // tailwag_web_service::application::http::route::HttpBody::None => todo!(),
            _ => todo!("Need to refactor FromRequest to return an error (for bad requests / etc)"),
        }
    }
}

pub async fn save_image(image: Image) -> String {
    let filename = format!("./downloaded_images/{}", &image.filename);
    std::fs::write(filename, image.bytes).unwrap();
    "Saved file!".to_string()
}

#[tokio::main]
pub async fn main() {
    WebService::builder("My Multipart Web Service")
        // .get("/", |image: Image| "Testing")
        .post("image", save_image)
        .build_service()
        .run()
        .await
        .unwrap();
}

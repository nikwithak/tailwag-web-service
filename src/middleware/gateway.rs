#[derive(
    Clone, // Needed to be able to create an editable version from an Arc<Brewery> without affecting the saved data.
    Debug,
    serde::Deserialize, // Needed for API de/serialization
    serde::Serialize,   // Needed for API de/serialization
    sqlx::FromRow,      // Needed for DB connectivity
                        // tailwag_orm::macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
                        // tailwag::macros::Insertable,
                        // tailwag::macros::Updateable,
                        // tailwag::macros::Deleteable,
                        // tailwag::macros::BuildRoutes, // Creates the functions needed for a REST service (full CRUD)
                        // tailwag::macros::AsEguiForm, // Renders the object into an editable form for an egui application.
                        // tailwag_forms::forms::macros::GetForm,
)]
pub struct User {
    email_address: String,
    passhash: String,
}

// pub struct UserSession {
//     user: User,
//     // expiration: chrono::
// }
// #[derive(Default, Display, Debug)]
struct AuthorizationGateway;

// #[derive(Display)]
enum AuthorizationStatus {
    Authorized,
    Unauthorized,
}

impl AuthorizationGateway {
    // pub async fn authorize_request(Request) -> AuthorizationStatus {
    //     req: Request<axum::body::Body>,
    //     next: Next<axum::body::Body>,

    //     todo!()
    // }
}

pub async fn add_session_to_request<B>(
    //     mut request: axum::extract::Request
    mut request: axum::http::Request<B>,
    next: axum::middleware::Next<B>,
    // ) -> String
) -> Result<impl axum::response::IntoResponse, hyper::StatusCode> {
    match request
        .headers()
        .get("Authorization")
        .map(|header| header.to_str().ok())
        .flatten()
        .map(|header| header.strip_prefix("Bearer ")) // Accept Bearer tokens. This is a quick one-liner, but if other patterns are needed then break out to separate function
        .flatten()
    {
        Some(authz_header) => {
            let session = format!("Hello, Middleware: AUTHORIZED: {{{:?}}}", authz_header);
            request.extensions_mut().insert(session);
            //  TODO: Lookup authzheader in application state
            Ok(next.run(request).await)
        },
        None => Err(hyper::StatusCode::UNAUTHORIZED), // TODO: Fix this to be more customizable / redirects.
                                                      // TODO: Whitelist / evalate rules?
    }
}

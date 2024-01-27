use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use uuid::Uuid;

mod tailwag {
    pub use crate as web;
    pub use tailwag_forms as forms;
    pub use tailwag_orm as orm;
}
#[derive(
    Clone, // Needed to be able to create an editable version from an Arc<Brewery> without affecting the saved data.
    Debug,
    Default,
    serde::Deserialize,                 // Needed for API de/serialization
    serde::Serialize,                   // Needed for API de/serialization
    sqlx::FromRow,                      // Needed for DB connectivity
    tailwag_macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
    tailwag_macros::Insertable,
    tailwag_macros::Updateable,
    tailwag_macros::Deleteable,
    tailwag_macros::BuildRoutes, // Creates the functions needed for a REST service (full CRUD)
    // tailwag::macros::AsEguiForm, // Renders the object into an editable form for an egui application.
    // tailwag_forms::forms::macros::GetForm,
    tailwag::forms::macros::GetForm,
)]
pub struct User {
    id: uuid::Uuid,
    email_address: String,
    passhash: String,
}
impl tailwag::orm::data_manager::rest_api::Id for User {
    fn id(&self) -> &uuid::Uuid {
        &self.id
    }
}

#[derive(
    Clone, // Needed to be able to create an editable version from an Arc<Brewery> without affecting the saved data.
    Debug,
    Default,
    serde::Deserialize,                 // Needed for API de/serialization
    serde::Serialize,                   // Needed for API de/serialization
    sqlx::FromRow,                      // Needed for DB connectivity
    tailwag_macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
    tailwag_macros::Insertable,
    tailwag_macros::Updateable,
    tailwag_macros::Deleteable,
    tailwag_macros::BuildRoutes, // Creates the functions needed for a REST service (full CRUD)
    tailwag::forms::macros::GetForm,
)]
pub struct Session {
    id: uuid::Uuid,
    user_id: uuid::Uuid,
    // start_time: chrono::DateTime<chrono::Utc>,
    // expiry_time: chrono::DateTime<chrono::Utc>,
}
impl tailwag::orm::data_manager::rest_api::Id for Session {
    fn id(&self) -> &uuid::Uuid {
        &self.id
    }
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

pub enum UserType {
    Anonymous, // Public
    Authenticated(User),
}

pub struct RequestContext {
    user: UserType,
    // session_data: String,
    // policy: String,
}

// The actual middleware function
pub async fn add_session_to_request<B>(
    //     mut request: axum::extract::Request
    // axum::extract::State(users): axum::extract::State<(PostgresDataProvider<User>)>,
    axum::extract::State(sessions): axum::extract::State<PostgresDataProvider<Session>>,
    mut request: axum::http::Request<B>,
    next: axum::middleware::Next<B>,
    // ) -> String
) -> Result<impl axum::response::IntoResponse, hyper::StatusCode> {
    match request
        .headers()
        .get("Authorization")
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer ")) // Accept Bearer tokens only. This is a quick one-liner, but when other patterns are needed then break out to separate function
        .and_then(|header| Uuid::try_from(header).ok())
    {
        Some(authz_header) => {
            // TODO: Shoulldn't use UUIDs for session, use encrypted JWTs instead
            match sessions.get(authz_header).await {
                Ok(Some(session)) => {
                    println!("Session found! {:?}", &session);
                    // DEBUG USE ONLY: Created session 28689ddd-c10a-4e66-a212-a4549a5eca62
                    request.extensions_mut().insert(session);
                    Ok(next.run(request).await)
                },
                Ok(None) => Err(hyper::StatusCode::UNAUTHORIZED),
                Err(e) => {
                    // NOTE:
                    log::error!("An error occurred while authorizing the user: {}", e);
                    Err(hyper::StatusCode::UNAUTHORIZED)
                },
            }
        },
        None => Err(hyper::StatusCode::UNAUTHORIZED), // TODO: Fix this to be more customizable / redirects.
                                                      // TODO: Whitelist / evalate rules?
    }
}

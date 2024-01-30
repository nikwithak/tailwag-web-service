use std::time::Duration;

use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use hyper::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::Database;
use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use uuid::Uuid;

const JWT_SECRET: &'static str = "MY_SECRET_STRING"; // TODO: PANIC if detected in Production

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
    // tailwag_macros::BuildRoutes, // Creates the functions needed for a REST service (full CRUD)
    // tailwag::macros::AsEguiForm, // Renders the object into an editable form for an egui application.
    // tailwag_forms::forms::macros::GetForm,
    tailwag::forms::macros::GetForm,
)]
pub struct Account {
    id: uuid::Uuid,
    email_address: String,
    passhash: String,
}
impl tailwag::orm::data_manager::rest_api::Id for Account {
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
    account_id: uuid::Uuid,
    start_time: chrono::NaiveDateTime,
    expiry_time: chrono::NaiveDateTime,
}
impl tailwag::orm::data_manager::rest_api::Id for Session {
    fn id(&self) -> &uuid::Uuid {
        &self.id
    }
}

#[derive(Default)]
pub enum AccountType {
    #[default]
    Anonymous, // Public
    Authenticated(Account),
}

#[derive(Default)]
pub struct RequestContext {
    account: AccountType,
}

pub struct AuthorizationGateway {}

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    session_id: Uuid,
    exp: usize,
}

impl AuthorizationGateway {
    pub async fn add_session_to_request<B>(
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
        // .and_then(|header| Uuid::try_from(header).ok())
    {
        Some(authz_header) => {
            // TODO: Shouldn't use UUIDs for session, use encrypted JWTs instead, maybe? Or just random token
            let decoded_jwt = match jsonwebtoken::decode::<JwtClaims>(
                &authz_header,
                &jsonwebtoken::DecodingKey::from_secret(JWT_SECRET.as_ref()),
                &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256),
            ) {
                Ok(jwt) => jwt,
                Err(e) => {
                    println!("error: {}",e);
                    return Err(hyper::StatusCode::UNAUTHORIZED);
                }
            };
            let JwtClaims {session_id, .. }= decoded_jwt.claims;
            log::debug!("SESSION_ID: {}", &session_id);

            match sessions.get(session_id).await {
                Ok(Some(session)) => {
                    log::debug!("Session found! {:?}", &session);
                    request.extensions_mut().insert(session);
                    Ok(next.run(request).await)
                },
                Ok(None) => Err(hyper::StatusCode::UNAUTHORIZED),
                Err(e) => {
                    // NOTE:
                    log::error!("An error occurred while authorizing the account: {}", e);
                    Err(hyper::StatusCode::UNAUTHORIZED)
                },
            }
        },
        None => Err(hyper::StatusCode::UNAUTHORIZED), // TODO: Fix this to be more customizable / redirects.
                                                      // TODO: Whitelist / evalate rules?
    }
    }
}

// The actual middleware function

#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    email_address: Uuid, // TODO: Why is this a UUID?
    password: String,
}

const SESSION_LENGTH_MS: u64 = 360000;

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    access: String,
    refresh: String,
}
pub async fn login(
    axum::extract::State((accounts, sessions)): axum::extract::State<(
        tailwag::orm::data_manager::PostgresDataProvider<Account>,
        tailwag::orm::data_manager::PostgresDataProvider<Session>,
    )>,
    // axum::extract::State(sessions): axum::extract::State<
    //     tailwag::orm::data_manager::PostgresDataProvider<Session>,
    // >,
    axum::Json(creds): axum::Json<LoginRequest>,
    // accounts: State<PostgresDataProvider<account>>,
) -> Response {
    let account = match accounts.get(creds.email_address).await {
        Ok(Some(account)) => {
            match argon2::Argon2::default().verify_password(
                creds.password.as_bytes(),
                &argon2::PasswordHash::new(&account.passhash).unwrap(),
            ) {
                Ok(()) => {
                    let account = Account {
                        passhash: "".into(),
                        ..account // roles: vec![AuthorizationRole::Admin],
                    };
                    let Ok(new_session) = sessions
                        .create(Session {
                            id: Uuid::new_v4(),
                            account_id: account.id,
                            start_time: Utc::now().naive_utc(),
                            expiry_time: Utc::now().naive_utc()
                                + Duration::from_millis(SESSION_LENGTH_MS),
                        })
                        .await
                    else {
                        log::error!("Unable to create session for new login");
                        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                    };
                    let jwt = jsonwebtoken::encode(
                        &Default::default(),
                        &JwtClaims {
                            session_id: new_session.id,
                            exp: new_session.expiry_time.timestamp() as usize,
                        },
                        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_ref()),
                    )
                    .expect("Couldn't encode JWT");
                    return axum::Json(LoginResponse {
                        access: jwt,
                        refresh: "".into(),
                    })
                    .into_response();
                },
                Err(_) => {
                    log::warn!(
                        "FAILED LOGIN ATTEMPT for account: {:?}",
                        &Account {
                            passhash: "[REDACTED]".into(),
                            ..account
                        }
                    );
                    return StatusCode::NOT_FOUND.into_response();
                },
            }
        },
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            log::error!("Error occurred while trying to fetch account: {}", e);
            return StatusCode::BAD_REQUEST.into_response();
        },
    };
}

#[derive(Serialize, Deserialize)]
pub struct RegisterRequest {
    email_address: String, // TODO: ValidatedString
    password: String,
}
#[derive(Serialize, Deserialize)]
pub struct RegisterResponse {
    account_id: Uuid,
}
pub async fn register(
    axum::extract::State((accounts, _)): axum::extract::State<(
        tailwag::orm::data_manager::PostgresDataProvider<Account>,
        tailwag::orm::data_manager::PostgresDataProvider<Session>,
    )>,
    axum::Json(request): axum::Json<RegisterRequest>,
) -> Result<axum::Json<RegisterResponse>, StatusCode> {
    let salt = &SaltString::generate(&mut OsRng);
    let Ok(passhash) = Argon2::default().hash_password(request.password.as_bytes(), salt) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let account = accounts
        .create(Account {
            id: Uuid::new_v4(),
            email_address: request.email_address,
            passhash: passhash.to_string(),
        })
        .await
        .unwrap();
    Ok(axum::Json(RegisterResponse {
        account_id: account.id,
    }))
}

// pub async fn login_page(
//     axum::extract::State(accounts): axum::extract::State<
//         tailwag::orm::data_manager::PostgresDataProvider<account>,
//     >,
//     axum::Json(creds): axum::Json<LoginRequest>,
//     // accounts: State<PostgresDataProvider<account>>,
// ) -> Html<String> {
//     // accounts.
//     axum::Json("".into())
// }

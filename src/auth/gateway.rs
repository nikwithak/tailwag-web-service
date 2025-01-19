use std::{pin::Pin, sync::Arc, time::Duration};
use tailwag_orm::{
    data_definition::exp_data_system::DataSystem, data_manager::traits::WithFilter,
    queries::filterable_types::FilterEq,
};

use crate::application::http::route::RoutePolicy;
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tailwag_macros::{BuildRoutes, Filterable};
use tailwag_orm::data_manager::{traits::DataProvider, PostgresDataProvider};
use uuid::Uuid;

use crate::application::{
    http::route::{IntoResponse, Request, RequestContext, Response},
    NextFn,
};

const JWT_SECRET: &str = "MY_SECRET_STRING"; // TODO: PANIC if detected in Production
mod tailwag {
    pub use crate as web;
    pub use tailwag_forms as forms;
    pub use tailwag_orm as orm;
}

#[derive(
    Clone, // Needed to be able to create an editable version from an Arc<Brewery> without affecting the saved data.
    Debug,
    Default,
    Deserialize, // Needed for API de/serialization
    Serialize,   // Needed for API de/serialization
    // sqlx::FromRow,                      // Needed for DB connectivity
    tailwag_macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
    tailwag_macros::Insertable,
    tailwag_macros::Updateable,
    tailwag_macros::Deleteable,
    Filterable,
    BuildRoutes,
    tailwag::forms::macros::GetForm,
)]
#[views(("/current", get_current_user, RoutePolicy::RequireAuthentication))]
#[policy(RoutePolicy::RequireRole("Admin".to_string()))]
#[create_type(AppUserCreateRequest)]
pub struct AppUser {
    id: uuid::Uuid,
    email_address: String,
    #[serde(skip_serializing)]
    passhash: String,
    // TEMPORARY - this flag should later be replaced with an actual RBAC / ABAC system.
    is_admin: bool,
}

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct AppUserCreateRequest {
    pub email_address: String,
    pub password: String,
    pub is_admin: bool,
}
impl Into<AppUser> for AppUserCreateRequest {
    fn into(self) -> AppUser {
        AppUser {
            id: Uuid::new_v4(),
            email_address: self.email_address,
            is_admin: self.is_admin,
            passhash: {
                let salt = &SaltString::generate(&mut OsRng);
                let passhash = Argon2::default()
                    .hash_password(self.password.as_bytes(), salt)
                    .expect("Failed to hash password - this should not happen")
                    .to_string();
                passhash
            },
        }
    }
}

impl AppUser {
    pub fn is_admin(&self) -> bool {
        self.is_admin
    }
}

// pub fn get_current_user(req: Request) -> Response {
//     Response::not_implemented()
// }

pub async fn get_current_user(
    _request: Request,
    users: PostgresDataProvider<AppUser>,
    ctx: RequestContext,
) -> Response {
    let Some(session) = ctx.get_request_data::<Session>() else {
        return Response::not_found();
    };
    let Some(user) = users.get(|u| u.id.eq(session.account_id)).await.ok().flatten() else {
        return Response::not_found();
    };
    user.into_response()
}

impl tailwag::orm::data_manager::rest_api::Id for AppUser {
    fn id(&self) -> &uuid::Uuid {
        &self.id
    }
}

#[derive(
    Clone, // Needed to be able to create an editable version from an Arc<Brewery> without affecting the saved data.
    Debug,
    Default,
    Deserialize, // Needed for API de/serialization
    Serialize,   // Needed for API de/serialization
    // sqlx::FromRow, // Needed for DB connectivity
    BuildRoutes,
    tailwag_macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
    tailwag_macros::Insertable,
    tailwag_macros::Updateable,
    tailwag_macros::Deleteable,
    tailwag_macros::Filterable,
    tailwag::forms::macros::GetForm,
)]
#[policy(RoutePolicy::RequireRole("Admin".to_string()))]
pub struct Session {
    id: uuid::Uuid,
    pub account_id: uuid::Uuid,
    start_time: chrono::NaiveDateTime,
    expiry_time: chrono::NaiveDateTime,
}
impl tailwag::orm::data_manager::rest_api::Id for Session {
    fn id(&self) -> &uuid::Uuid {
        &self.id
    }
}

pub type UserRole = String;

#[derive(Default)]
pub enum AccountType {
    #[default]
    Anonymous, // Public
    Authenticated(AppUser),
}

pub struct AuthorizationGateway;

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    session_id: Uuid,
    exp: usize,
}

pub fn extract_session(
    request: Request,
    mut context: RequestContext,
    next: Arc<NextFn>,
) -> Pin<Box<dyn std::future::Future<Output = Response> + Send>> {
    Box::pin(async move {
        let Some(sessions) = context.get::<Session>() else {
            return Response::internal_server_error();
        };

        // First, log request:
        log::debug!("{:?} {:?} {:?}", &request.method, &request.path, request.headers);
        fn extract_authz_token(request: &Request) -> Option<String> {
            if let Some(header) = request
                .headers
                .get("Authorization")
                .and_then(|header| header.as_str().strip_prefix("Bearer "))
            {
                Some(header.to_owned())
            } else if let Some(cookie) =
                request.headers.get("Cookie").map(|header| header.as_str().to_string())
            {
                let session_cookie = dbg!(cookie)
                    .split(';')
                    .map(|cookie| cookie.trim())
                    .find(|cookie| cookie.starts_with("_id"))
                    .and_then(|cookie| cookie.split('=').last())
                    .map(|cookie| cookie.trim().into());
                session_cookie
            } else {
                None
            }
        }

        let session_id = extract_authz_token(&request)
            .and_then(|token| {
                jsonwebtoken::decode::<JwtClaims>(
                    &token,
                    &jsonwebtoken::DecodingKey::from_secret(JWT_SECRET.as_ref()),
                    &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256),
                )
                .ok()
            })
            .map(|token_data| token_data.claims)
            .map(|claims| claims.session_id);

        let session = match session_id {
            Some(session_id) => sessions.get(|sess| sess.id.eq(session_id)).await,
            None => Ok(None),
        };
        match session {
            Ok(Some(session)) => {
                log::debug!("Session found! {:?}", &session);
                log::debug!("Adding session to RequestContext");
                context.insert_request_data(session);
                next(request, context).await
            },
            Ok(None) => {
                log::debug!("No session found fo request.");
                next(request, context).await
            },
            Err(e) => {
                log::error!("An error occurred while authorizing the account: {:?}", e);
                Response::unauthorized()
            },
        }
    })
}

// The actual middleware function
#[derive(Serialize, Deserialize)]
pub struct LoginRequest {
    email_address: String,
    password: String,
}

// TODO: Move to config
const SESSION_LENGTH_MS: u64 = 3600000;

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    access: String,
    refresh: String,
}
pub async fn login(
    creds: LoginRequest,
    providers: DataSystem,
) -> Result<Response, crate::Error> {
    let accounts = providers.get::<AppUser>().ok_or(crate::Error::NotFound)?;
    let sessions = providers.get::<Session>().ok_or(crate::Error::NotFound)?;

    let account = accounts
        .with_filter(|acct| acct.email_address.eq(&creds.email_address))
        .execute()
        .await
        .unwrap()
        // .ok()
        // TODO: Need to update get() to ensure only one exists
        // .and_then(|mut vec| vec.pop())
        .pop()
        .ok_or_else(|| {
            // TODO: Protect against authn timing attacks, by verifying the password against a dummy hash, and writing a dummy session to the store.
            crate::Error::NotFound
        })?;

    argon2::Argon2::default().verify_password(
        creds.password.as_bytes(),
        &argon2::PasswordHash::new(&account.passhash).unwrap(),
    )?;
    let account = AppUser {
        passhash: "".into(),
        // roles: vec![AuthorizationRole::Admin],
        ..account
    };
    let Ok(new_session) = sessions
        .create(SessionCreateRequest {
            account_id: account.id,
            start_time: Utc::now().naive_utc(),
            expiry_time: Utc::now().naive_utc() + Duration::from_millis(SESSION_LENGTH_MS),
        })
        .await
    else {
        log::error!("Unable to create session for new login");
        todo!("Handle errors with the IntoResponse stuff")
    };
    let jwt = jsonwebtoken::encode(
        &Default::default(),
        &JwtClaims {
            session_id: new_session.id,
            exp: new_session.expiry_time.and_utc().timestamp() as usize,
        },
        &jsonwebtoken::EncodingKey::from_secret(JWT_SECRET.as_ref()),
    )
    .expect("Couldn't encode JWT");

    let response = LoginResponse {
        access: jwt.clone(),
        refresh: "".into(),
    };
    let _cookie_header_val = format!(
        "_id={}; HttpOnly; SameSite=None",
        // "_id={}; HttpOnly; Domain={}; Path={}",
        jwt,
    );
    let response = response.into_response().with_header("Set-Cookie", _cookie_header_val);
    Ok(response)
}

pub async fn logout(
    _req: (),
    sessions: PostgresDataProvider<Session>,
    ctx: RequestContext,
) -> Response {
    if let Some(session) = ctx.get_request_data::<Session>() {
        sessions.delete(session.clone()).await.ok();
    }
    Response::ok()
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
    request: RegisterRequest,
    accounts: PostgresDataProvider<AppUser>,
) -> Option<RegisterResponse> {
    let account = accounts
        .create(AppUserCreateRequest {
            email_address: request.email_address,
            is_admin: false,
            password: request.password,
        })
        .await
        // TODO: Error instead of Option
        .ok()?;

    let response = RegisterResponse {
        account_id: account.id,
    };
    Some(response)
}

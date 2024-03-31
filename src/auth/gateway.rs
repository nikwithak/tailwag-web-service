use std::time::Duration;
use tailwag_orm::{
    data_definition::exp_data_system::DataSystem, data_manager::traits::WithFilter,
    queries::filterable_types::FilterEq,
};

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
    http::route::{Context, Request, Response},
    middleware::{Middleware, MiddlewareResult},
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
    Deserialize,                        // Needed for API de/serialization
    Serialize,                          // Needed for API de/serialization
    sqlx::FromRow,                      // Needed for DB connectivity
    tailwag_macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
    tailwag_macros::Insertable,
    tailwag_macros::Updateable,
    tailwag_macros::Deleteable,
    Filterable,
    BuildRoutes,
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
    Deserialize,   // Needed for API de/serialization
    Serialize,     // Needed for API de/serialization
    sqlx::FromRow, // Needed for DB connectivity
    BuildRoutes,
    tailwag_macros::GetTableDefinition, // Creates the data structure needed for the ORM to work.
    tailwag_macros::Insertable,
    tailwag_macros::Updateable,
    tailwag_macros::Deleteable,
    tailwag_macros::Filterable,
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
    _account: AccountType,
}

pub struct AuthorizationGateway;

#[derive(Serialize, Deserialize)]
struct JwtClaims {
    session_id: Uuid,
    exp: usize,
}

// TODO: DRY this out for generic functions, following the Handler pattern.
impl Into<Middleware> for AuthorizationGateway {
    fn into(self) -> Middleware {
        Middleware {
            handle_request: Box::new(|req, res| {
                Box::pin(async move { Self::add_session_to_request(req, res).await })
            }),
        }
    }
}

impl AuthorizationGateway {
    // TODO: Clean this up. Looks a bit too complex / a few different things going on
    pub async fn add_session_to_request(
        request: Request,
        context: Context,
        // sessions: PostgresDataProvider<Session>,
    ) -> MiddlewareResult {
        let Some(sessions) = context.data_providers.get::<Session>() else {
            return Response::internal_server_error().into();
        };
        // First, log request:
        // TODO: Middleware this somewhere else, inject Request ID, etc.
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
        let Some(authz_token) = extract_authz_token(&request) else {
            // TODO: Allow other whitelisted
            if ["/login", "/register"].contains(&request.path.as_str()) {
                return MiddlewareResult::Continue(request, context);
            }
            return Response::unauthorized().into();
        };

        let decoded_jwt = match jsonwebtoken::decode::<JwtClaims>(
            &authz_token,
            &jsonwebtoken::DecodingKey::from_secret(JWT_SECRET.as_ref()),
            &jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256),
        ) {
            Ok(jwt) => jwt,
            Err(e) => {
                println!("error: {}", e);
                return MiddlewareResult::Respond(Response::unauthorized());
            },
        };
        let JwtClaims {
            session_id,
            ..
        } = decoded_jwt.claims;
        log::debug!("SESSION_ID: {}", &session_id);

        match sessions.get(|sess| sess.id.eq(session_id)).await {
            Ok(Some(session)) => {
                log::debug!("Session found! {:?}", &session);
                {
                    // Here's where we add it to the request
                    // todo!()
                }
                MiddlewareResult::Continue(request, context)
            },
            Ok(None) => MiddlewareResult::Respond(Response::unauthorized()),
            Err(e) => {
                // NOTE:
                log::error!("An error occurred while authorizing the account: {:?}", e);
                MiddlewareResult::Respond(Response::unauthorized())
            },
        }
    }
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
) -> Option<LoginResponse> {
    let accounts = providers.get::<Account>()?;
    let sessions = providers.get::<Session>()?;

    let account = accounts
        .with_filter(|acct| acct.email_address.eq(&creds.email_address))
        .execute()
        .await
        .ok()
        // TODO: Need to update get() to ensure only one exists
        .and_then(|mut vec| vec.pop())?;

    argon2::Argon2::default()
        .verify_password(
            creds.password.as_bytes(),
            &argon2::PasswordHash::new(&account.passhash).unwrap(),
        )
        .ok()?; // TODO?: Should I throw an error, or just "Not Found" is good enough?
    let account = Account {
        passhash: "".into(),
        // roles: vec![AuthorizationRole::Admin],
        ..account
    };
    let Ok(new_session) = sessions
        .create(Session {
            id: Uuid::new_v4(),
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
            exp: new_session.expiry_time.timestamp() as usize,
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
    // TODO: Figure out how to set cookies from service response
    // response.headers_mut().insert(
    //     "Set-Cookie",
    //     HeaderValue::from_bytes(cookie_header_val.as_bytes())
    //         .expect("Failed to set cookie."),
    // );
    // response.into_response()
    Some(response)
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
    accounts: PostgresDataProvider<Account>,
) -> Option<RegisterResponse> {
    let salt = &SaltString::generate(&mut OsRng);
    // TODO: Error instead of Option
    let passhash = Argon2::default().hash_password(request.password.as_bytes(), salt).ok()?;
    let account = accounts
        .create(Account {
            id: Uuid::new_v4(),
            email_address: request.email_address,
            passhash: passhash.to_string(),
        })
        .await
        // TODO: Error instead of Option
        .ok()?;

    let response = RegisterResponse {
        account_id: account.id,
    };
    Some(response)
}

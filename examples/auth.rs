use tailwag_web_service::{
    application::{
        http::route::{HttpBody, Response},
        middleware::MiddlewareResult,
        WebService,
    },
    auth::gateway::{login, register, AuthorizationGateway},
};

#[tokio::main]
pub async fn main() {
    WebService::builder("AuthN/AuthZ Service")
        // .get("/", |image: Image| "Testing")
        .with_before(AuthorizationGateway {})
        .post("login", login)
        .post("register", register)
        .post("echo", echo)
        .build_service()
        .run()
        .await
        .unwrap();

    async fn echo(value: String) -> String {
        println!("Your request: {}", &value);
        value
    }
}

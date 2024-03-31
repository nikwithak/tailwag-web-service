use tailwag_web_service::{
    application::WebService,
    auth::gateway::{login, register, AuthorizationGateway},
};

#[tokio::main]
pub async fn main() {
    WebService::builder("AuthN/AuthZ Service")
        .with_before(AuthorizationGateway)
        .post_public("login", login)
        .post_public("register", register)
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

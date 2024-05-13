use tailwag_web_service::application::{
    http::route::{HttpBody, Response},
    middleware::MiddlewareResult,
    WebService,
};

#[tokio::main]
pub async fn main() {
    WebService::builder("Middleware Example Service")
        // .get("/", |image: Image| "Testing")
        .with_beforeware(|mut req, ctx| {
            Box::pin(async move {
                let HttpBody::Json(_body) = &req.body else {
                    return MiddlewareResult::Respond(Response::not_found());
                };
                println!("INSIDE MIDDLEWARE: Here's your request: {:?}", &req.body);
                req.body = tailwag_web_service::application::http::route::HttpBody::Json(format!(
                    "\"Your request was intercepted from the middleware. \"",
                    // &req.body,
                ));
                MiddlewareResult::Continue(req, ctx)
            })
        })
        .with_middleware(|req, ctx, next| {
            Box::pin(async move {
                println!("MALCOLM IN THE MIDDLEWARE!");
                println!("'ere's your request: {:?}", &req.body);
                let res = next(req, ctx).await;
                println!("Finished request");
                res
            })
        })
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

use tailwag_web_service::application::{
    http::route::{HttpBody, Response},
    middleware::MiddlewareResult,
    WebService,
};

#[tokio::main]
pub async fn main() {
    WebService::builder("Middleware Example Service")
        // .get("/", |image: Image| "Testing")
        .with_middleware_before(|mut req, ctx| {
            Box::pin(async move {
                let HttpBody::Json(body) = &req.body else {
                    return MiddlewareResult::Response(Response::not_found());
                };
                println!("INSIDE MIDDLEWARE: Here's your request: {:?}", &req.body);
                req.body = tailwag_web_service::application::http::route::HttpBody::Json(format!(
                    "\"Your rhquest was intercepted from the middleware. Original request: {:?}\"",
                    &req.body,
                ));
                MiddlewareResult::Continue(req, ctx)
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

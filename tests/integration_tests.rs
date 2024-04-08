use std::{collections::HashMap, thread::sleep, time::Duration};

use tailwag_macros::derive_magic;
use tailwag_web_service::{application::WebService, auth::gateway};

mod tailwag {
    pub use tailwag_forms as forms;
    pub use tailwag_macros as macros;
    pub use tailwag_orm as orm;
    pub use tailwag_web_service as web;
}

derive_magic! {
    pub struct Event {
        id: uuid::Uuid,
    }
}

#[tokio::main(flavor = "current_thread")]
async fn run_service() {
    WebService::builder("Hello World works")
        .get("/login", || "Login form goes here".to_string())
        .post("/login", gateway::login)
        .post("/register", gateway::register)
        .get("/", || "Hello, world!".to_string())
        .with_resource::<Event>()
        .build_service()
        .run()
        .await
        .unwrap();
}

#[test]
fn start_service() {
    run_service();
}

macro_rules! test_hurl_file {
    ($filename:literal) => {
        let result = hurl::runner::run(
            include_str!($filename),
            &hurl::runner::RunnerOptionsBuilder::new().build(),
            &HashMap::default(),
            &hurl::util::logger::LoggerOptionsBuilder::new().build(),
        );
        assert!(result.unwrap().success);
    };
}

#[test]
fn run_hurl_tests() {
    let thread = std::thread::Builder::new()
        .name("Hello World App".to_string())
        .spawn(run_service)
        .unwrap();

    println!("Starting service... waiting 2 seconds for status");
    sleep(Duration::from_secs(2)); // Wait for service to start up. TODO: Give a way to poll the service.
    println!("Checking server status");

    test_hurl_file!("login_register_work.hurl");

    todo!("All tests passed, but I haven't yet implemented a way to gracefully stop the webservice (so panicking is the only way");

    thread.join().unwrap();
}

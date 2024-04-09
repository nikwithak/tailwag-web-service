use std::{
    collections::HashMap,
    sync::{mpsc::Sender, Arc, OnceLock},
    thread::sleep,
    time::Duration,
};

use tailwag_macros::derive_magic;
use tailwag_web_service::{
    application::{AdminActions, WebService, WebServiceBuildResponse},
    auth::gateway,
};

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

type KillSignalCell = OnceLock<Sender<AdminActions>>;

#[tokio::main(flavor = "current_thread")]
async fn run_service(sender_cell: Arc<KillSignalCell>) {
    let WebServiceBuildResponse {
        service,
        sender,
    } = WebService::builder("Hello World works")
        .get("/login", || "Login form goes here".to_string())
        .post("/login", gateway::login)
        .post("/register", gateway::register)
        .get("/", || "Hello, world!".to_string())
        .with_resource::<Event>()
        .build_service();

    sender_cell.set(sender).unwrap();
    service.run().await.unwrap();
}

macro_rules! test_hurl_file {
    ($filename:literal) => {
        let result = hurl::runner::run(
            include_str!($filename),
            &hurl::runner::RunnerOptionsBuilder::new().build(),
            // &HashMap::default(),
            &vec![(
                "email_address".to_string(),
                hurl::runner::Value::String(format!(
                    "{}@localhost.local",
                    // Generates a unique random email address to verify the entire end_to_end flow
                    uuid::Uuid::new_v4()
                        .to_string()
                        .chars()
                        .filter(|c| c.is_alphanumeric())
                        .collect::<String>()
                )),
            )]
            .into_iter()
            .collect(),
            &hurl::util::logger::LoggerOptionsBuilder::new().build(),
        );
        assert!(result.unwrap().success);
    };
}

#[test]
fn run_hurl_tests() {
    // I did a quick hack-through to add a signal we can use to kill the server gracefully.
    // It's a condition on the while loop listneing for new requests - will only fire when another request
    // is received after sending the kill switch.
    let kill_signal_cell = Arc::new(OnceLock::new());
    let ksc = kill_signal_cell.clone();
    let thread = std::thread::Builder::new()
        .name("Hello World App".to_string())
        .spawn(move || run_service(ksc))
        .unwrap();

    println!("Starting service... waiting 2 seconds for status");
    sleep(Duration::from_secs(1)); // Wait for service to start up. TODO: Give a way to poll the service.
    println!("Checking server status");

    test_hurl_file!("login_register_work.hurl");

    // Tell the server to shut up now
    let signal = kill_signal_cell.get().unwrap();
    signal.send(AdminActions::KillServer).unwrap();
    println!("Sent kill signal");

    // The kill signal doesn't fire until another request comes in.
    // Not worth fixing rn, the kill signal was hacked together just for
    // these seamless tasks anyway. Smooth killing of service will be needed
    // later though - should plan to intercept SIGKILL signal
    hurl::runner::run(
        r#"GET http://localhost:8081/"#,
        &hurl::runner::RunnerOptionsBuilder::new().build(),
        &HashMap::default(),
        &hurl::util::logger::LoggerOptionsBuilder::new().build(),
    )
    .ok();

    thread.join().unwrap();
}

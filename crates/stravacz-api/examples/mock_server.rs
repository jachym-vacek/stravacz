//! A fake Strava.cz for developing frontends without a real account.
//!
//! ```sh
//! cargo run -p stravacz-api --example mock_server
//! STRAVA_BASE_URL=http://127.0.0.1:8555 cargo run -p stravacz-tui
//! ```
//!
//! Log in with any credentials. Orders are kept in memory until it exits.

#[tokio::main]
async fn main() {
    let listener = std::net::TcpListener::bind("127.0.0.1:8555").expect("port 8555 is free");
    let server = stravacz_api::mock::start_on(listener).await;

    println!("Mock Strava.cz listening on {}", server.uri());
    tokio::signal::ctrl_c().await.expect("ctrl-c handler");
}

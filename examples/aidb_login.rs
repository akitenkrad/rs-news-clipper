use chrono::{DateTime, Local, Utc};
use news_clipper::models::sites::ai_db::{auto_login, membership_cookie_expiry};

#[tokio::main]
async fn main() {
    let mut force = false;
    for argument in std::env::args().skip(1) {
        if argument == "--force" {
            force = true;
        } else {
            eprintln!("Usage: cargo run --example aidb_login -- [--force]");
            std::process::exit(2);
        }
    }

    match auto_login(force).await {
        Ok(cookies) => match membership_cookie_expiry(&cookies).and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0)) {
            Some(expiry) => println!(
                "AIDB auto-login succeeded; cookie expires at {}",
                expiry.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S %Z")
            ),
            None => println!("AIDB auto-login succeeded; cookie expiry is unavailable"),
        },
        Err(error) => {
            eprintln!("AIDB auto-login failed: {error}");
            std::process::exit(1);
        }
    }
}

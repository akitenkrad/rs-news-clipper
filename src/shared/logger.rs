use std::vec;

use anyhow::Result;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

pub fn init_logger<T: AsRef<str>>(log_level: T) -> Result<()> {
    assert!(vec!["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"].contains(&log_level.as_ref()));
    let log_level = log_level.as_ref().parse::<LevelFilter>()?;

    let env_filter = EnvFilter::builder()
        .with_default_directive(log_level.into())
        .from_env_lossy();
    let subscriber = tracing_subscriber::fmt::layer()
        .with_file(true)
        .with_line_number(true)
        .with_target(false);
    tracing_subscriber::registry()
        .with(subscriber)
        .with(env_filter)
        .try_init()?;

    Ok(())
}

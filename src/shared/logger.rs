use std::sync::Once;
use std::vec;

use anyhow::Result;
use tracing_subscriber::{
    EnvFilter, filter::LevelFilter, layer::SubscriberExt, util::SubscriberInitExt,
};

static INIT: Once = Once::new();

/// ロガーを初期化する（複数回呼ばれても安全）
pub fn init_logger<T: AsRef<str>>(log_level: T) -> Result<()> {
    assert!(vec!["OFF", "ERROR", "WARN", "INFO", "DEBUG", "TRACE"].contains(&log_level.as_ref()));

    let mut init_result: Result<()> = Ok(());

    INIT.call_once(|| {
        let log_level = match log_level.as_ref().parse::<LevelFilter>() {
            Ok(level) => level,
            Err(e) => {
                init_result = Err(e.into());
                return;
            }
        };

        let env_filter = EnvFilter::builder()
            .with_default_directive(log_level.into())
            .from_env_lossy();
        let subscriber = tracing_subscriber::fmt::layer()
            .with_file(true)
            .with_line_number(true)
            .with_target(false);

        if let Err(e) = tracing_subscriber::registry()
            .with(subscriber)
            .with(env_filter)
            .try_init()
        {
            init_result = Err(e.into());
        }
    });

    init_result
}

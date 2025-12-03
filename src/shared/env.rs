use std::env;
use strum::EnumString;

#[derive(Default, EnumString)]
#[strum(serialize_all = "lowercase")]
pub enum Environment {
    #[default]
    Development,
    Production,
}

pub fn which() -> Environment {
    #[cfg(debug_assertions)]
    let default_env = Environment::Development;
    #[cfg(not(debug_assertions))]
    let default_env = Environment::Production;

    match env::var("ENV") {
        Ok(env) => env.parse().unwrap_or(default_env),
        Err(_) => default_env,
    }
}

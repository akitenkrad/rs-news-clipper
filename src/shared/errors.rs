use axum::{http::StatusCode, response::IntoResponse};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("{0}")]
    InternalError(String),
    #[error("{0}")]
    EnumParseError(String),

    // from anyhow
    #[error("Error: {0}")]
    AnyhowError(#[from] anyhow::Error),

    // from uuid errors
    #[error("Uuid Error: {0}")]
    ConvertToUuidError(#[from] uuid::Error),

    // from crawler errors
    #[error("RssParser Error: {0}")]
    RssParseError(#[source] feed_parser::parsers::errors::ParseError),

    // from request errors
    #[error("Request Error: {0}")]
    RequestError(#[from] request::Error),
    #[error("Request Error - parse error: {0}")]
    ParseError(#[from] url::ParseError),

    // from serde errors
    #[error("Json Parse Error: {0}")]
    JsonParseError(#[from] serde_json::Error),

    // from chrono errors
    #[error("DateTime Parse Error: {0}")]
    DateTimeParseError(#[from] chrono::ParseError),

    // from scrape errors
    #[error("Scrape Error: {0}")]
    ScrapeError(String),

    // from openai-tools errors
    #[error("OpenAI Tools Error: {0}")]
    OpenAIToolError(#[from] openai_tools::common::OpenAIToolError),
}

fn app_error_to_status_code(error: &AppError) -> StatusCode {
    match error {
        AppError::InternalError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AppError::AnyhowError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AppError::EnumParseError(_) => StatusCode::BAD_REQUEST,
        AppError::ConvertToUuidError(_) => StatusCode::BAD_REQUEST,
        AppError::RssParseError(_) => StatusCode::BAD_REQUEST,
        AppError::RequestError(_) => StatusCode::BAD_REQUEST,
        AppError::ParseError(_) => StatusCode::BAD_REQUEST,
        AppError::JsonParseError(_) => StatusCode::BAD_REQUEST,
        AppError::ScrapeError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AppError::OpenAIToolError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        AppError::DateTimeParseError(_) => StatusCode::BAD_REQUEST,
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status_code = app_error_to_status_code(&self);
        status_code.into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;

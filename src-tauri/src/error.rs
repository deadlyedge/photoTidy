use std::fmt::Display;

use rusqlite::Error as SqliteError;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("sqlite error: {0}")]
    Sqlite(#[from] SqliteError),
    #[error("time error: {0}")]
    Time(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    pub fn time<E>(err: E) -> Self
    where
        E: Display,
    {
        Self::Time(err.to_string())
    }

    pub fn internal<E>(err: E) -> Self
    where
        E: Display,
    {
        Self::Internal(err.to_string())
    }
}

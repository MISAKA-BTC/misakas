//! One error type, and the HTTP shape it becomes.
//!
//! Two audiences read these: a person looking at a toast in the UI, and a program parsing
//! `error.type` out of an OpenAI-compatible response. So every variant maps to both a status
//! code and one of OpenAI's error type strings — a client written against the OpenAI SDK gets
//! the shape it expects, and a person gets a sentence naming what to do.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No model is loaded, and the request needed one.
    #[error("no model is loaded — load one from the Models tab, or POST /api/v1/models/{{id}}/load")]
    NoModelLoaded,

    #[error("no model with id '{id}' is installed")]
    ModelNotFound { id: String },

    /// The engine process failed: would not start, died, or answered with an error.
    #[error("{backend}: {message}")]
    Engine { backend: &'static str, message: String },

    /// The configured backend cannot run here.
    #[error("{backend} is not available: {reason}. {remedy}")]
    BackendUnavailable { backend: String, reason: String, remedy: String },

    #[error("the model catalog could not be reached: {message}")]
    Catalog { message: String },

    #[error("download failed: {message}")]
    Download { message: String },

    #[error("{message}")]
    BadRequest { message: String },

    #[error("{path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Core(#[from] misaka_studio_core::Error),

    /// The client went away, or a load/download was cancelled on purpose. Not a failure.
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn io(path: impl std::fmt::Display, source: std::io::Error) -> Self {
        Error::Io { path: path.to_string(), source }
    }

    pub fn engine(backend: &'static str, message: impl Into<String>) -> Self {
        Error::Engine { backend, message: message.into() }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Error::BadRequest { message: message.into() }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            // 503 rather than 400: the request is fine, the server is not ready for it — which
            // is what makes a retry the right client behaviour.
            Error::NoModelLoaded | Error::BackendUnavailable { .. } => StatusCode::SERVICE_UNAVAILABLE,
            Error::ModelNotFound { .. } => StatusCode::NOT_FOUND,
            Error::BadRequest { .. } => StatusCode::BAD_REQUEST,
            Error::Catalog { .. } => StatusCode::BAD_GATEWAY,
            Error::Cancelled => StatusCode::from_u16(499).unwrap_or(StatusCode::BAD_REQUEST),
            Error::Engine { .. } | Error::Download { .. } | Error::Io { .. } | Error::Core(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// OpenAI's `error.type` taxonomy, so an SDK's error handling works unchanged.
    pub fn openai_type(&self) -> &'static str {
        match self {
            Error::BadRequest { .. } => "invalid_request_error",
            Error::ModelNotFound { .. } => "model_not_found",
            Error::NoModelLoaded | Error::BackendUnavailable { .. } => "service_unavailable",
            _ => "server_error",
        }
    }
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Serialize)]
struct ErrorDetail {
    message: String,
    r#type: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<String>,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = ErrorBody {
            error: ErrorDetail {
                message: self.to_string(),
                r#type: self.openai_type(),
                code: match &self {
                    Error::ModelNotFound { .. } => Some("model_not_found".into()),
                    Error::NoModelLoaded => Some("no_model_loaded".into()),
                    _ => None,
                },
            },
        };
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "No model loaded" must be a 503, not a 400: clients retry the former and give up on the
    /// latter, and giving up is the wrong behaviour while a model is still loading.
    #[test]
    fn statuses_match_what_a_client_should_do() {
        assert_eq!(Error::NoModelLoaded.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(Error::ModelNotFound { id: "x".into() }.status(), StatusCode::NOT_FOUND);
        assert_eq!(Error::bad_request("nope").status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn every_error_says_what_to_do() {
        let e = Error::NoModelLoaded.to_string();
        assert!(e.contains("load one"), "got {e}");
    }
}

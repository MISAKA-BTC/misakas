//! [`Error`](enum@Error) variants for the wRPC client library.

use thiserror::Error;
use wasm_bindgen::JsError;
use wasm_bindgen::JsValue;
use workflow_core::channel::ChannelError;
use workflow_core::sendable::*;
use workflow_http::error::Error as HttpError;
use workflow_rpc::client::error::Error as RpcError;
use workflow_rpc::client::error::WebSocketError;
use workflow_wasm::error::Error as WasmError;
use workflow_wasm::printable::*;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Custom(String),

    #[error("wRPC address error -> {0}")]
    UrlError(String),

    #[error("wRPC -> {0}")]
    RpcError(#[from] RpcError),

    #[error("Kaspa RpcApi -> {0}")]
    RpcApiError(#[from] kaspa_rpc_core::error::RpcError),

    #[error("Kaspa RpcApi -> {0}")]
    WebSocketError(#[from] WebSocketError),

    #[error("Notification subsystem -> {0}")]
    NotificationError(#[from] kaspa_notify::error::Error),

    #[error("Channel -> {0}")]
    ChannelError(String),

    #[error("Serde WASM bindgen serialization or deserialization error: {0}")]
    SerdeWasmBindgen(Sendable<Printable>),

    #[error("{0}")]
    JsValue(Sendable<Printable>),

    #[error("{0}")]
    ToValue(String),

    #[error("invalid network type: {0}")]
    NetworkType(#[from] kaspa_consensus_core::network::NetworkTypeError),

    #[error(transparent)]
    ConsensusWasm(#[from] kaspa_consensus_wasm::error::Error),

    #[error(transparent)]
    HttpError(#[from] HttpError),

    #[error(transparent)]
    WasmError(#[from] WasmError),

    #[error(transparent)]
    AddressError(#[from] kaspa_addresses::AddressError),

    #[error(transparent)]
    TomlError(#[from] toml::de::Error),

    #[error(transparent)]
    NetworkId(#[from] kaspa_consensus_core::network::NetworkIdError),
}

impl Error {
    pub fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl From<String> for Error {
    fn from(err: String) -> Self {
        Self::Custom(err)
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Self {
        Self::Custom(err.to_string())
    }
}

impl<T> From<ChannelError<T>> for Error {
    fn from(err: ChannelError<T>) -> Self {
        Error::ChannelError(err.to_string())
    }
}

impl From<serde_wasm_bindgen::Error> for Error {
    fn from(err: serde_wasm_bindgen::Error) -> Self {
        Error::SerdeWasmBindgen(Sendable(Printable::new(err.into())))
    }
}

impl From<JsValue> for Error {
    fn from(err: JsValue) -> Self {
        Error::JsValue(Sendable(Printable::new(err)))
    }
}

impl From<JsError> for Error {
    fn from(err: JsError) -> Self {
        Error::JsValue(Sendable(Printable::new(err.into())))
    }
}

impl From<Error> for JsValue {
    fn from(value: Error) -> Self {
        match value {
            Error::JsValue(err) => err.as_ref().into(),
            _ => JsValue::from(value.to_string()),
        }
    }
}

// impl From<workflow_wasm::serde::Error> for Error {
//     fn from(err: workflow_wasm::serde::Error) -> Self {
//         Self::ToValue(err.to_string())
//     }
// }

/// **Whether a call's error is this client's report that the WebSocket went away under it** — the
/// one sign a caller has that a node closed the connection instead of answering (a node built
/// before an op drops the WebSocket on it; MISAKA's operator CLI reads that as "the node predates
/// the op").
///
/// The generated `RpcApi` methods flatten every transport error into
/// `RpcError::RpcSubsystem(e.to_string())` (`kaspa_rpc_macros`' wRPC client), so the variant is gone
/// by the time a caller sees it; this compares against the two errors' own renderings, which is
/// what the flattening kept. A request pending when the socket closed fails with
/// [`workflow_rpc::client::error::Error::Disconnect`] (the protocol's `handle_disconnect`); one
/// issued after it fails with `WebSocketError::NotConnected`. Nothing else matches: a timeout, or a
/// node's own refusal (which crosses the wire as a response, not as a closed socket), is not a
/// lost connection.
///
/// **Why not `is_connected()` after the error**: the client reconnects by itself (`reconnect` is
/// set once `connect` succeeds), and the pending request is failed from another task, so the flag a
/// caller reads after the error may not have dropped yet, or may already be up again — a dropped
/// connection then reads as a refusal (review of ADR-0152 P2-10, finding 2).
pub fn rpc_error_is_connection_loss(error: &kaspa_rpc_core::error::RpcError) -> bool {
    let kaspa_rpc_core::error::RpcError::RpcSubsystem(text) = error else { return false };
    *text == RpcError::Disconnect.to_string() || *text == RpcError::WebSocketError(WebSocketError::NotConnected).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_rpc_core::error::RpcError as KaspaRpcError;

    /// Flattened exactly as the generated `RpcApi` methods flatten a transport error.
    fn flattened(e: RpcError) -> KaspaRpcError {
        KaspaRpcError::RpcSubsystem(e.to_string())
    }

    /// **A closed socket reads as lost; a timeout, a node's refusal and any other text do not.**
    #[test]
    fn a_closed_socket_is_a_connection_loss_and_nothing_else_is() {
        assert!(rpc_error_is_connection_loss(&flattened(RpcError::Disconnect)), "pending when the node closed the socket");
        assert!(
            rpc_error_is_connection_loss(&flattened(RpcError::WebSocketError(WebSocketError::NotConnected))),
            "issued after it closed"
        );
        assert!(!rpc_error_is_connection_loss(&flattened(RpcError::Timeout)), "a slow node is not an old node");
        assert!(!rpc_error_is_connection_loss(&KaspaRpcError::General("name at most one of bond, payoutAddress and claimId".into())));
        assert!(
            !rpc_error_is_connection_loss(&KaspaRpcError::General(RpcError::Disconnect.to_string())),
            "only the client's own flattening carries it"
        );
    }
}

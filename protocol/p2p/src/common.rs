use crate::{KaspadMessagePayloadType, convert::error::ConversionError, core::peer::PeerKey};
use kaspa_consensus_core::errors::{block::RuleError, consensus::ConsensusError, pruning::PruningImportError};
use kaspa_mining_errors::manager::MiningManagerError;
use std::time::Duration;
use thiserror::Error;

/// Default P2P communication timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120); // 2 minutes

#[derive(Error, Debug, Clone)]
pub enum ProtocolError {
    #[error("timeout expired after {0:?}")]
    Timeout(Duration),

    #[error("P2P protocol version mismatch - local: {0}, remote: {1}")]
    VersionMismatch(u32, u32),

    #[error("Network mismatch - local: {0}, remote: {1}")]
    WrongNetwork(String, String),

    /// Same network name, different genesis. Kept distinct from `WrongNetwork` and
    /// `WrongConsensusParams` so an operator reading the log can tell which of the three
    /// "you are not on my chain" cases they hit.
    #[error(
        "Genesis mismatch on network {0} - local: {1}, remote: {2}. The peer answers to this network name but builds on a different genesis"
    )]
    WrongGenesis(String, String, String),

    /// Same network name and genesis, different rule set. This is the case that forked testnet-22:
    /// an older build computing different overlay commitments, indistinguishable at handshake.
    #[error(
        "Consensus params mismatch on network {0} - local: {1}, remote: {2}. The peer runs different consensus rules and cannot agree with this node on block validity; syncing from it would fork"
    )]
    WrongConsensusParams(String, String, String),

    /// **This peer could not serve a pruning-point sidecar — which is not a statement about the
    /// chain** (audit3 H2/H9). Kept distinct from `Other` so the IBD can tell "the chain I just
    /// committed is unusable" from "one auxiliary snapshot did not arrive from THIS peer", because
    /// the first is a reason to fail closed forever and the second is a reason to ask somebody
    /// else. The server answers `found: false` as a matter of course: it holds exactly one snapshot
    /// and serves it only on an exact pruning-point match, so a peer whose pruning point advanced
    /// mid-sync answers this to an entirely honest request.
    #[error("peer cannot serve the {0} snapshot for pruning point {1}; another peer may still have it")]
    PruningSidecarUnavailable(&'static str, String),

    #[error("expected message type/s {0} but got {1:?}")]
    UnexpectedMessage(&'static str, Option<KaspadMessagePayloadType>),

    #[error("{0}")]
    ConversionError(#[from] ConversionError),

    #[error("{0}")]
    RuleError(#[from] RuleError),

    #[error("{0}")]
    PruningImportError(#[from] PruningImportError),

    #[error("{0}")]
    ConsensusError(#[from] ConsensusError),

    // TODO: discuss if such an error type makes sense here
    #[error("{0}")]
    MiningManagerError(#[from] MiningManagerError),

    #[error("{0}")]
    IdentityError(#[from] uuid::Error),

    #[error("{0}")]
    Other(&'static str),

    #[error("{0}")]
    OtherOwned(String),

    #[error("misbehaving peer: {0}")]
    MisbehavingPeer(String),

    #[error("peer connection is closed")]
    ConnectionClosed,

    #[error("incoming route capacity for message type {0:?} has been reached (peer: {1})")]
    IncomingRouteCapacityReached(KaspadMessagePayloadType, String),

    #[error("outgoing route capacity has been reached (peer: {0})")]
    OutgoingRouteCapacityReached(String),

    #[error("no flow has been registered for message type {0:?}")]
    NoRouteForMessageType(KaspadMessagePayloadType),

    #[error("peer {0} already exists")]
    PeerAlreadyExists(PeerKey),

    #[error("loopback connection - node is connecting to itself")]
    LoopbackConnection(PeerKey),

    #[error("got reject message: {0}")]
    Rejected(String),

    #[error("got reject message: {0}")]
    IgnorableReject(String),
}

/// String used as a P2P convention to signal connection is rejected because we are connecting to ourselves
const LOOPBACK_CONNECTION_MESSAGE: &str = "LOOPBACK_CONNECTION";

/// String used as a P2P convention to signal connection is rejected because the peer already exists
const DUPLICATE_CONNECTION_MESSAGE: &str = "DUPLICATE_CONNECTION";

impl ProtocolError {
    pub fn is_connection_closed_error(&self) -> bool {
        matches!(self, Self::ConnectionClosed)
    }

    pub fn can_send_outgoing_message(&self) -> bool {
        !matches!(self, Self::ConnectionClosed | Self::OutgoingRouteCapacityReached(_))
    }

    pub fn to_reject_message(&self) -> String {
        match self {
            Self::LoopbackConnection(_) => LOOPBACK_CONNECTION_MESSAGE.to_owned(),
            Self::PeerAlreadyExists(_) => DUPLICATE_CONNECTION_MESSAGE.to_owned(),
            err => err.to_string(),
        }
    }

    pub fn from_reject_message(reason: String) -> Self {
        if reason == LOOPBACK_CONNECTION_MESSAGE || reason == DUPLICATE_CONNECTION_MESSAGE {
            ProtocolError::IgnorableReject(reason)
        } else if reason.contains("cannot find full block") {
            let hint = "Hint: If this error persists, it might be due to the other peer having pruned block data after syncing headers and UTXOs. In such a case, you may need to reset the database.";
            let detailed_reason = format!("{}. {}", reason, hint);
            ProtocolError::Rejected(detailed_reason)
        } else if reason.contains("sent invalid chain block") {
            // **This one is about THEM, and the message did not say so.** The peer was syncing from
            // us and holds a block of our chain as invalid — but our own consensus accepted that
            // block, or we would not be serving it. So the disagreement is about the peer's rules
            // or its stored state, and naming the block (all the old text did) points at the one
            // thing that is not the problem.
            //
            // Two causes, and an operator can act on both: the peer runs an older build whose
            // rules refuse a block ours accepts, or it rejected the block once under such a build
            // and cached that verdict — which survives an upgrade, because a block already marked
            // invalid is never re-validated. Measured on testnet-11 after a consensus fix shipped:
            // upgrading the binary alone left a node still rejecting; the database had to go too.
            let hint = "Hint: this is the OTHER peer's state, not yours — it holds a block your \
                        consensus accepted as invalid. It is usually running an older build, or it \
                        cached the rejection under one (an upgrade alone does not clear that; the \
                        peer must also reset its database). Your node is unaffected and stays on \
                        its chain.";
            ProtocolError::Rejected(format!("{reason}. {hint}"))
        } else {
            ProtocolError::Rejected(reason)
        }
    }
}

/// Wraps an inner payload message into a valid `KaspadMessage`.
/// Usage:
/// ```ignore
/// let msg = make_message!(Payload::Verack, verack_msg)
/// ```
#[macro_export]
macro_rules! make_message {
    ($pattern:path, $msg:expr) => {{
        $crate::pb::KaspadMessage {
            payload: Some($pattern($msg)),
            response_id: $crate::BLANK_ROUTE_ID,
            request_id: $crate::BLANK_ROUTE_ID,
        }
    }};

    ($pattern:path, $msg:expr, $response_id:expr, $request_id: expr) => {{ $crate::pb::KaspadMessage { payload: Some($pattern($msg)), response_id: $response_id, request_id: $request_id } }};
}

#[macro_export]
macro_rules! make_response {
    ($pattern:path, $msg:expr, $response_id:expr) => {{ $crate::pb::KaspadMessage { payload: Some($pattern($msg)), response_id: $response_id, request_id: 0 } }};
}

#[macro_export]
macro_rules! make_request {
    ($pattern:path, $msg:expr, $request_id:expr) => {{ $crate::pb::KaspadMessage { payload: Some($pattern($msg)), response_id: 0, request_id: $request_id } }};
}

/// Macro to extract a specific payload type from an `Option<pb::KaspadMessage>`.
/// Usage:
/// ```ignore
/// let res = unwrap_message!(op, Payload::Verack)
/// ```
#[macro_export]
macro_rules! unwrap_message {
    ($op:expr, $pattern:path) => {{
        if let Some(msg) = $op {
            if let Some($pattern(inner_msg)) = msg.payload {
                Ok(inner_msg)
            } else {
                Err($crate::common::ProtocolError::UnexpectedMessage(stringify!($pattern), msg.payload.as_ref().map(|v| v.into())))
            }
        } else {
            Err($crate::common::ProtocolError::ConnectionClosed)
        }
    }};
}

#[macro_export]
macro_rules! unwrap_message_with_request_id {
    ($op:expr, $pattern:path) => {{
        if let Some(msg) = $op {
            if let Some($pattern(inner_msg)) = msg.payload {
                Ok((inner_msg, msg.request_id))
            } else {
                Err($crate::common::ProtocolError::UnexpectedMessage(stringify!($pattern), msg.payload.as_ref().map(|v| v.into())))
            }
        } else {
            Err($crate::common::ProtocolError::ConnectionClosed)
        }
    }};
}

/// Macro to await a channel `Receiver<pb::KaspadMessage>::recv` call with a default/specified timeout and expect a specific payload type.
/// Usage:
/// ```ignore
/// let res = dequeue_with_timeout!(receiver, Payload::Verack) // Uses the default timeout
/// // or:
/// let res = dequeue_with_timeout!(receiver, Payload::Verack, Duration::from_secs(30))
/// ```
#[macro_export]
macro_rules! dequeue_with_timeout {
    ($receiver:expr, $pattern:path) => {{
        match tokio::time::timeout($crate::common::DEFAULT_TIMEOUT, $receiver.recv()).await {
            Ok(op) => {
                $crate::unwrap_message!(op, $pattern)
            }
            Err(_) => Err($crate::common::ProtocolError::Timeout($crate::common::DEFAULT_TIMEOUT)),
        }
    }};
    ($receiver:expr, $pattern:path, $timeout_duration:expr) => {{
        match tokio::time::timeout($timeout_duration, $receiver.recv()).await {
            Ok(op) => {
                $crate::unwrap_message!(op, $pattern)
            }
            Err(_) => Err($crate::common::ProtocolError::Timeout($timeout_duration)),
        }
    }};
}

/// Macro to indefinitely await a channel `Receiver<pb::KaspadMessage>::recv` call and expect a specific payload type (without a timeout).
/// Usage:
/// ```ignore
/// let res = dequeue!(receiver, Payload::Verack)
/// ```
#[macro_export]
macro_rules! dequeue {
    ($receiver:expr, $pattern:path) => {{ $crate::unwrap_message!($receiver.recv().await, $pattern) }};
}

#[macro_export]
macro_rules! dequeue_with_request_id {
    ($receiver:expr, $pattern:path) => {{ $crate::unwrap_message_with_request_id!($receiver.recv().await, $pattern) }};
}

#[cfg(test)]
mod reject_classification_tests {
    use super::*;

    /// **A reject is either about this node or about the peer, and the text has to say which.**
    ///
    /// `sent invalid chain block` is the second kind: the peer holds a block our own consensus
    /// accepted. Operators read the bare form as a fault in their own node — the message names a
    /// block, and a named block looks like an accusation — so the hint has to name the peer as the
    /// subject and say what clears it. It is also the one case where upgrading is not enough,
    /// which is exactly the part nobody guesses.
    #[test]
    fn an_invalid_chain_block_reject_says_it_is_the_peers_state() {
        let e = ProtocolError::from_reject_message("sent invalid chain block abc123".to_owned());
        let ProtocolError::Rejected(text) = e else { panic!("an invalid-chain-block reject is not ignorable") };
        assert!(text.contains("sent invalid chain block abc123"), "the original reason survives: {text}");
        assert!(text.contains("OTHER peer's state"), "it must name whose problem this is: {text}");
        assert!(text.contains("reset its database"), "and that an upgrade alone does not clear it: {text}");
    }

    /// The existing classifications are unchanged — a hint added for one reason must not reclassify
    /// another, and the two ignorable ones must stay ignorable or every duplicate connection
    /// becomes a warning.
    #[test]
    fn the_other_classifications_are_untouched() {
        assert!(matches!(
            ProtocolError::from_reject_message(LOOPBACK_CONNECTION_MESSAGE.to_owned()),
            ProtocolError::IgnorableReject(_)
        ));
        assert!(matches!(
            ProtocolError::from_reject_message(DUPLICATE_CONNECTION_MESSAGE.to_owned()),
            ProtocolError::IgnorableReject(_)
        ));
        let ProtocolError::Rejected(t) = ProtocolError::from_reject_message("cannot find full block x".to_owned()) else {
            panic!("still rejected")
        };
        assert!(t.contains("pruned block data"), "the pruning hint still applies: {t}");
        let ProtocolError::Rejected(t) = ProtocolError::from_reject_message("something else entirely".to_owned()) else {
            panic!("still rejected")
        };
        assert_eq!(t, "something else entirely", "an unrecognised reason is passed through verbatim");
    }
}

pub mod channel;
pub mod convert;
pub mod ext;
pub mod macros;
pub mod ops;

/// Maximum decoded gRPC message size to send and receive
pub const RPC_MAX_MESSAGE_SIZE: usize = 1024 * 1024 * 1024; // 1GB

/// kaspa-pq gRPC types and service traits.
///
/// Wire-level package is `protowire.kaspapq` (see ADR-0006 §2 +
/// `proto/messages.proto`); prost generates the types directly inside
/// this `protowire` module (no inner `kaspapq` segment) because the
/// only level after the dot is the service name. The wire identifiers
/// carrying the `kaspapq` package name remain the authoritative form
/// on the network — a mainline Kaspa client binding to
/// `protowire.RPC.MessageStream` will fail to handshake.
///
/// The renamed gRPC service `KaspaPqRpcService` is re-exported under
/// the upstream-compatible module names `rpc_server` / `rpc_client`
/// so the existing `connection_handler` and `client/lib.rs` import
/// lines remain stable. Inner types `KaspaPqRpcService{,Client,Server}`
/// keep the kaspa-pq-explicit name on the wire.
pub mod protowire {
    tonic::include_proto!("protowire.kaspapq");
    pub use self::kaspa_pq_rpc_service_client as rpc_client;
    pub use self::kaspa_pq_rpc_service_server as rpc_server;
}

//! Local endpoint registry — `~/.misaka/<network-id>/endpoints.json` (design §7).
//!
//! `kaspad` writes the loopback RPC endpoints it actually bound when it starts; the
//! miner / validator / unified CLI read it so the operator never has to type a port.
//! The registry is a HINT, not a trust anchor: a reader uses it only to pick a default
//! endpoint and MUST still re-verify the node's `network_id` / genesis after connecting.
//!
//! Resolution order for any one endpoint (design §7.3):
//!   explicit CLI flag  >  env var  >  config file  >  endpoint registry  >  network-id
//!   default loopback  >  helpful error.
//! clap merges the first three into `explicit`; this crate supplies the registry layer
//! and the network-default fallback via [`resolve`].

use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Schema tag written into the file so a future format change is detectable.
pub const SCHEMA: &str = "misaka-endpoints-v1";

/// The loopback RPC endpoints a node bound, by protocol. `None` = not enabled.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Endpoints {
    pub node_grpc: Option<String>,
    pub node_wrpc_borsh: Option<String>,
    pub node_wrpc_json: Option<String>,
    pub evm_rpc_http: Option<String>,
}

impl Endpoints {
    /// The endpoint for `kind`, if the node enabled it. (P2P is not an RPC endpoint and
    /// is intentionally absent.)
    pub fn get(&self, kind: EndpointKind) -> Option<&str> {
        match kind {
            EndpointKind::NodeGrpc => self.node_grpc.as_deref(),
            EndpointKind::NodeWrpcBorsh => self.node_wrpc_borsh.as_deref(),
            EndpointKind::NodeWrpcJson => self.node_wrpc_json.as_deref(),
            EndpointKind::EvmRpcHttp => self.evm_rpc_http.as_deref(),
            EndpointKind::P2p => None,
        }
    }
}

/// The on-disk registry document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EndpointRegistry {
    pub schema: String,
    pub network_id: String,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub profile: Option<String>,
    pub endpoints: Endpoints,
}

/// `~/.misaka/<network-id>/endpoints.json`, or `None` if the home dir is unknown.
pub fn registry_path(network_id: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".misaka").join(network_id).join("endpoints.json"))
}

impl EndpointRegistry {
    pub fn new(network_id: &str, endpoints: Endpoints, profile: Option<String>) -> Self {
        Self { schema: SCHEMA.to_string(), network_id: network_id.to_string(), pid: Some(std::process::id()), profile, endpoints }
    }

    /// Write the registry to its canonical path (creating `~/.misaka/<network-id>/`),
    /// `0600` on unix. Best-effort: a node should not fail to start because it could not
    /// write a convenience file, so the caller typically logs and ignores any error.
    pub fn write(&self) -> std::io::Result<()> {
        let path = registry_path(&self.network_id).ok_or_else(|| std::io::Error::other("cannot determine home directory"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// Load the registry for `network_id`. Returns `None` if it is absent, unreadable,
    /// malformed, or for a different network (it is a best-effort hint, never an error).
    pub fn load(network_id: &str) -> Option<EndpointRegistry> {
        let path = registry_path(network_id)?;
        let text = std::fs::read_to_string(path).ok()?;
        let reg: EndpointRegistry = serde_json::from_str(&text).ok()?;
        (reg.schema == SCHEMA && reg.network_id == network_id).then_some(reg)
    }
}

/// Resolve one endpoint, design §7.3: an `explicit` value (CLI/env/config, already merged
/// by the caller) wins; else the `registry` (the node's actually-bound endpoint, which may
/// be a non-standard port); else the network-id default loopback (`127.0.0.1:<port>`).
pub fn resolve(network_id: &NetworkId, kind: EndpointKind, explicit: Option<&str>, registry: Option<&EndpointRegistry>) -> String {
    if let Some(e) = explicit {
        return e.to_string();
    }
    if let Some(e) = registry.and_then(|r| r.endpoints.get(kind)) {
        return e.to_string();
    }
    format!("127.0.0.1:{}", network_id.default_endpoint_port(kind))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn nid(s: &str) -> NetworkId {
        NetworkId::from_str(s).unwrap()
    }

    #[test]
    fn resolve_precedence_explicit_then_registry_then_default() {
        let net = nid("testnet-10");
        // explicit wins over everything
        assert_eq!(resolve(&net, EndpointKind::NodeWrpcBorsh, Some("1.2.3.4:9"), None), "1.2.3.4:9");
        // registry (the node's actual bound port, here a non-standard 27610) beats the
        // network default 27210
        let reg = EndpointRegistry::new(
            "testnet-10",
            Endpoints { node_wrpc_borsh: Some("127.0.0.1:27610".into()), ..Default::default() },
            None,
        );
        assert_eq!(resolve(&net, EndpointKind::NodeWrpcBorsh, None, Some(&reg)), "127.0.0.1:27610");
        // neither → network default loopback
        assert_eq!(resolve(&net, EndpointKind::NodeGrpc, None, None), "127.0.0.1:26210");
        assert_eq!(resolve(&net, EndpointKind::NodeWrpcBorsh, None, Some(&reg)), "127.0.0.1:27610");
        // an endpoint the registry doesn't carry falls through to the default
        assert_eq!(resolve(&net, EndpointKind::NodeGrpc, None, Some(&reg)), "127.0.0.1:26210");
    }

    #[test]
    fn registry_json_roundtrip_and_schema_guard() {
        let reg = EndpointRegistry::new(
            "devnet",
            Endpoints {
                node_grpc: Some("127.0.0.1:26610".into()),
                node_wrpc_borsh: Some("127.0.0.1:27610".into()),
                ..Default::default()
            },
            Some("local-validator".into()),
        );
        let json = serde_json::to_string(&reg).unwrap();
        let back: EndpointRegistry = serde_json::from_str(&json).unwrap();
        assert_eq!(reg, back);
        // a wrong schema/network is rejected by load()'s guard logic
        assert!(json.contains("misaka-endpoints-v1"));
    }
}

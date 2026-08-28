use kaspa_consensus_core::subnets::SubnetworkId;
use kaspa_core::{
    kaspad_env::{name, version},
    time::unix_now,
};
use kaspa_utils::networking::{NetAddress, PeerId};

/// Maximum allowed length for the user agent field in a version message `VersionMessage`.
pub const MAX_USER_AGENT_LEN: usize = 256;

pub struct Version {
    pub protocol_version: u32,
    pub network: String,
    pub services: u64, // TODO
    pub timestamp: u64,
    pub address: Option<NetAddress>,
    pub id: PeerId,
    pub user_agent: String,
    pub disable_relay_tx: bool,
    pub subnetwork_id: Option<SubnetworkId>,
    /// The genesis this node builds on. Empty from peers predating this field.
    pub genesis_hash: Vec<u8>,
    /// Fingerprint of this node's consensus parameters — see `Params::consensus_params_id`.
    /// Empty from peers predating this field. Opaque: only ever compared, never interpreted.
    pub consensus_params_id: Vec<u8>,
    /// The fingerprint with every activation fence normalised away — what a handshake may refuse
    /// on. Empty from peers predating the field (then the exact `consensus_params_id` comparison
    /// stands, which is what those peers expect anyway).
    pub consensus_identity_id: Vec<u8>,
    /// The activation fences alone. Never a gate; reported so a schedule difference is legible.
    pub consensus_schedule_id: Vec<u8>,
}

impl Version {
    pub fn new(
        address: Option<NetAddress>,
        id: PeerId,
        network: String,
        subnetwork_id: Option<SubnetworkId>,
        protocol_version: u32,
        genesis_hash: Vec<u8>,
        consensus_params_id: Vec<u8>,
        consensus_identity_id: Vec<u8>,
        consensus_schedule_id: Vec<u8>,
    ) -> Self {
        Self {
            protocol_version,
            network,
            services: 0, // TODO: get number of live services
            timestamp: unix_now(),
            address,
            id,
            user_agent: format!("/{}:{}/", name(), version()),
            disable_relay_tx: false,
            subnetwork_id,
            genesis_hash,
            consensus_params_id,
            consensus_identity_id,
            consensus_schedule_id,
        }
    }

    pub fn add_user_agent(&mut self, name: &str, version: &str, comments: &[String]) {
        let comments = if !comments.is_empty() { format!("({})", comments.join("; ")) } else { "".to_string() };
        let new_user_agent = format!("{}:{}{}", name, version, comments);
        self.user_agent = format!("{}{}/", self.user_agent, new_user_agent);
        self.user_agent.truncate(MAX_USER_AGENT_LEN);
    }
}

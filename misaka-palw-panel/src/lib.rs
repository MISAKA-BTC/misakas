//! RFC-0010's dormant transition engine. This is NOT an entropy primitive or a licence to activate.
//! The caller supplies authenticated selected-chain facts through `ConsensusViewV1`; neither a
//! producer's envelope nor local readiness/RPC state may implement that authority in production.
//! Every fold is transactional and returns a new branch-scoped state. Legacy claims never enter it.

mod draw;
mod state;
mod types;

pub use draw::{panel_seed_v3, seat_order_v1, stratum_seat_order_v1, stratum_seed_v1};
pub use state::PermissionlessPanelStateV1;
pub use types::*;

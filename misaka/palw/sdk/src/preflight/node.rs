//! **What a live node says** (RFC-0002 Part II §II.2.6, `--node`): the facts a preflight judges against when a node is named — the
//! tip (the default height), the network, and the registry's reading of every class it holds: its lifecycle state, the ready seats
//! against the required, the seating (the independent operators against the floor), and the base population. A plain, serialisable
//! struct: the CLI that holds an RPC connection (`misaka model preflight --node`) fills it from `getPalwModelRegistry`; `palw-class
//! preflight --node-facts <file>` reads the same facts from a JSON file (a node's answer saved, or written by a test), so the
//! library has no network of its own and a verdict is reproducible from its inputs.

use serde::{Deserialize, Serialize};

/// One class as the node's registry reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeClassFact {
    pub class_id: String,
    pub artifact_root: String,
    /// The lifecycle state (`Prefetching`, `Probation { probes_passed: 3 }`, `Active`, …).
    pub state: String,
    pub ready_seats: u32,
    pub required_ready_seats: u32,
    /// The seating `palw_class_seating` reads (past the fence): the independent operators and the floor, the base population, the
    /// share of the class's claims whose outsider would hold it. `None` below the fence.
    #[serde(default)]
    pub seating: Option<NodeSeatingFact>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeSeatingFact {
    pub ready_operators: u32,
    pub needed_operators: u32,
    pub independent_operators: u32,
    pub needed_independent: u32,
    pub base_operators: u32,
    pub licensable_share_permille: u16,
}

/// What a node said about the network.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeFacts {
    /// The network the node runs (`testnet-12`).
    pub network: String,
    /// The node's tip, in DAA.
    pub tip_daa: u64,
    pub seat_count: u32,
    pub spare_seats: u32,
    /// Bonds on the network with the headroom to be a seat.
    pub bonds_with_headroom: u32,
    pub classes: Vec<NodeClassFact>,
}

impl NodeFacts {
    pub fn parse(text: &str) -> Result<NodeFacts, String> {
        serde_json::from_str(text).map_err(|e| format!("node facts: {e}"))
    }

    /// The class with this id (hex, case-insensitive).
    pub fn class(&self, class_id: &str) -> Option<&NodeClassFact> {
        self.classes.iter().find(|c| c.class_id.eq_ignore_ascii_case(class_id))
    }

    /// The classes registered over this artifact root (hex, case-insensitive): a root another class already owns.
    pub fn classes_over_root(&self, root: &str) -> Vec<&NodeClassFact> {
        self.classes.iter().filter(|c| c.artifact_root.eq_ignore_ascii_case(root)).collect()
    }

    /// The largest base population any class's seating reads: how many operators the network could draw an outsider from.
    pub fn base_operators(&self) -> Option<u32> {
        self.classes.iter().filter_map(|c| c.seating.as_ref().map(|s| s.base_operators)).max()
    }
}

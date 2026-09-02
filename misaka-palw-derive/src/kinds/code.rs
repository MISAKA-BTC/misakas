//! Kind `code` (ADR-0078 Decision 8). Grammar, transformer and canonical writer live here.

use crate::{Grammar, Transformer};

/// This kind's grammar and transformer, as the registry sees them. Empty until the kind lands.
pub fn register() -> (Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>) {
    (vec![], vec![])
}

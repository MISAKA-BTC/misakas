//! The registered grammars and transformers (ADR-0078 Decisions 2 and 3): the ones this build
//! ships, addressable by name and by id. A name that is not here is not a grammar or a transformer
//! under this ADR; an id that is not here is one another build named.

use crate::ids::{grammar_id_v1, transformer_id};
use crate::{Grammar, Transformer};
use kaspa_hashes::Hash64;
use std::sync::OnceLock;

struct Registry {
    grammars: Vec<Box<dyn Grammar>>,
    transformers: Vec<Box<dyn Transformer>>,
}

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry { grammars: crate::kinds::grammars(), transformers: crate::kinds::transformers() })
}

pub fn grammar_by_name(name: &str) -> Option<&'static dyn Grammar> {
    registry().grammars.iter().find(|g| g.name() == name).map(|g| g.as_ref())
}

pub fn grammar_by_id(id: &Hash64) -> Option<&'static dyn Grammar> {
    registry().grammars.iter().find(|g| grammar_id_v1(g.name()) == *id).map(|g| g.as_ref())
}

pub fn transformer_by_name(name: &str) -> Option<&'static dyn Transformer> {
    registry().transformers.iter().find(|t| t.manifest().name == name).map(|t| t.as_ref())
}

pub fn transformer_by_id(id: &Hash64) -> Option<&'static dyn Transformer> {
    registry().transformers.iter().find(|t| transformer_id(&t.manifest()) == *id).map(|t| t.as_ref())
}

/// Every registered grammar name.
pub fn grammar_names() -> Vec<&'static str> {
    registry().grammars.iter().map(|g| g.name()).collect()
}

/// Every registered transformer name, with its kind and grammar.
pub fn transformer_names() -> Vec<(&'static str, u16, &'static str)> {
    registry()
        .transformers
        .iter()
        .map(|t| {
            let m = t.manifest();
            (m.name, m.kind, m.grammar)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_ids_are_unique_and_every_transformer_names_a_registered_grammar() {
        let mut gnames = std::collections::BTreeSet::new();
        for g in grammar_names() {
            assert!(gnames.insert(g), "grammar {g} registered twice");
            assert!(grammar_by_id(&grammar_id_v1(g)).is_some());
        }
        let mut tnames = std::collections::BTreeSet::new();
        let mut tids = std::collections::BTreeSet::new();
        for (name, kind, grammar) in transformer_names() {
            assert!(tnames.insert(name), "transformer {name} registered twice");
            assert!(gnames.contains(grammar), "transformer {name} names unregistered grammar {grammar}");
            assert_ne!(kind, 0);
            let t = transformer_by_name(name).unwrap();
            let id = transformer_id(&t.manifest());
            assert!(tids.insert(id), "transformer {name}'s id collides");
            assert!(transformer_by_id(&id).is_some());
            assert_eq!(t.manifest().source_tree_sha256, crate::SOURCE_TREE_SHA256_HEX, "{name} names this build");
        }
    }
}

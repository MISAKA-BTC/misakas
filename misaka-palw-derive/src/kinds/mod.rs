//! The kinds this build ships (ADR-0078 Decision 8, in Decision 11's order of arrival). Each
//! kind is one module holding its grammar, its transformer and its canonical writer, and
//! registers itself through `register()`. Nothing else in the crate knows a kind by name.

use crate::{Grammar, Transformer};

pub mod cad;
pub mod code;
pub mod image;
pub mod map;
pub mod music;
pub mod scene;
pub mod simulation;

fn all() -> Vec<(Vec<Box<dyn Grammar>>, Vec<Box<dyn Transformer>>)> {
    vec![scene::register(), music::register(), code::register(), cad::register(), map::register(), image::register(), simulation::register()]
}

pub fn grammars() -> Vec<Box<dyn Grammar>> {
    all().into_iter().flat_map(|(g, _)| g).collect()
}

pub fn transformers() -> Vec<Box<dyn Transformer>> {
    all().into_iter().flat_map(|(_, t)| t).collect()
}

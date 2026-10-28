//! Emit the `PALW-BASE-0` Known-Answer Test vectors as JSON on stdout.
//!
//! The file is not committed: it is derived from the primitives, and `misaka_palw_base0::kat`'s
//! `KAT_DIGEST` is what freezes it. Publishing it alongside a release is what lets a third party
//! write a conforming implementation without reading this repository's Rust — which is the only
//! way the two-independent-implementations bar in ADR-0040 can be met by someone who is actually
//! independent of it.
//!
//! ```text
//! cargo run --release -p misaka-palw-base0 --bin base0-kat > palw-base0-kat-v1.json
//! ```

fn main() {
    let groups = misaka_palw_base0::kat::groups();
    let total: usize = groups.iter().map(|g| g.vectors.len()).sum();
    eprintln!(
        "{total} vectors across {} ops, digest {}",
        groups.len(),
        faster_hex::hex_string(&misaka_palw_base0::kat::digest(&groups))
    );
    print!("{}", misaka_palw_base0::kat::to_json(&groups));
}

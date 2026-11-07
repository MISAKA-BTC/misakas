//! **A genesis card reads its class roots out of a committed measurement, at compile time.**
//!
//! A registration pins the operand-inventory root of a class. Deriving it needs the artifact, which
//! a binary being compiled does not have, so the value was a `pub const` byte array a human pasted
//! in. That is the mechanism that shipped a flat `artifact_digest()` where the inventory root
//! belonged — on testnet-11, and then again on testnet-12 over a byte-identical artifact — and each
//! time the network's dense tier produced zero blocks.
//!
//! **Neither a test nor a better comment fixes that, because both leave a human typing the hash.**
//! What this module does instead: the `.palwmanifest` that `palw-class manifest` writes beside an
//! artifact is a few hundred bytes, so it is COMMITTED, and the genesis constant is parsed out of it
//! by a `const fn` at compile time. There is no second copy of the value to disagree with the first.
//! The only way to change what a genesis registers is to re-run the CLI over the artifact and commit
//! its output; the only way to get it wrong is for the CLI to derive it wrong, which is the same
//! expression the runtime evaluates when it decides whether it can serve the class.
//!
//! The manifest stays checkable on any host that holds the artifact: `palw-class manifest --check`
//! re-derives every row, and `--palw-verify-class-manifest` makes a node refuse to start on a
//! disagreement.

use crate::Hash64;

/// **The manifest the fleet's 2M Qwen2.5 artifact measured**, byte for byte as
/// `palw-class manifest` wrote it (169.58.39.220, 2026-09-23).
pub const QWEN25_A16_2M_MANIFEST_V1: &str = include_str!("class-manifests/qwen25-1.5b-a16-2m.palwmanifest");

/// **The manifest the fleet's 8k Qwen2.5 artifact measured** (`qwen25-convert --a16 --n-ctx 8192`
/// over the same source as the 2M artifact; 5.104.81.23, 2026-09-23), byte for byte as
/// `palw-class manifest` wrote it and `palw-class manifest --check` re-derived it. One row: the file
/// pairs with the `graph-v7@8192` class only.
pub const QWEN25_A16_8K_MANIFEST_V1: &str = include_str!("class-manifests/qwen25-1.5b-a16-8k.palwmanifest");

/// **The manifest the fleet's Qwen3.6 mapping measured** (`qwen36.palwq36`, the 512-context
/// conversion the hybrid tier has run since testnet-11), byte for byte as `palw-class manifest`
/// wrote it (169.58.39.220, 2026-09-23).
///
/// Four rows, sorted by class id, and the reason this file exists is the pair of them that disagree:
/// the `graph-v3` and v1 rows register the mapping's own `artifact_root` (`f4aad4fd…`), while the two
/// HELD `graph-v7` rows register the operand-inventory root under that graph (`f01230ae…`). Testnet-12
/// registers a held row and pinned `f4aad4fd…` for it — the third instance of the substitution this
/// module was written to end, found the day the sidecar was first derived for this artifact.
pub const QWEN36_512_MANIFEST_V1: &str = include_str!("class-manifests/qwen36-35b-a3b-512.palwmanifest");

/// One nibble, or `None` for a byte that is not lowercase hex. Named rather than inlined so the
/// refusal below can say WHICH character it choked on.
const fn nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// 128 lowercase hex characters at `at`, as a 64-byte hash.
///
/// `panic!` in a `const fn` is a COMPILE error, which is the whole point: a malformed manifest must
/// not produce a hash at all, rather than producing a plausible one.
const fn hash_at(src: &[u8], at: usize) -> Hash64 {
    let mut out = [0u8; 64];
    let mut i = 0;
    while i < 64 {
        let (hi, lo) = (src[at + i * 2], src[at + i * 2 + 1]);
        let (hi, lo) = match (nibble(hi), nibble(lo)) {
            (Some(h), Some(l)) => (h, l),
            _ => panic!("a committed class manifest carries a non-hex character where a 64-byte hash belongs"),
        };
        out[i] = hi * 16 + lo;
        i += 1;
    }
    Hash64::from_bytes(out)
}

/// Does `src[at..]` begin with `needle`?
const fn starts_with_at(src: &[u8], at: usize, needle: &[u8]) -> bool {
    if at + needle.len() > src.len() {
        return false;
    }
    let mut i = 0;
    while i < needle.len() {
        if src[at + i] != needle[i] {
            return false;
        }
        i += 1;
    }
    true
}

/// The offset just past the `n`th occurrence of `needle`, or a compile error naming what was sought.
const fn after_nth(src: &[u8], needle: &[u8], n: usize) -> usize {
    let mut seen = 0;
    let mut at = 0;
    while at < src.len() {
        if starts_with_at(src, at, needle) {
            seen += 1;
            if seen == n {
                return at + needle.len();
            }
        }
        at += 1;
    }
    panic!("a committed class manifest does not carry the field this genesis card reads from it")
}

/// The first byte of the quoted string that follows `"<key>":` — the hash's own first hex character.
const fn quoted_value_at(src: &[u8], key: &[u8], occurrence: usize) -> usize {
    let mut at = after_nth(src, key, occurrence);
    // past the colon and any spaces, then past the opening quote
    while at < src.len() && (src[at] == b':' || src[at] == b' ') {
        at += 1;
    }
    if at >= src.len() || src[at] != b'"' {
        panic!("a committed class manifest's field is not a quoted string");
    }
    at + 1
}

/// **The inventory root of the `occurrence`th class in a committed manifest.**
///
/// Positional rather than keyed by class id, because a `const fn` that searched for one id would have
/// to carry that id as a second literal — and a second literal is the thing this module exists to
/// remove. The pairing is asserted instead, at test time, against the class id the profile derives:
/// see `t12_genesis_reads_its_root_from_the_committed_manifest`.
pub const fn inventory_root_of_class(manifest: &str, occurrence: usize) -> Hash64 {
    hash_at(manifest.as_bytes(), quoted_value_at(manifest.as_bytes(), b"\"inventory_root\"", occurrence))
}

/// The class id of the `occurrence`th class, so a test can hold the positional read to the profile.
pub const fn class_id_of_class(manifest: &str, occurrence: usize) -> Hash64 {
    hash_at(manifest.as_bytes(), quoted_value_at(manifest.as_bytes(), b"\"class_id\"", occurrence))
}

/// The artifact digest the manifest is bound to — for a build that wants to say WHICH file its
/// genesis was measured from. Deliberately not used as a root anywhere: that substitution is what
/// [`crate::palw_class_identity_v1`] exists to make unrepresentable.
pub const fn artifact_digest_of(manifest: &str) -> Hash64 {
    hash_at(manifest.as_bytes(), quoted_value_at(manifest.as_bytes(), b"\"artifact_digest\"", 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed manifest parses, and the two hashes it yields are the ones a reader sees in the
    /// file. Transcribed here ONCE, in a test, so a bad `const fn` cannot pass by agreeing with
    /// itself.
    #[test]
    fn the_committed_manifest_parses_to_what_it_says() {
        assert_eq!(
            inventory_root_of_class(QWEN25_A16_2M_MANIFEST_V1, 1).to_string(),
            "f63af2c46b3816f6a16c168a130d28ec95b0ca5107da76d4abe24e4d9396e65c80d6d83014c47855e2394af20ce3333a59359fd3eb06090fdc7bfd502f75c7c2"
        );
        assert_eq!(
            class_id_of_class(QWEN25_A16_2M_MANIFEST_V1, 1).to_string(),
            "74c67e63d9c03daa05880c5d8a47b354ca20e952b1a2d49c107abe14f890a9c50790371bb715c7cea33ae8ac9213a3a63da409070cb2c98b8e861598db902f7a"
        );
        assert_eq!(
            artifact_digest_of(QWEN25_A16_2M_MANIFEST_V1).to_string(),
            "b5baca6364135a62bd4512a58c2ca747373019a495505968d884b2c0e52e4ce9322a8af0db90d3ec1001c15b5a89fe0e8008519e55a5f3f865e15562fb8967ae"
        );
    }

    /// The 8k sidecar, transcribed the same way: the three values `palw-class manifest` printed when it
    /// wrote the file.
    #[test]
    fn the_committed_8k_manifest_parses_to_what_it_says() {
        assert_eq!(
            inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1).to_string(),
            "88096dc177826d880c1c5fca4ec93cffe5ab51af108ed169a8e03cd4726308f91263f79f81904b043327bfa277e3558b1656f14259f6dd33603a9f91871aae20"
        );
        assert_eq!(
            class_id_of_class(QWEN25_A16_8K_MANIFEST_V1, 1).to_string(),
            "ebf44d0aa09ff7d1310a7855ab4005c275cdce557e32c269b0f3a984ea80ca73ad1ea0c9b1c0539c8ae04abb5fe24399e67e05bb0895a3dee82253e772246d01"
        );
        assert_eq!(
            artifact_digest_of(QWEN25_A16_8K_MANIFEST_V1).to_string(),
            "f4af38d91cb4d012189051742a98322ba9a20c9e62bbc9383b0d2c03727d0e5dbb7292be7186b7bc948a28ca0e02a4f59191edc71ff37bb4d7067b559bc3f0d0"
        );
        assert_ne!(artifact_digest_of(QWEN25_A16_8K_MANIFEST_V1), inventory_root_of_class(QWEN25_A16_8K_MANIFEST_V1, 1));
    }

    /// **The digest and the root are different values in this very file**, which is what made the
    /// substitution possible and what the manifest now keeps apart by NAME rather than by adjacency.
    #[test]
    fn the_digest_is_not_the_root() {
        assert_ne!(
            artifact_digest_of(QWEN25_A16_2M_MANIFEST_V1),
            inventory_root_of_class(QWEN25_A16_2M_MANIFEST_V1, 1),
            "if these were equal the manifest would be describing a class nothing can open"
        );
    }
}

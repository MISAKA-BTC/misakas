// ADR-0078 Decision 3 — "the build's source-tree hash", the manifest field that makes
// `transformer_id` name the CODE and not merely a string.
//
// This file is the ONE spelling of that hash: `build.rs` `include!`s it to compute the value the
// binary carries, and the crate compiles it so a consumer can recompute the same value from a
// checkout and compare. A name nothing can recompute is a declaration, and this repository keeps
// recording declarations-nobody-checks as the defect; recomputability is the whole point of a
// content name.
//
// Because `build.rs` includes this file textually it may use NO `use` statements (they would
// collide with the build script's own) and no `//!` inner docs (an inner doc comment is not
// permitted mid-file). Fully-qualified paths only. Its `#[cfg(test)]` module is stripped in the
// build script, which is compiled without `cfg(test)`.
//
// What the hash covers, stated so that a reader can audit the claim:
//
//   * every regular file under `src/`, at any depth, whose file name does not begin with `.`
//     — NOT only `*.rs`. A kind that `include_str!`s a table beside itself would otherwise
//       change what a transformer computes without moving the transformer's id, which is
//       exactly the "the id names the code" claim failing silently. Dot-files are the editor's
//       and the OS's droppings (`.DS_Store`), never source, and hashing them would make the id
//       depend on which host checked the tree out.
//   * sorted by the file's `/`-separated path relative to the crate root, globally — not
//     per-directory, so the order is a property of the path strings and nothing else.
//   * framed `path \0 len(u64 le) \0 bytes` per file, so no concatenation of a path and a body
//     can be read as another pair.
//
// It is a plain SHA-256 with no domain: this value is an INPUT to the manifest, and the
// manifest's own hash (`ids::transformer_id`) is domain-separated.

/// Every file the source-tree hash covers, in the order it hashes them: paths relative to the
/// crate root, `/`-separated, globally sorted. `root` is the crate root (the directory holding
/// `Cargo.toml` and `src/`).
pub fn source_files(root: &std::path::Path) -> Vec<String> {
    let mut out = Vec::new();
    walk(&root.join("src"), root, &mut out);
    out.sort();
    out
}

fn walk(dir: &std::path::Path, root: &std::path::Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        // A missing `src/` is not a source tree; the caller's own emptiness check names that,
        // because an unreadable directory silently hashing to "nothing" is the failure mode
        // this whole module exists to refuse.
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            walk(&path, root, out);
        } else if path.is_file() {
            let rel = match path.strip_prefix(root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            out.push(rel);
        }
    }
}

/// The bytes the hash is taken over: `path \0 len(u64 le) \0 bytes` per file, in
/// [`source_files`] order. Panics if a listed file cannot be read — a source-tree hash computed
/// over a tree that could not be read in full would be a name for code nobody has.
pub fn source_tree_preimage(root: &std::path::Path) -> Vec<u8> {
    let files = source_files(root);
    assert!(!files.is_empty(), "no source files under {}/src: this is not a checkout of the crate", root.display());
    let mut pre = Vec::new();
    for rel in &files {
        let bytes = std::fs::read(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        pre.extend_from_slice(rel.as_bytes());
        pre.push(0);
        pre.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        pre.push(0);
        pre.extend_from_slice(&bytes);
    }
    pre
}

/// The source-tree hash of a checkout, lower-case hex — the value a transformer manifest carries
/// in `source_tree_sha256`. A consumer holding an object and this crate's sources recomputes it
/// and compares with [`crate::SOURCE_TREE_SHA256_HEX`].
pub fn source_tree_sha256_hex(root: &std::path::Path) -> String {
    hex32(&sha256(&source_tree_preimage(root)))
}

/// Lower-case hex of a 32-byte digest.
pub fn hex32(digest: &[u8; 32]) -> String {
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push(char::from_digit(u32::from(*b >> 4), 16).expect("nibble"));
        s.push(char::from_digit(u32::from(*b & 0x0F), 16).expect("nibble"));
    }
    s
}

/// A small, dependency-free SHA-256 (FIPS 180-4), so `build.rs` needs no crate and the crate and
/// the build script cannot drift apart.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01,
        0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
        0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08,
        0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut a = h;
        for i in 0..64 {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ (!a[4] & a[6]);
            let t1 = a[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a = [t1.wrapping_add(t2), a[0], a[1], a[2], a[3].wrapping_add(t1), a[4], a[5], a[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(a[i]);
        }
    }
    let mut out = [0u8; 32];
    for i in 0..8 {
        out[4 * i..4 * i + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crate_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn sha256_known_vectors() {
        assert_eq!(hex32(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(hex32(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // 56 bytes: the length-padding boundary, where a wrong pad silently agrees on shorter inputs.
        assert_eq!(hex32(&sha256(&[b'a'; 56])), "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a");
    }

    /// **ADR-0078 Decision 3: `transformer_id` names THIS code.** The constant the manifests
    /// carry is recomputed here from the checkout on disk with the same function `build.rs`
    /// ran. If they disagree, the id names a tree nobody has.
    #[test]
    fn the_constant_is_the_hash_of_the_tree_on_disk() {
        assert_eq!(source_tree_sha256_hex(&crate_root()), crate::SOURCE_TREE_SHA256_HEX);
        assert_eq!(crate::SOURCE_TREE_SHA256_HEX.len(), 64);
        assert!(crate::SOURCE_TREE_SHA256_HEX.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()));
    }

    /// **A changed kind module moves the id.** The hash is recomputed over a preimage with one
    /// byte of each kind module flipped — in memory, never on disk — and must differ every time.
    /// This is the property Decision 3 rests on: an edit to a transformer is a new transformer.
    #[test]
    fn a_changed_kind_module_moves_the_source_tree_hash() {
        let root = crate_root();
        let base = source_tree_preimage(&root);
        let base_hash = sha256(&base);
        let kinds: Vec<String> = source_files(&root).into_iter().filter(|p| p.starts_with("src/kinds/")).collect();
        assert!(kinds.len() >= 7, "every kind module must be inside the hash's preimage, found {kinds:?}");
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(base_hash);
        for rel in &kinds {
            let body = std::fs::read(root.join(rel)).expect("read kind module");
            assert!(!body.is_empty(), "{rel} is empty");
            // Where the module's bytes sit in the preimage: the framing makes the body findable
            // by the path that precedes it, and flipping the last byte of that body is a change
            // no reordering could hide.
            let needle = {
                let mut n = Vec::new();
                n.extend_from_slice(rel.as_bytes());
                n.push(0);
                n.extend_from_slice(&(body.len() as u64).to_le_bytes());
                n.push(0);
                n
            };
            let at = base.windows(needle.len()).position(|w| w == needle).unwrap_or_else(|| panic!("{rel} is not in the preimage"));
            let mut mutated = base.clone();
            let last = at + needle.len() + body.len() - 1;
            mutated[last] ^= 1;
            assert!(seen.insert(sha256(&mutated)), "editing {rel} left the source-tree hash where it was");
        }
    }

    /// The hash covers EVERY file under `src/`, not only `*.rs`: a kind that `include_str!`s a
    /// table beside itself must move the id when the table moves.
    #[test]
    fn the_preimage_covers_every_non_dot_file_under_src_at_any_depth() {
        let root = crate_root();
        let files = source_files(&root);
        assert!(files.iter().any(|p| p == "src/lib.rs"));
        assert!(files.iter().any(|p| p == "src/kinds/mod.rs"), "a nested directory must be walked");
        assert!(files.iter().any(|p| p == "src/bin/palw-derive.rs"), "the tool is part of the build too");
        assert!(files.iter().all(|p| p.starts_with("src/")), "nothing outside src/ is hashed: {files:?}");
        assert!(files.iter().all(|p| !p.split('/').any(|seg| seg.starts_with('.'))), "a dot-file entered the hash: {files:?}");
        let mut sorted = files.clone();
        sorted.sort();
        assert_eq!(files, sorted, "the order is the globally sorted relative path, and nothing else");
        let mut unique = std::collections::BTreeSet::new();
        assert!(files.iter().all(|p| unique.insert(p.clone())), "a path was hashed twice");
    }
}

//! **A public, immutable source reference for a newly registered model.**
//!
//! Consensus guarantees that an economically active registration past
//! [`crate::config::params::Params::palw_public_model_source_required`] commits a URI and
//! revision sufficient for a stranger to obtain or reproduce the registered artifact. It does
//! **not** trust any hosting provider, and it never performs DNS, HTTP, download or API
//! lookup — a down, private, or 404 repository is not a consensus fact.
//!
//! Artifact identity remains `artifact_root` / inventory root / the semantic descriptor. The
//! URI is commitment metadata. CanonicalWork, reward, fork-choice, PWU and admission weight
//! do not read it.
//!
//! Testnet-11's current *policy* is that the URI must be an `https://huggingface.co/<owner>/<repo>`
//! form. That policy lives beside the fence as `allowed_uri_prefixes`, not as a named vendor in
//! the constitution: a later network may admit `ipfs://`, `https://github.com/`, or any other
//! public scheme without changing this object's shape.

use crate::Hash64;

/// The signed message domain. A signature is good for exactly one (class, URI, revision) on
/// one chain.
pub const PALW_CLASS_PUBLIC_SOURCE_V1_DOMAIN: &[u8] = b"misaka-palw/class-public-source/v1/message";
/// ML-DSA-87 context the registrant signs the commitment under.
pub const PALW_CLASS_PUBLIC_SOURCE_V1_MLDSA87_CONTEXT: &[u8] = b"misaka-palw/class-public-source/v1/mldsa87";

/// Testnet-11's current allowed URI prefix. Hashed with the fence so a prefix change is a
/// ruleset change; not a Hugging Face oracle.
pub const TESTNET_11_PUBLIC_SOURCE_URI_PREFIX: &str = "https://huggingface.co/";

/// What a post-fence registration commits as its public obtain/rebuild instructions.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwPublicModelSourceV1 {
    /// Absolute URI. Consensus checks syntax (and, on testnet-11, the allowed prefix). It never
    /// fetches the URI.
    pub uri: String,
    /// Immutable revision: 40-hex git SHA-1 or 64-hex SHA-256. Branch names are refused.
    pub revision: String,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwPublicModelSourceError {
    #[error("MissingPublicModelSource")]
    MissingPublicModelSource,
    #[error("InvalidPublicModelSource")]
    InvalidPublicModelSource,
}

/// **The signed message** a registrant binds the public source under. Network, bond, class, URI
/// and revision — nothing else, so a signature cannot be lifted onto another class or another
/// host string.
pub fn palw_class_public_source_message_v1(
    network_domain: Hash64,
    bond: &[u8],
    class_id: &Hash64,
    source: &PalwPublicModelSourceV1,
) -> Hash64 {
    let mut state = keyed64(PALW_CLASS_PUBLIC_SOURCE_V1_DOMAIN);
    state.update(network_domain.as_byte_slice());
    state.update(&(bond.len() as u64).to_le_bytes());
    state.update(bond);
    state.update(class_id.as_byte_slice());
    state.update(&(source.uri.len() as u64).to_le_bytes());
    state.update(source.uri.as_bytes());
    state.update(&(source.revision.len() as u64).to_le_bytes());
    state.update(source.revision.as_bytes());
    finish64(state)
}

/// **Admission of a carried public source, syntax only.** `None` is [`PalwPublicModelSourceError::MissingPublicModelSource`].
/// A malformed URI or revision is [`PalwPublicModelSourceError::InvalidPublicModelSource`].
/// `allowed_uri_prefixes` empty accepts any syntactically valid public URI; otherwise the URI
/// must start with one prefix.
///
/// This function does not perform I/O. A host that is down, private, or 404 is not observable
/// here, so IBD, reorg and a machine with no network produce one verdict.
pub fn palw_public_model_source_admit_v1(
    carried: Option<&PalwPublicModelSourceV1>,
    allowed_uri_prefixes: &[String],
) -> Result<(), PalwPublicModelSourceError> {
    let Some(source) = carried else {
        return Err(PalwPublicModelSourceError::MissingPublicModelSource);
    };
    palw_public_model_source_syntax_v1(source, allowed_uri_prefixes)
}

/// Syntax of a committed public source. No DNS, no HTTP, no download.
pub fn palw_public_model_source_syntax_v1(
    source: &PalwPublicModelSourceV1,
    allowed_uri_prefixes: &[String],
) -> Result<(), PalwPublicModelSourceError> {
    if !palw_public_uri_is_well_formed_v1(&source.uri) {
        return Err(PalwPublicModelSourceError::InvalidPublicModelSource);
    }
    if !allowed_uri_prefixes.is_empty() && !allowed_uri_prefixes.iter().any(|prefix| source.uri.starts_with(prefix.as_str())) {
        return Err(PalwPublicModelSourceError::InvalidPublicModelSource);
    }
    // Testnet-11's Hugging Face prefix additionally requires exactly `owner/repo` after the host.
    if allowed_uri_prefixes.iter().any(|p| p == TESTNET_11_PUBLIC_SOURCE_URI_PREFIX)
        && source.uri.starts_with(TESTNET_11_PUBLIC_SOURCE_URI_PREFIX)
        && !palw_hf_owner_repo_path_v1(&source.uri[TESTNET_11_PUBLIC_SOURCE_URI_PREFIX.len()..])
    {
        return Err(PalwPublicModelSourceError::InvalidPublicModelSource);
    }
    if !palw_immutable_revision_v1(&source.revision) {
        return Err(PalwPublicModelSourceError::InvalidPublicModelSource);
    }
    Ok(())
}

fn palw_public_uri_is_well_formed_v1(uri: &str) -> bool {
    // Absolute URI with a scheme and a host. No userinfo, no query, no fragment — those are
    // tracking surfaces, not obtain instructions.
    let Some((scheme, rest)) = uri.split_once("://") else {
        return false;
    };
    if scheme != "https" && scheme != "ipfs" && scheme != "hf" {
        return false;
    }
    if rest.is_empty() || rest.contains('?') || rest.contains('#') || rest.contains('@') || rest.contains('\\') {
        return false;
    }
    let host_and_path = rest.trim_end_matches('/');
    let host = host_and_path.split('/').next().unwrap_or("");
    if host.is_empty() || host.starts_with('.') || host.starts_with('[') {
        return false;
    }
    host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn palw_hf_owner_repo_path_v1(path: &str) -> bool {
    let path = path.trim_end_matches('/');
    let mut parts = path.split('/');
    let Some(owner) = parts.next() else { return false };
    let Some(repo) = parts.next() else { return false };
    if parts.next().is_some() {
        return false;
    }
    palw_hf_path_segment_v1(owner) && palw_hf_path_segment_v1(repo)
}

fn palw_hf_path_segment_v1(seg: &str) -> bool {
    if seg.is_empty() || seg.len() > 96 || seg.starts_with('.') || seg.ends_with('.') {
        return false;
    }
    seg.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

fn palw_immutable_revision_v1(revision: &str) -> bool {
    let n = revision.len();
    if n != 40 && n != 64 {
        return false;
    }
    revision.bytes().all(|b| b.is_ascii_hexdigit())
}

fn keyed64(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}

fn finish64(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_context_ladder::palw_a16_context_row_profile_v5;
    use crate::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX;

    fn hf_prefixes() -> Vec<String> {
        vec![TESTNET_11_PUBLIC_SOURCE_URI_PREFIX.to_string()]
    }

    fn valid_source() -> PalwPublicModelSourceV1 {
        PalwPublicModelSourceV1 {
            uri: "https://huggingface.co/Misakachain/Qwen3.6-35B-A3B-PALW-runtime".to_string(),
            revision: "7a944595a425b0c1d2e3f4567890abcdef123456".to_string(),
        }
    }

    #[test]
    fn fence_before_missing_source_is_not_this_gates_problem() {
        // Historical rule: the gate is not consulted. A missing source is therefore not invalid
        // *here*; the caller skips this function below the fence.
        assert!(palw_public_model_source_admit_v1(None, &hf_prefixes()).is_err());
    }

    #[test]
    fn fence_after_missing_source_is_invalid() {
        assert_eq!(
            palw_public_model_source_admit_v1(None, &hf_prefixes()),
            Err(PalwPublicModelSourceError::MissingPublicModelSource)
        );
    }

    #[test]
    fn malformed_uri_or_revision_is_invalid() {
        let prefixes = hf_prefixes();
        let mut src = valid_source();
        src.uri = "https://example.com/not-hf/repo".to_string();
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &prefixes),
            Err(PalwPublicModelSourceError::InvalidPublicModelSource)
        );
        src = valid_source();
        src.uri = "https://huggingface.co/owner".to_string();
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &prefixes),
            Err(PalwPublicModelSourceError::InvalidPublicModelSource)
        );
        src = valid_source();
        src.uri = "http://huggingface.co/owner/repo".to_string();
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &prefixes),
            Err(PalwPublicModelSourceError::InvalidPublicModelSource)
        );
        src = valid_source();
        src.revision = "main".to_string();
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &prefixes),
            Err(PalwPublicModelSourceError::InvalidPublicModelSource)
        );
        src = valid_source();
        src.uri = "https://huggingface.co/owner/repo?token=1".to_string();
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &prefixes),
            Err(PalwPublicModelSourceError::InvalidPublicModelSource)
        );
    }

    #[test]
    fn valid_hf_url_and_revision_is_admitted() {
        palw_public_model_source_admit_v1(Some(&valid_source()), &hf_prefixes()).expect("well-formed HF source");
        let sha256 = PalwPublicModelSourceV1 {
            uri: valid_source().uri,
            revision: "aa".repeat(32),
        };
        palw_public_model_source_admit_v1(Some(&sha256), &hf_prefixes()).expect("64-hex revision");
    }

    #[test]
    fn empty_prefixes_accept_any_well_formed_https_uri() {
        let src = PalwPublicModelSourceV1 {
            uri: "https://github.com/misaka/weights".to_string(),
            revision: "7a944595a425b0c1d2e3f4567890abcdef123456".to_string(),
        };
        palw_public_model_source_admit_v1(Some(&src), &[]).expect("generic public URI");
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &hf_prefixes()),
            Err(PalwPublicModelSourceError::InvalidPublicModelSource),
            "testnet policy still refuses a non-HF URI"
        );
    }

    #[test]
    fn changing_the_uri_does_not_change_canonical_work() {
        let profile = palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("shipped dense row");
        let class_id = profile.shape_profile_id();
        let mut other = valid_source();
        other.uri = "https://huggingface.co/other/mirror".to_string();
        palw_public_model_source_admit_v1(Some(&other), &hf_prefixes()).expect("another well-formed source");
        assert_eq!(
            class_id,
            palw_a16_context_row_profile_v5(QWEN25_A16_GRAPH_V5_N_CTX).expect("again").shape_profile_id(),
            "the class id is the graph; a hosting URI is not an input to CanonicalWork, reward or fork-weight"
        );
    }

    #[test]
    fn network_unavailability_cannot_change_the_verdict() {
        // The gate is a string predicate. A URI naming a host that is (or is not) reachable is
        // admitted or refused on syntax alone; this test never opens a socket.
        let src = PalwPublicModelSourceV1 {
            uri: "https://huggingface.co/this-repo-does-not-exist-on-purpose/missing".to_string(),
            revision: "0123456789abcdef0123456789abcdef01234567".to_string(),
        };
        palw_public_model_source_admit_v1(Some(&src), &hf_prefixes())
            .expect("404 / NXDOMAIN / no-nic is not a consensus input");
        assert_eq!(
            palw_public_model_source_admit_v1(Some(&src), &hf_prefixes()),
            palw_public_model_source_admit_v1(Some(&src), &hf_prefixes()),
            "IBD, reorg and a retry produce one verdict"
        );
    }

    #[test]
    fn the_signed_message_binds_uri_and_revision() {
        let class = Hash64::from_u64_word(1);
        let bond = [7u8; 8];
        let domain = Hash64::from_u64_word(2);
        let a = palw_class_public_source_message_v1(domain, &bond, &class, &valid_source());
        let mut moved = valid_source();
        moved.uri.push('x');
        let b = palw_class_public_source_message_v1(domain, &bond, &class, &moved);
        assert_ne!(a, b);
    }
}

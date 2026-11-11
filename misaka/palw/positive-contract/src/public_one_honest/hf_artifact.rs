//! Bounded Artifact material for ordinary TFP1. The referenced checkpoint and
//! conversion recipe are data identities. They confer no availability or
//! acceptance authority.

use super::{ManifestEntryV1, ManifestScopeRules};
use crate::{
    ContractError,
    identity::{Digest, hash},
};

const MAGIC: &[u8; 4] = b"HFA1";
const VERSION: u16 = 1;
const REPO_COMPONENT_MAX_BYTES: usize = 96;
const REPO_MAX_BYTES: usize = 2 * REPO_COMPONENT_MAX_BYTES + 1;
const HEADER_BYTES: usize = MAGIC.len() + size_of::<u16>() + size_of::<u8>();
const REVISION_BYTES: usize = 20;
const DIGEST_BYTES: usize = 64;
const FIXED_BYTES: usize = HEADER_BYTES + REVISION_BYTES + 8 * DIGEST_BYTES;

pub const HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1: usize = FIXED_BYTES + REPO_MAX_BYTES;

pub fn hf_artifact_material_codec_v1() -> Digest {
    hash(b"misaka/palw/material-codec/v1", &b"tir-hf-artifact-reference/v1/whole")
}

/// Expected values are resolved independently from the accepted claim and
/// registered class/container. Never take them from a downloaded recipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HfArtifactNativePinsV1 {
    pub class: Digest,
    pub program: Digest,
    pub tokenizer: Digest,
    pub inventory: Digest,
    pub container: Digest,
    pub binding_manifest: Digest,
}

impl HfArtifactNativePinsV1 {
    fn as_array_v1(self) -> [Digest; 6] {
        [self.class, self.program, self.tokenizer, self.inventory, self.container, self.binding_manifest]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HfArtifactDescriptorV1 {
    pins: HfArtifactNativePinsV1,
    repository: [u8; REPO_MAX_BYTES],
    repository_len: u8,
    revision: [u8; REVISION_BYTES],
    source_manifest: Digest,
    conversion_recipe: Digest,
}

impl HfArtifactDescriptorV1 {
    /// Repository spelling is preserved exactly. Revision is the binary form
    /// of the full 40-hex-digit Git object ID, with no branch or tag spelling.
    pub fn new_v1(
        pins: HfArtifactNativePinsV1,
        repository: &str,
        revision: [u8; REVISION_BYTES],
        source_manifest: Digest,
        conversion_recipe: Digest,
    ) -> Result<Self, ContractError> {
        check_repository_v1(repository)?;
        if revision == [0; REVISION_BYTES]
            || pins.as_array_v1().into_iter().chain([source_manifest, conversion_recipe]).any(|id| id == Digest([0; DIGEST_BYTES]))
        {
            return Err(ContractError::Identity);
        }
        let mut bytes = [0; REPO_MAX_BYTES];
        bytes[..repository.len()].copy_from_slice(repository.as_bytes());
        Ok(Self { pins, repository: bytes, repository_len: repository.len() as u8, revision, source_manifest, conversion_recipe })
    }

    /// Decode the bounded literal wire without allocating from untrusted
    /// length fields. Exact EOF also rejects trailing data.
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, ContractError> {
        if !(FIXED_BYTES..=HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1).contains(&bytes.len()) || !bytes.starts_with(MAGIC) {
            return Err(ContractError::NonCanonical);
        }
        let mut body = &bytes[MAGIC.len()..];
        let version = u16::from_le_bytes(read_v1(&mut body)?);
        if version != VERSION {
            return Err(ContractError::Version);
        }
        let [repository_len] = read_v1(&mut body)?;
        if FIXED_BYTES + usize::from(repository_len) != bytes.len() {
            return Err(ContractError::NonCanonical);
        }
        let (repository, rest) = body.split_at(usize::from(repository_len));
        let repository = std::str::from_utf8(repository).map_err(|_| ContractError::NonCanonical)?;
        let mut body = rest;
        let revision = read_v1(&mut body)?;
        let mut read_digest = || read_v1(&mut body).map(Digest);
        let pins = HfArtifactNativePinsV1 {
            class: read_digest()?,
            program: read_digest()?,
            tokenizer: read_digest()?,
            inventory: read_digest()?,
            container: read_digest()?,
            binding_manifest: read_digest()?,
        };
        let source_manifest = read_digest()?;
        let conversion_recipe = read_digest()?;
        debug_assert!(body.is_empty());
        Self::new_v1(pins, repository, revision, source_manifest, conversion_recipe)
    }

    pub fn encode_v1(&self) -> Result<Vec<u8>, ContractError> {
        let (bytes, len) = self.canonical_wire_v1();
        let mut encoded = Vec::new();
        encoded.try_reserve_exact(len).map_err(|_| ContractError::Budget)?;
        if encoded.capacity() != len {
            return Err(ContractError::Budget);
        }
        encoded.extend_from_slice(&bytes[..len]);
        Ok(encoded)
    }

    pub fn manifest_entry_v1(&self, rules: ManifestScopeRules) -> Result<ManifestEntryV1, ContractError> {
        if rules.codec != hf_artifact_material_codec_v1() {
            return Err(ContractError::UnsupportedProfile);
        }
        let (bytes, len) = self.canonical_wire_v1();
        let total_bytes = len as u64;
        if total_bytes > rules.max_bytes || total_bytes > u64::from(rules.chunk_bytes) {
            return Err(ContractError::Budget);
        }
        let root =
            blake2b_simd::Params::new().hash_length(DIGEST_BYTES).key(b"misaka/palw/tir-hf-artifact-reference/v1").hash(&bytes[..len]);
        Ok(ManifestEntryV1 {
            subject: self.pins.inventory,
            content_root: Digest(root.as_bytes().try_into().expect("64-byte digest")),
            codec: rules.codec,
            total_bytes,
            chunk_bytes: rules.chunk_bytes,
        })
    }

    pub fn check_native_pins_v1(&self, expected: HfArtifactNativePinsV1) -> Result<(), ContractError> {
        if self.pins != expected {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub fn native_pins_v1(&self) -> HfArtifactNativePinsV1 {
        self.pins
    }

    pub fn repository_v1(&self) -> &str {
        std::str::from_utf8(&self.repository[..usize::from(self.repository_len)]).expect("canonical ASCII repository")
    }

    pub fn revision_v1(&self) -> [u8; REVISION_BYTES] {
        self.revision
    }

    pub fn source_manifest_v1(&self) -> Digest {
        self.source_manifest
    }

    pub fn conversion_recipe_v1(&self) -> Digest {
        self.conversion_recipe
    }

    fn canonical_wire_v1(&self) -> ([u8; HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1], usize) {
        let mut bytes = [0; HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1];
        let mut len = 0;
        let mut append = |chunk: &[u8]| {
            bytes[len..len + chunk.len()].copy_from_slice(chunk);
            len += chunk.len();
        };
        append(MAGIC);
        append(&VERSION.to_le_bytes());
        append(&[self.repository_len]);
        append(&self.repository[..usize::from(self.repository_len)]);
        append(&self.revision);
        for digest in self.pins.as_array_v1().into_iter().chain([self.source_manifest, self.conversion_recipe]) {
            append(&digest.0);
        }
        (bytes, len)
    }
}

fn read_v1<const N: usize>(remaining: &mut &[u8]) -> Result<[u8; N], ContractError> {
    let (field, rest) = remaining.split_at_checked(N).ok_or(ContractError::NonCanonical)?;
    *remaining = rest;
    field.try_into().map_err(|_| ContractError::NonCanonical)
}

fn check_repository_v1(repository: &str) -> Result<(), ContractError> {
    if repository.len() > REPO_MAX_BYTES {
        return Err(ContractError::Budget);
    }
    let (owner, name) = repository.split_once('/').ok_or(ContractError::NonCanonical)?;
    for component in [owner, name] {
        if component.is_empty()
            || component.len() > REPO_COMPONENT_MAX_BYTES
            || matches!(component, "." | "..")
            || !component.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        {
            return Err(ContractError::NonCanonical);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins() -> HfArtifactNativePinsV1 {
        let digest = |tag| Digest([tag; DIGEST_BYTES]);
        HfArtifactNativePinsV1 {
            class: digest(1),
            program: digest(2),
            tokenizer: digest(3),
            inventory: digest(4),
            container: digest(5),
            binding_manifest: digest(6),
        }
    }

    fn descriptor(repository: &str) -> Result<HfArtifactDescriptorV1, ContractError> {
        HfArtifactDescriptorV1::new_v1(pins(), repository, [7; REVISION_BYTES], Digest([8; DIGEST_BYTES]), Digest([9; DIGEST_BYTES]))
    }

    fn rules() -> ManifestScopeRules {
        ManifestScopeRules {
            codec: hf_artifact_material_codec_v1(),
            max_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u64,
            chunk_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u32,
        }
    }

    #[test]
    fn hf_artifact_descriptor_canonical_bounds_and_exact_eof() {
        let maximum = format!("{}/{}", "A".repeat(REPO_COMPONENT_MAX_BYTES), "B".repeat(REPO_COMPONENT_MAX_BYTES));
        let original = descriptor(&maximum).unwrap();
        let mut encoded = original.encode_v1().unwrap();
        assert_eq!(encoded.len(), HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1);
        assert_eq!(HfArtifactDescriptorV1::decode_v1(&encoded), Ok(original));
        for end in 0..encoded.len() {
            assert!(HfArtifactDescriptorV1::decode_v1(&encoded[..end]).is_err());
        }
        encoded.push(0);
        assert_eq!(HfArtifactDescriptorV1::decode_v1(&encoded), Err(ContractError::NonCanonical));
        encoded.pop();
        encoded[MAGIC.len()] += 1;
        assert_eq!(HfArtifactDescriptorV1::decode_v1(&encoded), Err(ContractError::Version));
        encoded[MAGIC.len()] -= 1;
        encoded[HEADER_BYTES - 1] = u8::MAX;
        assert_eq!(HfArtifactDescriptorV1::decode_v1(&encoded), Err(ContractError::NonCanonical));
        for repository in ["", "model", "/model", "owner/", "owner/name/extra", "../model", "owner/..", "owner/%2E", "日本/model"] {
            assert!(descriptor(repository).is_err(), "{repository:?}");
        }
    }

    #[test]
    fn hf_artifact_descriptor_binds_native_pins_and_only_charges_descriptor_bytes() {
        let descriptor = descriptor("HuggingFaceTB/SmolLM2-135M").unwrap();
        let encoded = descriptor.encode_v1().unwrap();
        let entry = descriptor.manifest_entry_v1(rules()).unwrap();
        assert_eq!(entry.total_bytes, encoded.len() as u64);
        assert_eq!(entry.subject, pins().inventory);
        assert_ne!(entry.content_root, entry.subject);
        assert_eq!(descriptor.check_native_pins_v1(pins()), Ok(()));
        for wrong in [
            HfArtifactNativePinsV1 { class: Digest([10; 64]), ..pins() },
            HfArtifactNativePinsV1 { program: Digest([10; 64]), ..pins() },
            HfArtifactNativePinsV1 { tokenizer: Digest([10; 64]), ..pins() },
            HfArtifactNativePinsV1 { inventory: Digest([10; 64]), ..pins() },
            HfArtifactNativePinsV1 { container: Digest([10; 64]), ..pins() },
            HfArtifactNativePinsV1 { binding_manifest: Digest([10; 64]), ..pins() },
        ] {
            assert_eq!(descriptor.check_native_pins_v1(wrong), Err(ContractError::Scope));
        }
        let rules = rules();
        assert_eq!(
            descriptor.manifest_entry_v1(ManifestScopeRules { max_bytes: entry.total_bytes - 1, ..rules }),
            Err(ContractError::Budget)
        );
        assert_eq!(
            descriptor.manifest_entry_v1(ManifestScopeRules { chunk_bytes: entry.total_bytes as u32 - 1, ..rules }),
            Err(ContractError::Budget)
        );
        assert_eq!(
            descriptor.manifest_entry_v1(ManifestScopeRules { codec: super::super::tir_inventory_material_codec_v1(), ..rules }),
            Err(ContractError::UnsupportedProfile)
        );
        for offset in [
            HEADER_BYTES,
            HEADER_BYTES + descriptor.repository_v1().len(),
            encoded.len() - 2 * DIGEST_BYTES,
            encoded.len() - DIGEST_BYTES,
        ] {
            let mut substituted = encoded.clone();
            substituted[offset] ^= 1;
            let changed = HfArtifactDescriptorV1::decode_v1(&substituted).unwrap();
            assert_ne!(changed.manifest_entry_v1(rules).unwrap().content_root, entry.content_root);
        }
    }
}

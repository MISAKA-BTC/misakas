//! Prospective public EOF inventory, before claim-wide work/retention is priced.
//! A registered hash and a signed declaration of length are not an actual-length
//! certificate. Only a successful native stream finish may install a certificate.
use crate::palw_rendering_resources_v1::{PalwRenderingResourcePlanV1, PalwStagedPublicCourtConfigV1};
use crate::palw_rendering_stream_v1::LeafHashContinuationV1;
use crate::palw_staged_bundle_v1::PalwTokenizerInventoryPlanV1;
use crate::{Hash64, palw_state_v2::PalwBondKeyV2};
use borsh::{BorshDeserialize, BorshSerialize};

pub const PALW_ARTIFACT_INVENTORY_CONTEXT_V1: &[u8] = b"misaka-palw/tokenizer-artifact/inventory/v1/mldsa87";
pub const PALW_ARTIFACT_INVENTORY_PART_BYTES_V1: usize = 65_536;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwTokenizerArtifactInventoryOfferV1 {
    pub version: u16,
    pub artifact_hash: Hash64,
    pub opener: PalwBondKeyV2,
    /// Paid operand length, authenticated only at exact length-prefixed EOF.
    /// This is not a new global raw-file format cap.
    pub maximum_bytes: u64,
    pub accept_until_daa: u64,
    pub signature: Vec<u8>,
}
impl BorshDeserialize for PalwTokenizerArtifactInventoryOfferV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            artifact_hash: BorshDeserialize::deserialize_reader(reader)?,
            opener: BorshDeserialize::deserialize_reader(reader)?,
            maximum_bytes: BorshDeserialize::deserialize_reader(reader)?,
            accept_until_daa: BorshDeserialize::deserialize_reader(reader)?,
            signature: crate::palw_public_binding_v1::read_signature_v1(reader)?,
        })
    }
}
impl PalwTokenizerArtifactInventoryOfferV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        if self.version != 1
            || self.artifact_hash == Hash64::default()
            || self.accept_until_daa == 0
            || self.signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN
        {
            return Err("artifact inventory offer lacks original hash, expiry or signature".into());
        }
        Ok(())
    }
    pub fn id_v1(&self) -> Hash64 {
        let bytes = borsh::to_vec(&(self.version, self.artifact_hash, self.opener, self.maximum_bytes, self.accept_until_daa))
            .expect("fixed inventory offer serializes");
        Hash64::from_bytes(
            blake2b_simd::Params::new()
                .hash_length(64)
                .key(b"misaka-palw/tokenizer-artifact/inventory-offer/v1")
                .hash(&bytes)
                .as_bytes()
                .try_into()
                .expect("64"),
        )
    }
    pub fn validate_signature_v1<F>(&self, key: &[u8], verify: F) -> Result<(), String>
    where
        F: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
    {
        self.check_v1()?;
        if key.len() != crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN
            || !verify(key, self.id_v1().as_byte_slice(), &self.signature, PALW_ARTIFACT_INVENTORY_CONTEXT_V1)
        {
            return Err("artifact inventory signature does not verify under the exact opener bond".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub enum PalwTokenizerArtifactInventoryActionV1 {
    Part { offset: u64, bytes: Vec<u8> },
    Finish,
}
impl BorshDeserialize for PalwTokenizerArtifactInventoryActionV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        match u8::deserialize_reader(reader)? {
            0 => {
                let offset = u64::deserialize_reader(reader)?;
                let bytes = crate::palw_public_binding_v1::read_bounded_bytes_v1(reader, PALW_ARTIFACT_INVENTORY_PART_BYTES_V1)?;
                if bytes.is_empty() {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "empty artifact inventory part"));
                }
                Ok(Self::Part { offset, bytes })
            }
            1 => Ok(Self::Finish),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown artifact inventory action")),
        }
    }
}

/// A trusted rooted-state fact, not a self-authenticating wire claim. Importing
/// this struct from a peer must never substitute for native EOF acceptance.
#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwTokenizerArtifactInventoryCertificateV1 {
    pub artifact_hash: Hash64,
    pub actual_bytes: u64,
    pub inventory_id: Hash64,
    pub opener: PalwBondKeyV2,
    pub opened_daa: u64,
    pub completed_daa: u64,
}
impl PalwTokenizerArtifactInventoryCertificateV1 {
    pub fn check_v1(&self) -> Result<(), String> {
        if self.artifact_hash == Hash64::default() || self.inventory_id == Hash64::default() || self.completed_daa < self.opened_daa {
            return Err("artifact inventory certificate lacks a native accepted EOF context".into());
        }
        Ok(())
    }
    /// Every parser object-key/vocabulary event consumes at least one raw byte.
    /// This bound needs no trust in the registrant's possibly false event count.
    pub fn key_event_upper_bound_v1(&self) -> u64 {
        self.actual_bytes.max(1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
pub struct PalwTokenizerArtifactInventoryV1 {
    pub offer: PalwTokenizerArtifactInventoryOfferV1,
    pub opened_daa: u64,
    pub until_daa: u64,
    pub plan: PalwRenderingResourcePlanV1,
    pub offset: u64,
    pub moves_used: u64,
    pub certificate: Option<PalwTokenizerArtifactInventoryCertificateV1>,
    hash: LeafHashContinuationV1,
}
impl PalwTokenizerArtifactInventoryV1 {
    /// The native caller verifies the offer signature, registered artifact link,
    /// independent admission limit and full rent/deposit before installing this row.
    pub fn new_v1(
        offer: PalwTokenizerArtifactInventoryOfferV1,
        now: u64,
        config: PalwStagedPublicCourtConfigV1,
    ) -> Result<Self, String> {
        offer.check_v1()?;
        config.check()?;
        if now >= offer.accept_until_daa {
            return Err("artifact inventory offer expired before native opening".into());
        }
        let plan = Self::quote_v1(offer.maximum_bytes, config)?;
        let until_daa = now
            .checked_add(config.acquisition_daa)
            .and_then(|n| n.checked_add(plan.inclusion_daa))
            .ok_or("artifact inventory original deadline overflows")?;
        let mut hash = LeafHashContinuationV1::new(crate::palw_tokenizer_rendering_v1::PALW_TOKENIZER_RAW_DOMAIN_V1);
        hash.update(&offer.maximum_bytes.to_le_bytes());
        let row = Self { offer, opened_daa: now, until_daa, plan, offset: 0, moves_used: 1, certificate: None, hash };
        row.check_v1()?;
        Ok(row)
    }
    pub fn quote_v1(maximum_bytes: u64, config: PalwStagedPublicCourtConfigV1) -> Result<PalwRenderingResourcePlanV1, String> {
        config.check()?;
        let mut plan = PalwTokenizerInventoryPlanV1::derive_v1(maximum_bytes, config)?.envelope;
        // Reversible old/new hash checkpoints plus bounded metadata accompany
        // every accepted part. Do not price only the last live hash cursor.
        let extra = plan
            .maximum_moves
            .checked_mul(32_768)
            .and_then(|n| n.checked_add(16_384 + 512))
            .ok_or("artifact inventory native journal storage overflows")?;
        // The initial outer move and signed Offer have two distinct witnesses.
        plan.maximum_stream_bytes =
            plan.maximum_stream_bytes.checked_add(8192).ok_or("artifact inventory carrier envelope overflows")?;
        plan.row_bytes = plan.row_bytes.checked_add(extra).ok_or("artifact inventory storage overflows")?;
        plan.rent_sompi = plan
            .rent_sompi
            .checked_add(extra.checked_mul(config.prices.rent_per_row_byte).ok_or("artifact inventory journal rent overflows")?)
            .and_then(|n| config.prices.rent_per_stream_byte.checked_mul(8192).and_then(|cost| n.checked_add(cost)))
            .ok_or("artifact inventory rent overflows")?;
        // Keep the signed carrier, canonical serialization and retry copy under
        // one actor permit, including the old/new native hash checkpoints.
        plan.resident_ram_bytes = plan.resident_ram_bytes.max(2 * 1024 * 1024);
        plan.rent_sompi.checked_add(plan.deposit_sompi).ok_or("artifact inventory prepaid capital overflows")?;
        Ok(plan)
    }
    pub fn check_v1(&self) -> Result<(), String> {
        self.offer.check_v1()?;
        if self.opened_daa >= self.offer.accept_until_daa
            || self.opened_daa >= self.until_daa
            || self.offset > self.offer.maximum_bytes
            || self.moves_used == 0
            || self.moves_used > self.plan.maximum_moves
            || self.hash.byte_count() != Some(128 + 8 + u128::from(self.offset))
        {
            return Err("artifact inventory changes its original paid clock, work or hash offset".into());
        }
        if let Some(certificate) = &self.certificate {
            certificate.check_v1()?;
            if certificate.artifact_hash != self.offer.artifact_hash
                || certificate.actual_bytes != self.offset
                || certificate.inventory_id != self.offer.id_v1()
                || certificate.opener != self.offer.opener
                || certificate.opened_daa != self.opened_daa
                || certificate.completed_daa >= self.until_daa
                || self.offset != self.offer.maximum_bytes
                || self.hash.finish() != self.offer.artifact_hash
            {
                return Err("artifact inventory certificate differs from its exact native EOF authenticator".into());
            }
        }
        Ok(())
    }
    pub fn apply_v1(
        &mut self,
        action: &PalwTokenizerArtifactInventoryActionV1,
        now: u64,
    ) -> Result<Option<PalwTokenizerArtifactInventoryCertificateV1>, String> {
        self.check_v1()?;
        if self.certificate.is_some() || now < self.opened_daa || now >= self.until_daa || self.moves_used >= self.plan.maximum_moves {
            return Err("artifact inventory is sealed, expired or exhausted its paid attempt".into());
        }
        match action {
            PalwTokenizerArtifactInventoryActionV1::Part { offset, bytes } => {
                if *offset != self.offset || bytes.is_empty() || bytes.len() > PALW_ARTIFACT_INVENTORY_PART_BYTES_V1 {
                    return Err("artifact inventory part changes the bounded original stream offset".into());
                }
                let end = offset
                    .checked_add(bytes.len() as u64)
                    .filter(|end| *end <= self.offer.maximum_bytes)
                    .ok_or("artifact inventory exceeds its paid attempt size")?;
                self.hash.update(bytes);
                self.offset = end;
                self.moves_used += 1;
                Ok(None)
            }
            PalwTokenizerArtifactInventoryActionV1::Finish => {
                if self.offset != self.offer.maximum_bytes || self.hash.finish() != self.offer.artifact_hash {
                    return Err("artifact inventory EOF does not authenticate the registered raw file".into());
                }
                let certificate = PalwTokenizerArtifactInventoryCertificateV1 {
                    artifact_hash: self.offer.artifact_hash,
                    actual_bytes: self.offset,
                    inventory_id: self.offer.id_v1(),
                    opener: self.offer.opener,
                    opened_daa: self.opened_daa,
                    completed_daa: now,
                };
                self.moves_used += 1;
                self.certificate = Some(certificate.clone());
                Ok(Some(certificate))
            }
        }
    }
}

/// Canonical original resource commitment, independent of unknown future claim
/// acceptance DAA or which honest inventory opener produced the accepted fact.
pub fn palw_inventory_resource_plan_id_v1(
    semantics: &crate::palw_class_semantics_v1::PalwClassSemanticsV1,
    inventory: Option<&PalwTokenizerArtifactInventoryCertificateV1>,
    families: &[(crate::palw_staged_bundle_v1::PalwStagedFamilyV1, PalwRenderingResourcePlanV1)],
    maximum_custodians: u64,
    config: PalwStagedPublicCourtConfigV1,
) -> Result<Hash64, String> {
    config.check()?;
    if families.is_empty()
        || families.len() > crate::palw_staged_bundle_v1::PALW_STAGED_REQUIRED_FAMILIES_V1
        || families.windows(2).any(|w| w[0].0 >= w[1].0)
        || maximum_custodians == 0
    {
        return Err("inventory resource commitment needs the exact bounded required-family geometry".into());
    }
    let fact = match (semantics.raw_tokenizer_bytes, inventory) {
        (Some(_), Some(certificate)) => {
            certificate.check_v1()?;
            if certificate.artifact_hash != semantics.tokenizer_id {
                return Err("inventory substitutes a foreign registered raw hash".into());
            }
            Some((certificate.artifact_hash, certificate.actual_bytes))
        }
        (None, None) => None,
        _ => return Err("claim-wide raw work requires an accepted EOF inventory, not declared size".into()),
    };
    let bytes = borsh::to_vec(&(1u16, semantics, fact, families, maximum_custodians, config)).map_err(|e| e.to_string())?;
    Ok(Hash64::from_bytes(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(b"misaka-palw/history-custody/inventoried-resources/v1")
            .hash(&bytes)
            .as_bytes()
            .try_into()
            .expect("64"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }
    fn offer(raw: &[u8], operand_bytes: u64) -> PalwTokenizerArtifactInventoryOfferV1 {
        PalwTokenizerArtifactInventoryOfferV1 {
            version: 1,
            artifact_hash: crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(raw),
            opener: bond(7),
            maximum_bytes: operand_bytes,
            accept_until_daa: 200,
            signature: vec![9; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
        }
    }
    fn engine(raw: &[u8], operand_bytes: u64) -> PalwTokenizerArtifactInventoryV1 {
        PalwTokenizerArtifactInventoryV1::new_v1(offer(raw, operand_bytes), 100, PalwStagedPublicCourtConfigV1::network_v1()).unwrap()
    }
    fn feed(row: &mut PalwTokenizerArtifactInventoryV1, raw: &[u8]) {
        for bytes in raw.chunks(PALW_ARTIFACT_INVENTORY_PART_BYTES_V1) {
            row.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Part { offset: row.offset, bytes: bytes.to_vec() }, 101).unwrap();
        }
    }
    fn semantics(hash: Hash64, declared: u64) -> crate::palw_class_semantics_v1::PalwClassSemanticsV1 {
        crate::palw_class_semantics_v1::PalwClassSemanticsV1 {
            version: 1,
            class_id: Hash64::from_u64_word(1),
            artifact_root: Hash64::from_u64_word(2),
            bound_from_daa: 80,
            model_profile_id: Hash64::from_u64_word(3),
            runtime_manifest_hash: Hash64::from_u64_word(4),
            runtime_class_id: Hash64::from_u64_word(5),
            tokenizer_id: hash,
            cu_ruleset_id: Hash64::from_u64_word(6),
            token_table_root: Some(Hash64::from_u64_word(8)),
            rendering_rule: Some(crate::palw_tokenizer_rendering_v1::PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1),
            raw_tokenizer_bytes: Some(declared),
            tokenizer_key_events: Some(1),
            prefix_state_relation: None,
        }
    }
    #[test]
    fn native_eof_inventory_matches_original_length_prefixed_raw_hash_after_restart() {
        let raw = vec![b'x'; PALW_ARTIFACT_INVENTORY_PART_BYTES_V1 + 137];
        let mut row = engine(&raw, raw.len() as u64);
        row.apply_v1(
            &PalwTokenizerArtifactInventoryActionV1::Part { offset: 0, bytes: raw[..PALW_ARTIFACT_INVENTORY_PART_BYTES_V1].to_vec() },
            101,
        )
        .unwrap();
        let mut row: PalwTokenizerArtifactInventoryV1 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        row.check_v1().unwrap();
        row.apply_v1(
            &PalwTokenizerArtifactInventoryActionV1::Part {
                offset: row.offset,
                bytes: raw[PALW_ARTIFACT_INVENTORY_PART_BYTES_V1..].to_vec(),
            },
            102,
        )
        .unwrap();
        let certificate = row.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 103).unwrap().unwrap();
        assert_eq!(certificate.actual_bytes, raw.len() as u64);
        assert_eq!(certificate.artifact_hash, crate::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw));
        assert_eq!(certificate.key_event_upper_bound_v1(), raw.len() as u64);
        row.check_v1().unwrap();
        assert!(row.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 104).is_err());
    }
    #[test]
    fn wrong_operand_prefix_body_or_premature_eof_never_installs_size_certificate() {
        let raw = b"public registered tokenizer operand";
        let mut long = engine(raw, raw.len() as u64 + 1);
        feed(&mut long, raw);
        assert!(long.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 102).is_err());
        feed(&mut long, &[0]);
        assert!(long.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 102).is_err());
        assert!(long.certificate.is_none());
        let mut short = engine(raw, raw.len() as u64 - 1);
        feed(&mut short, &raw[..raw.len() - 1]);
        assert!(short.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 102).is_err());
        assert!(short.certificate.is_none());
        let mut bad = engine(raw, raw.len() as u64);
        let mut changed = raw.to_vec();
        changed[0] ^= 1;
        feed(&mut bad, &changed);
        assert!(bad.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 102).is_err());
        assert!(bad.certificate.is_none());
    }
    #[test]
    fn offset_work_and_original_deadline_cannot_be_reset_by_restart_or_bad_packet() {
        let raw = b"a longer public operand";
        let mut row = engine(raw, raw.len() as u64);
        let before = row.clone();
        for action in [
            PalwTokenizerArtifactInventoryActionV1::Part { offset: 1, bytes: vec![1] },
            PalwTokenizerArtifactInventoryActionV1::Part { offset: 0, bytes: Vec::new() },
            PalwTokenizerArtifactInventoryActionV1::Part { offset: 0, bytes: vec![0; PALW_ARTIFACT_INVENTORY_PART_BYTES_V1 + 1] },
        ] {
            assert!(row.apply_v1(&action, 101).is_err());
            assert_eq!(row, before);
        }
        assert!(
            row.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Part { offset: 0, bytes: raw.to_vec() }, row.until_daa).is_err()
        );
        assert_eq!(row, before);
        feed(&mut row, raw);
        let restarted: PalwTokenizerArtifactInventoryV1 = borsh::from_slice(&borsh::to_vec(&row).unwrap()).unwrap();
        assert_eq!(restarted.until_daa, before.until_daa);
        let mut corrupted = restarted;
        corrupted.offset -= 1;
        assert!(corrupted.check_v1().is_err());
    }
    #[test]
    fn price_quotes_do_not_substitute_a_new_global_raw_file_cap() {
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let small = PalwTokenizerArtifactInventoryV1::quote_v1(1024, config).unwrap();
        let large = PalwTokenizerArtifactInventoryV1::quote_v1(1u64 << 32, config).unwrap();
        assert!(large.maximum_stream_bytes > small.maximum_stream_bytes);
        assert!(large.row_bytes > large.maximum_moves * 8192);
        assert_eq!(large.resident_ram_bytes, small.resident_ram_bytes);
        assert!(PalwTokenizerArtifactInventoryV1::quote_v1(u64::MAX, config).is_err());
        // This verifies metadata arithmetic only, not a 4GiB runtime completion.
    }
    #[test]
    fn artifact_part_declared_length_is_bounded_before_body_allocation() {
        let mut bytes = vec![0];
        bytes.extend(0u64.to_le_bytes());
        bytes.extend(u32::MAX.to_le_bytes());
        assert!(borsh::from_slice::<PalwTokenizerArtifactInventoryActionV1>(&bytes).is_err());
        let part = PalwTokenizerArtifactInventoryActionV1::Part { offset: 0, bytes: vec![1; PALW_ARTIFACT_INVENTORY_PART_BYTES_V1] };
        assert_eq!(borsh::from_slice::<PalwTokenizerArtifactInventoryActionV1>(&borsh::to_vec(&part).unwrap()).unwrap(), part);
    }
    #[test]
    fn original_resource_identity_requires_eof_fact_but_not_inventory_opener_or_claim_clock() {
        let raw = b"an authenticated public operand";
        let config = PalwStagedPublicCourtConfigV1::network_v1();
        let mut row = engine(raw, raw.len() as u64);
        feed(&mut row, raw);
        let certificate = row.apply_v1(&PalwTokenizerArtifactInventoryActionV1::Finish, 102).unwrap().unwrap();
        let statement = semantics(certificate.artifact_hash, raw.len() as u64 - 1);
        let families = vec![(
            crate::palw_staged_bundle_v1::PalwStagedFamilyV1::Tokenizer,
            PalwTokenizerArtifactInventoryV1::quote_v1(certificate.actual_bytes, config).unwrap(),
        )];
        assert!(palw_inventory_resource_plan_id_v1(&statement, None, &families, 10, config).is_err());
        let id = palw_inventory_resource_plan_id_v1(&statement, Some(&certificate), &families, 10, config).unwrap();
        let mut equivalent = certificate.clone();
        equivalent.opener = bond(77);
        equivalent.inventory_id = Hash64::from_u64_word(99);
        equivalent.opened_daa = 120;
        equivalent.completed_daa = 140;
        assert_eq!(palw_inventory_resource_plan_id_v1(&statement, Some(&equivalent), &families, 10, config).unwrap(), id);
        equivalent.actual_bytes += 1;
        assert_ne!(palw_inventory_resource_plan_id_v1(&statement, Some(&equivalent), &families, 10, config).unwrap(), id);
        equivalent.artifact_hash = Hash64::from_u64_word(88);
        assert!(palw_inventory_resource_plan_id_v1(&statement, Some(&equivalent), &families, 10, config).is_err());
        assert_ne!(palw_inventory_resource_plan_id_v1(&statement, Some(&certificate), &families, 11, config).unwrap(), id);
        // Only an authenticated accepted-state reader supplies certificates. A
        // structurally plausible peer certificate is never native EOF evidence.
    }
}

// Read-only native allocation inventory; no state/root/codec field is added.
#[path = "palw_native_resource_v1/private/palw_tokenizer_artifact_inventory_v1.rs"]
mod native_resource_inventory_v1;

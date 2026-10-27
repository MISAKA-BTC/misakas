//! **`misaka palw material-session draft|sign|seal`** (RFC-0008 PAV3, 2026-10-15 round 3; unverified source — nothing here
//! has been compiled or run): the signed `MonetaryMaterialEnrolledV1` carrying a PROSPECTIVE native session
//! (`terms.native_session`), the object an executor's non-floor class needs before its attempts can take the PAV3 custody form.
//!
//! ```text
//! draft --spec SPEC.json --out DRAFT.json             the exact unsigned enrollment, its consent id and every signer
//! sign  --draft DRAFT.json --key-file … --bond …       one party's signature (sponsor or one provider), offline
//!       --out SIG.json
//! seal  --draft DRAFT.json --signature SIG.json …      every party's signature checked, then the borsh object
//!       --out OBJECT.borsh                             file it with `misaka palw submit-object --object OBJECT.borsh`
//! ```
//!
//! The pattern of `palw da-accuse` / `tir-registration`: nothing is broadcast here. Each party signs the SAME consent payload
//! (`PalwMonetaryMaterialEnrollmentV1::payload`) under `palw_provider_court_message_v1(domain, 240, bond, payload)`, the
//! message the native acceptance layer verifies with each bond's REGISTERED key. `seal` verifies each signature under the key
//! the signature file names (it cannot read the chain offline): a key that is not the bond's registered one is refused by the
//! chain, and the carrier fee is spent. The spec's terms are the caller's; the fold re-checks every policy bound.
use std::path::{Path, PathBuf};

use clap::Subcommand;
use kaspa_consensus_core::{
    Hash64,
    palw_attempt_material_v3::{PalwNativeMaterialSessionV3, native_job_policy_v3},
    palw_material_capacity_v1::PalwMaterialCodecV1,
    palw_monetary_material_v1::{
        PALW_MONETARY_MATERIAL_ENROLL_TAG_V1, PalwMonetaryMaterialEnrollmentV1, PalwMonetaryMaterialOfferV1,
        PalwMonetaryMaterialTermsV1, monetary_material_enrollment_object_v1,
    },
    palw_provider_court_v1::{PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1, palw_provider_court_message_v1},
    palw_state_v2::PalwBondKeyV2,
};
use serde::{Deserialize, Serialize};

use crate::{CliError, CliResult, exit};

pub(crate) const MATERIAL_SESSION_DRAFT_SCHEMA_V1: &str = "misaka.palw.material-session-draft.v1";
pub(crate) const MATERIAL_SESSION_SIGNATURE_SCHEMA_V1: &str = "misaka.palw.material-session-signature.v1";

#[derive(Subcommand, Debug)]
pub(crate) enum MaterialSessionCmd {
    /// Build the exact unsigned enrollment from a JSON spec and write the draft every party signs.
    Draft {
        #[arg(long)]
        spec: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    /// Sign the draft's consent as the sponsor or one provider (offline; the key must be the bond's registered key).
    Sign {
        #[command(flatten)]
        key: crate::KeyArgs,
        #[arg(long)]
        draft: PathBuf,
        /// The signing bond, `<txid>:<index>`: the sponsor or one offered provider.
        #[arg(long)]
        bond: String,
        #[arg(long)]
        out: PathBuf,
    },
    /// Check every party's signature and write the borsh `PalwConsensusObjectV2::MonetaryMaterialEnrolledV1`.
    Seal {
        #[arg(long)]
        draft: PathBuf,
        #[arg(long = "signature", required = true, num_args = 1..)]
        signatures: Vec<PathBuf>,
        #[arg(long)]
        out: PathBuf,
    },
}

/// The prospective session, as a spec names it (hex Hash64 ids, `<txid>:<index>` bonds).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeSessionSpecV1 {
    pub class_id: String,
    pub artifact_root: String,
    pub executor: String,
    pub operator: String,
    pub max_units: u32,
    pub admit_from_daa: u64,
    pub admit_until_daa: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OfferSpecV1 {
    pub provider: String,
    pub collateral_sompi: u64,
    pub prepaid_fee_sompi: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MaterialSessionSpecV1 {
    pub network_domain: String,
    pub sponsor: String,
    pub policy_id: String,
    pub window_daa: u64,
    pub max_material_bytes: u64,
    pub enrollment_until_daa: u64,
    /// Optional: derived from the other terms when absent (deterministic, nonzero).
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub nonce: Option<String>,
    pub native_session: NativeSessionSpecV1,
    pub offers: Vec<OfferSpecV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MaterialSessionDraftV1 {
    pub schema: String,
    pub network_domain: String,
    pub consent: String,
    pub root: String,
    /// Hex borsh of the unsigned `PalwMonetaryMaterialEnrollmentV1`.
    pub enrollment: String,
    /// Sponsor first, then providers in key order.
    pub signers: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct MaterialSessionSignatureV1 {
    pub schema: String,
    pub consent: String,
    pub bond: String,
    pub public_key: String,
    pub signature: String,
}

fn hash(field: &str, text: &str) -> Result<Hash64, CliError> {
    text.parse::<Hash64>().map_err(|e| CliError::generic(format!("{field}: '{text}' is not a 128-hex id: {e}")))
}
fn bond(field: &str, text: &str) -> Result<PalwBondKeyV2, CliError> {
    crate::bond::parse_outpoint(text).map(PalwBondKeyV2).map_err(|e| CliError::generic(format!("{field}: {}", e.msg)))
}
fn bond_text(b: &PalwBondKeyV2) -> String {
    format!("{}:{}", b.0.transaction_id, b.0.index)
}
fn unhex(field: &str, text: &str) -> Result<Vec<u8>, CliError> {
    let mut out = vec![0u8; text.len() / 2];
    faster_hex::hex_decode(text.as_bytes(), &mut out).map_err(|e| CliError::generic(format!("{field}: not hex: {e:?}")))?;
    Ok(out)
}
/// A keyed BLAKE2b-512 of the borsh terms: a deterministic, nonzero-with-overwhelming-probability default root/nonce.
fn derived(domain: &[u8], terms: &impl borsh::BorshSerialize) -> Result<Hash64, CliError> {
    let bytes = borsh::to_vec(terms).map_err(|e| CliError::generic(e.to_string()))?;
    let mut out = [0u8; 64];
    out.copy_from_slice(blake2b_simd::Params::new().hash_length(64).key(domain).hash(&bytes).as_bytes());
    Ok(Hash64::from_bytes(out))
}
fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, CliError> {
    let bytes = std::fs::read(path).map_err(|e| CliError::generic(format!("{}: {e}", path.display())))?;
    serde_json::from_slice(&bytes).map_err(|e| CliError::generic(format!("{}: {e}", path.display())))
}
/// A fresh file only: a draft, signature or object is never overwritten in place.
fn write_new(path: &Path, bytes: &[u8]) -> CliResult {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| CliError::generic(format!("{}: {e}", path.display())))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| CliError::generic(format!("{}: {e}", path.display())))
}

/// **The exact unsigned enrollment a spec names.** Offers are sorted by provider (the fold refuses another order); the
/// session carries the native job policy, the NativeHeld codec and the one-claim cap the fold requires; a missing root or nonce
/// is derived from the rest of the terms.
pub(crate) fn enrollment_from_spec_v1(spec: &MaterialSessionSpecV1) -> Result<PalwMonetaryMaterialEnrollmentV1, CliError> {
    let s = &spec.native_session;
    let session = PalwNativeMaterialSessionV3 {
        version: 3,
        class_id: hash("nativeSession.classId", &s.class_id)?,
        artifact_root: hash("nativeSession.artifactRoot", &s.artifact_root)?,
        executor: bond("nativeSession.executor", &s.executor)?,
        operator: hash("nativeSession.operator", &s.operator)?,
        job_policy: native_job_policy_v3(),
        codec: PalwMaterialCodecV1::NativeHeldComplete,
        max_units: s.max_units,
        one_claim_cap: 1,
        admit_from_daa: s.admit_from_daa,
        admit_until_daa: s.admit_until_daa,
    };
    if session.max_units == 0
        || session.admit_from_daa > session.admit_until_daa
        || session.admit_until_daa > spec.enrollment_until_daa
    {
        return Err(CliError::generic("nativeSession needs maxUnits > 0 and admitFrom ≤ admitUntil ≤ enrollmentUntilDaa"));
    }
    let mut offers = spec
        .offers
        .iter()
        .map(|o| {
            Ok(PalwMonetaryMaterialOfferV1 {
                provider: bond("offers.provider", &o.provider)?,
                collateral_sompi: o.collateral_sompi,
                prepaid_fee_sompi: o.prepaid_fee_sompi,
                signature: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    offers.sort_by_key(|o| o.provider);
    if offers.len() < 2 || offers.windows(2).any(|pair| pair[0].provider == pair[1].provider) {
        return Err(CliError::generic("a native session needs at least two distinct provider offers"));
    }
    let sponsor = bond("sponsor", &spec.sponsor)?;
    let nonce = match &spec.nonce {
        Some(text) => hash("nonce", text)?,
        None => derived(b"MISAKA/PALW/CLI/MATERIAL-SESSION-NONCE/V1", &(sponsor, session.clone(), spec.enrollment_until_daa))?,
    };
    let root = match &spec.root {
        Some(text) => hash("root", text)?,
        None => derived(b"MISAKA/PALW/CLI/MATERIAL-SESSION-ROOT/V1", &(sponsor, nonce, session.clone()))?,
    };
    if root == Hash64::default() || nonce == Hash64::default() {
        return Err(CliError::generic("root and nonce must be nonzero"));
    }
    Ok(PalwMonetaryMaterialEnrollmentV1 {
        terms: PalwMonetaryMaterialTermsV1 {
            version: 1,
            root,
            dependencies: Vec::new(),
            exec_plan_root: None,
            sponsor,
            nonce,
            policy: hash("policyId", &spec.policy_id)?,
            max_material_bytes: spec.max_material_bytes,
            material_manifest: None,
            native_session: Some(session),
            window_daa: spec.window_daa,
            enrollment_until_daa: spec.enrollment_until_daa,
            payment_relative_until_finality: true,
        },
        offers,
    })
}

fn decode_draft(draft: &MaterialSessionDraftV1) -> Result<(Hash64, PalwMonetaryMaterialEnrollmentV1), CliError> {
    if draft.schema != MATERIAL_SESSION_DRAFT_SCHEMA_V1 {
        return Err(CliError::generic(format!("not a {MATERIAL_SESSION_DRAFT_SCHEMA_V1} draft")));
    }
    let domain = hash("networkDomain", &draft.network_domain)?;
    let enrollment: PalwMonetaryMaterialEnrollmentV1 = borsh::from_slice(&unhex("enrollment", &draft.enrollment)?)
        .map_err(|e| CliError::generic(format!("draft enrollment: {e}")))?;
    if enrollment.id().to_string() != draft.consent || enrollment.terms.root.to_string() != draft.root {
        return Err(CliError::generic("the draft's consent/root do not match its enrollment bytes"));
    }
    Ok((domain, enrollment))
}

fn signers(enrollment: &PalwMonetaryMaterialEnrollmentV1) -> Vec<PalwBondKeyV2> {
    std::iter::once(enrollment.terms.sponsor).chain(enrollment.offers.iter().map(|o| o.provider)).collect()
}

pub(crate) fn draft_v1(spec: &MaterialSessionSpecV1) -> Result<MaterialSessionDraftV1, CliError> {
    hash("networkDomain", &spec.network_domain)?;
    let enrollment = enrollment_from_spec_v1(spec)?;
    Ok(MaterialSessionDraftV1 {
        schema: MATERIAL_SESSION_DRAFT_SCHEMA_V1.into(),
        network_domain: spec.network_domain.clone(),
        consent: enrollment.id().to_string(),
        root: enrollment.terms.root.to_string(),
        enrollment: faster_hex::hex_string(&borsh::to_vec(&enrollment).map_err(|e| CliError::generic(e.to_string()))?),
        signers: signers(&enrollment).iter().map(bond_text).collect(),
    })
}

/// The one message a party signs for this consent (`palw_validate`'s tag-240 check).
fn message(domain: Hash64, enrollment: &PalwMonetaryMaterialEnrollmentV1, signer: &PalwBondKeyV2) -> Hash64 {
    palw_provider_court_message_v1(domain, PALW_MONETARY_MATERIAL_ENROLL_TAG_V1, signer, &enrollment.payload())
}

pub(crate) fn run(cmd: MaterialSessionCmd) -> CliResult {
    match cmd {
        MaterialSessionCmd::Draft { spec, out } => {
            let draft = draft_v1(&read_json(&spec)?)?;
            write_new(&out, &serde_json::to_vec_pretty(&draft).map_err(|e| CliError::generic(e.to_string()))?)?;
            println!(
                "{}",
                serde_json::json!({"ok": true, "draft": out.display().to_string(), "consent": draft.consent,
                    "root": draft.root, "signers": draft.signers, "submitted": false})
            );
            Ok(())
        }
        MaterialSessionCmd::Sign { key, draft, bond: signer, out } => {
            let draft: MaterialSessionDraftV1 = read_json(&draft)?;
            let (domain, enrollment) = decode_draft(&draft)?;
            let signer = bond("--bond", &signer)?;
            if !signers(&enrollment).contains(&signer) {
                return Err(CliError::new(exit::GENERIC, "--bond is neither the sponsor nor an offered provider of this draft"));
            }
            let key = key.source().load_key()?;
            let signature = key
                .sign_with_context(message(domain, &enrollment, &signer).as_byte_slice(), PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1)
                .to_vec();
            let file = MaterialSessionSignatureV1 {
                schema: MATERIAL_SESSION_SIGNATURE_SCHEMA_V1.into(),
                consent: draft.consent.clone(),
                bond: bond_text(&signer),
                public_key: faster_hex::hex_string(key.public_key()),
                signature: faster_hex::hex_string(&signature),
            };
            write_new(&out, &serde_json::to_vec_pretty(&file).map_err(|e| CliError::generic(e.to_string()))?)?;
            println!(
                "{}",
                serde_json::json!({"ok": true, "signature": out.display().to_string(), "bond": file.bond, "consent": file.consent})
            );
            Ok(())
        }
        MaterialSessionCmd::Seal { draft, signatures, out } => {
            let draft: MaterialSessionDraftV1 = read_json(&draft)?;
            let (domain, enrollment) = decode_draft(&draft)?;
            let mut by_bond = std::collections::BTreeMap::new();
            for path in &signatures {
                let file: MaterialSessionSignatureV1 = read_json(path)?;
                if file.schema != MATERIAL_SESSION_SIGNATURE_SCHEMA_V1 || file.consent != draft.consent {
                    return Err(CliError::generic(format!("{}: a signature for another consent", path.display())));
                }
                let signer = bond("signature.bond", &file.bond)?;
                let signature = unhex("signature", &file.signature)?;
                let verified = kaspa_txscript::verify_mldsa87_with_context(
                    &unhex("publicKey", &file.public_key)?,
                    message(domain, &enrollment, &signer).as_byte_slice(),
                    &signature,
                    PALW_PROVIDER_COURT_MLDSA87_CONTEXT_V1,
                )
                .map_err(|e| CliError::generic(format!("{}: {e}", path.display())))?;
                if !verified || by_bond.insert(signer, signature).is_some() {
                    return Err(CliError::generic(format!("{}: signature does not verify, or a bond signed twice", path.display())));
                }
            }
            let missing: Vec<String> = signers(&enrollment).iter().filter(|b| !by_bond.contains_key(*b)).map(bond_text).collect();
            if !missing.is_empty() || by_bond.len() != signers(&enrollment).len() {
                return Err(CliError::generic(format!("unsigned or extra parties: missing {missing:?}")));
            }
            let object = monetary_material_enrollment_object_v1(domain, enrollment, |bond, _, _| by_bond.get(bond).cloned())
                .ok_or_else(|| CliError::generic("a party's signature is missing"))?;
            kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
                .map_err(|why| CliError::generic(format!("the enrollment cannot ride a carrier: {why}")))?;
            write_new(&out, &borsh::to_vec(&object).map_err(|e| CliError::generic(e.to_string()))?)?;
            println!(
                "{}",
                serde_json::json!({"ok": true, "object": out.display().to_string(), "consent": draft.consent, "root": draft.root,
                    "submitted": false, "next": "misaka palw submit-object --object <object> --yes"})
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> MaterialSessionSpecV1 {
        let txid = "ab".repeat(64);
        MaterialSessionSpecV1 {
            network_domain: Hash64::from_u64_word(1).to_string(),
            sponsor: format!("{txid}:0"),
            policy_id: Hash64::from_u64_word(2).to_string(),
            window_daa: 20,
            max_material_bytes: 1 << 20,
            enrollment_until_daa: 1_000,
            root: None,
            nonce: None,
            native_session: NativeSessionSpecV1 {
                class_id: Hash64::from_u64_word(3).to_string(),
                artifact_root: Hash64::from_u64_word(4).to_string(),
                executor: format!("{txid}:0"),
                operator: Hash64::from_u64_word(5).to_string(),
                max_units: 64,
                admit_from_daa: 0,
                admit_until_daa: 1_000,
            },
            offers: vec![
                OfferSpecV1 { provider: format!("{txid}:2"), collateral_sompi: 100, prepaid_fee_sompi: 100 },
                OfferSpecV1 { provider: format!("{txid}:1"), collateral_sompi: 100, prepaid_fee_sompi: 100 },
            ],
        }
    }

    #[test]
    fn a_spec_drafts_one_exact_sorted_native_session_and_refuses_a_one_provider_roster() {
        let draft = draft_v1(&spec()).unwrap();
        assert_eq!(draft_v1(&spec()).unwrap().consent, draft.consent, "deterministic");
        let (_, enrollment) = decode_draft(&draft).unwrap();
        assert!(enrollment.offers[0].provider < enrollment.offers[1].provider);
        let session = enrollment.terms.native_session.as_ref().unwrap();
        assert_eq!((session.version, session.one_claim_cap, session.job_policy), (3, 1, native_job_policy_v3()));
        assert!(enrollment.terms.material_manifest.is_none() && enrollment.terms.dependencies.is_empty());
        assert_eq!(draft.signers.len(), 3);
        let mut one = spec();
        one.offers.pop();
        assert!(draft_v1(&one).is_err());
        let mut late = spec();
        late.native_session.admit_until_daa = 1_001;
        assert!(draft_v1(&late).is_err());
        let mut tampered = draft.clone();
        tampered.consent = Hash64::from_u64_word(9).to_string();
        assert!(decode_draft(&tampered).is_err());
    }
}

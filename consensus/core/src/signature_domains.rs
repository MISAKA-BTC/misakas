//! kaspa-pq **ADR-0040 §D — the signature domain table**.
//!
//! # Why a table rather than per-pair fixes
//!
//! Cross-protocol signature replay is a *class* of defect, not a sequence of incidents. It was closed
//! once for the PALW auditor vote (a beacon-commit signature must not be replayable as an audit vote),
//! but closing it pairwise means the next signing object re-opens it, and the review that would have
//! caught it has nothing to check against.
//!
//! This table is the enforcement point for the rule "**every ML-DSA-87 signing object declares a
//! distinct libcrux `ctx`**". A new signed object is added here or the table test fails — the same
//! shape as ADR-0040's other rule/enforcement pairings (§2.6): a rule that lives only in prose is a
//! rule that will be violated by a type.
//!
//! # What belongs here
//!
//! Only **signature** contexts — the `ctx` argument to `verify_mldsa87_with_context` / `sign`. Keyed
//! *hash* domains (`blake2b_512_keyed` keys such as `OverlayCommit64` or `EvmPayload64`) are a separate
//! namespace: they domain-separate digests, not signatures, and a collision between the two namespaces
//! is harmless because neither is ever fed to the other's primitive. Mixing them into one table would
//! make the distinctness assertion say less than it appears to.
//!
//! # Known naming inconsistency (deliberately surfaced, not silently normalised)
//!
//! Most contexts follow `"<project>-v1/<purpose>/mldsa87"`. PALW's compact contexts do not (for
//! example `"PALWBeaconV1"` and `"PALWAuditorVoteV2"`). The auditor context was deliberately revised
//! with the V2 summary-binding wire; any further rename changes every signature it covers and is a
//! re-genesis-only change.

/// One row of the signature domain table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SignatureDomain {
    /// The object whose signature this context covers.
    pub object: &'static str,
    /// The libcrux `ctx` bytes.
    pub context: &'static [u8],
    /// Where the signing preimage is defined.
    pub defined_in: &'static str,
}

/// **Every ML-DSA-87 signature context in consensus.** Adding a signed object without adding a row
/// here is caught by [`tests::every_signature_domain_is_distinct`] only if the row is added — so the
/// discipline is: *new signing object ⇒ new row, in the same commit.*
pub const SIGNATURE_DOMAINS: &[SignatureDomain] = &[
    SignatureDomain {
        object: "DNS validator attestation",
        context: crate::dns_finality::ATTESTATION_MLDSA87_CONTEXT,
        defined_in: "dns_finality::StakeAttestationPayload",
    },
    SignatureDomain {
        object: "DNS unbond request",
        context: crate::dns_finality::UNBOND_REQUEST_CONTEXT,
        defined_in: "dns_finality::UnbondRequestPayload",
    },
    SignatureDomain {
        object: "DNS validator takeover token",
        context: crate::dns_finality::TAKEOVER_TOKEN_CONTEXT,
        defined_in: "dns_finality::TakeoverToken",
    },
    SignatureDomain {
        object: "DNS audit checkpoint",
        context: crate::dns_finality::AUDIT_CHECKPOINT_MLDSA87_CONTEXT,
        defined_in: "dns_finality::AuditCheckpoint",
    },
    SignatureDomain {
        object: "PALW beacon commit/reveal",
        context: crate::palw::PALW_BEACON_MLDSA87_CONTEXT,
        defined_in: "palw::PalwBeaconCommitV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW batch-certificate auditor vote",
        context: crate::palw::PALW_AUDITOR_V2_MLDSA87_CONTEXT,
        defined_in: "palw::PalwAuditorVoteV2::signing_hash",
    },
    SignatureDomain {
        object: "PALW per-block ticket authorization",
        context: crate::palw::PALW_AUTHORIZATION_MLDSA87_CONTEXT,
        defined_in: "palw::PalwBlockAuthorizationV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW provider-bond unbond request",
        context: crate::palw::PALW_PROVIDER_UNBOND_MLDSA87_CONTEXT,
        defined_in: "palw::PalwProviderUnbondRequestV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW off-chain compute receipt v3",
        context: crate::palw::PALW_RECEIPT_V3_MLDSA87_CONTEXT,
        defined_in: "misaka_palw::receipt_v3::ComputeReceiptV3::signing_digest",
    },
    SignatureDomain {
        object: "PALW off-chain compute job-spec transport",
        context: crate::palw::PALW_COMPUTE_JOBSPEC_V2_MLDSA87_CONTEXT,
        defined_in: "runtime testnet::job_spec_signing_digest (worker wire jobspec.v2+scheduler-mldsa87)",
    },
    SignatureDomain {
        object: "PALW PCPB partner-B receipt",
        context: crate::palw::PALW_PCPB_RECEIPT_MLDSA87_CONTEXT,
        defined_in: "palw PCPB partner-B receipt preimage (PALW_PCPB_RECEIPT_TAG || A_commit)",
    },
    SignatureDomain {
        object: "PALW search job assignment",
        context: crate::palw::search_snapshot::PALW_SEARCH_ASSIGNMENT_MLDSA87_CONTEXT,
        defined_in: "palw::search_snapshot::PalwSearchAssignmentV1::verify",
    },
    SignatureDomain {
        object: "PALW search snapshot anchor",
        context: crate::palw::search_snapshot::PALW_SEARCH_ANCHOR_MLDSA87_CONTEXT,
        defined_in: "palw::search_snapshot::PalwSearchSignedAnchorV1::verify",
    },
    SignatureDomain {
        object: "PALW search availability challenge",
        context: crate::palw::search_snapshot::PALW_SEARCH_CHALLENGE_MLDSA87_CONTEXT,
        defined_in: "palw::search_snapshot::PalwSearchChallengeTxV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW search availability timeout evidence",
        context: crate::palw::search_snapshot::PALW_SEARCH_TIMEOUT_MLDSA87_CONTEXT,
        defined_in: "palw::search_snapshot::PalwSearchTimeoutTxV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW DA replica execution receipt v1",
        context: crate::palw::da::PALW_REPLICA_RECEIPT_V1_MLDSA87_CONTEXT,
        defined_in: "palw::ReplicaExecutionReceiptV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW DA provider owner-to-session authorization",
        context: crate::palw::da::PALW_PROVIDER_SESSION_V1_MLDSA87_CONTEXT,
        defined_in: "palw::da::PalwProviderSessionAuthorizationV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW DA on-chain challenge",
        context: crate::palw::da::PALW_DA_CHALLENGE_V1_MLDSA87_CONTEXT,
        defined_in: "palw::da::PalwDaChallengeV1::signing_hash",
    },
    SignatureDomain {
        object: "PALW DA provider response",
        context: crate::palw::da::PALW_DA_RESPONSE_V1_MLDSA87_CONTEXT,
        defined_in: "palw::da::PalwDaResponseV1::signing_hash",
    },
    SignatureDomain {
        object: "F003 PREA account root key",
        context: crate::evm::F003_PREA_ROOT_MLDSA87_CONTEXT,
        defined_in: "evm::F003 PREA",
    },
    SignatureDomain {
        object: "F003 PREA per-operation key",
        context: crate::evm::F003_PREA_OP_MLDSA87_CONTEXT,
        defined_in: "evm::F003 PREA",
    },
    SignatureDomain { object: "F003 FSL verify", context: crate::evm::F003_FSL_VERIFY_MLDSA87_CONTEXT, defined_in: "evm::F003 FSL" },
];

/// Objects that ADR-0040 expects to sign something but which have **no context yet**, because the
/// object itself is unimplemented. Listed so the gap is visible in the same place as the table rather
/// than being discovered when someone implements one and reaches for an existing constant.
pub const PENDING_SIGNATURE_DOMAINS: &[&str] = &[
    // ADR-0040 §16′: the per-job completion tip is a fee-era lever, not yet specified.
    "PALW job completion tip (§16′ fee lane)",
    // ADR-0040 P2-1: provider/credential registration in the single bonded registry.
    "PALW provider credential registration (P2-1)",
    // The MIL inference lane (F003 MIL-receipt precompile) remains distinct from the now-active PALW
    // Receipt v3 wire contract. Porting that lane must add its own row rather than reusing PALW's ctx.
    "MIL provider receipt (F003 MIL receipt — lane not built here)",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// The enforcement point for "cross-protocol replay is closed as a class".
    ///
    /// Distinctness is the whole property: two objects sharing a `ctx` means a signature over one can be
    /// presented as a signature over the other whenever their preimages can be made to coincide — and
    /// preimage coincidence is exactly the kind of thing a later refactor introduces by accident.
    #[test]
    fn every_signature_domain_is_distinct() {
        let mut seen: HashSet<&[u8]> = HashSet::new();
        for d in SIGNATURE_DOMAINS {
            assert!(!d.context.is_empty(), "{}: an empty context provides no separation at all", d.object);
            assert!(seen.insert(d.context), "duplicate signature context {:?} — {} collides with an earlier row", d.context, d.object);
        }
        assert_eq!(seen.len(), SIGNATURE_DOMAINS.len());
    }

    /// No context may be a prefix of another. Distinctness alone is not sufficient for every encoding:
    /// if `ctx` were ever concatenated with a variable-length field rather than passed as its own
    /// argument, `"A"` and `"AB"` would become confusable. Enforcing prefix-freedom costs nothing now
    /// and removes that whole failure mode from the design's future.
    #[test]
    fn signature_domains_are_prefix_free() {
        for a in SIGNATURE_DOMAINS {
            for b in SIGNATURE_DOMAINS {
                if a.context == b.context {
                    continue;
                }
                assert!(
                    !a.context.starts_with(b.context),
                    "{:?} ({}) is a prefix of {:?} ({})",
                    b.context,
                    b.object,
                    a.context,
                    a.object
                );
            }
        }
    }

    /// The PALW rows deliberately diverge from the `"<project>-v1/<purpose>/mldsa87"` convention. This
    /// test PINS that divergence rather than hiding it: renaming them changes every signature they
    /// cover, so it is a re-genesis-only change. If a future PALW object copies the wrong convention,
    /// this is where the decision surfaces.
    #[test]
    fn palw_naming_divergence_is_pinned_not_forgotten() {
        let palw: Vec<_> = SIGNATURE_DOMAINS.iter().filter(|d| d.object.starts_with("PALW")).collect();
        // Thirteen compact-context PALW objects plus the two OFF-CHAIN wire contracts (Receipt v3
        // and the compute job-spec transport), which use the slash convention. DA-01 deliberately
        // keeps each role replay-incompatible.
        assert_eq!(palw.len(), 15, "if a PALW signing object was added, decide its naming convention explicitly");
        let off_chain_slash: &[(&str, &[u8])] = &[
            ("PALW off-chain compute receipt v3", b"misaka-palw-v3/receipt/mldsa87"),
            ("PALW off-chain compute job-spec transport", b"misaka-palw-v3/jobspec/mldsa87"),
        ];
        for (object, context) in off_chain_slash {
            let row = palw.iter().find(|d| d.object == *object).expect("off-chain slash-convention row");
            assert_eq!(&row.context, context);
        }
        let auditor =
            palw.iter().find(|d| d.object == "PALW batch-certificate auditor vote").expect("auditor vote signature-domain row");
        assert_eq!(auditor.context, b"PALWAuditorVoteV2");
        for d in palw.iter().filter(|d| !off_chain_slash.iter().any(|(object, _)| d.object == *object)) {
            assert!(
                !d.context.contains(&b'/'),
                "{} now follows the slash convention — update this test and the module note",
                d.object
            );
        }
    }

    /// **The table is LOCKED (2026-07-25).** This golden is an independent duplication of every
    /// row: the registry can only change when this test is updated in the same commit, which is
    /// exactly the review surface the lock exists to force. Removing or editing a row is a
    /// re-genesis-scale decision; adding one requires an explicit naming decision above AND a new
    /// golden line here.
    #[test]
    fn signature_domain_table_is_locked() {
        let golden: &[(&str, &[u8])] = &[
            ("DNS validator attestation", b"kaspa-pq-v1/att/mldsa87"),
            ("DNS unbond request", b"kaspa-pq-v1/unbond/mldsa87"),
            ("DNS validator takeover token", b"kaspa-pq-v1/takeover/mldsa87"),
            ("DNS audit checkpoint", b"kaspa-pq-v1/audit-ckpt/mldsa87"),
            ("PALW beacon commit/reveal", b"PALWBeaconV1"),
            ("PALW batch-certificate auditor vote", b"PALWAuditorVoteV2"),
            ("PALW per-block ticket authorization", b"PALWBlockAuthorizationV1"),
            ("PALW provider-bond unbond request", b"PALWProviderUnbondV1"),
            ("PALW off-chain compute receipt v3", b"misaka-palw-v3/receipt/mldsa87"),
            ("PALW off-chain compute job-spec transport", b"misaka-palw-v3/jobspec/mldsa87"),
            ("PALW PCPB partner-B receipt", b"PALWPcpbReceiptV1"),
            ("PALW search job assignment", b"PALWSearchAssignmentV1"),
            ("PALW search snapshot anchor", b"PALWSearchAnchorV1"),
            ("PALW search availability challenge", b"PALWSearchChallengeV1"),
            ("PALW search availability timeout evidence", b"PALWSearchTimeoutV1"),
            ("PALW DA replica execution receipt v1", b"PALWReplicaReceiptV1"),
            ("PALW DA provider owner-to-session authorization", b"PALWProviderSessionV1"),
            ("PALW DA on-chain challenge", b"PALWDAChallengeV1"),
            ("PALW DA provider response", b"PALWDAResponseV1"),
            ("F003 PREA account root key", b"misaka-pq-evm-v1/root/mldsa87"),
            ("F003 PREA per-operation key", b"misaka-pq-evm-v1/op/mldsa87"),
            ("F003 FSL verify", b"misaka-pq-fsl-v1/verify/mldsa87"),
        ];
        assert_eq!(SIGNATURE_DOMAINS.len(), golden.len(), "signature-domain table size drifted from the LOCKED golden");
        for (row, (object, context)) in SIGNATURE_DOMAINS.iter().zip(golden) {
            assert_eq!(row.object, *object, "signature-domain table order/name drifted from the LOCKED golden");
            assert_eq!(&row.context, context, "{object}: context bytes drifted from the LOCKED golden");
        }
    }

    /// A row must not silently lose its context (e.g. a constant refactored to `b""`).
    #[test]
    fn pending_domains_are_named_not_empty() {
        for p in PENDING_SIGNATURE_DOMAINS {
            assert!(!p.is_empty());
        }
    }

    #[test]
    fn retired_auditor_v1_context_is_never_reused() {
        for domain in SIGNATURE_DOMAINS {
            assert_ne!(
                domain.context,
                crate::palw::PALW_RETIRED_AUDITOR_V1_MLDSA87_CONTEXT,
                "{} reuses the retired PALW auditor V1 context",
                domain.object
            );
        }
    }
}

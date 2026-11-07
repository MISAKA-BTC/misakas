//! **ADR-0150: the fingerprint must see the RULE, not only the height.**
//!
//! `consensus_params_id` hashes `Params`, and for a fenced rule it hashes the fence's HEIGHT — the
//! address the rule takes effect at, never the rule. So two builds that disagree about what a valid
//! block IS, at a height they both schedule, fingerprint identically: they peer, agree about every
//! block until that height, and split there with nothing anywhere saying which rule either was
//! running. 2026-09-20 walked into exactly that (the V2 possession proof changed meaning at an
//! unchanged `palw_readiness_v2`), and it was safe only because that fence has never been in force.
//!
//! This module is the answer: one hand-written manifest of **semantic revisions**, one per ruleset,
//! hashed into the fingerprint. A developer who changes what VALID means raises the revision of the
//! ruleset they changed; the build then fingerprints differently, and an operator can see it.
//!
//! **Why not the binary, and why not the commit.** A build hash makes a log line, an optimisation
//! and a compiler bump into consensus incompatibilities, and a field that cries wolf is a field
//! operators learn to ignore. The cost of writing a revision by hand is the point: it is a
//! developer saying "this changes validity", in the diff, where a reviewer reads it.

use kaspa_hashes::Hash64;

fn keyed64(domain: &[u8]) -> blake2b_simd::State {
    blake2b_simd::Params::new().hash_length(64).key(domain).to_state()
}
fn finish64(state: blake2b_simd::State) -> Hash64 {
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The domain the manifest hashes under. Distinct from the fingerprint's own domain so a manifest
/// digest can never collide with a params digest.
pub const PALW_RULE_MANIFEST_DOMAIN_V1: &[u8] = b"misaka-palw/consensus-rule-manifest/v1";

/// **Which fence a ruleset rides**, so the identity can tell "in force" from "scheduled".
///
/// The identity normalises a FUTURE fence away (`for_each_fence` rewrites its height to "not yet"),
/// which is what lets a fleet roll a build out before the height it takes effect at. A ruleset's
/// revision follows its fence: in force now, it separates networks; scheduled, it is reported and
/// not gated, exactly like the height it rides with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRulesetFenceV1 {
    /// No fence: the rule is in force wherever this ruleset exists at all.
    Unfenced,
    /// The named fence of `Params`, read through [`PalwRuleManifestGatesV1`].
    Fenced(PalwRulesetGateV1),
}

/// The fences a ruleset can ride. One variant per gate the manifest needs to ask about — deliberately
/// a small closed set, because a ruleset whose gate is not here has not been thought about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRulesetGateV1 {
    ReadinessV2,
    VerificationV2,
    ModelRegistry,
    EconomicPayout,
    WorkTarget,
    CanonicalWork,
    AdmissionIndependence,
    FpDerivedWork,
    AnchorClock,
    DaCourt,
}

/// **What a `Params` says about each gate**: in force at genesis (or unfenced), scheduled for a
/// future height, or absent. Built by `Params` and read here, so this module needs no `Params`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwRuleManifestGatesV1 {
    pub readiness_v2: PalwGateStateV1,
    pub verification_v2: PalwGateStateV1,
    pub model_registry: PalwGateStateV1,
    pub economic_payout: PalwGateStateV1,
    pub work_target: PalwGateStateV1,
    pub canonical_work: PalwGateStateV1,
    pub admission_independence: PalwGateStateV1,
    pub fp_derived_work: PalwGateStateV1,
    pub anchor_clock: PalwGateStateV1,
    pub da_court: PalwGateStateV1,
}

/// A gate's standing on one network.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PalwGateStateV1 {
    /// The fence is not set at all: the ruleset cannot fire on this network.
    #[default]
    Absent,
    /// Set to a height above genesis: the rule is not in force yet, and a rollout may cross it.
    Scheduled,
    /// Active at genesis (`always()`, or height zero): the rule is in force now.
    InForce,
}

impl PalwRuleManifestGatesV1 {
    fn state(&self, gate: PalwRulesetGateV1) -> PalwGateStateV1 {
        match gate {
            PalwRulesetGateV1::ReadinessV2 => self.readiness_v2,
            PalwRulesetGateV1::VerificationV2 => self.verification_v2,
            PalwRulesetGateV1::ModelRegistry => self.model_registry,
            PalwRulesetGateV1::EconomicPayout => self.economic_payout,
            PalwRulesetGateV1::WorkTarget => self.work_target,
            PalwRulesetGateV1::CanonicalWork => self.canonical_work,
            PalwRulesetGateV1::AdmissionIndependence => self.admission_independence,
            PalwRulesetGateV1::FpDerivedWork => self.fp_derived_work,
            PalwRulesetGateV1::AnchorClock => self.anchor_clock,
            PalwRulesetGateV1::DaCourt => self.da_court,
        }
    }
}

/// One ruleset: a name, the revision of its SEMANTICS, and the fence it takes effect at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRulesetV1 {
    pub name: &'static str,
    pub revision: u32,
    pub fence: PalwRulesetFenceV1,
}

/// **The build's own manifest.** Every entry is a rule this tree can change without moving a field
/// of `Params`; raising a revision is how a build says it did.
///
/// Adding an entry moves every fingerprint once, which is correct and is why the set should be
/// frozen before mainnet genesis (ADR-0150 §5).
pub const PALW_CONSENSUS_RULE_MANIFEST_V1: &[PalwRulesetV1] = &[
    // **The unfenced half: rules in force wherever this code runs.** A verdict can come from any of
    // them, so each is listed even while it sits at R1 and says nothing — a subsystem that is not
    // here cannot be versioned when it changes, and "we forgot to list it" is the same silent fork
    // as "we forgot to bump it".
    //
    // Header validity: what makes a header well-formed and acceptable before any state is read.
    PalwRulesetV1 { name: "header_validity", revision: 1, fence: PalwRulesetFenceV1::Unfenced },
    // The state transition: how an accepted block moves the PALW state (`PALW_STATE_V2_VERSION` is
    // hashed separately and stays; this names the RULES around it).
    PalwRulesetV1 { name: "state_transition", revision: 1, fence: PalwRulesetFenceV1::Unfenced },
    // Fork choice and the weight a block carries into it.
    PalwRulesetV1 { name: "fork_choice", revision: 1, fence: PalwRulesetFenceV1::Unfenced },
    // Pruning and what an IBD node reconstructs — an interpretation two builds can differ on
    // without either rejecting a block, which is the quietest divergence of all.
    PalwRulesetV1 { name: "pruning_ibd", revision: 1, fence: PalwRulesetFenceV1::Unfenced },
    // The DAA window and what counts toward it. R1 is the rule as shipped.
    PalwRulesetV1 { name: "daa", revision: 1, fence: PalwRulesetFenceV1::Unfenced },
    // ADR-0138/0142: the anchor clock and the cursor that paces it.
    PalwRulesetV1 { name: "palw_clock", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::AnchorClock) },
    // ADR-0131/0137/0145: what an attempt's work IS, and what it is priced in.
    PalwRulesetV1 { name: "palw_accounting", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::CanonicalWork) },
    // ADR-0148/0149: the free-prompt lane's derivation, quanta and reservation.
    PalwRulesetV1 { name: "palw_freeprompt", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::FpDerivedWork) },
    // ADR-0135/0147: the registry's lifecycle, the jury, and what a class must show to be admitted.
    PalwRulesetV1 { name: "palw_admission", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::ModelRegistry) },
    // **ADR-0133 §11.2, R2 as of 2026-09-20** (`9ef0d326`): the V2 possession challenge is spent in
    // BYTES — the prefix of its draw that one carrier's budget buys — because sixteen leaves
    // serialized to 184,037 bytes for the shipped class and no transaction carries that. R1 is the
    // sixteen-leaf rule as first written. This entry is the reason ADR-0150 exists.
    PalwRulesetV1 { name: "palw_readiness", revision: 2, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::ReadinessV2) },
    // ADR-0132/0124: what a licensed claim pays and who it pays.
    PalwRulesetV1 { name: "palw_payout", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::EconomicPayout) },
    // ADR-0062/0082/0092: the court's moves, its ladder and its clock.
    PalwRulesetV1 { name: "palw_court", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::DaCourt) },
    // ADR-0133 S1: what a seat must verify before it licenses.
    PalwRulesetV1 { name: "palw_verification", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::VerificationV2) },
    // ADR-0137: the work target the ticket is measured against.
    PalwRulesetV1 { name: "palw_work_target", revision: 1, fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::WorkTarget) },
    // ADR-0145 §7: registration is not eligibility — the independence half of admission.
    PalwRulesetV1 {
        name: "palw_independence",
        revision: 1,
        fence: PalwRulesetFenceV1::Fenced(PalwRulesetGateV1::AdmissionIndependence),
    },
];

/// **Every revision, for the fingerprint an operator reads.** A build's whole answer to "what rules
/// do you carry", independent of which of them are in force here.
pub fn palw_rule_manifest_digest_v1() -> Hash64 {
    let mut state = keyed64(PALW_RULE_MANIFEST_DOMAIN_V1);
    state.update(b"all");
    for ruleset in PALW_CONSENSUS_RULE_MANIFEST_V1 {
        state.update(&(ruleset.name.len() as u64).to_le_bytes());
        state.update(ruleset.name.as_bytes());
        state.update(&ruleset.revision.to_le_bytes());
    }
    finish64(state)
}

/// **The revisions of the rulesets this network can run at all** — the one function both ids use.
///
/// `consensus_params_id` calls it with the network's real gates, so a ruleset scheduled for a
/// future height contributes its revision: an operator's fingerprint moves when a rule they will
/// run changes, which is ADR-0150's whole point.
///
/// `consensus_identity_id` calls it through the SAME path, on a clone whose fences have already
/// been normalised to "not yet" — which reads here as [`PalwGateStateV1::Absent`], so a scheduled
/// ruleset drops out and only the rules in force today separate networks. That is what lets a
/// fleet roll a build out before the height its new rule takes effect at (ADR-0066 SA-4, audit3
/// H1): two builds that differ about a rule neither is running yet are still one network, and the
/// operator sees the difference in the fingerprint and the printed line instead of at the fence.
pub fn palw_rule_manifest_digest_for_gates_v1(gates: &PalwRuleManifestGatesV1) -> Option<Hash64> {
    let mut spoken = 0usize;
    let mut body = Vec::new();
    for ruleset in PALW_CONSENSUS_RULE_MANIFEST_V1 {
        // **R1 is the rule as this tree first shipped it, and it says nothing.**
        //
        // A manifest that spoke at R1 would add bytes to every id on every network, so the day this
        // landed every node's identity would move and a fleet mid-rollout would partition — for a
        // change that redefined nothing. R1 is therefore silent by construction: a build where
        // every ruleset is R1 fingerprints byte-identically to a build from before the manifest
        // existed, and the FIRST bump is the first time anybody's number moves. That also makes the
        // encoding honest about what it is: a list of rules that have changed since they were
        // written, not a census.
        if ruleset.revision <= 1 {
            continue;
        }
        let present = match ruleset.fence {
            PalwRulesetFenceV1::Unfenced => true,
            PalwRulesetFenceV1::Fenced(gate) => gates.state(gate) != PalwGateStateV1::Absent,
        };
        if !present {
            continue;
        }
        // **Length-prefixed, typed and in the manifest's own order** — so `("ab", 1)` and
        // `("a", 0xb1)` cannot produce one preimage, and no iteration order can make two nodes hash
        // the same manifest differently. The const array IS the canonical order.
        spoken += 1;
        body.extend_from_slice(&(ruleset.name.len() as u64).to_le_bytes());
        body.extend_from_slice(ruleset.name.as_bytes());
        body.extend_from_slice(&ruleset.revision.to_le_bytes());
    }
    if spoken == 0 {
        return None;
    }
    // The format's own tag and the number of entries lead the preimage: a future encoding cannot
    // collide with this one, and a manifest of N entries cannot be read as a manifest of M.
    let mut state = keyed64(PALW_RULE_MANIFEST_DOMAIN_V1);
    state.update(b"gated/v1");
    state.update(&(spoken as u64).to_le_bytes());
    state.update(&body);
    Some(finish64(state))
}

/// The manifest as an operator reads it: `daa=1 palw_clock=1 …`.
pub fn palw_rule_manifest_line_v1() -> String {
    PALW_CONSENSUS_RULE_MANIFEST_V1.iter().map(|r| format!("{}={}", r.name, r.revision)).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gates_all(state: PalwGateStateV1) -> PalwRuleManifestGatesV1 {
        PalwRuleManifestGatesV1 {
            readiness_v2: state,
            verification_v2: state,
            model_registry: state,
            economic_payout: state,
            work_target: state,
            canonical_work: state,
            admission_independence: state,
            fp_derived_work: state,
            anchor_clock: state,
            da_court: state,
        }
    }

    /// **The manifest is revisions and nothing else** (ADR-0150 §3.2): no paths, no build ids, no
    /// timestamps — nothing a log change, an optimisation or a compiler bump can move.
    #[test]
    fn the_manifest_is_revisions_and_nothing_else() {
        assert!(!PALW_CONSENSUS_RULE_MANIFEST_V1.is_empty());
        let mut names: Vec<&str> = PALW_CONSENSUS_RULE_MANIFEST_V1.iter().map(|r| r.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "two rulesets share a name; a digest cannot tell them apart");
        for ruleset in PALW_CONSENSUS_RULE_MANIFEST_V1 {
            assert!(ruleset.revision >= 1, "{} has no revision", ruleset.name);
            assert!(
                ruleset.name.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "{}: a ruleset name is a stable identifier, not prose",
                ruleset.name
            );
        }
        // The one entry this mechanism was built for.
        let readiness = PALW_CONSENSUS_RULE_MANIFEST_V1.iter().find(|r| r.name == "palw_readiness").expect("readiness is listed");
        assert_eq!(readiness.revision, 2, "2026-09-20's byte-budget prefix is R2; raising it again is a new rule");
    }

    /// **Every subsystem a consensus verdict can come from is listed** (ADR-0150 §2.2).
    ///
    /// Coverage is the failure mode that actually happens: a ruleset nobody listed cannot be
    /// versioned when it changes, and "we forgot to list it" produces the same silent fork as "we
    /// forgot to bump it". The list is asserted rather than described, so adding a subsystem to the
    /// tree without adding it here is a red build.
    #[test]
    fn every_subsystem_a_verdict_comes_from_is_in_the_manifest() {
        let listed: Vec<&str> = PALW_CONSENSUS_RULE_MANIFEST_V1.iter().map(|r| r.name).collect();
        for wanted in [
            "header_validity",
            "state_transition",
            "fork_choice",
            "pruning_ibd",
            "daa",
            "palw_clock",
            "palw_accounting",
            "palw_freeprompt",
            "palw_admission",
            "palw_readiness",
            "palw_payout",
            "palw_court",
            "palw_verification",
            "palw_work_target",
            "palw_independence",
        ] {
            assert!(listed.contains(&wanted), "{wanted} decides verdicts and is not in the manifest: {listed:?}");
        }
    }

    /// **The encoding cannot be confused by concatenation, and has one canonical order.**
    ///
    /// `("ab", 1)` and `("a", 0xb1)` must not share a preimage; nor may the same manifest hash
    /// differently on two nodes because something iterated in a different order. The const array is
    /// the order, the fields are length-prefixed, and the format tag and the entry count lead.
    #[test]
    fn the_manifest_encoding_is_canonical_and_unambiguous() {
        // Same bytes, split differently: distinct digests.
        let split_a = digest_of_pairs(&[("ab", 1), ("c", 2)]);
        let split_b = digest_of_pairs(&[("a", 1), ("bc", 2)]);
        assert_ne!(split_a, split_b, "two field splits share a preimage");
        // Order is part of the meaning: the same pairs in another order are another manifest.
        assert_ne!(digest_of_pairs(&[("a", 1), ("b", 2)]), digest_of_pairs(&[("b", 2), ("a", 1)]));
        // A prefix of a manifest is not that manifest.
        assert_ne!(digest_of_pairs(&[("a", 1)]), digest_of_pairs(&[("a", 1), ("b", 1)]));
        // And the const list is what the real digest walks, in its own order.
        let real: Vec<(&str, u32)> =
            PALW_CONSENSUS_RULE_MANIFEST_V1.iter().filter(|r| r.revision > 1).map(|r| (r.name, r.revision)).collect();
        assert_eq!(
            palw_rule_manifest_digest_for_gates_v1(&gates_all(PalwGateStateV1::InForce)),
            Some(digest_of_pairs(&real)),
            "the digest is the const array's own order, length-prefixed"
        );
    }

    /// The preimage the real digest builds, for the tests above.
    fn digest_of_pairs(pairs: &[(&str, u32)]) -> Hash64 {
        let mut body = Vec::new();
        for (name, revision) in pairs {
            body.extend_from_slice(&(name.len() as u64).to_le_bytes());
            body.extend_from_slice(name.as_bytes());
            body.extend_from_slice(&revision.to_le_bytes());
        }
        let mut state = keyed64(PALW_RULE_MANIFEST_DOMAIN_V1);
        state.update(b"gated/v1");
        state.update(&(pairs.len() as u64).to_le_bytes());
        state.update(&body);
        finish64(state)
    }

    /// **A revision moves the digest** — the property the fingerprint lacked.
    #[test]
    fn a_ruleset_revision_moves_the_manifest_digest() {
        let all = palw_rule_manifest_digest_v1();
        let bumped = {
            let mut state = keyed64(PALW_RULE_MANIFEST_DOMAIN_V1);
            state.update(b"all");
            for ruleset in PALW_CONSENSUS_RULE_MANIFEST_V1 {
                state.update(&(ruleset.name.len() as u64).to_le_bytes());
                state.update(ruleset.name.as_bytes());
                let revision = if ruleset.name == "palw_readiness" { ruleset.revision + 1 } else { ruleset.revision };
                state.update(&revision.to_le_bytes());
            }
            finish64(state)
        };
        assert_ne!(all, bumped, "R2 and R3 of one ruleset must not digest the same");
    }

    /// **A scheduled ruleset's revision leaves the identity alone, an in-force one does not**
    /// (ADR-0150 §2.1). The first is what lets a fleet roll out before a height; the second is what
    /// stops two builds running different rules TODAY from calling themselves one network.
    #[test]
    fn a_scheduled_rulesets_revision_leaves_the_identity_alone() {
        // The identity's path normalises a scheduled fence to "not yet", which reads here as
        // Absent — so what the handshake compares is the in-force column of this table.
        let scheduled = palw_rule_manifest_digest_for_gates_v1(&gates_all(PalwGateStateV1::Scheduled));
        let absent = palw_rule_manifest_digest_for_gates_v1(&gates_all(PalwGateStateV1::Absent));
        let in_force = palw_rule_manifest_digest_for_gates_v1(&gates_all(PalwGateStateV1::InForce));
        assert!(scheduled.is_some(), "a network that schedules a changed rule speaks it");
        assert_eq!(absent, None, "a network that cannot run the rule says nothing about it");
        assert_eq!(scheduled, in_force, "the digest is the same bytes either way; the identity's normalisation is what drops it");
        // And a manifest with nothing above R1 is silent everywhere — the property that lets this
        // land without moving a single identity on the networks running today.
        assert_eq!(
            PALW_CONSENSUS_RULE_MANIFEST_V1.iter().filter(|r| r.revision > 1).map(|r| r.name).collect::<Vec<_>>(),
            vec!["palw_readiness"],
            "one ruleset has moved since it was written; add to this list deliberately"
        );
        // The unfenced rulesets are in every one of them: they are in force wherever they exist.
        let unfenced: Vec<&str> = PALW_CONSENSUS_RULE_MANIFEST_V1
            .iter()
            .filter(|r| matches!(r.fence, PalwRulesetFenceV1::Unfenced))
            .map(|r| r.name)
            .collect();
        assert!(unfenced.contains(&"daa"), "the DAA ruleset rides no fence: {unfenced:?}");
    }
}

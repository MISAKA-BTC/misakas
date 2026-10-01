//! **The fold's half of pipeline-claim data availability** (spec 17 §17.14), dormant under
//! `palw_improvement_v1`. A child module of `palw_state_v2`, as the improvement lanes' fold modules are,
//! so it reads the builder and the state's tables directly.
//!
//! * [`pipeline_claim_facts_v1`]: what kind of pipeline claim a claim is, from the fold's own state — an
//!   evaluation claim is one in `improvement_eval_claims`; a generative claim is one whose class is in
//!   `gen_classes` — and the facts every answer is checked against (kind, class, execution root);
//! * [`open_da_session_pipeline_step_v1`]: `DefaultAccusedPipelineStep` (object tag 83) opens an
//!   R-core+ DA session naming exactly the demanded unit — no binding, no draws — and every gate, budget,
//!   clock and record of `open_da_session_rcore_v1` applies unchanged;
//! * [`check_pipeline_answer_v1`]: an answer to a pipeline unit, by hash arithmetic over the claim's
//!   committed roots alone (`palw_pipeline_da_v1`).

use super::*;
use crate::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
use crate::palw_pipeline_da_v1::*;

/// **What the fold knows of a pipeline claim**: its kind, its class and its execution root — or why the
/// claim is not one. An evaluation claim is one of an epoch's jobs (`improvement_eval_claims`); a
/// generative claim is one whose class is a registered generative class (`gen_classes`).
pub(super) fn pipeline_claim_facts_v1(
    state: &PalwChainStateV2,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> Result<PalwPipelineClaimFactsV1, &'static str> {
    let kind = if state.improvement_eval_claims.contains_key(claim_id) {
        PalwPipelineKindV1::Eval
    } else if state.gen_classes.contains_key(&claim.class_id) {
        PalwPipelineKindV1::Gen
    } else {
        return Err(
            "the claim is not a pipeline claim: a generative claim's class is a registered generative class, an evaluation claim one of an epoch's jobs",
        );
    };
    Ok(PalwPipelineClaimFactsV1 { kind, class_id: claim.class_id, execution_root: claim.execution_root })
}

/// **A pipeline step demand opens an R-core+ DA session** (spec 17 §17.14.4).
///
/// Keyed by the claim alone: the claim must be a pipeline claim, and the unit a step leaf or an interior
/// step node inside the widest execution ([`palw_pipeline_step_unit_is_admissible_v1`]). No binding
/// rides — the fold derives no step space — so a unit past the claim's own stage is the accused's to
/// prove (`PipelineStepOutOfRange`). The session names that one unit, no draws.
pub(super) fn open_da_session_pipeline_step_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    accusation: &PalwPipelineStepAccusationV1,
) -> Result<(), PalwStateV2Error> {
    if !builder.da_court {
        return Err(PalwStateV2Error::DaCourtDormant);
    }
    if !builder.params.pipeline_da_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::PipelineDaRefused("a pipeline step demand before palw_improvement_v1 is in force"));
    }
    if !builder.params.rcore_plus_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::PipelineDaRefused(
            "a pipeline step demand opens an R-core+ session, and R-core+ is not in force",
        ));
    }
    let claim_id = accusation.claim;
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
    pipeline_claim_facts_v1(&builder.state, &claim_id, claim)
        .map_err(|why| PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why })?;
    palw_pipeline_step_unit_is_admissible_v1(&accusation.unit).map_err(PalwStateV2Error::PipelineDaRefused)?;
    open_da_session_rcore_v1(builder, ctx, claim_id, accusation.accuser, accusation.unit, None)
}

/// **Check an answer to a pipeline unit** (spec 17 §17.14.3): the fence, the claim's kind and facts, then
/// the answer's own arithmetic against the unit. `Ok(())` answers the unit; every refusal is a
/// `DaOpeningRefused` or `DaAnswerMalformed` the caller names the claim in.
pub(super) fn check_pipeline_answer_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
    unit: &PalwDaUnitV1,
    answer: &PalwDaAnswerV1,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.pipeline_da_active_at(ctx.daa_score) {
        return Err(PalwStateV2Error::DaAnswerMalformed {
            claim: claim_id,
            why: "a pipeline answer before palw_improvement_v1 is in force",
        });
    }
    let facts = pipeline_claim_facts_v1(&builder.state, &claim_id, claim)
        .map_err(|why| PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why })?;
    let refused = |why: &'static str| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: why.to_string() };
    match (unit, answer) {
        (PalwDaUnitV1::PipelineStepLeaf { stage, index }, PalwDaAnswerV1::PipelineStepLeaf(d)) => {
            check_pipeline_step_leaf_v1(&facts, *stage, *index, d).map_err(refused)
        }
        (PalwDaUnitV1::PipelineStepNode { stage, level, index }, PalwDaAnswerV1::PipelineStepNode(d)) => {
            check_pipeline_step_node_v1(&facts, *stage, *level, *index, d).map_err(refused)
        }
        (PalwDaUnitV1::PipelineStepLeaf { .. } | PalwDaUnitV1::PipelineStepNode { .. }, PalwDaAnswerV1::PipelineStepOutOfRange(d)) => {
            check_pipeline_out_of_range_v1(&facts, unit, d).map_err(refused)
        }
        _ => Err(PalwStateV2Error::DaAnswerMalformed {
            claim: claim_id,
            why: "a pipeline step unit is answered by its disclosure or an out-of-range proof",
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
    use crate::palw_gen_step_v1::{PalwGenLeafCoordV1, PalwGenLeafKindV1, palw_gen_stage_root_v1};
    use crate::tx::{TransactionId, TransactionOutpoint};

    const PRODUCER: u64 = 1;
    /// The claim's one panel seat: four sessions on the claim over its life (DA-8).
    const SEAT: u64 = 2;
    /// A bond off the panel (its own budget: three open, sixteen ever).
    const OTHER: u64 = 3;
    /// The improvement fence, after the claim is bound at 3.
    const FENCE: u64 = 6;
    const WINDOW_CHALLENGE: u64 = 20;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn bond_key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(n), index: 0 })
    }

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, WINDOW_CHALLENGE, 500, 1000, h64(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_fp_quanta(8, 64)
            .unwrap()
            .with_fp_exposure_ceiling(500)
            .unwrap()
            .with_rcore_plus_mirrors(Some(0), 0, Vec::new())
            .with_improve_from_daa(Some(FENCE))
            .with_improve_ceilings(Some(crate::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1))
    }

    struct Run {
        p: PalwStateParamsV2,
        s: PalwChainStateV2,
        daa: u64,
    }

    impl Run {
        fn ctx(daa: u64) -> PalwBlockContextV2 {
            PalwBlockContextV2 { block: h64(0xB10C_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 0 }
        }

        fn try_at(
            &self,
            daa: u64,
            objects: &[PalwConsensusObjectV2],
            att: Option<&PalwAttemptEnvelopeV2>,
        ) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
            apply_palw_transition_v2_with_extras(
                &self.s,
                &self.p,
                &Self::ctx(daa),
                objects,
                att,
                false,
                false,
                false,
                true,
                &PalwTransitionExtrasV1::default(),
            )
        }

        fn at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], att: Option<&PalwAttemptEnvelopeV2>) {
            assert!(daa > self.daa, "DAA moves forward");
            let (child, delta) = self.try_at(daa, objects, att).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
            assert_eq!(apply_delta_v2(&self.s, &delta, &self.p).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
            assert_eq!(revert_delta_v2(&child, &delta, &self.p).expect("reverts"), self.s, "DAA {daa}: the delta reverts");
            self.s = child;
            self.daa = daa;
        }

        fn step(&mut self, objects: &[PalwConsensusObjectV2]) {
            self.at(self.daa + 1, objects, None);
        }

        fn refused(&self, objects: &[PalwConsensusObjectV2]) -> PalwStateV2Error {
            self.try_at(self.daa + 1, objects, None).expect_err("the fold refuses it")
        }

        fn session_units(&self, claim: &Hash64, accuser: u64) -> Option<Vec<PalwDaUnitV1>> {
            self.s.da_sessions_of(claim).find(|(bond, _)| **bond == bond_key(accuser)).map(|(_, s)| s.units.clone())
        }
    }

    fn bond(n: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(n),
            pubkey: vec![6 + n as u8; 4],
            operator_pubkey: vec![20 + n as u8; 8],
            collateral: 1_000_000,
            payout_payload: Hash64::from_u64_word(0x9A00 + n),
            capable_classes: Default::default(),
            signature: Vec::new(),
        }
    }

    fn coord(stage: u8, i: u64) -> PalwGenLeafCoordV1 {
        PalwGenLeafCoordV1 {
            stage,
            pos: (i / 3) as u32,
            kind: PalwGenLeafKindV1::Commit { occurrence: 0, node: (i % 3) as u16 },
            tile: (i % 5) as u32,
        }
    }

    fn lanes(i: u64) -> Vec<u8> {
        (0..(1 + i % 4)).flat_map(|k| ((i * 7 + k) as u32).to_le_bytes()).collect()
    }

    fn stage_leaves(stage: u8, n: u64) -> Vec<Hash64> {
        (0..n).map(|i| palw_pipeline_leaf_hash_v1(&coord(stage, i), &lanes(i)).expect("whole lanes")).collect()
    }

    /// The class a claim runs: a generative class row's own id.
    fn gen_class_id() -> Hash64 {
        crate::palw_gen_class_v1::PalwGenClassRecordV1::test_row_v1(2).class_id
    }

    /// A generative claim over three stages of `counts` leaves: the claim's compact binding, and each stage's leaves.
    fn gen_binding(counts: [u64; 3]) -> (PalwPipelineBindingV1, Vec<Vec<Hash64>>) {
        let leaves: Vec<Vec<Hash64>> = counts.iter().enumerate().map(|(s, n)| stage_leaves(s as u8, *n)).collect();
        let stage_roots: Vec<Hash64> = leaves.iter().enumerate().map(|(s, l)| palw_gen_stage_root_v1(s as u8, l)).collect();
        let binding = PalwPipelineBindingV1::Gen(PalwPipelineGenPartsV1 {
            job_id: h64(500),
            class_id: gen_class_id(),
            step_leaf_count: counts.iter().sum(),
            stage_roots,
            generated: vec![5, 6, 7],
        });
        (binding, leaves)
    }

    /// The chain up to a live claim whose execution root is `binding`'s: the floor class, three bonds and a
    /// class that is a registered generative class (`is_gen`) or not, the claim at 2, its panel — one seat,
    /// `SEAT` — at 3. The improvement fence arms at `FENCE`.
    fn claimed(binding: &PalwPipelineBindingV1, is_gen: bool) -> (Run, Hash64) {
        let mut run = Run { p: params(), s: PalwChainStateV2::genesis(), daa: 0 };
        let class_id = gen_class_id();
        let class = |id: Hash64, share: u16| PalwConsensusObjectV2::ClassRegistered {
            class_id: id,
            artifact_root: h64(11),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::MaxPerAttempt(160),
            initial_target: u128::MAX / 2,
            share_permille: share,
            activation_daa: 0,
            admission: None,
        };
        run.at(1, &[class(h64(1), 1000), class(class_id, 0), bond(PRODUCER), bond(SEAT), bond(OTHER)], None);
        if is_gen {
            run.s.gen_classes.insert(class_id, crate::palw_gen_class_v1::PalwGenClassRecordV1::test_row_v1(2));
        }
        let network_domain = h64(999);
        let producer = bond_key(PRODUCER).0;
        let env = PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain,
                challenge: challenge_v2(network_domain, h64(5), 1_700, 1, class_id, &producer),
                class_id,
                executor_bond: producer,
                executor_pubkey: vec![6 + PRODUCER as u8; 4],
                operator_id: palw_operator_id_v2(&vec![20 + PRODUCER as u8; 8]),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu: 40,
                trace_manifest_root: h64(33),
                trace_chunk_count: 1,
                trace_retention_daa: 999_999,
                execution_root: binding.execution_root(),
            },
            signature: vec![0; 8],
        };
        let claim_id = attempt_id_v2(&env.attempt);
        run.at(2, &[], Some(&env));
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(SEAT), operator_id: palw_operator_id_v2(&vec![20 + SEAT as u8; 8]) }];
        run.at(3, &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }], None);
        assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }));
        (run, claim_id)
    }

    fn demand(claim: Hash64, unit: PalwDaUnitV1, accuser: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::DefaultAccusedPipelineStep {
            accusation: Box::new(PalwPipelineStepAccusationV1 { claim, unit, accuser: bond_key(accuser), signature: vec![1; 8] }),
        }
    }

    fn disclosed(claim: Hash64, unit: PalwDaUnitV1, answer: PalwDaAnswerV1) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::MaterialDisclosedV2 { claim, unit, answer, discloser: bond_key(PRODUCER), signature: vec![2; 8] }
    }

    fn leaf_answer(
        binding: &PalwPipelineBindingV1,
        leaves: &[Vec<Hash64>],
        claim: Hash64,
        stage: u8,
        i: u64,
    ) -> PalwConsensusObjectV2 {
        let l = &leaves[stage as usize];
        disclosed(
            claim,
            PalwDaUnitV1::PipelineStepLeaf { stage, index: i },
            PalwDaAnswerV1::PipelineStepLeaf(Box::new(PalwPipelineStepLeafDisclosureV1 {
                version: PALW_PIPELINE_DA_VERSION_V1,
                binding: binding.clone(),
                stage_leaf_count: l.len() as u64,
                coord: coord(stage, i),
                lanes_le: lanes(i),
                siblings: palw_pipeline_leaf_siblings_v1(l, i).expect("in the tree"),
            })),
        )
    }

    fn node_answer(
        binding: &PalwPipelineBindingV1,
        leaves: &[Vec<Hash64>],
        claim: Hash64,
        stage: u8,
        level: u8,
        i: u64,
    ) -> PalwConsensusObjectV2 {
        let l = &leaves[stage as usize];
        let (frontier, siblings) = palw_pipeline_node_parts_v1(l, level, i).expect("a node");
        disclosed(
            claim,
            PalwDaUnitV1::PipelineStepNode { stage, level, index: i },
            PalwDaAnswerV1::PipelineStepNode(Box::new(PalwPipelineStepNodeDisclosureV1 {
                version: PALW_PIPELINE_DA_VERSION_V1,
                binding: binding.clone(),
                stage_leaf_count: l.len() as u64,
                frontier,
                siblings,
            })),
        )
    }

    fn out_of_range(
        binding: &PalwPipelineBindingV1,
        leaves: &[Vec<Hash64>],
        claim: Hash64,
        unit: PalwDaUnitV1,
        stage: u8,
    ) -> PalwConsensusObjectV2 {
        let stage_tree = leaves
            .get(stage as usize)
            .map(|l| PalwPipelineStageTreeV1 { leaf_count: l.len() as u64, merkle_root: palw_pipeline_merkle_root_v1(l) });
        disclosed(
            claim,
            unit,
            PalwDaAnswerV1::PipelineStepOutOfRange(Box::new(PalwPipelineOutOfRangeV1 {
                version: PALW_PIPELINE_DA_VERSION_V1,
                binding: binding.clone(),
                stage_tree,
            })),
        )
    }

    /// **A demand names one unit and the producer's answer refutes its session; the claim stands.** Below the
    /// improvement fence every move of the section is refused by name, a replayed demand opens nothing, and a
    /// unit past every execution is refused at the door.
    #[test]
    fn a_pipeline_demand_names_one_unit_and_the_producers_answer_refutes_its_session() {
        let (binding, leaves) = gen_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, true);
        let node = PalwDaUnitV1::PipelineStepNode { stage: 1, level: 11, index: 0 };
        let named = demand(claim_id, node, OTHER);
        // Below the fence: refused by name — the second lock behind the acceptance walk's drop — whichever the object.
        assert!(palw_object_is_pipeline_da_v1(&named));
        assert!(matches!(run.try_at(FENCE - 1, std::slice::from_ref(&named), None), Err(PalwStateV2Error::PipelineDaRefused(_))));
        let answer = node_answer(&binding, &leaves, claim_id, 1, 11, 0);
        assert!(palw_object_is_pipeline_da_v1(&answer));
        assert!(matches!(run.try_at(FENCE - 1, std::slice::from_ref(&answer), None), Err(PalwStateV2Error::PipelineDaRefused(_))));
        run.at(FENCE, &[named], None);
        assert_eq!(run.session_units(&claim_id, OTHER), Some(vec![node]), "named only: no draws");
        run.step(&[answer]);
        assert!(run.session_units(&claim_id, OTHER).is_none(), "the session is refuted and closed");
        assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");
        // A replay of the answered demand opens nothing (the M3 review's F3 rule).
        assert!(matches!(run.refused(&[demand(claim_id, node, OTHER)]), PalwStateV2Error::DaUnitAlreadyAnswered(_)));
        // A unit past every execution never opens a session.
        for unit in [
            PalwDaUnitV1::PipelineStepLeaf { stage: 16, index: 0 },
            PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 1 << 40 },
            PalwDaUnitV1::PipelineStepNode { stage: 0, level: 0, index: 0 },
            PalwDaUnitV1::PipelineStepNode { stage: 0, level: 41, index: 0 },
            PalwDaUnitV1::Event { row: 0, tile: 0 },
        ] {
            assert!(matches!(run.refused(&[demand(claim_id, unit, SEAT)]), PalwStateV2Error::PipelineDaRefused(_)), "{unit:?}");
        }
    }

    /// **Leaves and out-of-range proofs.** A leaf is answered by its preimage and opening; a unit past its stage by the
    /// binding with the stage's tree; each answers its demand and nothing else's.
    #[test]
    fn leaves_and_out_of_range_proofs_answer_their_demands() {
        let (binding, leaves) = gen_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, true);
        let leaf = PalwDaUnitV1::PipelineStepLeaf { stage: 1, index: 700 };
        let past = PalwDaUnitV1::PipelineStepLeaf { stage: 2, index: 2 };
        let beyond = PalwDaUnitV1::PipelineStepLeaf { stage: 5, index: 0 };
        run.at(FENCE, &[demand(claim_id, leaf, OTHER), demand(claim_id, past, SEAT)], None);
        assert_eq!(run.session_units(&claim_id, OTHER), Some(vec![leaf]));
        // The wrong answers: another leaf's, a leaf past its stage, an out-of-range proof of a unit inside it.
        assert!(matches!(run.refused(&[leaf_answer(&binding, &leaves, claim_id, 1, 701)]), PalwStateV2Error::DaUnitNotDemanded(_)));
        assert!(matches!(
            run.refused(&[out_of_range(&binding, &leaves, claim_id, leaf, 1)]),
            PalwStateV2Error::DaOpeningRefused { .. }
        ));
        let mut wrong = leaf_answer(&binding, &leaves, claim_id, 1, 700);
        if let PalwConsensusObjectV2::MaterialDisclosedV2 { answer: PalwDaAnswerV1::PipelineStepLeaf(d), .. } = &mut wrong {
            d.lanes_le[0] ^= 1;
        }
        assert!(matches!(run.refused(&[wrong]), PalwStateV2Error::DaOpeningRefused { .. }), "other lanes do not reach the stage root");
        run.step(&[leaf_answer(&binding, &leaves, claim_id, 1, 700)]);
        assert!(run.session_units(&claim_id, OTHER).is_none());
        // Past the stage's tree, and past the claim's stages: the binding proves it.
        assert!(matches!(run.refused(&[leaf_answer(&binding, &leaves, claim_id, 2, 1)]), PalwStateV2Error::DaUnitNotDemanded(_)));
        run.step(&[out_of_range(&binding, &leaves, claim_id, past, 2)]);
        assert!(run.session_units(&claim_id, SEAT).is_none(), "an index at the stage's count is proven out of range");
        run.step(&[demand(claim_id, beyond, OTHER)]);
        run.step(&[out_of_range(&binding, &leaves, claim_id, beyond, 5)]);
        assert!(run.session_units(&claim_id, OTHER).is_none(), "a stage past the claim's three is proven so by the binding alone");
    }

    /// **An answer is the claim's own, or it is refused**: another execution's binding (the claim commits another
    /// root), and a binding of another kind than the claim's.
    #[test]
    fn an_answer_of_another_executions_roots_is_refused() {
        let (binding, leaves) = gen_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, true);
        let unit = PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 3 };
        run.at(FENCE, &[demand(claim_id, unit, OTHER)], None);
        let (other, other_leaves) = gen_binding([9, 1031, 2]);
        assert!(matches!(
            run.refused(&[leaf_answer(&other, &other_leaves, claim_id, 0, 3)]),
            PalwStateV2Error::DaOpeningRefused { .. }
        ));
        // The same parts as an evaluation binding's kind: refused (the claim is a generative claim).
        let PalwPipelineBindingV1::Gen(parts) = &binding else { unreachable!() };
        let eval = PalwPipelineBindingV1::Eval(PalwPipelineEvalPartsV1 {
            job_id: parts.job_id,
            subject_class: parts.class_id,
            step_leaf_count: parts.step_leaf_count,
            stage_roots: parts.stage_roots.clone(),
            prompt_root: h64(1),
            prompt_tokens: 3,
            params: crate::palw_improve_eval_v1::PalwEvalStageParamsV1::Pairwise { margin: 0, logit_scale_q24: 1 << 24 },
            generated_root: h64(2),
            finalized_root: h64(3),
            score: vec![0],
        });
        assert!(matches!(run.refused(&[leaf_answer(&eval, &leaves, claim_id, 0, 3)]), PalwStateV2Error::DaOpeningRefused { .. }));
        run.step(&[leaf_answer(&binding, &leaves, claim_id, 0, 3)]);
        assert!(run.session_units(&claim_id, OTHER).is_none());
    }

    /// **A demand on a claim that is not a pipeline claim is refused at the door**: it could never be answered, and the
    /// default would convict an honest producer of another kind of claim.
    #[test]
    fn a_demand_on_a_claim_that_is_not_a_pipeline_claim_is_refused() {
        let (binding, _) = gen_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, false);
        let refused = run.try_at(FENCE, &[demand(claim_id, PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 0 }, OTHER)], None);
        assert!(matches!(refused, Err(PalwStateV2Error::DaAnswerMalformed { .. })), "{refused:?}");
        let missing = run.try_at(FENCE, &[demand(h64(0xDEAD), PalwDaUnitV1::PipelineStepLeaf { stage: 0, index: 0 }, OTHER)], None);
        assert!(matches!(missing, Err(PalwStateV2Error::MissingClaim(_))));
    }

    /// **A producer that answers nothing inside `W_disclose` defaults the claim.**
    #[test]
    fn a_producer_silent_past_the_window_defaults() {
        let (binding, _) = gen_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, true);
        run.at(FENCE, &[demand(claim_id, PalwDaUnitV1::PipelineStepNode { stage: 1, level: 11, index: 0 }, SEAT)], None);
        assert!(run.session_units(&claim_id, SEAT).is_some());
        let deadline = run.s.da_sessions_of(&claim_id).map(|(_, s)| s.deadline_daa).max().expect("a deadline");
        run.at(deadline + 1, &[], None);
        let phase = run.s.claim(&claim_id).map(|c| c.phase.clone());
        assert!(matches!(phase, Some(PalwClaimPhaseV2::Voided { .. })), "a withheld node defaults the claim: {phase:?}");
        assert!(run.session_units(&claim_id, SEAT).is_none(), "the session closed with the default");
    }

    /// **The tags**: the units 5 and 6, the answers 7–9, the demand object 83, and the reserved tags 84 and 85 that no
    /// block can carry.
    #[test]
    fn the_tags_are_the_specs() {
        use borsh::BorshDeserialize;
        let leaf = PalwDaUnitV1::PipelineStepLeaf { stage: 1, index: 0x0102_0304_0506_0708 };
        assert_eq!(borsh::to_vec(&leaf).unwrap(), vec![5, 1, 8, 7, 6, 5, 4, 3, 2, 1]);
        let node = PalwDaUnitV1::PipelineStepNode { stage: 2, level: 9, index: 5 };
        assert_eq!(borsh::to_vec(&node).unwrap(), vec![6, 2, 9, 5, 0, 0, 0, 0, 0, 0, 0]);
        assert!(PalwDaUnitV1::TirRowNode { level: 255, index: u64::MAX } < leaf && leaf < node, "appended in order");
        let (binding, leaves) = gen_binding([3, 3, 3]);
        let tags: Vec<u8> = [
            leaf_answer(&binding, &leaves, h64(1), 0, 0),
            node_answer(&binding, &leaves, h64(1), 0, 1, 0),
            out_of_range(&binding, &leaves, h64(1), leaf, 0),
        ]
        .iter()
        .map(|object| {
            let PalwConsensusObjectV2::MaterialDisclosedV2 { answer, .. } = object else { unreachable!() };
            borsh::to_vec(answer).unwrap()[0]
        })
        .collect();
        assert_eq!(tags, vec![7, 8, 9]);
        assert_eq!(borsh::to_vec(&demand(h64(1), leaf, SEAT)).unwrap()[0], 83, "the demand is object tag 83");
        for reserved in [84u8, 85] {
            let bytes = [vec![reserved], vec![0; 64]].concat();
            assert!(PalwConsensusObjectV2::try_from_slice(&bytes).is_err(), "tag {reserved} decodes to nothing");
        }
    }

    /// An evaluation claim's compact binding over the same three-stage tree shape as [`gen_binding`]'s.
    fn eval_binding(counts: [u64; 3]) -> (PalwPipelineBindingV1, Vec<Vec<Hash64>>) {
        let leaves: Vec<Vec<Hash64>> = counts.iter().enumerate().map(|(s, n)| stage_leaves(s as u8, *n)).collect();
        let stage_roots: Vec<Hash64> = leaves.iter().enumerate().map(|(s, l)| palw_gen_stage_root_v1(s as u8, l)).collect();
        let binding = PalwPipelineBindingV1::Eval(PalwPipelineEvalPartsV1 {
            job_id: h64(501),
            subject_class: gen_class_id(),
            step_leaf_count: counts.iter().sum(),
            stage_roots,
            prompt_root: h64(1),
            prompt_tokens: 3,
            params: crate::palw_improve_eval_v1::PalwEvalStageParamsV1::Pairwise { margin: 0, logit_scale_q24: 1 << 24 },
            generated_root: h64(2),
            finalized_root: h64(3),
            score: vec![0],
        });
        (binding, leaves)
    }

    /// **An evaluation claim is one of an epoch's jobs**, answered by an evaluation binding and by no other
    /// kind: before the chain records the job the claim is no pipeline claim and a demand on it is refused at
    /// the door; once it does, a generative binding of the same trees is refused and the evaluation binding
    /// answers.
    #[test]
    fn an_evaluation_claim_is_answered_by_an_evaluation_binding() {
        use crate::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
        let (binding, leaves) = eval_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, false);
        let unit = PalwDaUnitV1::PipelineStepLeaf { stage: 1, index: 500 };
        let refused = run.try_at(FENCE, &[demand(claim_id, unit, OTHER)], None);
        assert!(matches!(refused, Err(PalwStateV2Error::DaAnswerMalformed { .. })), "no job recorded: {refused:?}");
        // The index is derived from the job table (`improvement_eval_jobs`), so the test records the job row that holds the
        // claim and rebuilds the index, as every load and delta path does.
        let job = crate::palw_improve_eval_v1::PalwEvalJobV1 {
            line_id: h64(10),
            epoch: 1,
            item: 0,
            subject: PalwEvalSubjectV1::Parent,
            kind: PalwScoringKindV1::ExactMatch,
            part: 0,
            mode: crate::palw_improve_eval_v1::PalwEvalModeV1::Generate { seed: h64(0), max_new: 4, stop_ids: vec![] },
        };
        let claim = crate::palw_improve_eval_v1::PalwEvalClaimRefV1 {
            claim_id,
            executor: bond_key(PRODUCER),
            accepted_daa: 2,
            output_root: h64(32),
            answer: None,
            final_daa: None,
            score: None,
        };
        run.s.improvement_eval_jobs.insert(job.key(), crate::palw_improve_eval_v1::PalwEvalJobStateV1 { job, claim: Some(claim) });
        run.s.improvement_eval_claims = super::palw_improve_eval_fold_v1::palw_improve_eval_claims_index_v1(&run.s.improvement_eval_jobs);
        run.at(FENCE, &[demand(claim_id, unit, OTHER)], None);
        assert_eq!(run.session_units(&claim_id, OTHER), Some(vec![unit]));
        let (generative, gen_leaves) = gen_binding([9, 1030, 2]);
        assert!(matches!(
            run.refused(&[leaf_answer(&generative, &gen_leaves, claim_id, 1, 500)]),
            PalwStateV2Error::DaOpeningRefused { .. }
        ));
        run.step(&[leaf_answer(&binding, &leaves, claim_id, 1, 500)]);
        assert!(run.session_units(&claim_id, OTHER).is_none(), "the evaluation binding answers it");
        assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");
    }

    /// A tensor claim's compact binding (an image or an embedding: its output is a digest) over the same shape of trees.
    fn tensor_binding(counts: [u64; 3]) -> (PalwPipelineBindingV1, Vec<Vec<Hash64>>) {
        let leaves: Vec<Vec<Hash64>> = counts.iter().enumerate().map(|(s, n)| stage_leaves(s as u8, *n)).collect();
        let stage_roots: Vec<Hash64> = leaves.iter().enumerate().map(|(s, l)| palw_gen_stage_root_v1(s as u8, l)).collect();
        let binding = PalwPipelineBindingV1::Tensor(PalwPipelineTensorPartsV1 {
            job_id: h64(502),
            class_id: gen_class_id(),
            step_leaf_count: counts.iter().sum(),
            stage_roots,
            output_root: h64(7),
        });
        (binding, leaves)
    }

    /// **A tensor claim is a generative claim**: the same demand, answered by a tensor binding whose execution root's own
    /// domain (`palw_gen_tensor_execution_root_v1`) is the claim's, and by no text binding of the same trees.
    #[test]
    fn a_tensor_claim_is_answered_by_a_tensor_binding() {
        let (binding, leaves) = tensor_binding([9, 1030, 2]);
        let (mut run, claim_id) = claimed(&binding, true);
        let unit = PalwDaUnitV1::PipelineStepLeaf { stage: 1, index: 700 };
        run.at(FENCE, &[demand(claim_id, unit, OTHER)], None);
        // The text binding of the same trees: its execution root is in another domain, so it is not the claim's.
        let (text, text_leaves) = gen_binding([9, 1030, 2]);
        assert!(matches!(
            run.refused(&[leaf_answer(&text, &text_leaves, claim_id, 1, 700)]),
            PalwStateV2Error::DaOpeningRefused { .. }
        ));
        run.step(&[leaf_answer(&binding, &leaves, claim_id, 1, 700)]);
        assert!(run.session_units(&claim_id, OTHER).is_none(), "the tensor binding answers it");
        assert!(matches!(run.s.claim(&claim_id).expect("live").phase, PalwClaimPhaseV2::PanelBound { .. }), "the claim stands");
    }
}

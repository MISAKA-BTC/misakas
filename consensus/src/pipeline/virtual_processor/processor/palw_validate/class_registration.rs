//! Object authentication and admission for the existing class_registration family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_class_registration_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
                Obj::ClassRegistered {
                    class_id,
                    share_permille,
                    admission,
                    // Checked by `palw_v2_class_registration_starts_at_the_chains_target`, below.
                    initial_target: _,
                    // Signed over by `palw_v2_class_registration_is_signed`, below.
                    activation_daa: _,
                    artifact_root: _,
                    slash_value_per_pwu: _,
                    pwu_rule: _,
                } => {
                    // **ADR-0049 Decision H: the gate, where the refusal used to be.**
                    //
                    // A class registration is the one object whose validity is an arithmetic fact
                    // about a GRAPH — that every kernel it reaches is adjudicable (ADR-0038 A4),
                    // that its longest job fits the ruleset's ladder, that prosecuting it costs
                    // what the ruleset allows (Decision C), and that its declared
                    // `pwu_per_inference` is the COUNTED one rather than a number that multiplies
                    // its own fork-choice weight. `verify_class_admission_v2` performs all four,
                    // and it needs the shape profile and the canonical job to do it.
                    //
                    // Neither used to ride the object, so the honest answer was no. Both ride it
                    // now, so the honest answer is "let the gate decide" — and the refusal that
                    // stood here was never a policy, it was the absence of a check. Genesis
                    // registrations do not reach this path at all; they are checked by
                    // `verify_palw_genesis_v2` against the catalog the ruleset id commits to.
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("class {class_id} registered on a network with no V2 bundle"));
                    };
                    let Some(carriage) = admission.as_ref() else {
                        return Err(format!(
                            "class {class_id} carries no shape profile, so coverage, ladder depth, court cost and pwu                              cannot be checked (ADR-0049 Decision H)"
                        ));
                    };
                    // Decision H's share rule: an entrant joins at the MINIMUM grantable share and
                    // no more. A registrant that could name its own permille would be donating
                    // itself an arbitrary slice of every incumbent's cadence, which is the share
                    // table's own conservation rule read backwards.
                    // **The entrant's difficulty is the chain's, not the registrant's** (audit
                    // M2-12). `initial_target` seeds the class's own retarget, so a registrant
                    // naming a huge one mines its class for free until the first retarget catches
                    // up — and naming a tiny one makes the class unminable, which is a way to park
                    // a share nobody can use. The base class's live target is what a registration
                    // is offered (`palw_v2_registration_terms`), and it is what must arrive.
                    self.palw_v2_class_registration_starts_at_the_chains_target(state, object)?;
                    // **An entrant joins at the minimum grantable share — or at NOTHING, when no
                    // certified family can prosecute it** (ADR-0069 Decisions 5 and 6).
                    //
                    // Decision H's rule was `share == min_grantable`, full stop, and
                    // `min_grantable` is never zero (`⌈10⁶/(tol·E)⌉.max(1)`). ADR-0069 then refused
                    // a nonzero grant to a family no drill has certified. Together those two closed
                    // the door completely: measured on this build, the adjudicator catalogs 44
                    // kernels and the certified families cover 37, so a class reaching one of the
                    // other 7 was statically adjudicable, refused weight by the gate, and refused
                    // registration by this line — it could not join at ALL.
                    //
                    // That is the opposite of what ADR-0069 decided. Weight is what certification
                    // buys; EXISTENCE is not, and Decision 5 made a zero grant expressible in
                    // `granted_share_table_v2` precisely so an uncertified family could register,
                    // produce, gossip and count for liveness while its adjudication was built. This
                    // path never let a zero through, so that state was reachable only at genesis —
                    // which is how the model tiers sat at 0‰ while nobody else could.
                    //
                    // The share is still exactly one value, so a registrant cannot choose it — the
                    // thing Decision H protects. What changed is that the value is a function of
                    // the class's own graph rather than a constant.
                    //
                    // **The certified set is the NETWORK's, not this process's.** The drilled
                    // registry is filled by a crate consensus links only as a dev dependency
                    // (ADR-0042 Decision 4: "a node's consensus never links a model runtime"), so
                    // reading it here would make this rule correct on a producing node and wrong on
                    // a validating one — two nodes disagreeing about a block, which is the failure
                    // `court_e2e_root` exists to prevent. `palw_rc_certified_families_v1` derives
                    // the same set from profiles every node already has, and hashes to that root.
                    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
                    // ADR-0075 Decision 4: the chain's own certified families count exactly as
                    // the genesis ones — an entrant whose family a `FamilyCertified` object
                    // certified joins at the floor, not weightless.
                    let chain_certified =
                        state.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt);
                    let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
                        bundle.court_e2e_root,
                        &certified,
                        &chain_certified,
                        &kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&carriage.profile),
                    )
                    .map_err(|e| format!("class {class_id}: {e}"))?
                    .is_some();
                    let floor = state_params.min_grantable_share_permille();
                    // **ADR-0145 §7 and I4, past `palw_admission_independence`: registration buys
                    // existence, and cadence is earned.** The minimum grantable share is not a
                    // small number — it is a permille taken from every incumbent by donation, and
                    // it moves their weight, their budget and (through
                    // `attempt_target_seed_v1(share, pwu)`) their difficulty, for a class nobody
                    // has yet verified. Past the fence every entrant joins weightless, exactly as
                    // an uncertified one already does, and earns its share from the registry once
                    // it has passed an admission its own registrant cannot grant it.
                    //
                    // This gate and the transition must agree or no prosecutable class could
                    // register at all: the fold refuses a bought registration carrying any share
                    // (`RegistrationTakesNoShare`), so a `required` of `floor` here would be a rule
                    // demanding exactly what the next layer rejects.
                    let independence = self.palw_admission_independence_at(point.daa_score);
                    let required = if prosecutable && !independence { floor } else { 0 };
                    if *share_permille != required {
                        return Err(if prosecutable && !independence {
                            format!(
                                "class {class_id} registers at {share_permille}‰; a post-genesis entrant joins at the \
                                 minimum grantable share ({floor}‰) — ADR-0049 Decision H"
                            )
                        } else if independence {
                            format!(
                                "class {class_id} registers at {share_permille}‰; past palw_admission_independence a \
                                 registration buys existence and not cadence, so every entrant joins at 0‰ and earns its \
                                 share from an admission it has passed — ADR-0145 §7"
                            )
                        } else {
                            format!(
                                "class {class_id} registers at {share_permille}‰; no end-to-end certified family covers \
                                 the kernels it reaches, so it joins WEIGHTLESS (0‰) and earns cadence once some build \
                                 certifies a backend for it — ADR-0069 Decision 6"
                            )
                        });
                    }
                    // **And WHO is registering it** (launch blockers §3). Everything above is a
                    // fact about the graph and the share; none of it is a fact about the sender.
                    // A registration takes a permille from EVERY incumbent through
                    // largest-remainder donation, and the share's own doc says "whoever may
                    // register a class may fund it, and nobody else may move a permille" — there
                    // was no `whoever`. Any stranger could move the cadence table for a fee.
                    //
                    // The registrant must hold an ACTIVE bond and have signed the class it is
                    // registering together with the share it is taking. Not a permission system:
                    // the smallest answer to "who", denominated in the collateral every other
                    // authority on this chain is denominated in.
                    self.palw_v2_class_registration_is_signed(state, object)?;
                    // **The certified family set, from this build** (ADR-0069 Decision 5). The
                    // gate reads it only for a registration asking a nonzero share, and it refuses
                    // a set that does not hash to `bundle.court_e2e_root` — so a node whose court
                    // can play a different set of families than the network agreed to is stopped
                    // here rather than quietly granting or refusing weight on its own authority.
                    // The set is filled by the node's boot drill; the boot gate is what makes sure
                    // it agrees with the network's pin before a block is ever validated.
                    // **The gate is called with the fences this block runs under** (audit D H-3).
                    //
                    // This called `verify_class_admission_v3` — six arguments, `ladder: None` and
                    // `court: None` — which is the gate as it stood before ADR-0077 Phase B and
                    // ADR-0082 existed. Two consequences, on the ONLY permissionless registration
                    // path there is: every graph-v5 profile was refused
                    // `FusedAttentionNeedsTheKaryCourt` even on a chain whose `palw_kary_court` is
                    // armed (ADR-0082's own U-08 route, closed), and the close-chunk, ladder and
                    // `CourtWindowTooShort` bounds were never evaluated against the armed court at
                    // all. Every one of the three reads a fence, and a fence resolved anywhere but
                    // at the ONE accessor is a rule two nodes can answer differently — so all
                    // three come from the accessors above, at this block's own DAA score.
                    //
                    // `decode_rules` is `false` and not a fence read: ADR-0082 Decision 10's
                    // numerator is not in the transition, and `Params::validate_palw_v2` refuses to
                    // start a node that armed `palw_fp_decode_rules` (audit D M-1), so there is no
                    // configuration in which this may be anything else.
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("class {class_id} is judged under a court with no shape: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: kaspa_consensus_core::palw_class_admission_v2::palw_class_court_window_for_shape_v1(
                                    bundle.state.window_court(),
                                    self.palw_model_court_window_active_at(point.daa_score),
                                    self.palw_held_context_at(point.daa_score),
                                    &derived,
                                    &carriage.profile,
                                )
                                .map_err(|e| format!("class {class_id}: {e}"))?,
                            })
                        }
                    };
                    let ladder = self.palw_context_ladder_at(point.daa_score).then(|| {
                        kaspa_consensus_core::palw_context_ladder::palw_class_ladder_rules_for_court_v1(
                            &carriage.profile,
                            court,
                            bundle.court.max_step_leaf_count(),
                        )
                    });
                    // **RFC-0002 Phase H (`palw_tir_only_v1`): past it a new class is an IR program or an IR pipeline** — a legacy-family
                    // registration (an admission carriage of hand-written kernels) is refused by name, before the graph walk. Dormant
                    // everywhere: no preset arms it.
                    if self.palw_tir_only_at(point.daa_score) {
                        return Err(format!(
                            "class {class_id} is a legacy-family registration and palw_tir_only_v1 admits only IR programs and pipelines (RFC-0002 Phase H)"
                        ));
                    }
                    kaspa_consensus_core::palw_class_admission_v2::verify_class_admission_v9(
                        bundle,
                        &carriage.profile,
                        &carriage.canonical,
                        object,
                        // The same committed set the share rule above read — see its note on why
                        // consensus must not read the drilled registry — and the chain's own.
                        &certified,
                        &chain_certified,
                        ladder.flatten(),
                        court,
                        false,
                        // ADR-0102: the per-token lift kernel is admitted by its fence alone.
                        self.palw_token_lift_at(point.daa_score),
                        // ADR-0093 Decision 6: past its fence, a fused tile must be one head's.
                        self.palw_fused_dissectable_at(point.daa_score),
                        // **F4, on the economic bundle's fence.** Past it the ladder is compared
                        // against the deepest job the class can LEGALLY run rather than against a
                        // count of one decode call — so a class is refused at REGISTRATION instead
                        // of being admitted and then refused, silently, when it runs its own jobs.
                        // Resolved at the block, unlike the claim-borne half of the same fence: a
                        // registration is decided once and never re-derived.
                        self.palw_canonical_work_daa.is_some_and(|height| point.daa_score >= height),
                        // ADR-0103: the held map is admitted by its fence alone, and a held class's
                        // walls are read under the regime — the PanelDa fence says whether its
                        // widest job commits with no ids.
                        kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1 {
                            armed: self.palw_held_context_at(point.daa_score),
                            panel_da: self.palw_panel_da_at(point.daa_score),
                        },
                        self.palw_kimi_k3_at(point.daa_score),
                        // 2026-09-23 audit C-4: past its fence a non-fused class's priced geometry
                        // must fit the query row its graph reads.
                        self.palw_audit_2026_09_23_at(point.daa_score),
                        // P0a: a version-3 profile (the derived GDN key-head count) is admitted by its
                        // fence alone, and past it a V1/V2 hybrid whose head counts differ may not
                        // register a map written over one head count.
                        self.palw_gdn_key_heads_at(point.daa_score),
                    )
                    .map_err(|e| format!("class {class_id} is not admissible: {e}"))?;
                    // ADR-0152 §4-ter C5: past `palw_offence_attribution` a held class is admitted
                    // only where its attention lie is attributable — its dissection answerable
                    // (`palw_held_class_unanswerable_v1`: the context bound, a recurrent layer, a
                    // compute turn past the cap — a pure function of the profile).
                    kaspa_consensus_core::palw_class_admission_v2::palw_held_class_is_attributable_v2(
                        &carriage.profile,
                        self.palw_offence_attribution_at(point.daa_score),
                        self.palw_memory_routing_court_v2_at(point.daa_score),
                    )
                    .map_err(|e| format!("class {class_id} is not admissible: {e}"))?;
                    // **ADR-0152 v3.1 addendum §4-bis.8: every claim of the class must be
                    // attributable.** Past `palw_offence_attribution` a registration is refused
                    // unless its canonical job is the formula's (a class too narrow for it is
                    // refused too, so `IdentityNotDerivable` is no registrant's way out of J5), its
                    // logits row is a provable step output (which refuses Float32), it reaches no
                    // Kimi K3 kernel, and a canonical prompt past J5b's inline bound is committed in
                    // the Merkle form. Processor only: the gate decides, the fold never re-derives a
                    // registration.
                    if self.palw_offence_attribution_at(point.daa_score) {
                        kaspa_consensus_core::palw_attempt_rules_v1::palw_attributable_class_v1(
                            &carriage.profile,
                            &carriage.canonical,
                            false,
                            self.palw_prompt_ids_form_at(point.daa_score),
                        )
                        .map_err(|e| format!("class {class_id} is not attributable past palw_offence_attribution: {e}"))?;
                    }
                }
                Obj::ClassFrozen { .. } => {
                    return Err("a class freeze carries no verified contradiction; the object is refused".to_string());
                }
                Obj::ClassManifestV2 { class_id, artifact_bytes, registrant_bond, signature } => {
                    if !self.palw_model_registry_at(point.daa_score) {
                        return Err(format!("a manifest for class {class_id} below the model registry's fence"));
                    }
                    let record = state
                        .bond(registrant_bond)
                        .ok_or_else(|| format!("a manifest names bond {registrant_bond:?} this chain does not have"))?;
                    let message = kaspa_consensus_core::palw_model_registry_v1::palw_class_manifest_message_v2(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        &borsh::to_vec(registrant_bond).expect("a bond key is borsh-serializable"),
                        class_id,
                        *artifact_bytes,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_model_registry_v1::PALW_CLASS_MANIFEST_V2_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("bond {registrant_bond:?}'s manifest is not signed by the key it registered"));
                    }
                }
                // **ADR-0100 Decision 4: per-shard licensing's three objects.** The fence first, the
                // same doubled-fence doctrine as every court move; then each object's authority:
                // the class's registrant over a plan, the bond over its shard list, and over a part
                // the seats' own receipts, checked exactly as a whole licence checks them, over
                // that shard's seats.
                Obj::ClassShardPlanDeclared { class_id, shard_count, signature } => {
                    if !self.palw_shard_licensing_at(point.daa_score) {
                        return Err(format!("class {class_id}: per-shard licensing is not armed on this network (ADR-0100)"));
                    }
                    let class = state
                        .class(class_id)
                        .ok_or_else(|| format!("a shard plan names class {class_id} this chain does not have"))?;
                    let registrant = class
                        .registrant_bond
                        .ok_or_else(|| format!("class {class_id} was registered at genesis and has no registrant"))?;
                    let record = state.bond(&registrant).ok_or_else(|| format!("class {class_id}'s registrant bond is gone"))?;
                    let message = kaspa_consensus_core::palw_shard_licensing_v1::palw_class_shard_plan_message_v1(
                        kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                            self.network_id_bytes.as_slice(),
                            Some(self.genesis.hash),
                        ),
                        class_id,
                        *shard_count,
                    );
                    if !Self::verify_mldsa87_with_context_bool(
                        &record.pubkey,
                        message.as_byte_slice(),
                        signature,
                        kaspa_consensus_core::palw_shard_licensing_v1::PALW_SHARD_PLAN_MLDSA87_CONTEXT,
                    ) {
                        return Err(format!("class {class_id}'s shard plan is not signed by its registrant"));
                    }
                }
                // **RFC-0003 (tag 68): a generative class registration.** Below `palw_gen_v1` it is
                // dropped by name, exactly as an older build skips the payload it cannot decode (A-2;
                // the walk dropped it first). Past it, the IR arm's order: the chain's target (one map
                // read), the registrant's signature, then the pipeline admission — which decodes the
                // class, counts its step tree and prices its court — and its share rule (0‰: no
                // attempt lane exists for pipelines).
                Obj::ClassRegisteredGenV1 { class_id, .. } => {
                    if !self.palw_gen_at(point.daa_score) {
                        return Err(format!(
                            "generative class {class_id} is refused: palw_gen_v1 is not in force at this block (RFC-0003)"
                        ));
                    }
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("generative class {class_id} registered on a network with no V2 bundle"));
                    };
                    self.palw_v2_class_registration_starts_at_the_chains_target(state, object)?;
                    self.palw_v2_class_registration_is_signed(state, object)?;
                    // The k-ary court at this block, resolved as the IR arm resolves it: a cone that
                    // reduces over the history is dissected only under it (F7 composed).
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("generative class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("generative class {class_id} is judged under a court with no shape: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: bundle.state.window_court(),
                            })
                        }
                    };
                    let rules = kaspa_consensus_core::palw_gen_admission_v1::PalwGenAdmissionRulesV1 {
                        fence: self.palw_gen_v1.expect("palw_gen_at said the fence is in force"),
                        court,
                        held_armed: self.palw_held_context_at(point.daa_score),
                        // RFC-0003 decision 22, read off the bundle's mirror: with the held leaf challenge a close up to
                        // the carried cap is convictable under the held regime (PALW-GEN-21's bound lifts).
                        held_close_chunks: self.palw_held_close_chunks_at(point.daa_score),
                        // The block's box-demand rules, as the IR arm reads them (the fold's mirror).
                        demand: kaspa_consensus_core::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(&bundle.state, point.daa_score),
                        // The close-sizing twin, read off the bundle's mirror (`palw_gen_range_twin_v1`).
                        twin: if bundle.state.gen_range_twin_active_at(point.daa_score) {
                            kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1::Range
                        } else {
                            kaspa_consensus_core::palw_tir_close_range_v1::PalwTirCloseTwinV1::Element
                        },
                        // The task-head profile at the block: tag 6 is a `Head` class only where it is in force.
                        heads: self.palw_task_heads_v1.filter(|fence| fence.activation.is_active(point.daa_score)),
                    };
                    kaspa_consensus_core::palw_gen_admission_v1::verify_gen_class_admission_v1(bundle, &rules, object)
                        .map_err(|e| format!("generative class {class_id} is not admissible: {e}"))?;
                }
                // **RFC-0002 Phase F (tag 61): an IR class registration.** Below `palw_tir_v1` it is
                // refused by name (the walk drops it first, exactly as an older build skips the payload
                // it cannot decode, A-2); above it, a bought registration (a slot and the 1 MSK burn)
                // judged in the legacy arm's order and then by admission v10 — at most one a block
                // reaches here (`PALW_TIR_REGISTRATION_MAX_PER_BLOCK_V1`, the walk's by-name drop).
                Obj::ClassRegisteredTirV1 { class_id, share_permille, admission, .. } => {
                    if !self.palw_tir_at(point.daa_score) {
                        return Err(format!(
                            "IR class {class_id} is refused: palw_tir_v1 is not in force at this block (RFC-0002 Phase F)"
                        ));
                    }
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err(format!("IR class {class_id} registered on a network with no V2 bundle"));
                    };
                    // The legacy arm's order, the graph gate last: the chain's target (one map
                    // read), the registrant's signature, then admission v10 — which decodes the
                    // program and derives the primitives the share rule reads.
                    self.palw_v2_class_registration_starts_at_the_chains_target(state, object)?;
                    self.palw_v2_class_registration_is_signed(state, object)?;
                    // RFC-0002 F7: the k-ary court at this block, resolved as the legacy arm resolves
                    // it — a cone that reduces over the history is dissected only under it.
                    let court = match self.palw_kary_court_active_at(point.daa_score) {
                        false => None,
                        true => {
                            let derived = self
                                .palw_court_params_at(point.daa_score)
                                .ok_or_else(|| format!("IR class {class_id} is registered on a chain with no V2 ruleset"))?
                                .map_err(|e| format!("IR class {class_id} is judged under a court with no shape: {e}"))?;
                            let window = kaspa_consensus_core::palw_class_admission_v2::palw_court_window_for_history_v1(
                                bundle.state.window_court(),
                                self.palw_model_court_window_active_at(point.daa_score),
                                self.palw_held_context_at(point.daa_score),
                                &derived,
                                admission.class.layout.max_context as u64,
                                admission.class.layout.h_tile.max(1),
                            )
                            .map_err(|e| format!("IR class {class_id}: {e}"))?;
                            Some(kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1 {
                                dissection_arity: derived.dissection_arity(),
                                prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                                window_court_daa: window,
                            })
                        }
                    };
                    let rules = kaspa_consensus_core::palw_tir_admission_v1::PalwTirAdmissionRulesV1 {
                        fence: self.palw_tir_v1.expect("palw_tir_at said the fence is in force"),
                        held: kaspa_consensus_core::palw_class_admission_v2::PalwHeldAdmissionV1 {
                            armed: self.palw_held_context_at(point.daa_score),
                            panel_da: self.palw_panel_da_at(point.daa_score),
                        },
                        prompt_ids_form: self.palw_prompt_ids_form_at(point.daa_score),
                        court,
                        demand: kaspa_consensus_core::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(&bundle.state, point.daa_score),
                        model_court_window_active: self.palw_model_court_window_active_at(point.daa_score),
                    };
                    // The network's committed certified families and the chain's own, as the legacy
                    // arm reads them (consensus never reads the drilled registry).
                    let certified = kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1();
                    let chain_certified =
                        state.chain_certified_families(kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::Attempt);
                    let (entry, _) = kaspa_consensus_core::palw_tir_admission_v1::verify_class_admission_v10(
                        bundle,
                        &rules,
                        object,
                        &certified,
                        &chain_certified,
                    )
                    .map_err(|e| format!("IR class {class_id} is not admissible: {e}"))?;
                    // Decision H's and ADR-0069's share rule, as the legacy arm states it: the
                    // minimum grantable share for a class a certified family covers (below
                    // `palw_admission_independence`), nothing otherwise.
                    let prosecutable = kaspa_consensus_core::palw_e2e_adjudicability::family_certified_for_weight_v2(
                        bundle.court_e2e_root,
                        &certified,
                        &chain_certified,
                        &entry.reachable_kernels,
                    )
                    .map_err(|e| format!("IR class {class_id}: {e}"))?
                    .is_some();
                    let independence = self.palw_admission_independence_at(point.daa_score);
                    let required = if prosecutable && !independence { state_params.min_grantable_share_permille() } else { 0 };
                    if *share_permille != required {
                        return Err(format!(
                            "IR class {class_id} registers at {share_permille}‰; the share rule requires {required}‰ (ADR-0049 \
                             Decision H, ADR-0069 Decision 6, ADR-0145 §7)"
                        ));
                    }
                }
                // ADR-0075: both certification objects are judged entirely by the transition —
                // the evidence by the court's grader, the class binding by the class's own profile
                // hash and kernel coverage — and neither needs a signature, a bundle or a store.
                Obj::FamilyCertified { .. } | Obj::ClassLaneCertified { .. } => {}
                // RFC-0002 Phase F (tag 63): judged by the transition like the legacy lane object;
                // below `palw_tir_v1` dropped by name (the acceptance walk drops it first).
                Obj::ClassLaneCertifiedTirV1 { class_id, .. } => {
                    if !self.palw_tir_at(point.daa_score) {
                        return Err(format!("IR class {class_id}'s lane certification is refused: palw_tir_v1 is not in force"));
                    }
                }
                Obj::TirClassLayoutPublishedV1 { layout, .. } => {
                    // A-2: below its owning fence (`palw_staged_tir_route_v1`) the kind rides unjudged.
                    if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score)
                        || !self.palw_staged_public_court_v1.is_some_and(|activation| activation.is_active(point.daa_score))
                    {
                        return Ok(());
                    }
                    // Public class data: the fold authenticates it against the registered record's digest.
                    if layout.version != kaspa_consensus_core::palw_tir_class_v1::PALW_TIR_LAYOUT_VERSION_V1 {
                        return Err("published TIR layout is not its prospective version".into());
                    }
                }
                // **Lane BUDGET (tag 140): a bond's capital assignment** — the fence, an Active signer, its signature; ADR-0177 D3's rules
                // are the fold's.
                // RFC-0002 02-A (tag 175): a TIR class's container binding, signed by the class's original registrant while it is
                // Active (open and unsigned otherwise, audit C-01). A-2: below
                // its owning fence the kind rides unjudged. The opening, the class and immutability are the fold's.
                Obj::TirClassContainerBoundV1 { binding, signature } => {
                    use kaspa_consensus_core::palw_tir_container_v1 as container;
                    if !self.palw_lifecycle_kind_fences.kind_in_force_at(object, point.daa_score) {
                        return Ok(());
                    }
                    // Audit C-01: an Active registrant has priority and must sign; with none (sentinel, or a registrant no longer
                    // Active) the binding is open and carries an empty signature — the fold's identity check is what makes it true.
                    let Some((_, bond)) = container::palw_tir_container_binding_signer_v1(state, binding)? else {
                        if !signature.is_empty() {
                            return Err("an open TIR container binding (no Active registrant) carries no signature".into());
                        }
                        return Ok(());
                    };
                    let message = container::palw_tir_container_binding_message_v1(self.palw_network_domain_v2(), binding);
                    if !Self::verify_mldsa87_with_context_bool(
                        &bond.pubkey,
                        message.as_byte_slice(),
                        signature,
                        container::PALW_TIR_CONTAINER_BINDING_SIGNATURE_CONTEXT_V1,
                    ) {
                        return Err("the original class registrant did not sign this TIR container binding".into());
                    }
                }
                Obj::ClassSemanticsDeclaredV1 { declaration, signature } => {
                    use kaspa_consensus_core::palw_class_semantics_v1 as semantics;
                    if !self.palw_class_semantics_v1.is_some_and(|f| f.is_active(point.daa_score)) {
                        return Err("semantic declaration is below palw_class_semantics_v1".into());
                    }
                    declaration.check().map_err(str::to_string)?;
                    let class = state.class(&declaration.class_id).ok_or("semantic declaration names an unknown class")?;
                    if class.artifact_root != declaration.artifact_root {
                        return Err("semantic declaration names another artifact".into());
                    }
                    let registrant = class
                        .registrant_bond
                        .as_ref()
                        .ok_or("a sentinel registration has no signing registrant for prospective migration")?;
                    let bond = state.bond(registrant).ok_or("the signing registrant is not a public bond")?;
                    if !matches!(bond.status, kaspa_consensus_core::palw_state_v2::PalwBondStatusV2::Active) {
                        return Err("the signing registrant is not Active".into());
                    }
                    let message = semantics::palw_class_semantics_message_v1(self.palw_network_domain_v2(), declaration);
                    if !Self::verify_mldsa87_with_context_bool(
                        &bond.pubkey,
                        message.as_byte_slice(),
                        signature,
                        semantics::PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1,
                    ) {
                        return Err("the original class registrant did not sign this prospective semantic declaration".into());
                    }
                }
                Obj::AdapterClassListed { payload, lister, signature } => {
                    if !self.palw_adapter_class_at(point.daa_score) {
                        return Err("an adapter class listing is refused: palw_adapter_class_v1 is not in force at this block (RFC-0001 §2.10)".to_string());
                    }
                    self.palw_adapter_listing_is_signed(state, payload.as_ref(), lister, signature)?;
                    let Some(bundle) = self.palw_v2_bundle.as_ref() else {
                        return Err("an adapter class listing on a network with no V2 bundle".to_string());
                    };
                    let rules = kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeAdmissionV1 {
                        court: self.palw_kary_court_active_at(point.daa_score),
                        carriable: kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&bundle.court),
                        work_cap: kaspa_consensus_core::palw_tir_close_size_v1::PALW_TIR_CLOSE_SIZING_WORK_CAP_V1,
                    };
                    state
                        .adapter_class_acceptance_v1(payload, rules)
                        .map_err(|why| format!("adapter class {} is not admissible: {why} (RFC-0001 §2.10)", payload.class_id))?;
                }
            _ => unreachable!("class_registration validator received another object family"),
        }
        Ok(())
    }
}

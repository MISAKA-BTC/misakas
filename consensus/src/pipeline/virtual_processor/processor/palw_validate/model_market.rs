//! Object authentication and admission for the existing model_market family.
//! Immutable chain inputs are explicit; ordered state application remains in the fold.
use super::*;

impl VirtualStateProcessor {
    pub(super) fn palw_validate_model_market_object_v1(
        &self,
        state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
        point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
        object: &kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2,
    ) -> Result<(), String> {
        use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2 as Obj;
        match object {
            // ADR-0087 Decision 6: the market's two moves exist only past the fence, refused by
            // name before it (the drop-not-invalidate shape). A sell is signed by the key whose
            // id is the holder (M8), checked here where the verifier lives. The id is not the
            // payee: past the 2026-09-23 fence the fold pays the key's own address payload
            // (the 2026-09-23 Position route matrix, P-B4).
            Obj::ModelBuy { line_id, .. } => {
                if !self.palw_model_market_active_at(point.daa_score) {
                    return Err(format!("a model buy of line {line_id} on a chain where the model market is not in force"));
                }
            }
            // ADR-0090: the seed is the market's third move, under the same fence; its sink
            // binding is checked with the buy's (`palw_model_buy_binds_its_carrier_v1`).
            Obj::ModelSeed { line_id, .. } => {
                if !self.palw_model_market_active_at(point.daa_score) {
                    return Err(format!("a model seed of line {line_id} on a chain where the model market is not in force"));
                }
            }
            // **ADR-0152-adjacent (Activation Pool): a top-up exists only past the pool's fence**,
            // refused by name before it (the drop-not-invalidate shape; its MSK comes back through
            // P-B1 where the carrier is refunded). Its sink binding was checked at extraction
            // (`palw_activation_pool_binds_its_carrier_v1`), and an activation sink nothing binds
            // made the block invalid at isolation; the class, its status and the least top-up are
            // the fold's (`palw_activation_pool_admits_v1`).
            Obj::ActivationPoolFunded { class_id, .. } => {
                if self.palw_activation_pool_at(point.daa_score).is_none() {
                    return Err(format!("a top-up of class {class_id}'s Activation Pool on a chain where the pool is not in force"));
                }
            }
            Obj::ModelSell { line_id, holder, units_in, min_msk_out, held_units, not_after_daa, pubkey, signature } => {
                if !self.palw_model_market_active_at(point.daa_score) {
                    return Err(format!("a model sell of line {line_id} on a chain where the model market is not in force"));
                }
                if kaspa_consensus_core::palw_model_market_v1::palw_model_holder_of_pubkey_v1(pubkey) != *holder {
                    return Err(format!("a model sell of line {line_id} is signed by a key that is not the holder's"));
                }
                // **ADR-0087 M8 (mainnet audit 2026-09-06, M-11): an authority with an end.**
                // Past its own score the sell is refused; and a window longer than
                // `PALW_MODEL_SELL_MAX_WINDOW_DAA_V1` is refused at any score, so "forever" is
                // not expressible. Both compared against the ACCEPTING block's DAA, which is
                // the point the holder's key was asked about.
                match kaspa_consensus_core::palw_model_market_v1::palw_model_sell_window_v1(point.daa_score, *not_after_daa) {
                    kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::Live => {}
                    kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::Expired => {
                        return Err(format!(
                            "a model sell of line {line_id} expired at daa {not_after_daa}; this block is at {}",
                            point.daa_score
                        ));
                    }
                    kaspa_consensus_core::palw_model_market_v1::PalwModelSellWindowV1::TooLong { daa } => {
                        return Err(format!(
                            "a model sell of line {line_id} claims authority for {daa} daa, over the {} the chain allows",
                            kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MAX_WINDOW_DAA_V1
                        ));
                    }
                }
                let message = kaspa_consensus_core::palw_model_market_v1::palw_model_sell_message_v1(
                    // **The network domain every other market/registry message already carries**
                    // (audit M-11). ADR-0088's ten objects all sign under it; this one did not,
                    // so one signature was good on any chain sharing the line id.
                    self.palw_network_domain_v2(),
                    line_id,
                    holder,
                    *units_in,
                    *min_msk_out,
                    *held_units,
                    *not_after_daa,
                );
                if !kaspa_txscript::verify_mldsa87_with_context(
                    pubkey,
                    &message,
                    signature,
                    kaspa_consensus_core::palw_model_market_v1::PALW_MODEL_SELL_MLDSA87_CONTEXT,
                )
                .unwrap_or(false)
                {
                    return Err(format!("a model sell of line {line_id} carries a signature the holder's key does not verify"));
                }
            }
            // ADR-0088 Decision 11: the registry's ten objects exist only past the fence, refused
            // by name before it. Each is attributed to a bond the fold names — the founder, the
            // line's developer, its owner, the proposer, the evaluator — and its signature is
            // checked HERE, against that bond's stored key, over the message the object's fields
            // spell; the fold then enforces referential integrity and the bounds.
            Obj::ModelLineFounded { class_id, name, founder, root, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a line founding on class {class_id} on a chain where the model registry is not in force"));
                }
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_line_founded_message_v1(
                    self.palw_network_domain_v2(),
                    class_id,
                    name,
                    founder,
                    root,
                );
                self.palw_model_check_bond_signature(state, founder, &message, signature, "a line founding")?;
            }
            Obj::ModelVersionPublished {
                line_id,
                version,
                root,
                parent,
                adopted_from,
                runtime_hash,
                dataset_commitment,
                training_config_hash,
                notes_hash,
                preview,
                signature,
            } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a version of line {line_id} on a chain where the model registry is not in force"));
                }
                let developer = self.palw_model_line_role(state, line_id, "developer")?;
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    *version,
                    root,
                    *parent,
                    adopted_from.as_ref(),
                    runtime_hash.as_ref(),
                    dataset_commitment.as_ref(),
                    training_config_hash.as_ref(),
                    notes_hash.as_ref(),
                    *preview,
                );
                self.palw_model_check_bond_signature(state, &developer, &message, signature, "a version")?;
            }
            Obj::ModelVersionPromoted { line_id, version, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a promotion on line {line_id} on a chain where the model registry is not in force"));
                }
                let developer = self.palw_model_line_role(state, line_id, "developer")?;
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_move_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    *version,
                    b"promote",
                );
                self.palw_model_check_bond_signature(state, &developer, &message, signature, "a promotion")?;
            }
            Obj::ModelVersionWithdrawn { line_id, version, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a withdrawal on line {line_id} on a chain where the model registry is not in force"));
                }
                let developer = self.palw_model_line_role(state, line_id, "developer")?;
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_version_move_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    *version,
                    b"withdraw",
                );
                self.palw_model_check_bond_signature(state, &developer, &message, signature, "a withdrawal")?;
            }
            Obj::ModelLineBenefitsDeclared { line_id, tiers, cadence_daa, expires_daa, signature } => {
                // ADR-0095 §4.11 as corrected: its own fence, checked here as well as in the
                // fold, so an object that cannot apply never rides in the first place.
                if !self.palw_model_benefits_active_at(point.daa_score) {
                    return Err(format!("a benefits declaration for line {line_id} on a chain where the membership is not in force"));
                }
                // §4.1: the OWNER signs what the line promises, not the developer who ships it.
                let owner = self.palw_model_line_role(state, line_id, "owner")?;
                let message = kaspa_consensus_core::palw_model_benefits_v1::palw_model_benefits_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    tiers,
                    *cadence_daa,
                    *expires_daa,
                );
                self.palw_model_check_bond_signature(state, &owner, &message, signature, "a benefits declaration")?;
            }
            Obj::ModelLineRolesSet { line_id, developer, maintainer, contributor_permille_of_leg, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a roles change on line {line_id} on a chain where the model registry is not in force"));
                }
                let owner = self.palw_model_line_role(state, line_id, "owner")?;
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_roles_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    developer.as_ref(),
                    maintainer.as_ref(),
                    *contributor_permille_of_leg,
                );
                self.palw_model_check_bond_signature(state, &owner, &message, signature, "a roles change")?;
            }
            Obj::ModelLineOwnerTransferred { line_id, new_owner, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a transfer of line {line_id} on a chain where the model registry is not in force"));
                }
                let owner = self.palw_model_line_role(state, line_id, "owner")?;
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_transfer_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    new_owner,
                );
                self.palw_model_check_bond_signature(state, &owner, &message, signature, "a transfer")?;
            }
            Obj::ModelLineRetired { line_id, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a retirement of line {line_id} on a chain where the model registry is not in force"));
                }
                let owner = self.palw_model_line_role(state, line_id, "owner")?;
                let message =
                    kaspa_consensus_core::palw_model_lines_v1::palw_model_retire_message_v1(self.palw_network_domain_v2(), line_id);
                self.palw_model_check_bond_signature(state, &owner, &message, signature, "a retirement")?;
            }
            Obj::ModelProposalPosted { line_id, root, note_hash, by, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a proposal on line {line_id} on a chain where the model registry is not in force"));
                }
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_proposal_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    root,
                    note_hash,
                    by,
                );
                self.palw_model_check_bond_signature(state, by, &message, signature, "a proposal")?;
            }
            Obj::ModelProposalClosed { line_id, proposal_id, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("a proposal close on line {line_id} on a chain where the model registry is not in force"));
                }
                let developer = self.palw_model_line_role(state, line_id, "developer")?;
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_proposal_close_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    proposal_id,
                );
                self.palw_model_check_bond_signature(state, &developer, &message, signature, "a proposal close")?;
            }
            Obj::ModelEvaluationPosted { line_id, version, evaluator_id, score_permille, report_hash, by, signature } => {
                if !self.palw_model_lines_active_at(point.daa_score) {
                    return Err(format!("an evaluation on line {line_id} on a chain where the model registry is not in force"));
                }
                let message = kaspa_consensus_core::palw_model_lines_v1::palw_model_evaluation_message_v1(
                    self.palw_network_domain_v2(),
                    line_id,
                    *version,
                    evaluator_id,
                    *score_permille,
                    report_hash,
                    by,
                );
                self.palw_model_check_bond_signature(state, by, &message, signature, "an evaluation")?;
            }
            _ => unreachable!("model_market validator received another object family"),
        }
        Ok(())
    }
}

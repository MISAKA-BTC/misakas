//! **A testnet-12 drill node: `--palw-drill-genesis-salt` and what it refuses** (ADR-0152 §8.2 and
//! §8.3 item 3, `phase2-plan.md` §1.8 and P2-12, T53).
//!
//! The salt, the drill genesis and the drill keyring are consensus-core's
//! (`kaspa_consensus_core::config::drill`); this module is the node half: which flags a salted node
//! must and must not carry, the datadir marker that keeps a drill and a public node out of each
//! other's app directory, and the keyring export the drill script reads
//! (`scripts/misaka-palw-t12-rcore-drill.sh`).
//!
//! **Everything here is refused at start-up, before a peer is dialed or a block is signed**, because
//! every failure it prevents is one a running drill cannot undo:
//!
//! * **testnet-12 only.** A salt drills the network whose shipping rules it keeps; there is no drill
//!   genesis for mainnet, testnet-11, testnet-10, devnet or simnet.
//! * **No discovery.** `--nodnsseed` and an explicit `--connect`/`--addpeer` list are required, and
//!   the drill params carry no DNS seeders, so a drill never asks public testnet-12's seeders for
//!   peers (the handshake would refuse them on the genesis anyway; the requirement keeps the drill
//!   from advertising itself to them).
//! * **The shipping rules, unedited.** `--override-params-file` is refused: the salt swaps the params
//!   for the drill's, and an override would either be discarded silently or drill rules nobody ships.
//!   **The one narrow exception is the post-launch release's flag day**: `--palw-drill-fence-at=<DAA>`
//!   arms (or moves) exactly the fences of `PALW_T12_POST_LAUNCH_FENCES_V1` — the ones the release
//!   arms at DAA 500 — to a low height on the drill chain, so a drill crosses that flag day with the
//!   shipping binary; nothing else moves, and it needs the salt
//!   (`config::drill::palw_drill_post_launch_fences_at_v1`).
//! * **Drill-only keys.** The producer key must be a bond key of THIS drill's keyring, the validator
//!   key a validator key of it, and the pay and heartbeat addresses must be addresses of it. A card
//!   key signing on a drill chain is the replay ADR-0152 §8.2 forbids, and a public miner script on
//!   a drill coinbase mints a public outpoint. `--palw-producer-bond`, `--palw-fee-outpoint` and
//!   `--stake-bond` naming public testnet-12's genesis txids are refused by name: they are a public
//!   unit's flags copied onto a drill. The same rule reaches past start-up: `getBlockTemplate` pays
//!   only a drill address (`rpc/service`), and the EVM ingress admits only the drill's EVM accounts
//!   (`FlowContext::submit_rpc_evm_transaction`).
//! * **The EVM lane is not separated by the salt.** An EVM transaction is bound to `EVM_CHAIN_ID` —
//!   one constant on every network — and to its sender's nonce, never to the genesis, so an account
//!   that holds value on public testnet-12 signs, on a drill, a transaction valid there too. A drill
//!   therefore uses the keyring's EVM accounts only: `--evm-fee-recipient` must be one, and the
//!   keyring export lists them (`evm`). A per-genesis chain id would close it structurally; that is a
//!   consensus change outside the drill.
//! * **Its own app directory.** A drill writes a marker into `<appdir>/<network>/`; a salted node
//!   refuses a directory that holds another node's data without its marker, and an unsalted node
//!   refuses one with a marker. Without it a drill pointed at a public node's app dir meets
//!   "Genesis not found in active consensus DB … delete?" — and with `--yes` deletes that node's
//!   database — and shares its panel state (`palw-panel/palw-fee-outpoint`) besides. The marker also
//!   names the flag day the drill chain was started with (`fence_at=`, `--palw-drill-fence-at`): a
//!   stored drill chain is never reopened under another one, so a chain that passed the height
//!   unfenced is never reported as having crossed it. The second post-launch flag day (lane F2,
//!   `--palw-drill-fence2-at`: every fence of `PALW_T12_POST_LAUNCH_FENCES_V2`, applied after the
//!   first) is recorded beside it (`fence2_at=`) and held to the same rule.
use crate::args::Args;
use kaspa_addresses::{Address, Prefix, Version};
use kaspa_consensus_core::config::Config;
use kaspa_consensus_core::config::drill::{
    PALW_DRILL_KEYRING_SPAN_V1, PalwDrillKeyRoleV1, PalwDrillKeyringV1, PalwDrillSaltV1, palw_drill_network_v1,
    palw_t12_drill_cards_v1, palw_t12_drill_fee_float_outpoint_v1, palw_t12_drill_main_key_v1, palw_t12_drill_premine_outpoint_v1,
    palw_t12_public_genesis_txid_v1,
};
use kaspa_consensus_core::errors::config::{ConfigError, ConfigResult};
use kaspa_consensus_core::network::NetworkType;
use std::path::{Path, PathBuf};

fn refused(why: impl Into<String>) -> ConfigError {
    ConfigError::PalwDrillRefused(why.into())
}

/// **The six fences beyond the IR fence a drill may arm**: testnet-12's second IR fence (the DAA-3,600 flag
/// day), the per-model court window (`palw_model_court_window`, dormant on the release) and RFC-0003 /
/// RFC-0004's four — the generative fence, the decode rules, FP Job V5 and the improvement fence — each its own
/// flag (`--palw-drill-tir2-at`, `--palw-drill-model-court-at`, `--palw-drill-gen-at`,
/// `--palw-drill-decode-rules-at`, `--palw-drill-fp-v5-at`, `--palw-drill-improve-at`), armed in that order after
/// the IR fence so the prerequisites of FP Job V5 and of the improvement fence (the generative fence, the
/// decode rules) stand before them. The one list the node's config, the start-up validation, the keyring export
/// and the datadir marker all read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwDrillExtraFencesV1 {
    pub tir2_at: Option<u64>,
    pub model_court_at: Option<u64>,
    /// The capacity ramp's ready steps (ADR-0160 stage 3): ρ = 25 as F-L's second step, ρ = 100 as its third, and
    /// ρ = 100 straight after ρ = 10 (the second step again — never with the other two).
    pub capacity_network_room_at: Option<u64>,
    pub capacity_network_verify_at: Option<u64>,
    pub capacity_step2_at: Option<u64>,
    pub capacity_step3_at: Option<u64>,
    pub capacity_rho100_at: Option<u64>,
    /// ADR-0164's ready entries: ρ = 250 and ρ = 1000 (F-L's fourth and fifth steps), F-EM, F-M1 and F-K.
    pub capacity_step4_at: Option<u64>,
    pub capacity_step5_at: Option<u64>,
    pub capacity_emission_at: Option<u64>,
    pub capacity_multi_claim_at: Option<u64>,
    pub capacity_rho_breaker_at: Option<u64>,
    pub gen_at: Option<u64>,
    pub decode_rules_at: Option<u64>,
    pub fp_v5_at: Option<u64>,
    pub fp_constraint_at: Option<u64>,
    pub fp_prefix_at: Option<u64>,
    pub fp_tokenizer_at: Option<u64>,
    pub fp_constraint2_at: Option<u64>,
    pub adapter_at: Option<u64>,
    /// RFC-0003's held leaf challenge (decision 22; `palw_held_close_chunks_v1`, object tag 90).
    pub held_chunks_at: Option<u64>,
    /// RFC-0007's verification vertex (`palw_verification_vertex_v1`, object tags 100 and 101).
    pub vertex_at: Option<u64>,
    /// RFC-0007 Part II's witness manifest (`palw_witness_manifest_v1`).
    pub witness_at: Option<u64>,
    /// RFC-0007 Part IV.1's audit mesh (`palw_audit_mesh_v1`, object tags 102 and 103).
    pub audit_mesh_at: Option<u64>,
    /// RFC-0007 Part IV.2's capped onboarding (`palw_capped_onboarding_v1`).
    pub capped_at: Option<u64>,
    /// Lane PL's panel-liveness list (ADR-0166; `palw_panel_unavailable_expiry`, `palw_panel_standby`, `palw_seat_availability`).
    pub panel_liveness_at: Option<u64>,
    pub improve_at: Option<u64>,
    /// RFC-0006's layer-sharded panels (`--palw-drill-tir-shard-at`, `palw_tir_shard_v1`): armed after the fences it names.
    pub tir_shard_at: Option<u64>,
    /// The int-11 flag day as the release arms it (`--palw-drill-int11-at`): the whole list at H', ρ = 100 at H' + 95 — instead of the
    /// per-fence flags of its entries, never with them.
    pub int11_at: Option<u64>,
    /// testnet-12's fourth post-launch flag day (P0a, the GDN key-head count; `palw_gdn_key_heads`), `--palw-drill-fence4-at`: its own
    /// list, no prerequisite among the other fences.
    pub fence4_at: Option<u64>,
    /// RFC-0002 Part II Proposal A's seating fence (`palw_class_seating`), `--palw-drill-class-seating-at`: over the generative fence.
    pub class_seating_at: Option<u64>,
}

impl PalwDrillExtraFencesV1 {
    /// The flags on this command line.
    pub fn of(args: &Args) -> Self {
        Self {
            tir2_at: args.palw_drill_tir2_at,
            model_court_at: args.palw_drill_model_court_at,
            capacity_network_room_at: args.palw_drill_capacity_network_room_at,
            capacity_network_verify_at: args.palw_drill_capacity_network_verify_at,
            capacity_step2_at: args.palw_drill_capacity_step2_at,
            capacity_step3_at: args.palw_drill_capacity_step3_at,
            capacity_rho100_at: args.palw_drill_capacity_rho100_at,
            capacity_step4_at: args.palw_drill_capacity_step4_at,
            capacity_step5_at: args.palw_drill_capacity_step5_at,
            capacity_emission_at: args.palw_drill_capacity_emission_at,
            capacity_multi_claim_at: args.palw_drill_capacity_multi_claim_at,
            capacity_rho_breaker_at: args.palw_drill_capacity_rho_breaker_at,
            gen_at: args.palw_drill_gen_at,
            decode_rules_at: args.palw_drill_decode_rules_at,
            fp_v5_at: args.palw_drill_fp_v5_at,
            fp_constraint_at: args.palw_drill_fp_constraint_at,
            fp_prefix_at: args.palw_drill_fp_prefix_at,
            fp_tokenizer_at: args.palw_drill_fp_tokenizer_at,
            fp_constraint2_at: args.palw_drill_fp_constraint2_at,
            adapter_at: args.palw_drill_adapter_at,
            held_chunks_at: args.palw_drill_held_chunks_at,
            vertex_at: args.palw_drill_vertex_at,
            witness_at: args.palw_drill_witness_at,
            audit_mesh_at: args.palw_drill_audit_mesh_at,
            capped_at: args.palw_drill_capped_at,
            panel_liveness_at: args.palw_drill_panel_liveness_at,
            improve_at: args.palw_drill_improve_at,
            tir_shard_at: args.palw_drill_tir_shard_at,
            int11_at: args.palw_drill_int11_at,
            fence4_at: args.palw_drill_fence4_at,
            class_seating_at: args.palw_drill_class_seating_at,
        }
    }

    /// Does any of them stand?
    pub fn any(&self) -> bool {
        self.tir2_at.is_some()
            || self.model_court_at.is_some()
            || self.capacity_any()
            || self.gen_at.is_some()
            || self.decode_rules_at.is_some()
            || self.fp_v5_at.is_some()
            || self.fp_constraint_at.is_some()
            || self.fp_prefix_at.is_some()
            || self.fp_tokenizer_at.is_some()
            || self.fp_constraint2_at.is_some()
            || self.adapter_at.is_some()
            || self.held_chunks_at.is_some()
            || self.vertex_at.is_some()
            || self.witness_at.is_some()
            || self.audit_mesh_at.is_some()
            || self.capped_at.is_some()
            || self.panel_liveness_at.is_some()
            || self.improve_at.is_some()
            || self.tir_shard_at.is_some()
            || self.int11_at.is_some()
            || self.fence4_at.is_some()
            || self.class_seating_at.is_some()
    }

    /// Does any capacity step flag stand?
    fn capacity_any(&self) -> bool {
        self.capacity_network_room_at.is_some()
            || self.capacity_network_verify_at.is_some()
            || self.capacity_step2_at.is_some()
            || self.capacity_step3_at.is_some()
            || self.capacity_rho100_at.is_some()
            || self.capacity2_any()
    }

    /// Does any of ADR-0164's five flags stand?
    fn capacity2_any(&self) -> bool {
        self.capacity_step4_at.is_some()
            || self.capacity_step5_at.is_some()
            || self.capacity_emission_at.is_some()
            || self.capacity_multi_claim_at.is_some()
            || self.capacity_rho_breaker_at.is_some()
    }

    /// The first flag that stands, named (for the unsalted refusal).
    fn first_named(&self) -> Option<(&'static str, u64, &'static str)> {
        [
            ("--palw-drill-fence4-at", self.fence4_at, "testnet-12's fourth post-launch flag day (palw_gdn_key_heads, P0a)"),
            ("--palw-drill-class-seating-at", self.class_seating_at, "RFC-0002 Part II's class-seating fence (palw_class_seating)"),
            ("--palw-drill-tir2-at", self.tir2_at, "testnet-12's DAA-3,600 flag day (palw_tir_fence2)"),
            ("--palw-drill-model-court-at", self.model_court_at, "the per-model court-window fence (palw_model_court_window)"),
            (
                "--palw-drill-capacity-network-room-at",
                self.capacity_network_room_at,
                "the capacity network room and fair share (palw_capacity_network_room, F-N)",
            ),
            (
                "--palw-drill-capacity-network-verify-at",
                self.capacity_network_verify_at,
                "F-N's static verification term (palw_capacity_network_verify)",
            ),
            ("--palw-drill-capacity-step2-at", self.capacity_step2_at, "the capacity ramp's rho = 25 step (F-L's second step)"),
            (
                "--palw-drill-capacity-step3-at",
                self.capacity_step3_at,
                "the capacity ramp's rho = 100 step after rho = 25 (F-L's third step)",
            ),
            ("--palw-drill-capacity-rho100-at", self.capacity_rho100_at, "the capacity ramp's rho = 100 step straight after rho = 10"),
            ("--palw-drill-capacity-step4-at", self.capacity_step4_at, "the capacity ramp's rho = 250 step (F-L's fourth step)"),
            ("--palw-drill-capacity-step5-at", self.capacity_step5_at, "the capacity ramp's rho = 1000 step (F-L's fifth step)"),
            ("--palw-drill-capacity-emission-at", self.capacity_emission_at, "the per-DAA PALW reward budget (palw_capacity_emission_budget, F-EM)"),
            ("--palw-drill-capacity-multi-claim-at", self.capacity_multi_claim_at, "riders (palw_capacity_multi_claim, F-M1)"),
            ("--palw-drill-capacity-rho-breaker-at", self.capacity_rho_breaker_at, "the lower-only rho breaker (palw_capacity_rho_breaker, F-K)"),
            ("--palw-drill-gen-at", self.gen_at, "RFC-0003's generative fence (palw_gen_v1)"),
            ("--palw-drill-decode-rules-at", self.decode_rules_at, "the decode rules (palw_fp_decode_rules)"),
            ("--palw-drill-fp-v5-at", self.fp_v5_at, "FP Job V5 (palw_fp_job_v5)"),
            ("--palw-drill-fp-constraint-at", self.fp_constraint_at, "the decode constraint (palw_fp_decode_constraint, FP job version 6)"),
            ("--palw-drill-fp-prefix-at", self.fp_prefix_at, "the prefix-state receipt (palw_fp_prefix_state)"),
            ("--palw-drill-fp-tokenizer-at", self.fp_tokenizer_at, "the tokenizer-match rule (palw_fp_tokenizer_match)"),
            ("--palw-drill-fp-constraint2-at", self.fp_constraint2_at, "the second constraint subset (palw_fp_constraint_v2)"),
            ("--palw-drill-adapter-at", self.adapter_at, "the adapter class listing (palw_adapter_class_v1)"),
            ("--palw-drill-held-chunks-at", self.held_chunks_at, "RFC-0003's held leaf challenge (palw_held_close_chunks_v1)"),
            ("--palw-drill-vertex-at", self.vertex_at, "RFC-0007's verification vertex (palw_verification_vertex_v1)"),
            ("--palw-drill-witness-at", self.witness_at, "RFC-0007's witness manifest (palw_witness_manifest_v1)"),
            ("--palw-drill-audit-mesh-at", self.audit_mesh_at, "RFC-0007's audit mesh (palw_audit_mesh_v1)"),
            ("--palw-drill-capped-at", self.capped_at, "RFC-0007's capped onboarding (palw_capped_onboarding_v1)"),
            ("--palw-drill-panel-liveness-at", self.panel_liveness_at, "lane PL's panel-liveness list (ADR-0166)"),
            ("--palw-drill-improve-at", self.improve_at, "RFC-0004's improvement fence (palw_improvement_v1)"),
            ("--palw-drill-tir-shard-at", self.tir_shard_at, "RFC-0006's layer-sharded panels (palw_tir_shard_v1)"),
            ("--palw-drill-int11-at", self.int11_at, "the int-11 flag day's whole list (RFC-0003, RFC-0004, the capacity ramp to rho = 25 / 100)"),
        ]
        .into_iter()
        .find_map(|(flag, at, what)| at.map(|at| (flag, at, what)))
    }

    /// **Arm them on `params`**, in order, each refusal naming its flag; the moves, for the node to print.
    pub fn apply(&self, params: &mut kaspa_consensus_core::config::params::Params) -> Result<Vec<kaspa_consensus_core::config::drill::PalwDrillFenceMoveV1>, String> {
        use kaspa_consensus_core::config::drill as d;
        // Steps 2 and 3 of the ramp, and rho = 100 straight after rho = 10, are the same slots: one or the other.
        if self.capacity_rho100_at.is_some() && (self.capacity_step2_at.is_some() || self.capacity_step3_at.is_some()) {
            return Err(
                "--palw-drill-capacity-rho100-at is the ramp's second step with rho = 100, and --palw-drill-capacity-step2-at / -step3-at \
                 are rho = 25 then rho = 100: pick one ramp"
                    .to_owned(),
            );
        }
        // The whole int-11 list moves its entries together; a per-fence flag of any of them would move one a second time.
        if self.int11_at.is_some() {
            let per_fence = [
                ("--palw-drill-gen-at", self.gen_at),
                ("--palw-drill-decode-rules-at", self.decode_rules_at),
                ("--palw-drill-fp-v5-at", self.fp_v5_at),
                ("--palw-drill-fp-constraint-at", self.fp_constraint_at),
                ("--palw-drill-fp-prefix-at", self.fp_prefix_at),
                ("--palw-drill-fp-tokenizer-at", self.fp_tokenizer_at),
                ("--palw-drill-fp-constraint2-at", self.fp_constraint2_at),
                ("--palw-drill-adapter-at", self.adapter_at),
                ("--palw-drill-held-chunks-at", self.held_chunks_at),
                ("--palw-drill-improve-at", self.improve_at),
                ("--palw-drill-capacity-network-verify-at", self.capacity_network_verify_at),
                ("--palw-drill-capacity-step2-at", self.capacity_step2_at),
                ("--palw-drill-capacity-step3-at", self.capacity_step3_at),
                ("--palw-drill-capacity-rho100-at", self.capacity_rho100_at),
                ("--palw-drill-capacity-step4-at", self.capacity_step4_at),
                ("--palw-drill-capacity-step5-at", self.capacity_step5_at),
                ("--palw-drill-capacity-emission-at", self.capacity_emission_at),
                ("--palw-drill-capacity-multi-claim-at", self.capacity_multi_claim_at),
                ("--palw-drill-capacity-rho-breaker-at", self.capacity_rho_breaker_at),
            ];
            if let Some((flag, _)) = per_fence.into_iter().find(|(_, at)| at.is_some()) {
                return Err(format!(
                    "--palw-drill-int11-at moves the int-11 flag day's whole list (decode rules, gen, FP Job V5, the held leaf challenge, \
                     improvement, F-N's verification term, F-EM, F-M1, F-K, rho = 25 at H', rho = 100 at H' + 95, rho = 250 at H' + 190 and rho = 1000 at H' + 285), and {flag} moves one of its entries: \
                     pick the combined crossing or the per-fence flags"
                ));
            }
        }
        let mut moves = Vec::new();
        if let Some(at) = self.fence4_at {
            moves.extend(d::palw_drill_post_launch_fences_v4_at_v1(params, at).map_err(|e| format!("--palw-drill-fence4-at: {e}"))?);
        }
        if let Some(at) = self.tir2_at {
            moves.extend(d::palw_drill_tir_fence2_at_v1(params, at).map_err(|e| format!("--palw-drill-tir2-at: {e}"))?);
        }
        if let Some(at) = self.model_court_at {
            moves.extend(d::palw_drill_model_court_window_at_v1(params, at).map_err(|e| format!("--palw-drill-model-court-at: {e}"))?);
        }
        // After the IR fences it needs below it, before the per-fence flags it excludes.
        if let Some(at) = self.int11_at {
            moves.extend(d::palw_drill_int11_at_v1(params, at).map_err(|e| format!("--palw-drill-int11-at: {e}"))?);
        }
        if let Some(at) = self.capacity_network_room_at {
            moves.extend(
                d::palw_drill_capacity_network_room_at_v1(params, at)
                    .map_err(|e| format!("--palw-drill-capacity-network-room-at: {e}"))?,
            );
        }
        if let Some(at) = self.capacity_network_verify_at {
            moves.extend(
                d::palw_drill_capacity_network_verify_at_v1(params, at)
                    .map_err(|e| format!("--palw-drill-capacity-network-verify-at: {e}"))?,
            );
        }
        if let Some(at) = self.capacity_step2_at {
            moves.extend(d::palw_drill_capacity_step2_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-step2-at: {e}"))?);
        }
        if let Some(at) = self.capacity_rho100_at {
            moves
                .extend(d::palw_drill_capacity_rho100_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-rho100-at: {e}"))?);
        }
        if let Some(at) = self.capacity_step3_at {
            moves.extend(d::palw_drill_capacity_step3_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-step3-at: {e}"))?);
        }
        // ADR-0164's entries: the budget, the riders and the breaker before the steps that need them, then ρ = 250 and ρ = 1000.
        if let Some(at) = self.capacity_emission_at {
            moves.extend(d::palw_drill_capacity_emission_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-emission-at: {e}"))?);
        }
        if let Some(at) = self.capacity_multi_claim_at {
            moves.extend(
                d::palw_drill_capacity_multi_claim_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-multi-claim-at: {e}"))?,
            );
        }
        if let Some(at) = self.capacity_rho_breaker_at {
            moves.extend(
                d::palw_drill_capacity_rho_breaker_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-rho-breaker-at: {e}"))?,
            );
        }
        if let Some(at) = self.capacity_step4_at {
            moves.extend(d::palw_drill_capacity_step4_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-step4-at: {e}"))?);
        }
        if let Some(at) = self.capacity_step5_at {
            moves.extend(d::palw_drill_capacity_step5_at_v1(params, at).map_err(|e| format!("--palw-drill-capacity-step5-at: {e}"))?);
        }
        if let Some(at) = self.gen_at {
            moves.extend(d::palw_drill_gen_fence_at_v1(params, at).map_err(|e| format!("--palw-drill-gen-at: {e}"))?);
        }
        if let Some(at) = self.decode_rules_at {
            moves.extend(d::palw_drill_decode_rules_at_v1(params, at).map_err(|e| format!("--palw-drill-decode-rules-at: {e}"))?);
        }
        if let Some(at) = self.fp_v5_at {
            moves.extend(d::palw_drill_fp_v5_at_v1(params, at).map_err(|e| format!("--palw-drill-fp-v5-at: {e}"))?);
        }
        if let Some(at) = self.fp_constraint_at {
            moves.extend(d::palw_drill_fp_constraint_at_v1(params, at).map_err(|e| format!("--palw-drill-fp-constraint-at: {e}"))?);
        }
        if let Some(at) = self.fp_prefix_at {
            moves.extend(d::palw_drill_fp_prefix_at_v1(params, at).map_err(|e| format!("--palw-drill-fp-prefix-at: {e}"))?);
        }
        if let Some(at) = self.fp_tokenizer_at {
            moves.extend(d::palw_drill_fp_tokenizer_at_v1(params, at).map_err(|e| format!("--palw-drill-fp-tokenizer-at: {e}"))?);
        }
        if let Some(at) = self.fp_constraint2_at {
            moves.extend(d::palw_drill_fp_constraint2_at_v1(params, at).map_err(|e| format!("--palw-drill-fp-constraint2-at: {e}"))?);
        }
        if let Some(at) = self.adapter_at {
            moves.extend(d::palw_drill_adapter_at_v1(params, at).map_err(|e| format!("--palw-drill-adapter-at: {e}"))?);
        }
        if let Some(at) = self.held_chunks_at {
            moves.extend(d::palw_drill_held_close_chunks_at_v1(params, at).map_err(|e| format!("--palw-drill-held-chunks-at: {e}"))?);
        }
        if let Some(at) = self.vertex_at {
            moves.extend(d::palw_drill_vertex_at_v1(params, at).map_err(|e| format!("--palw-drill-vertex-at: {e}"))?);
        }
        if let Some(at) = self.witness_at {
            moves.extend(d::palw_drill_witness_at_v1(params, at).map_err(|e| format!("--palw-drill-witness-at: {e}"))?);
        }
        if let Some(at) = self.audit_mesh_at {
            moves.extend(d::palw_drill_audit_mesh_at_v1(params, at).map_err(|e| format!("--palw-drill-audit-mesh-at: {e}"))?);
        }
        if let Some(at) = self.capped_at {
            moves.extend(d::palw_drill_capped_at_v1(params, at).map_err(|e| format!("--palw-drill-capped-at: {e}"))?);
        }
        if let Some(at) = self.panel_liveness_at {
            moves.extend(d::palw_drill_panel_liveness_at_v1(params, at).map_err(|e| format!("--palw-drill-panel-liveness-at: {e}"))?);
        }
        if let Some(at) = self.improve_at {
            moves.extend(d::palw_drill_improve_fence_at_v1(params, at).map_err(|e| format!("--palw-drill-improve-at: {e}"))?);
        }
        if let Some(at) = self.tir_shard_at {
            moves.extend(d::palw_drill_tir_shard_at_v1(params, at).map_err(|e| format!("--palw-drill-tir-shard-at: {e}"))?);
        }
        if let Some(at) = self.class_seating_at {
            moves.extend(d::palw_drill_class_seating_at_v1(params, at).map_err(|e| format!("--palw-drill-class-seating-at: {e}"))?);
        }
        Ok(moves)
    }

    /// The marker's `tir2_at=` line — the release line's own (`none` without the flag, also what a marker
    /// written before it counts as): a chain that line's binary started reopens under this build.
    fn tir2_marker_text(&self) -> String {
        palw_drill_marker_fence_text_v1(self.tir2_at)
    }

    /// The marker's `model_court_at=` line — the release line's own, as above.
    fn model_court_marker_text(&self) -> String {
        palw_drill_marker_fence_text_v1(self.model_court_at)
    }

    /// The marker's `capacity_at=` line — the ramp's steps (`none` when none stands, else
    /// `room:… verify:… step2:… step3:… rho100:…`), its own line so the lines of the flags before it keep their text.
    fn capacity_marker_text(&self) -> String {
        if !self.capacity_any() {
            return palw_drill_marker_fence_text_v1(None);
        }
        let at = |v: Option<u64>| palw_drill_marker_fence_text_v1(v);
        format!(
            "room:{},verify:{},step2:{},step3:{},rho100:{}",
            at(self.capacity_network_room_at),
            at(self.capacity_network_verify_at),
            at(self.capacity_step2_at),
            at(self.capacity_step3_at),
            at(self.capacity_rho100_at)
        )
    }

    /// The marker's `capacity2_at=` line — ADR-0164's five entries (`none` when none stands, else
    /// `step4:… step5:… emission:… multi:… breaker:…`), its own line so no earlier line changes its text.
    fn capacity2_marker_text(&self) -> String {
        if !self.capacity2_any() {
            return palw_drill_marker_fence_text_v1(None);
        }
        let at = |v: Option<u64>| palw_drill_marker_fence_text_v1(v);
        format!(
            "step4:{},step5:{},emission:{},multi:{},breaker:{}",
            at(self.capacity_step4_at),
            at(self.capacity_step5_at),
            at(self.capacity_emission_at),
            at(self.capacity_multi_claim_at),
            at(self.capacity_rho_breaker_at)
        )
    }

    /// **The marker's lines for the flags that came after `extra_at=`** (int-11): one line each — `(key=, flag, text)` —
    /// `none` when absent, also what a marker written before the flag existed counts as, so no earlier line changes its text
    /// and a chain a release line's binary started reopens under this build.
    fn later_marker_lines(&self) -> Vec<(&'static str, &'static str, String)> {
        vec![
            ("held_chunks_at=", "--palw-drill-held-chunks-at", palw_drill_marker_fence_text_v1(self.held_chunks_at)),
            ("int11_at=", "--palw-drill-int11-at", palw_drill_marker_fence_text_v1(self.int11_at)),
            ("vertex_at=", "--palw-drill-vertex-at", palw_drill_marker_fence_text_v1(self.vertex_at)),
            ("witness_at=", "--palw-drill-witness-at", palw_drill_marker_fence_text_v1(self.witness_at)),
            ("audit_mesh_at=", "--palw-drill-audit-mesh-at", palw_drill_marker_fence_text_v1(self.audit_mesh_at)),
            ("capped_at=", "--palw-drill-capped-at", palw_drill_marker_fence_text_v1(self.capped_at)),
            ("tir_shard_at=", "--palw-drill-tir-shard-at", palw_drill_marker_fence_text_v1(self.tir_shard_at)),
            ("rfc1_at=", "--palw-drill-fp-prefix-at / -fp-tokenizer-at / -fp-constraint2-at / -adapter-at", self.rfc1_marker_text()),
            ("fence4_at=", "--palw-drill-fence4-at", palw_drill_marker_fence_text_v1(self.fence4_at)),
            ("class_seating_at=", "--palw-drill-class-seating-at", palw_drill_marker_fence_text_v1(self.class_seating_at)),
            ("panel_liveness_at=", "--palw-drill-panel-liveness-at", palw_drill_marker_fence_text_v1(self.panel_liveness_at)),
            (
                "capacity2_at=",
                "--palw-drill-capacity-step4-at/-step5-at/-emission-at/-multi-claim-at/-rho-breaker-at",
                self.capacity2_marker_text(),
            ),
        ]
    }

    /// The marker's `rfc1_at=` line — RFC-0001's four fences (`none` when none stands, else
    /// `fp_prefix:… fp_tokenizer:… fp_constraint2:… adapter:…`), its own line so every earlier line keeps its text.
    fn rfc1_marker_text(&self) -> String {
        if self.fp_constraint_at.is_none() && self.fp_prefix_at.is_none() && self.fp_tokenizer_at.is_none() && self.fp_constraint2_at.is_none() && self.adapter_at.is_none() {
            return palw_drill_marker_fence_text_v1(None);
        }
        let at = |v: Option<u64>| palw_drill_marker_fence_text_v1(v);
        format!(
            "fp_constraint:{},fp_prefix:{},fp_tokenizer:{},fp_constraint2:{},adapter:{}",
            at(self.fp_constraint_at),
            at(self.fp_prefix_at),
            at(self.fp_tokenizer_at),
            at(self.fp_constraint2_at),
            at(self.adapter_at)
        )
    }

    /// The marker's `extra_at=` line — RFC-0003 / RFC-0004's four flags only (`none` when none stands, else
    /// `gen:… decode_rules:… fp_v5:… improve:…`).
    fn marker_text(&self) -> String {
        if self.gen_at.is_none() && self.decode_rules_at.is_none() && self.fp_v5_at.is_none() && self.improve_at.is_none() {
            return palw_drill_marker_fence_text_v1(None);
        }
        let at = |v: Option<u64>| palw_drill_marker_fence_text_v1(v);
        format!(
            "gen:{},decode_rules:{},fp_v5:{},improve:{}",
            at(self.gen_at),
            at(self.decode_rules_at),
            at(self.fp_v5_at),
            at(self.improve_at)
        )
    }
}

/// **The salt the command line carries, parsed** — `None` without the flag. One parser
/// (`PalwDrillSaltV1::from_hex`), so every refusal of a malformed salt says the same thing.
pub fn palw_drill_salt_of_v1(args: &Args) -> ConfigResult<Option<PalwDrillSaltV1>> {
    args.palw_drill_genesis_salt.as_deref().map(|hex| PalwDrillSaltV1::from_hex(hex).map_err(|e| refused(e.to_string()))).transpose()
}

/// **Every start-up refusal a salted node answers to**, in the order an operator fixes them. Called
/// from `daemon::validate_args`, so every entry point that builds a node runs it. Without the salt
/// it only refuses the keyring export (which needs one) and returns.
pub fn palw_drill_validate_args_v1(args: &Args) -> ConfigResult<()> {
    let Some(salt) = palw_drill_salt_of_v1(args)? else {
        if args.palw_drill_write_keyring.is_some() {
            return Err(refused("--palw-drill-write-keyring writes a drill's keyring and needs --palw-drill-genesis-salt"));
        }
        if let Some(at) = args.palw_drill_fence_at {
            return Err(refused(format!(
                "--palw-drill-fence-at={at} moves the post-launch release's fences on a DRILL chain and needs \
                 --palw-drill-genesis-salt: on a real network the fences are the release's, and every node must agree on them"
            )));
        }
        if let Some(at) = args.palw_drill_fence2_at {
            return Err(refused(format!(
                "--palw-drill-fence2-at={at} arms testnet-12's second post-launch flag day on a DRILL chain and needs \
                 --palw-drill-genesis-salt: on a real network the fences are the release's, and every node must agree on them"
            )));
        }
        if let Some(at) = args.palw_drill_fence3_at {
            return Err(refused(format!(
                "--palw-drill-fence3-at={at} moves testnet-12's third post-launch flag day on a DRILL chain and needs \
                 --palw-drill-genesis-salt: on a real network the fences are the release's, and every node must agree on them"
            )));
        }
        if let Some(at) = args.palw_drill_tir_at {
            return Err(refused(format!(
                "--palw-drill-tir-at={at} arms testnet-12's IR fence (palw_tir_v1) on a DRILL chain and needs \
                 --palw-drill-genesis-salt: on a real network the fences are the release's, and every node must agree on them"
            )));
        }
        if let Some((flag, at, what)) = PalwDrillExtraFencesV1::of(args).first_named() {
            return Err(refused(format!(
                "{flag}={at} arms {what} on a DRILL chain and needs --palw-drill-genesis-salt: on a real network the fences are the \
                 release's, and every node must agree on them"
            )));
        }
        return Ok(());
    };
    let network = args.network();
    if network != palw_drill_network_v1() {
        return Err(refused(format!(
            "a drill genesis salt applies to testnet-12 only (--testnet --netsuffix=12), and this node is on {network}: a salt drills \
             the network whose shipping rules it keeps"
        )));
    }
    // The flag day's move, on the drill params the salt names — the same constructor and the same move
    // `apply_to_config` runs, so every refusal is a start-up refusal here and never a panic there.
    // Lane F2: the second flag day's move after the first, on the same params, as `apply_to_config`
    // makes them — so a second height that collides with the first is refused here, by name.
    if args.palw_drill_fence_at.is_some()
        || args.palw_drill_fence2_at.is_some()
        || args.palw_drill_fence3_at.is_some()
        || args.palw_drill_tir_at.is_some()
        || PalwDrillExtraFencesV1::of(args).any()
    {
        let mut params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network, Some(&salt)).map_err(refused)?;
        if let Some(at) = args.palw_drill_fence_at {
            kaspa_consensus_core::config::drill::palw_drill_post_launch_fences_at_v1(&mut params, at).map_err(refused)?;
        }
        if let Some(at) = args.palw_drill_fence2_at {
            kaspa_consensus_core::config::drill::palw_drill_post_launch_fences_v2_at_v1(&mut params, at).map_err(refused)?;
        }
        if let Some(at) = args.palw_drill_fence3_at {
            kaspa_consensus_core::config::drill::palw_drill_post_launch_fences_v3_at_v1(&mut params, at).map_err(refused)?;
        }
        // RFC-0002 Phase F: the IR fence after the flag days, as `apply_to_config` arms it.
        if let Some(at) = args.palw_drill_tir_at {
            kaspa_consensus_core::config::drill::palw_drill_tir_fence_at_v1(&mut params, at).map_err(refused)?;
        }
        // RFC-0003 / RFC-0004: the second IR fence, the generative fence, FP Job V5 and the improvement
        // fence, in `apply_to_config`'s order.
        PalwDrillExtraFencesV1::of(args).apply(&mut params).map_err(refused)?;
    }
    // The export writes files and exits; it dials nobody and signs nothing, so the node-only
    // requirements below do not apply to it.
    if args.palw_drill_write_keyring.is_some() {
        return Ok(());
    }
    if !args.disable_dns_seeding {
        return Err(refused(
            "a drill node needs --nodnsseed: it finds its peers by explicit address only and never asks public testnet-12's seeders",
        ));
    }
    if args.connect_peers.is_empty() && args.add_peers.is_empty() {
        return Err(refused(
            "a drill node needs an explicit peer list (--addpeer=<ip:port> or --connect=<ip:port>, the other drill hosts)",
        ));
    }
    if args.override_params_file.is_some() {
        return Err(refused(
            "--override-params-file is refused on a drill: the salt installs testnet-12's shipping rules on the drill genesis, and a drill of \
             edited rules drills a network nobody runs (to cross the post-launch release's flag day, --palw-drill-fence-at moves \
             exactly its fences)",
        ));
    }
    let ring = PalwDrillKeyringV1::new(salt);
    palw_drill_keys_are_drill_only_v1(args, &ring)?;
    for (flag, value) in [
        ("--palw-producer-bond", &args.palw_producer_bond),
        ("--palw-fee-outpoint", &args.palw_fee_outpoint),
        ("--stake-bond", &args.stake_bond),
    ] {
        if let Some(outpoint) = value.as_deref().and_then(|s| crate::palw_producer::parse_outpoint(s).ok())
            && palw_t12_public_genesis_txid_v1(&outpoint.transaction_id)
        {
            return Err(refused(format!(
                "{flag}={} names public testnet-12's genesis txid — a public unit's flag copied onto a drill. A drill seat's bond and fee \
                 float are on the drill's own premine txid (the keyring manifest lists them: --palw-drill-write-keyring)",
                value.as_deref().unwrap_or_default()
            )));
        }
    }
    Ok(())
}

/// **The drill-only key rule** (ADR-0152 §8.2: drill-only keys "for miners, heartbeats, payouts and
/// bonds"): the producer key is one of this drill's bond keys, the validator key one of its
/// validator keys, the pay and heartbeat addresses are this drill's, and the EVM fee recipient is
/// one of its EVM accounts. A key or address the keyring does not derive is refused whether or not
/// public testnet-12 knows it: the keyring is the positive list, so no deny-list has to stay
/// complete.
fn palw_drill_keys_are_drill_only_v1(args: &Args, ring: &PalwDrillKeyringV1) -> ConfigResult<()> {
    let salt_id = ring.salt().id();
    if let Some(path) = args.palw_producer_key.as_deref() {
        let seed = kaspa_pq_validator_core::load_validator_seed(path).map_err(|e| refused(format!("--palw-producer-key: {e}")))?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        if ring.find_bond_pubkey(key.public_key()).is_none() {
            return Err(refused(format!(
                "--palw-producer-key={path} is not a bond key of drill {salt_id} (bond keys 0..{PALW_DRILL_KEYRING_SPAN_V1}). A drill signs \
                 with drill-only keys, never a card key: take a seed from the keyring --palw-drill-write-keyring writes"
            )));
        }
    }
    if let Some(path) = args.validator_key.as_deref() {
        let seed = kaspa_pq_validator_core::load_validator_seed(path).map_err(|e| refused(format!("--validator-key: {e}")))?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        if ring.find_validator_pubkey(key.public_key()).is_none() {
            return Err(refused(format!(
                "--validator-key={path} is not a validator key of drill {salt_id} (validator keys 0..{PALW_DRILL_KEYRING_SPAN_V1}). A \
                 drill's attestations are signed with drill-only keys: take a seed from the keyring's `validator` rows"
            )));
        }
    }
    #[cfg(feature = "evm")]
    if let Some(text) = args.evm_fee_recipient.as_deref() {
        let hex = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")).unwrap_or(text);
        let mut account = [0u8; 20];
        if hex.len() != 40 || faster_hex::hex_decode(hex.as_bytes(), &mut account).is_err() {
            return Err(refused(format!("--evm-fee-recipient={text} is not a 20-byte hex address")));
        }
        if !kaspa_evm::tx::palw_drill_evm_accounts_v1(ring.salt()).contains(&account) {
            return Err(refused(format!(
                "--evm-fee-recipient={text} is not an EVM account of drill {salt_id}. The EVM lane is the one the salt does not \
                 separate (one chain id, no genesis in the signature), so a drill pays and signs only with the keyring's `evm` accounts"
            )));
        }
    }
    for (flag, value) in [
        ("--palw-producer-pay-address", &args.palw_producer_pay_address),
        ("--palw-heartbeat-miner-address", &args.palw_heartbeat_miner_address),
    ] {
        let Some(text) = value.as_deref() else { continue };
        let address = Address::try_from(text).map_err(|e| refused(format!("{flag}={text} is not an address: {e}")))?;
        if address.version != Version::PubKeyHashMlDsa87 || ring.find_payload(address.payload.as_slice()).is_none() {
            return Err(refused(format!(
                "{flag}={text} is not an address of drill {salt_id}. A drill is paid only at drill-only addresses: a public miner script \
                 on a drill coinbase mints an outpoint public testnet-12 can mint too, and a spend of one is a spend of the other"
            )));
        }
    }
    Ok(())
}

/// **Where drill injectors that are otherwise devnet/simnet-only may run**: devnet, simnet, and a
/// salted testnet-12 — a private chain by construction. Public testnet-12 stays refused.
///
/// Read off the chain the node RUNS — its `Config` — never off the arguments (P2-12 review
/// finding 7): a salt counts only when the params it installed carry a genesis that is not the
/// network's public one, so a node whose params swap was lost would run public testnet-12 with its
/// injectors refused rather than armed.
pub fn palw_private_drill_network_v1(config: &Config) -> bool {
    matches!(config.params.net.network_type, NetworkType::Devnet | NetworkType::Simnet)
        || (config.palw_drill_genesis_salt.is_some()
            && config.params.genesis.hash != kaspa_consensus_core::config::params::Params::from(config.params.net).genesis.hash)
}

/// **What a drill that crosses the post-launch release's flag day prints at start-up**
/// (`--palw-drill-fence-at`): a header with the params and schedule ids the move produced, then one
/// line per fence — armed (dormant in the release) or moved (from the release's height). Empty on
/// every node without the flag.
pub fn palw_drill_fence_lines_v1(config: &Config) -> Vec<String> {
    let moves = &config.palw_drill_fence_moves;
    if moves.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![format!(
        "PALW DRILL FLAG DAY (--palw-drill-fence-at / --palw-drill-fence2-at / --palw-drill-fence3-at / --palw-drill-model-court-at / --palw-drill-tir-at / --palw-drill-tir2-at): {} post-launch fence(s) of testnet-12's release set \
         on this drill chain, nothing else moved — params fingerprint {}, schedule id {}",
        moves.len(),
        config.params.consensus_params_id(),
        config.params.consensus_schedule_id()
    )];
    lines.extend(moves.iter().map(|m| format!("PALW DRILL FLAG DAY: {m}")));
    lines
}

/// The marker a drill writes into `<appdir>/<network>/`.
pub const PALW_DRILL_DATADIR_MARKER_V1: &str = "palw-drill-genesis";

/// How the drill marker names a flag day (`fence_at=`): the height `--palw-drill-fence-at` gave, or
/// `none` — also what a marker written before the flag existed counts as.
fn palw_drill_marker_fence_text_v1(fence_at: Option<u64>) -> String {
    fence_at.map_or_else(|| "none".to_owned(), |at| at.to_string())
}

/// **The app-directory guard** (see the module doc): `prefixed_dir` is `<appdir>/<network prefixed>`.
///
/// * salted: a marker naming another drill is refused; a directory holding anything but `logs/`
///   (the logger opens it before this runs) with no marker is refused — it may be a public node's;
///   otherwise the marker is written (idempotent) and the drill proceeds.
/// * salted, the same drill: the marker's flag day (`fence_at=`, `none` when absent) must be this
///   start's `--palw-drill-fence-at` (`fence_at`). A stored chain is never reopened under another
///   flag day (the int-4 audit of 2026-09-26): armed after the chain passed the height, every block
///   from it to the tip was validated unfenced and the node would still print "ARMED at DAA n" — a
///   false pass of the flag-day drill that gates the release; dropped or moved, the node runs rules
///   its stored chain was not validated under. A directory holding nothing but `logs/` and the
///   marker (the chain never started) takes the new flag day.
/// * unsalted: a marker is refused — the directory holds a drill chain.
pub fn palw_drill_datadir_guard_v1(
    prefixed_dir: &Path,
    salt: Option<&PalwDrillSaltV1>,
    genesis_hash: &str,
    fence_at: Option<u64>,
) -> Result<(), String> {
    palw_drill_datadir_guard_v2(prefixed_dir, salt, genesis_hash, fence_at, None, None)
}

/// [`palw_drill_datadir_guard_v1`] over BOTH flag days (lane F2, 2026-09-27): the marker also names the
/// `--palw-drill-fence2-at` the chain was started with (`fence2_at=`, `none` when absent — also what a
/// marker written before the flag existed counts as), and a stored chain is reopened only under the
/// same pair.
pub fn palw_drill_datadir_guard_v2(
    prefixed_dir: &Path,
    salt: Option<&PalwDrillSaltV1>,
    genesis_hash: &str,
    fence_at: Option<u64>,
    fence2_at: Option<u64>,
    fence3_at: Option<u64>,
) -> Result<(), String> {
    palw_drill_datadir_guard_v3(prefixed_dir, salt, genesis_hash, fence_at, fence2_at, fence3_at, None)
}

/// [`palw_drill_datadir_guard_v2`] with the IR fence (`--palw-drill-tir-at`, RFC-0002 Phase F): the
/// marker also names the height the chain armed `palw_tir_v1` at (`tir_at=`, `none` when absent —
/// also what a marker written before the flag existed counts as), and a stored chain is reopened
/// only under the same one.
pub fn palw_drill_datadir_guard_v3(
    prefixed_dir: &Path,
    salt: Option<&PalwDrillSaltV1>,
    genesis_hash: &str,
    fence_at: Option<u64>,
    fence2_at: Option<u64>,
    fence3_at: Option<u64>,
    tir_at: Option<u64>,
) -> Result<(), String> {
    palw_drill_datadir_guard_v4(
        prefixed_dir,
        salt,
        genesis_hash,
        fence_at,
        fence2_at,
        fence3_at,
        tir_at,
        PalwDrillExtraFencesV1::default(),
    )
}

/// [`palw_drill_datadir_guard_v3`] with the later fences ([`PalwDrillExtraFencesV1`]: the second IR fence —
/// testnet-12's DAA-3,600 flag day —, the per-model court window, and RFC-0003 / RFC-0004's generative fence,
/// decode rules, FP Job V5 and improvement fence). The marker names the release line's two as its own lines
/// (`tir2_at=`, `model_court_at=`, as the release writes them: a chain started by that binary reopens under
/// this one) and the other four as one (`extra_at=`); each reads `none` when absent — also what a marker
/// written before it counts as — and a stored chain is reopened only under the same set.
#[allow(clippy::too_many_arguments)]
pub fn palw_drill_datadir_guard_v4(
    prefixed_dir: &Path,
    salt: Option<&PalwDrillSaltV1>,
    genesis_hash: &str,
    fence_at: Option<u64>,
    fence2_at: Option<u64>,
    fence3_at: Option<u64>,
    tir_at: Option<u64>,
    extra: PalwDrillExtraFencesV1,
) -> Result<(), String> {
    let marker_path = prefixed_dir.join(PALW_DRILL_DATADIR_MARKER_V1);
    let marker = std::fs::read_to_string(&marker_path).ok();
    let marker_line = |key: &str| {
        marker.as_deref().and_then(|text| text.lines().find_map(|line| line.strip_prefix(key).map(|v| v.trim().to_owned())))
    };
    let marker_id = marker_line("salt_id=");
    let fence_text = palw_drill_marker_fence_text_v1(fence_at);
    let fence2_text = palw_drill_marker_fence_text_v1(fence2_at);
    // The third flag day (the capacity package): recorded beside the first two, `none` when absent —
    // also what a marker written before the flag existed counts as.
    let fence3_text = palw_drill_marker_fence_text_v1(fence3_at);
    let tir_text = palw_drill_marker_fence_text_v1(tir_at);
    let tir2_text = extra.tir2_marker_text();
    let model_court_text = extra.model_court_marker_text();
    let capacity_text = extra.capacity_marker_text();
    let extra_text = extra.marker_text();
    let later = extra.later_marker_lines();
    let later_lines: String = later.iter().map(|(key, _, text)| format!("{key}{text}\n")).collect();
    let write_marker = |salt: &PalwDrillSaltV1| {
        std::fs::create_dir_all(prefixed_dir).map_err(|e| format!("cannot create {}: {e}", prefixed_dir.display()))?;
        std::fs::write(
            &marker_path,
            format!(
                "salt_id={}\ngenesis={genesis_hash}\nfence_at={fence_text}\nfence2_at={fence2_text}\nfence3_at={fence3_text}\ntir_at={tir_text}\ntir2_at={tir2_text}\nmodel_court_at={model_court_text}\ncapacity_at={capacity_text}\nextra_at={extra_text}\n{later_lines}",
                salt.id()
            ),
        )
        .map_err(|e| format!("cannot write the drill marker {}: {e}", marker_path.display()))
    };
    match (salt, marker_id) {
        (None, None) => Ok(()),
        (None, Some(id)) => Err(format!(
            "{} holds the chain of testnet-12 drill {id} ({}). Start it with that drill's --palw-drill-genesis-salt, or give this node its \
             own --appdir: an unsalted node would find the drill genesis in place of its own and offer to delete the database",
            prefixed_dir.display(),
            marker_path.display()
        )),
        (Some(salt), Some(id)) if id != salt.id() => Err(format!(
            "{} holds the chain of testnet-12 drill {id}, and this node runs drill {}: give each drill its own --appdir",
            prefixed_dir.display(),
            salt.id()
        )),
        (Some(salt), Some(id)) => {
            let recorded = marker_line("fence_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded2 = marker_line("fence2_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded3 = marker_line("fence3_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded_tir = marker_line("tir_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded_tir2 = marker_line("tir2_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded_model_court = marker_line("model_court_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded_capacity = marker_line("capacity_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            let recorded_extra = marker_line("extra_at=").unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
            if recorded == fence_text
                && recorded2 == fence2_text
                && recorded3 == fence3_text
                && recorded_tir == tir_text
                && recorded_tir2 == tir2_text
                && recorded_model_court == model_court_text
                && recorded_capacity == capacity_text
                && recorded_extra == extra_text
                && later.iter().all(|(key, _, text)| marker_line(key).unwrap_or_else(|| palw_drill_marker_fence_text_v1(None)) == *text)
            {
                return Ok(());
            }
            let started = std::fs::read_dir(prefixed_dir)
                .map(|entries| {
                    entries.filter_map(|e| e.ok()).any(|e| e.file_name() != "logs" && e.file_name() != PALW_DRILL_DATADIR_MARKER_V1)
                })
                .unwrap_or(false);
            if !started {
                // The marker and the logger's directory only: no chain was stored under the old flag day.
                return write_marker(salt);
            }
            let flag = |text: &str, text2: &str| {
                let first = if text == "none" {
                    "without --palw-drill-fence-at".to_owned()
                } else {
                    format!("with --palw-drill-fence-at={text}")
                };
                // Lane F2: the second flag day is named only where the two starts disagree about it.
                let mut named = first;
                if recorded2 != fence2_text {
                    let second = if text2 == "none" {
                        "without --palw-drill-fence2-at".to_owned()
                    } else {
                        format!("with --palw-drill-fence2-at={text2}")
                    };
                    named = format!("{named} and {second}");
                }
                if recorded3 != fence3_text {
                    named = format!("{named} and a different --palw-drill-fence3-at (recorded {recorded3}, now {fence3_text})");
                }
                if recorded_tir != tir_text {
                    named = format!("{named} and a different --palw-drill-tir-at (recorded {recorded_tir}, now {tir_text})");
                }
                if recorded_tir2 != tir2_text {
                    named = format!("{named} and a different --palw-drill-tir2-at (recorded {recorded_tir2}, now {tir2_text})");
                }
                if recorded_model_court != model_court_text {
                    named = format!(
                        "{named} and a different --palw-drill-model-court-at (recorded {recorded_model_court}, now {model_court_text})"
                    );
                }
                if recorded_capacity != capacity_text {
                    named = format!(
                        "{named} and different --palw-drill-capacity-network-room-at/-network-verify-at/-step2-at/-step3-at/-rho100-at (recorded {recorded_capacity}, now {capacity_text})"
                    );
                }
                if recorded_extra != extra_text {
                    named = format!(
                        "{named} and different --palw-drill-gen-at/-decode-rules-at/-fp-v5-at/-improve-at (recorded {recorded_extra}, now {extra_text})"
                    );
                }
                for (key, flag_name, text) in &later {
                    let recorded_later = marker_line(key).unwrap_or_else(|| palw_drill_marker_fence_text_v1(None));
                    if recorded_later != *text {
                        named = format!("{named} and a different {flag_name} (recorded {recorded_later}, now {text})");
                    }
                }
                named
            };
            Err(format!(
                "{} holds the chain of testnet-12 drill {id} started {}, and this node starts {}. A stored drill chain is never \
                 reopened under another flag day: armed after the chain passed the height, its blocks past it were validated \
                 without crossing it and the drill would report a crossing that never happened; dropped or moved, the node would \
                 run rules its stored chain was not validated under. Restart it as it was started, or give every drill node a \
                 fresh --appdir and the same --palw-drill-fence-at from the drill's genesis",
                prefixed_dir.display(),
                flag(&recorded, &recorded2),
                flag(&fence_text, &fence2_text)
            ))
        }
        (Some(salt), None) => {
            let occupied = std::fs::read_dir(prefixed_dir)
                .map(|entries| entries.filter_map(|e| e.ok()).any(|e| e.file_name() != "logs"))
                .unwrap_or(false);
            if occupied {
                return Err(format!(
                    "{} holds a node's data and no drill marker — it may be a public testnet-12 node's. A drill never opens it: at the \
                     genesis check it would offer to delete that node's database (and with --yes would), and it would share that node's \
                     panel state. Give the drill a fresh --appdir",
                    prefixed_dir.display()
                ));
            }
            write_marker(salt)
        }
    }
}

/// How many keys of each exported role the keyring export writes (bond keys `0..16` covers the eight
/// genesis seats and D-9's and D-10's registrations with room to spare; the keyring itself spans
/// [`PALW_DRILL_KEYRING_SPAN_V1`]).
pub const PALW_DRILL_EXPORT_PER_ROLE_V1: u32 = 16;

/// The manifest's format tag; the drill script checks it before reading a field.
pub const PALW_DRILL_KEYRING_FORMAT_V1: &str = "misaka-palw-drill-keyring/v1";

/// **Write a drill's keyring to `dir` and return the manifest's path** (`--palw-drill-write-keyring`).
///
/// Seeds are written as hex, mode 0600 (what `--palw-producer-key` accepts): the main wallet, bond
/// keys `0..16` (the genesis seats first), heartbeat and payout keys `0..16`. `manifest.json` names
/// the drill — salt id, genesis hash, fingerprint, public testnet-12's genesis beside it — and every
/// seat's bond outpoint, fee float and address, so the drill script reads the binary's own answer and
/// never re-derives one. The salt itself is not written: the operator already holds it.
///
/// `fence_at` is the `--palw-drill-fence-at` on the same command line: the manifest's
/// `consensus_params_id` is the fingerprint a node started with that flag announces — the post-launch
/// fences moved exactly as `apply_to_config` moves them — and `fence_at` names the flag day (`null`
/// without the flag), so a crossing drill's manifest never names the release drill's fingerprint.
///
/// A directory holding another drill's manifest is refused; the same drill's is rewritten.
pub fn palw_drill_write_keyring_v1(salt: &PalwDrillSaltV1, dir: &Path, fence_at: Option<u64>) -> Result<PathBuf, String> {
    palw_drill_write_keyring_v2(salt, dir, fence_at, None, None)
}

/// [`palw_drill_write_keyring_v1`] over BOTH flag days (lane F2, 2026-09-27): `fence2_at` is the
/// `--palw-drill-fence2-at` on the same command line, applied after `fence_at` as `apply_to_config`
/// applies it, so the manifest's `consensus_params_id` is the one that node announces; the manifest
/// names it (`fence2_at`, `null` without the flag).
pub fn palw_drill_write_keyring_v2(
    salt: &PalwDrillSaltV1,
    dir: &Path,
    fence_at: Option<u64>,
    fence2_at: Option<u64>,
    fence3_at: Option<u64>,
) -> Result<PathBuf, String> {
    palw_drill_write_keyring_v3(salt, dir, fence_at, fence2_at, fence3_at, None)
}

/// [`palw_drill_write_keyring_v2`] with the IR fence (`--palw-drill-tir-at`, RFC-0002 Phase F), armed
/// after the flag days as `apply_to_config` arms it, so the manifest's `consensus_params_id` is the
/// one that node announces; the manifest names it (`tir_at`, `null` without the flag).
pub fn palw_drill_write_keyring_v3(
    salt: &PalwDrillSaltV1,
    dir: &Path,
    fence_at: Option<u64>,
    fence2_at: Option<u64>,
    fence3_at: Option<u64>,
    tir_at: Option<u64>,
) -> Result<PathBuf, String> {
    palw_drill_write_keyring_v4(salt, dir, fence_at, fence2_at, fence3_at, tir_at, PalwDrillExtraFencesV1::default())
}

/// [`palw_drill_write_keyring_v3`] with the later fences ([`PalwDrillExtraFencesV1`]: the second IR fence, the
/// per-model court window, and RFC-0003 / RFC-0004's four), armed after the IR fence as `apply_to_config` arms
/// them, so the manifest's `consensus_params_id` is the one such a node announces; the manifest names each
/// (`tir2_at`, `model_court_at`, `gen_at`, `decode_rules_at`, `fp_v5_at`, `improve_at`, `null` without it).
pub fn palw_drill_write_keyring_v4(
    salt: &PalwDrillSaltV1,
    dir: &Path,
    fence_at: Option<u64>,
    fence2_at: Option<u64>,
    fence3_at: Option<u64>,
    tir_at: Option<u64>,
    extra: PalwDrillExtraFencesV1,
) -> Result<PathBuf, String> {
    let manifest_path = dir.join("manifest.json");
    if let Ok(existing) = std::fs::read_to_string(&manifest_path) {
        let existing: serde_json::Value = serde_json::from_str(&existing).map_err(|e| format!("{}: {e}", manifest_path.display()))?;
        if existing.get("salt_id").and_then(|v| v.as_str()) != Some(salt.id().as_str()) {
            return Err(format!("{} is another drill's keyring; write this drill's elsewhere", manifest_path.display()));
        }
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let prefix = Prefix::Testnet;
    let ring = PalwDrillKeyringV1::new(*salt);
    let write_seed = |name: String, seed: &[u8; 32]| -> Result<String, String> {
        let path = dir.join(&name);
        write_private(&path, faster_hex::hex_string(seed).as_bytes())?;
        Ok(name)
    };

    let mut params = kaspa_consensus_core::config::params::palw_t12_drill_params_v1(salt);
    if let Some(at) = fence_at {
        kaspa_consensus_core::config::drill::palw_drill_post_launch_fences_at_v1(&mut params, at)
            .map_err(|e| format!("--palw-drill-fence-at: {e}"))?;
    }
    if let Some(at) = fence2_at {
        kaspa_consensus_core::config::drill::palw_drill_post_launch_fences_v2_at_v1(&mut params, at)
            .map_err(|e| format!("--palw-drill-fence2-at: {e}"))?;
    }
    if let Some(at) = fence3_at {
        kaspa_consensus_core::config::drill::palw_drill_post_launch_fences_v3_at_v1(&mut params, at)
            .map_err(|e| format!("--palw-drill-fence3-at: {e}"))?;
    }
    if let Some(at) = tir_at {
        kaspa_consensus_core::config::drill::palw_drill_tir_fence_at_v1(&mut params, at)
            .map_err(|e| format!("--palw-drill-tir-at: {e}"))?;
    }
    extra.apply(&mut params)?;
    let public = kaspa_consensus_core::config::params::Params::from(palw_drill_network_v1());
    let main = palw_t12_drill_main_key_v1(salt);
    let cards = palw_t12_drill_cards_v1(salt);
    let mut seats = Vec::new();
    for (n, card) in cards.iter().enumerate() {
        seats.push(serde_json::json!({
            "n": n,
            "premine_index": card.premine_index,
            "bond_outpoint": outpoint_text(palw_t12_drill_premine_outpoint_v1(salt, card.premine_index)),
            "fee_float_outpoint": outpoint_text(palw_t12_drill_fee_float_outpoint_v1(salt, n as u32)),
            "address": card.bond.address(prefix).to_string(),
            "seed_file": write_seed(format!("bond-{n}.seed"), &card.bond.seed)?,
            "operator_pubkey": faster_hex::hex_string(&card.operator.pubkey),
        }));
    }
    let role_rows = |role: PalwDrillKeyRoleV1, from: u32| -> Result<Vec<serde_json::Value>, String> {
        (from..PALW_DRILL_EXPORT_PER_ROLE_V1)
            .map(|n| {
                let key = ring.key(role, n);
                Ok(serde_json::json!({
                    "n": n,
                    "address": key.address(prefix).to_string(),
                    "seed_file": write_seed(format!("{}-{n}.seed", role.name()), &key.seed)?,
                }))
            })
            .collect()
    };
    let manifest = serde_json::json!({
        "format": PALW_DRILL_KEYRING_FORMAT_V1,
        "network": palw_drill_network_v1().to_string(),
        "salt_id": salt.id(),
        "genesis_hash": params.genesis.hash.to_string(),
        "consensus_params_id": params.consensus_params_id().to_string(),
        "fence_at": fence_at,
        "fence2_at": fence2_at,
        "fence3_at": fence3_at,
        "tir_at": tir_at,
        "tir2_at": extra.tir2_at,
        "model_court_at": extra.model_court_at,
        "capacity_network_room_at": extra.capacity_network_room_at,
        "capacity_network_verify_at": extra.capacity_network_verify_at,
        "capacity_step2_at": extra.capacity_step2_at,
        "capacity_step3_at": extra.capacity_step3_at,
        "capacity_rho100_at": extra.capacity_rho100_at,
        "capacity_step4_at": extra.capacity_step4_at,
        "capacity_step5_at": extra.capacity_step5_at,
        "capacity_emission_at": extra.capacity_emission_at,
        "capacity_multi_claim_at": extra.capacity_multi_claim_at,
        "capacity_rho_breaker_at": extra.capacity_rho_breaker_at,
        "gen_at": extra.gen_at,
        "decode_rules_at": extra.decode_rules_at,
        "fp_v5_at": extra.fp_v5_at,
        "held_chunks_at": extra.held_chunks_at,
        "vertex_at": extra.vertex_at,
        "witness_at": extra.witness_at,
        "audit_mesh_at": extra.audit_mesh_at,
        "capped_at": extra.capped_at,
        "panel_liveness_at": extra.panel_liveness_at,
        "int11_at": extra.int11_at,
        "improve_at": extra.improve_at,
        "tir_shard_at": extra.tir_shard_at,
        "fence4_at": extra.fence4_at,
        "class_seating_at": extra.class_seating_at,
        "public_genesis_hash": public.genesis.hash.to_string(),
        "public_consensus_params_id": public.consensus_params_id().to_string(),
        "premine_txid": kaspa_consensus_core::config::premine::palw_t12_drill_premine_txid_v1(salt).to_string(),
        "community_txid": kaspa_consensus_core::config::premine::palw_t12_drill_community_txid_v1(salt).to_string(),
        "main": {
            "address": main.address(prefix).to_string(),
            "outpoint": outpoint_text(palw_t12_drill_premine_outpoint_v1(salt, kaspa_consensus_core::config::premine::MAIN_PREMINE_INDEX)),
            "seed_file": write_seed("main-0.seed".to_owned(), &main.seed)?,
        },
        "seats": seats,
        // Bond keys past the genesis seats: what a drill registers (D-9's 130,000 MSK seat, D-10's
        // re-registration), each its own operator identity (`--palw-register-bond` signs both halves
        // with the one key).
        "bonds": role_rows(PalwDrillKeyRoleV1::Bond, cards.len() as u32)?,
        "heartbeat": role_rows(PalwDrillKeyRoleV1::Heartbeat, 0)?,
        "payout": role_rows(PalwDrillKeyRoleV1::Payout, 0)?,
        "validator": role_rows(PalwDrillKeyRoleV1::Validator, 0)?,
        // The EVM lane's accounts — the only ones a drill signs with (the salt does not separate
        // the lane; see the module doc). `key_file` holds the secp256k1 secret as 64 hex.
        "evm": evm_rows(salt, dir)?,
    });
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    std::fs::write(&manifest_path, text).map_err(|e| format!("cannot write {}: {e}", manifest_path.display()))?;
    Ok(manifest_path)
}

/// The keyring export's `evm` rows: accounts `0..PALW_DRILL_EXPORT_PER_ROLE_V1`, each with its
/// address and a 0600 file holding its secret. Empty in a build without the EVM lane (which cannot
/// run testnet-12 anyway).
fn evm_rows(salt: &PalwDrillSaltV1, dir: &Path) -> Result<Vec<serde_json::Value>, String> {
    #[cfg(feature = "evm")]
    {
        let accounts = kaspa_evm::tx::palw_drill_evm_accounts_v1(salt);
        (0..PALW_DRILL_EXPORT_PER_ROLE_V1)
            .map(|n| {
                let name = format!("evm-{n}.key");
                write_private(
                    &dir.join(&name),
                    faster_hex::hex_string(&kaspa_consensus_core::config::drill::palw_drill_evm_secret_v1(salt, n)).as_bytes(),
                )?;
                Ok(serde_json::json!({
                    "n": n,
                    "address": format!("0x{}", faster_hex::hex_string(&accounts[n as usize])),
                    "key_file": name,
                }))
            })
            .collect()
    }
    #[cfg(not(feature = "evm"))]
    {
        let _ = (salt, dir);
        Ok(Vec::new())
    }
}

fn outpoint_text(outpoint: kaspa_consensus_core::tx::TransactionOutpoint) -> String {
    format!("{}:{}", outpoint.transaction_id, outpoint.index)
}

/// Write `bytes` to `path` readable by the owner only — what `load_validator_seed` insists on.
fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // `mode` applies at creation only; a rewritten seed file is narrowed too.
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    file.write_all(bytes).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// **The drill's pre-flight**, run by `daemon::create_core` before the logger opens a file under
/// the app directory: the argument rules, the keyring export (which exits), and the app-directory
/// guard. Prints and exits on a refusal, like the other start-up rails.
pub fn palw_drill_preflight_v1(args: &Args) {
    if let Err(e) = palw_drill_validate_args_v1(args) {
        println!("{e}");
        std::process::exit(1);
    }
    let salt = palw_drill_salt_of_v1(args).ok().flatten();
    if let (Some(salt), Some(dir)) = (salt.as_ref(), args.palw_drill_write_keyring.as_deref()) {
        match palw_drill_write_keyring_v4(
            salt,
            Path::new(dir),
            args.palw_drill_fence_at,
            args.palw_drill_fence2_at,
            args.palw_drill_fence3_at,
            args.palw_drill_tir_at,
            PalwDrillExtraFencesV1::of(args),
        ) {
            Ok(manifest) => {
                println!("testnet-12 drill {} keyring written: {}", salt.id(), manifest.display());
                std::process::exit(0);
            }
            Err(e) => {
                println!("--palw-drill-write-keyring: {e}");
                std::process::exit(1);
            }
        }
    }
    palw_drill_datadir_guard_or_exit_v1(args, salt.as_ref());
}

/// [`palw_drill_datadir_guard_v1`] on this node's `<appdir>/<network>/`, exiting on a refusal.
/// Runs from `create_core` (before the logger) and again from `create_core_with_runtime` (before
/// the database opens), so an entry point that skips the first still meets the second.
pub fn palw_drill_datadir_guard_or_exit_v1(args: &Args, salt: Option<&PalwDrillSaltV1>) {
    let prefixed = crate::daemon::get_app_dir_from_args(args).join(args.network().to_prefixed());
    let genesis =
        salt.map(|s| kaspa_consensus_core::config::drill::palw_t12_drill_genesis_block_v1(s).hash.to_string()).unwrap_or_default();
    if let Err(e) = palw_drill_datadir_guard_v4(
        &prefixed,
        salt,
        &genesis,
        args.palw_drill_fence_at,
        args.palw_drill_fence2_at,
        args.palw_drill_fence3_at,
        args.palw_drill_tir_at,
        PalwDrillExtraFencesV1::of(args),
    ) {
        println!("{e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Args;

    const SALT: &str = "53535353535353535353535353535353535353535353535353535353535353aa";

    fn parse(extra: &[&str]) -> Args {
        let mut argv = vec!["kaspad"];
        argv.extend_from_slice(extra);
        Args::parse(argv).expect("parses")
    }

    fn salt() -> PalwDrillSaltV1 {
        PalwDrillSaltV1::from_hex(SALT).unwrap()
    }

    /// The release line's DAA-3,600 drill flag alone (`--palw-drill-tir2-at`).
    fn tir2_only(at: u64) -> PalwDrillExtraFencesV1 {
        PalwDrillExtraFencesV1 { tir2_at: Some(at), ..Default::default() }
    }

    fn refusal(args: &Args) -> String {
        match palw_drill_validate_args_v1(args) {
            Err(ConfigError::PalwDrillRefused(why)) => why,
            other => panic!("expected a drill refusal, got {other:?}"),
        }
    }

    fn seed_file(dir: &Path, name: &str, seed: &[u8; 32]) -> String {
        let path = dir.join(name);
        write_private(&path, faster_hex::hex_string(seed).as_bytes()).unwrap();
        path.to_str().unwrap().to_owned()
    }

    /// **The flag's own rules**: testnet-12 only, a well-formed salt, no discovery, an explicit peer
    /// list, no params override — and the node without the flag is untouched (the keyring export
    /// alone needs the salt). The flag is not read from the environment or a config file.
    #[test]
    fn t53_a_salted_node_is_testnet_12_with_no_discovery_and_the_shipping_rules() {
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let with = |extra: &[&str]| {
            let mut argv: Vec<&str> = base.to_vec();
            argv.extend_from_slice(extra);
            parse(&argv)
        };
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        assert!(palw_drill_validate_args_v1(&with(&[&flag])).is_ok(), "the minimal drill node");
        assert!(palw_drill_validate_args_v1(&with(&[])).is_ok(), "no salt, no drill rule");
        assert_eq!(palw_drill_salt_of_v1(&with(&[&flag])).unwrap(), Some(salt()));

        for (argv, needle) in [
            (vec!["--testnet", "--netsuffix=10", "--nodnsseed", "--addpeer=10.0.0.2:26311", flag.as_str()], "testnet-12 only"),
            (vec!["--devnet", "--nodnsseed", "--addpeer=10.0.0.2:26311", flag.as_str()], "testnet-12 only"),
            (vec!["--nodnsseed", "--addpeer=10.0.0.2:26311", flag.as_str()], "testnet-12 only"),
            (vec!["--testnet", "--netsuffix=12", "--addpeer=10.0.0.2:26311", flag.as_str()], "--nodnsseed"),
            (vec!["--testnet", "--netsuffix=12", "--nodnsseed", flag.as_str()], "explicit peer list"),
        ] {
            let why = refusal(&parse(&argv));
            assert!(why.contains(needle), "{argv:?}: {why}");
        }
        let why = refusal(&with(&[&flag, "--override-params-file=/tmp/p.json"]));
        assert!(why.contains("--override-params-file"), "{why}");
        let why = refusal(&with(&["--palw-drill-genesis-salt=00"]));
        assert!(why.contains("64 hex characters") || why.contains("hex"), "{why}");
        let why = refusal(&with(&[&format!("--palw-drill-genesis-salt={}", "00".repeat(32))]));
        assert!(why.contains("all zero"), "{why}");
        let why = refusal(&with(&["--palw-drill-write-keyring=/tmp/k"]));
        assert!(why.contains("needs --palw-drill-genesis-salt"), "{why}");
        // The export dials nobody: it needs the salt and testnet-12, nothing else.
        assert!(
            palw_drill_validate_args_v1(&parse(&["--testnet", "--netsuffix=12", &flag, "--palw-drill-write-keyring=/tmp/k"])).is_ok()
        );

        // Wired into the start-up check every entry point runs.
        assert!(matches!(
            crate::daemon::validate_args(&parse(&["--testnet", "--netsuffix=10", flag.as_str()])),
            Err(ConfigError::PalwDrillRefused(_))
        ));

        // Command line only: no environment variable and no config-file key reaches it.
        let help = crate::args::cli().render_long_help().to_string();
        assert!(help.contains("--palw-drill-genesis-salt"));
        assert!(!help.contains("KASPAD_PALW_DRILL_GENESIS_SALT"), "never from the environment");
        let from_file: Result<Args, _> = toml::from_str(&format!("palw-drill-genesis-salt = \"{SALT}\""));
        assert!(from_file.is_err(), "never from a config file: an unknown key is refused");

        // The help strings themselves (review finding 9): no lost `\` continuation leaves a run of
        // spaces inside one, and the three injectors say where they now run.
        let cmd = crate::args::cli();
        let help_of = |id: &str| cmd.get_arguments().find(|a| a.get_id() == id).and_then(|a| a.get_help()).unwrap().to_string();
        for id in [
            "palw-drill-genesis-salt",
            "palw-drill-write-keyring",
            "palw-drill-answer-only",
            "palw-drill-refuse-leaf-evidence",
            "palw-drill-tamper-fp-leaf",
            "palw-drill-tamper-eval",
        ] {
            let text = help_of(id);
            assert!(!text.contains("  "), "--{id}'s help has a run of spaces: {text}");
        }
        for id in ["palw-drill-answer-only", "palw-drill-refuse-leaf-evidence", "palw-drill-tamper-fp-leaf", "palw-drill-tamper-eval"] {
            assert!(help_of(id).contains("OR A SALTED TESTNET-12 DRILL ONLY"), "--{id}");
        }
    }

    /// The node's `Config` as `create_core_with_runtime` builds it from these arguments.
    fn config_of(args: &Args) -> Config {
        let mut config = Config::new(kaspa_consensus_core::config::params::Params::from(args.network()));
        args.apply_to_config(&mut config);
        config
    }

    /// **`apply_to_config` installs the drill params and the salt together, and the injector gate
    /// reads the chain the node runs** (P2-12 review finding 7). Salted: the drill genesis, the
    /// salt, a private drill. Unsalted testnet-12: public testnet-12's genesis, no salt, not a
    /// drill. A salted config whose params swap was lost — public genesis, salt set — is NOT a
    /// private drill: the gate refuses the injectors on the chain it would actually run.
    #[test]
    fn t53_the_config_carries_the_drill_genesis_and_the_injector_gate_reads_it() {
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        let public_t12 = kaspa_consensus_core::config::params::Params::from(palw_drill_network_v1());

        let salted_argv: Vec<&str> = base.iter().copied().chain([flag.as_str()]).collect();
        let salted = config_of(&parse(&salted_argv));
        assert_eq!(salted.params.genesis.hash, kaspa_consensus_core::config::drill::palw_t12_drill_genesis_block_v1(&salt()).hash);
        assert_ne!(salted.params.genesis.hash, public_t12.genesis.hash);
        assert_eq!(salted.palw_drill_genesis_salt, Some(salt()), "the salt rides with the params it salted");
        assert_eq!(salted.params.net, palw_drill_network_v1());
        assert!(palw_private_drill_network_v1(&salted));

        let public = config_of(&parse(&base));
        assert_eq!(public.params.genesis.hash, public_t12.genesis.hash);
        assert_eq!(public.palw_drill_genesis_salt, None);
        assert!(!palw_private_drill_network_v1(&public), "public testnet-12 is not a drill");

        let mut lost_swap = config_of(&parse(&salted_argv));
        lost_swap.params = public_t12;
        assert!(!palw_private_drill_network_v1(&lost_swap), "a salt on public testnet-12's genesis arms nothing");

        assert!(palw_private_drill_network_v1(&config_of(&parse(&["--devnet"]))));
        assert!(palw_private_drill_network_v1(&config_of(&parse(&["--simnet"]))));
    }

    /// **Drill-only keys, by the keyring**: a drill bond seed and drill addresses pass; a card key,
    /// another drill's key, a non-drill address and a public genesis outpoint are refused by name.
    #[test]
    fn t53_a_salted_node_signs_and_is_paid_only_with_drill_keys() {
        let dir = tempfile::tempdir().unwrap();
        let ring = PalwDrillKeyringV1::new(salt());
        let other = PalwDrillKeyringV1::new(PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap());
        let drill_bond = seed_file(dir.path(), "drill-bond", &ring.key(PalwDrillKeyRoleV1::Bond, 3).seed);
        let drill_operator = seed_file(dir.path(), "drill-operator", &ring.key(PalwDrillKeyRoleV1::Operator, 3).seed);
        let drill_validator = seed_file(dir.path(), "drill-validator", &ring.key(PalwDrillKeyRoleV1::Validator, 0).seed);
        let foreign_bond = seed_file(dir.path(), "foreign-bond", &other.key(PalwDrillKeyRoleV1::Bond, 3).seed);
        let heartbeat = ring.key(PalwDrillKeyRoleV1::Heartbeat, 0).address(Prefix::Testnet).to_string();
        let payout = ring.key(PalwDrillKeyRoleV1::Payout, 1).address(Prefix::Testnet).to_string();
        let foreign = other.key(PalwDrillKeyRoleV1::Heartbeat, 0).address(Prefix::Testnet).to_string();
        let card = Address::new(
            Prefix::Testnet,
            Version::PubKeyHashMlDsa87,
            &kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS[0].payout_payload,
        )
        .to_string();
        let drill_seat = palw_t12_drill_premine_outpoint_v1(&salt(), 0);
        let public_seat = kaspa_consensus_core::config::premine::premine_outpoint_for(palw_drill_network_v1(), 0);

        let node = |extra: &[String]| {
            let mut argv: Vec<String> =
                ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"].iter().map(|s| s.to_string()).collect();
            argv.push(format!("--palw-drill-genesis-salt={SALT}"));
            argv.extend_from_slice(extra);
            let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
            parse(&refs)
        };
        let ok = node(&[
            format!("--palw-producer-key={drill_bond}"),
            format!("--palw-producer-bond={}:{}", drill_seat.transaction_id, drill_seat.index),
            format!("--palw-producer-pay-address={payout}"),
            format!("--palw-heartbeat-miner-address={heartbeat}"),
            format!("--validator-key={drill_validator}"),
            format!("--stake-bond={}:{}", drill_seat.transaction_id, drill_seat.index),
        ]);
        assert!(palw_drill_validate_args_v1(&ok).is_ok(), "{:?}", palw_drill_validate_args_v1(&ok));

        // Review finding 6: the validator key and the stake bond are drill-only too.
        for (extra, needle) in [
            (format!("--validator-key={drill_bond}"), "is not a validator key of drill"),
            (format!("--validator-key={foreign_bond}"), "is not a validator key of drill"),
            (format!("--stake-bond={}:{}", public_seat.transaction_id, public_seat.index), "names public testnet-12's genesis txid"),
        ] {
            let why = refusal(&node(&[extra.clone()]));
            assert!(why.contains(needle), "{extra}: {why}");
        }

        // Review finding 2: the EVM fee recipient is one of the drill's EVM accounts.
        #[cfg(feature = "evm")]
        {
            let ours = kaspa_evm::tx::palw_drill_evm_accounts_v1(&salt())[2];
            let ok = node(&[format!("--evm-fee-recipient=0x{}", faster_hex::hex_string(&ours))]);
            assert!(palw_drill_validate_args_v1(&ok).is_ok(), "{:?}", palw_drill_validate_args_v1(&ok));
            let theirs = kaspa_evm::tx::palw_drill_evm_accounts_v1(&PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap())[2];
            for text in [format!("0x{}", faster_hex::hex_string(&theirs)), format!("0x{}", "11".repeat(20))] {
                let why = refusal(&node(&[format!("--evm-fee-recipient={text}")]));
                assert!(why.contains("is not an EVM account of drill"), "{text}: {why}");
            }
            let why = refusal(&node(&["--evm-fee-recipient=0x12".to_owned()]));
            assert!(why.contains("20-byte hex"), "{why}");
        }

        for (extra, needle) in [
            (format!("--palw-producer-key={drill_operator}"), "is not a bond key of drill"),
            (format!("--palw-producer-key={foreign_bond}"), "is not a bond key of drill"),
            (format!("--palw-heartbeat-miner-address={foreign}"), "is not an address of drill"),
            (format!("--palw-heartbeat-miner-address={card}"), "is not an address of drill"),
            (format!("--palw-producer-pay-address={card}"), "is not an address of drill"),
            (
                format!("--palw-producer-bond={}:{}", public_seat.transaction_id, public_seat.index),
                "names public testnet-12's genesis txid",
            ),
            (format!("--palw-fee-outpoint={}:41", public_seat.transaction_id), "names public testnet-12's genesis txid"),
        ] {
            let why = refusal(&node(&[extra.clone()]));
            assert!(why.contains(needle), "{extra}: {why}");
        }
    }

    /// **The app-directory marker**: a fresh directory (the logger's `logs/` aside) takes a drill
    /// and keeps it; a directory with another node's data and no marker refuses a drill; another
    /// drill's marker refuses this one; an unsalted node refuses a drill's directory; an unsalted
    /// node's own directory is untouched.
    #[test]
    fn t53_a_drill_and_a_public_node_never_share_an_app_directory() {
        let (a, b) = (salt(), PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap());
        let root = tempfile::tempdir().unwrap();

        let fresh = root.path().join("fresh/misaka-testnet-12");
        std::fs::create_dir_all(fresh.join("logs")).unwrap();
        palw_drill_datadir_guard_v1(&fresh, Some(&a), "g", None).expect("a fresh app dir takes the drill");
        assert!(std::fs::read_to_string(fresh.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap().contains(&format!("salt_id={}", a.id())));
        std::fs::create_dir_all(fresh.join("datadir")).unwrap();
        palw_drill_datadir_guard_v1(&fresh, Some(&a), "g", None).expect("and keeps it across restarts");
        let why = palw_drill_datadir_guard_v1(&fresh, Some(&b), "g", None).unwrap_err();
        assert!(why.contains(&a.id()) && why.contains(&b.id()), "{why}");
        let why = palw_drill_datadir_guard_v1(&fresh, None, "", None).unwrap_err();
        assert!(why.contains("holds the chain of testnet-12 drill"), "{why}");

        let public = root.path().join("public/misaka-testnet-12");
        std::fs::create_dir_all(public.join("datadir")).unwrap();
        std::fs::create_dir_all(public.join("palw-panel")).unwrap();
        palw_drill_datadir_guard_v1(&public, None, "", None).expect("a public node's own directory is untouched");
        let why = palw_drill_datadir_guard_v1(&public, Some(&a), "g", None).unwrap_err();
        assert!(why.contains("may be a public testnet-12 node's") && why.contains("delete"), "{why}");
        assert!(!public.join(PALW_DRILL_DATADIR_MARKER_V1).exists(), "a refused drill writes nothing");

        let absent = root.path().join("absent/misaka-testnet-12");
        palw_drill_datadir_guard_v1(&absent, None, "", None).expect("nothing there, nothing to refuse");
        palw_drill_datadir_guard_v1(&absent, Some(&a), "g", None).expect("created with its marker");
        assert!(absent.join(PALW_DRILL_DATADIR_MARKER_V1).exists());
    }

    /// **A stored drill chain is never reopened under another flag day** (the int-4 audit of
    /// 2026-09-26): the marker names the `--palw-drill-fence-at` the chain was started with. The
    /// audit's case first — a drill started without the flag (the script's default), run past DAA 40,
    /// then restarted with `--palw-drill-fence-at=40`, would print "ARMED at DAA 40" over blocks that
    /// were validated unfenced; now it is refused, as is a crossing drill restarted without the flag or
    /// at another height. A directory with no chain stored yet takes the new flag day; a marker
    /// written before the flag existed counts as started without it.
    #[test]
    fn a_drill_chain_is_never_reopened_under_another_flag_day() {
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();

        // The audit's case: a marker from before the flag (no `fence_at=` line), a stored chain, the
        // flag added on restart.
        let old = root.path().join("old/misaka-testnet-12");
        std::fs::create_dir_all(old.join("datadir")).unwrap();
        std::fs::write(old.join(PALW_DRILL_DATADIR_MARKER_V1), format!("salt_id={}\ngenesis=g\n", a.id())).unwrap();
        palw_drill_datadir_guard_v1(&old, Some(&a), "g", None).expect("started without the flag, restarted without it");
        let why = palw_drill_datadir_guard_v1(&old, Some(&a), "g", Some(40)).unwrap_err();
        assert!(
            why.contains("started without --palw-drill-fence-at") && why.contains("starts with --palw-drill-fence-at=40"),
            "{why}"
        );
        assert!(why.contains("fresh") && why.contains("--appdir"), "{why}");
        assert!(!marker_of(&old).contains("fence_at="), "a refused start rewrites nothing");

        // A crossing drill: the marker names its height; the same flag reopens it, and nothing else does.
        let crossing = root.path().join("crossing/misaka-testnet-12");
        std::fs::create_dir_all(crossing.join("logs")).unwrap();
        palw_drill_datadir_guard_v1(&crossing, Some(&a), "g", Some(40)).expect("a fresh app dir takes the crossing drill");
        assert!(marker_of(&crossing).contains("fence_at=40\n"), "{}", marker_of(&crossing));
        // Nothing stored yet (the logger's directory and the marker only): a corrected start is taken.
        palw_drill_datadir_guard_v1(&crossing, Some(&a), "g", Some(41)).expect("no chain stored yet: the new flag day");
        assert!(marker_of(&crossing).contains("fence_at=41\n"));
        palw_drill_datadir_guard_v1(&crossing, Some(&a), "g", Some(40)).expect("and back");
        std::fs::create_dir_all(crossing.join("datadir")).unwrap();
        palw_drill_datadir_guard_v1(&crossing, Some(&a), "g", Some(40)).expect("the same flag day across restarts");
        for (at, wording) in [(None, "starts without --palw-drill-fence-at"), (Some(41), "starts with --palw-drill-fence-at=41")] {
            let why = palw_drill_datadir_guard_v1(&crossing, Some(&a), "g", at).unwrap_err();
            assert!(why.contains("started with --palw-drill-fence-at=40") && why.contains(wording), "{why}");
        }
        assert!(marker_of(&crossing).contains("fence_at=40\n"), "a refused start rewrites nothing");

        // A release drill (no flag) writes `none`, and keeps it.
        let release = root.path().join("release/misaka-testnet-12");
        palw_drill_datadir_guard_v1(&release, Some(&a), "g", None).expect("created with its marker");
        assert!(marker_of(&release).contains("fence_at=none\n"));
        std::fs::create_dir_all(release.join("datadir")).unwrap();
        palw_drill_datadir_guard_v1(&release, Some(&a), "g", None).expect("kept");
        assert!(palw_drill_datadir_guard_v1(&release, Some(&a), "g", Some(40)).is_err(), "never armed over a stored chain");

        // Both start-up calls pass the command line's flag.
        let source = include_str!("palw_drill.rs");
        let wrapper = &source[source.find("pub fn palw_drill_datadir_guard_or_exit_v1(").unwrap()..];
        let wrapper = &wrapper[..wrapper.find("\n}\n").unwrap()];
        assert!(wrapper.contains("palw_drill_datadir_guard_v4("), "{wrapper}");
        for flag in [
            "args.palw_drill_fence_at,",
            "args.palw_drill_fence2_at,",
            "args.palw_drill_fence3_at,",
            "args.palw_drill_tir_at,",
            "PalwDrillExtraFencesV1::of(args),",
        ] {
            assert!(wrapper.contains(flag), "the guard passes {flag}: {wrapper}");
        }
    }

    /// **The IR fence rides the marker and the keyring like a flag day** (`--palw-drill-tir-at`,
    /// RFC-0002 Phase F's D-F drills): the marker names it (`tir_at=`, `none` without it), a stored
    /// chain is never reopened under another one, and the flag is refused without the salt.
    #[test]
    fn the_ir_fence_is_a_drill_flag_day_of_its_own() {
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let dir = root.path().join("tir/misaka-testnet-12");
        palw_drill_datadir_guard_v3(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(100)).expect("a fresh app dir");
        assert!(marker_of(&dir).contains("tir_at=100\n"), "{}", marker_of(&dir));
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        palw_drill_datadir_guard_v3(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(100)).expect("the same start");
        let why = palw_drill_datadir_guard_v3(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(120)).unwrap_err();
        assert!(why.contains("--palw-drill-tir-at (recorded 100, now 120)"), "{why}");
        let why = palw_drill_datadir_guard_v2(&dir, Some(&a), "g", Some(40), Some(60), Some(80)).unwrap_err();
        assert!(why.contains("recorded 100, now none"), "dropped over a stored chain: {why}");
        let unsalted = parse(&["--testnet", "--netsuffix=12", "--palw-drill-tir-at=100"]);
        let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
        assert!(refused.contains("--palw-drill-tir-at=100") && refused.contains("--palw-drill-genesis-salt"), "{refused}");
    }

    /// **The second IR fence is a drill flag day of its own too** (`--palw-drill-tir2-at`,
    /// `palw_tir_fence2`): the marker names it (`tir2_at=`), a stored chain is never reopened under
    /// another one, the keyring's manifest names it and announces the fingerprint it moves, and the flag
    /// is refused without the salt and — by the ruleset's own check — without the first IR fence below it.
    #[test]
    fn the_second_ir_fence_is_a_drill_flag_day_of_its_own() {
        use kaspa_consensus_core::config::params::ForkActivation;
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let dir = root.path().join("tir2/misaka-testnet-12");
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(100), tir2_only(130))
            .expect("a fresh app dir");
        assert!(marker_of(&dir).contains("tir2_at=130\n"), "{}", marker_of(&dir));
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(100), tir2_only(130))
            .expect("the same start");
        let why =
            palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(100), tir2_only(140)).unwrap_err();
        assert!(why.contains("--palw-drill-tir2-at (recorded 130, now 140)"), "{why}");
        let why = palw_drill_datadir_guard_v3(&dir, Some(&a), "g", Some(40), Some(60), Some(80), Some(100)).unwrap_err();
        assert!(why.contains("recorded 130, now none"), "dropped over a stored chain: {why}");
        // `apply_to_config` arms it after the first IR fence — the drill's own heights (audit-tir's
        // lib-df.sh: flag days 6/10/14, IR fence 20) — with the mirror the fold reads, and the keyring
        // names the fingerprint the node on the same command line announces.
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let salted = format!("--palw-drill-genesis-salt={SALT}");
        let days = ["--palw-drill-fence-at=6", "--palw-drill-fence2-at=10", "--palw-drill-fence3-at=14", "--palw-drill-tir-at=20"];
        let argv = |extra: &[&str]| -> Vec<String> {
            base.iter().copied().chain([salted.as_str()]).chain(days).chain(extra.iter().copied()).map(str::to_owned).collect()
        };
        let parsed = |extra: &[&str]| {
            let v = argv(extra);
            parse(&v.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let with = config_of(&parsed(&["--palw-drill-tir2-at=30"]));
        let without = config_of(&parsed(&[]));
        assert_eq!(with.params.palw_tir_fence2, Some(ForkActivation::new(30)));
        assert_eq!(
            without.params.palw_tir_fence2,
            Some(ForkActivation::new(3_600)),
            "the release arms it at DAA 3,600 with the court window; the flag moves it alone"
        );
        assert!(with.params.palw_tir_fence2_active_at(30) && !with.params.palw_tir_fence2_active_at(29));
        assert_eq!(with.palw_drill_fence_moves.len(), without.palw_drill_fence_moves.len() + 1, "the fence alone moves");
        with.params.validate_palw_v2().expect("the drill crossing both IR fences validates");
        assert_ne!(with.params.consensus_params_id(), without.params.consensus_params_id(), "the fence moves the fingerprint");
        palw_drill_validate_args_v1(&parsed(&["--palw-drill-tir2-at=30"])).expect("with the first IR fence below it");
        let keys = tempfile::tempdir().unwrap();
        let path =
            palw_drill_write_keyring_v4(&a, keys.path(), Some(6), Some(10), Some(14), Some(20), tir2_only(30)).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["tir2_at"], serde_json::json!(30));
        assert_eq!(manifest["consensus_params_id"], with.params.consensus_params_id().to_string().as_str());
        let keys_without = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v3(&a, keys_without.path(), Some(6), Some(10), Some(14), Some(20)).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["tir2_at"], serde_json::Value::Null);
        assert_eq!(manifest["consensus_params_id"], without.params.consensus_params_id().to_string().as_str());
        // Refused without the salt, and — by the ruleset's own check, at start-up and in the keyring
        // export — below `palw_tir_v1` (the release arms it at DAA 2,000; here without --palw-drill-tir-at).
        let unsalted = parse(&["--testnet", "--netsuffix=12", "--palw-drill-tir2-at=130"]);
        let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
        assert!(refused.contains("--palw-drill-tir2-at=130") && refused.contains("--palw-drill-genesis-salt"), "{refused}");
        let alone: Vec<&str> = base.iter().copied().chain([salted.as_str(), "--palw-drill-tir2-at=130"]).collect();
        let refused = palw_drill_validate_args_v1(&parse(&alone)).unwrap_err().to_string();
        assert!(refused.contains("palw_tir_fence2 needs palw_tir_v1"), "{refused}");
        let why =
            palw_drill_write_keyring_v4(&a, tempfile::tempdir().unwrap().path(), None, None, None, None, tir2_only(130)).unwrap_err();
        assert!(why.contains("--palw-drill-tir2-at") || why.contains("palw_tir_fence2 needs palw_tir_v1"), "{why}");
    }

    /// **The capacity ramp's ready steps are drill flags of their own** (`--palw-drill-capacity-step2-at` ρ = 25,
    /// `-step3-at` ρ = 100 after it, `--palw-drill-capacity-rho100-at` ρ = 100 straight after ρ = 10; ADR-0160 stage 3,
    /// lane C's combined drill arms ρ = 25 and ρ = 100 above a moved ρ = 10 flag day): each is refused without the
    /// salt and, by name rather than by a panic, where F-L does not carry the steps it builds on or the height is not above
    /// the step before; the two ramps are never combined; on the ρ = 10 flag day moved low each appends its step
    /// alone; the marker names them (`capacity_at=`) and a stored chain is never reopened under another set; the
    /// keyring's manifest names them and announces the fingerprint the node on the same command line announces.
    #[test]
    fn the_capacity_ramp_steps_are_drill_flags_of_their_own() {
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let salted = format!("--palw-drill-genesis-salt={SALT}");
        let days = ["--palw-drill-fence-at=6", "--palw-drill-fence2-at=10", "--palw-drill-fence3-at=14"];
        let parsed = |extra: &[&str]| {
            let v: Vec<String> =
                base.iter().copied().chain([salted.as_str()]).chain(days).chain(extra.iter().copied()).map(str::to_owned).collect();
            parse(&v.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let ramp = |c: &Config| -> Vec<(u64, u32)> {
            c.params
                .palw_capacity_aggregate_liability
                .as_ref()
                .map(|v| v.steps.iter().map(|s| (s.from_daa, s.rho)).collect())
                .unwrap_or_default()
        };
        // Refused without the salt, by name.
        for flag in [
            "--palw-drill-capacity-network-room-at=30",
            "--palw-drill-capacity-network-verify-at=30",
            "--palw-drill-capacity-step2-at=30",
            "--palw-drill-capacity-step3-at=40",
            "--palw-drill-capacity-rho100-at=30",
        ] {
            let unsalted = parse(&["--testnet", "--netsuffix=12", flag]);
            let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
            assert!(refused.contains(flag) && refused.contains("--palw-drill-genesis-salt"), "{flag}: {refused}");
        }
        // ρ = 25 then ρ = 100 above the ρ = 10 flag day (14), through the node's own config.
        let both = config_of(&parsed(&["--palw-drill-capacity-step2-at=30", "--palw-drill-capacity-step3-at=40"]));
        assert_eq!(ramp(&both), vec![(14, 10), (30, 25), (40, 100)]);
        both.params.validate_palw_v2().expect("the ramp validates");
        palw_drill_validate_args_v1(&parsed(&["--palw-drill-capacity-step2-at=30", "--palw-drill-capacity-step3-at=40"]))
            .expect("both");
        let step2 = config_of(&parsed(&["--palw-drill-capacity-step2-at=30"]));
        // The release keeps ρ = 100 at its own H + 95: moving ρ = 25 alone leaves that step where the release armed it.
        let rho100 = kaspa_consensus_core::config::params::PALW_T12_INT11_RHO100_DAA.expect("the release's ρ = 100");
        assert_eq!(ramp(&step2), vec![(14, 10), (30, 25), (rho100, 100)]);
        assert_eq!(step2.palw_drill_fence_moves.len() + 1, both.palw_drill_fence_moves.len(), "each flag moves its one step");
        // F-N moves alone, later than the list's 14 (the stage-4 gate timed apart); never below the rooms it needs.
        let room = config_of(&parsed(&["--palw-drill-capacity-network-room-at=50"]));
        assert_eq!(room.params.palw_capacity_network_room, Some(kaspa_consensus_core::config::params::ForkActivation::new(50)));
        assert_eq!(room.palw_drill_fence_moves.len(), step2.palw_drill_fence_moves.len(), "one fence moved");
        // int-11: the static verification term arms at or above F-N (14); below it the ruleset does not validate.
        let verify = config_of(&parsed(&["--palw-drill-capacity-network-verify-at=60"]));
        assert_eq!(verify.params.palw_capacity_network_verify, Some(kaspa_consensus_core::config::params::ForkActivation::new(60)));
        let why = palw_drill_validate_args_v1(&parsed(&["--palw-drill-capacity-network-verify-at=12"])).unwrap_err().to_string();
        assert!(why.contains("--palw-drill-capacity-network-verify-at") && why.contains("does not validate"), "{why}");
        let why = palw_drill_validate_args_v1(&parsed(&["--palw-drill-capacity-network-room-at=12"])).unwrap_err().to_string();
        assert!(why.contains("--palw-drill-capacity-network-room-at") && why.contains("does not validate"), "{why}");
        let straight = config_of(&parsed(&["--palw-drill-capacity-rho100-at=30"]));
        // The release's own third step (ρ = 100 at H + 95) stays: the same ρ, so it changes nothing past 30.
        assert_eq!(ramp(&straight), vec![(14, 10), (30, 100), (rho100, 100)]);
        // Refused by name, never by a panic: step 3 without step 2, a height not above the step before, both ramps.
        let why = palw_drill_validate_args_v1(&parsed(&["--palw-drill-capacity-step3-at=40"])).unwrap_err().to_string();
        // The release carries step 2 at H, so step 3 alone below it is refused for its height (before the release: for the missing step).
        assert!(
            why.contains("--palw-drill-capacity-step3-at")
                && (why.contains("builds on 2 earlier step(s)") || why.contains("must start above step 2")),
            "{why}"
        );
        let why = palw_drill_validate_args_v1(&parsed(&["--palw-drill-capacity-step2-at=14"])).unwrap_err().to_string();
        assert!(why.contains("must start above step 1"), "{why}");
        let why = palw_drill_validate_args_v1(&parsed(&["--palw-drill-capacity-rho100-at=30", "--palw-drill-capacity-step2-at=30"]))
            .unwrap_err()
            .to_string();
        assert!(why.contains("pick one ramp"), "{why}");
        // Without the ρ = 10 flag day moved low the release's F-L (DAA 1,700) is above the step.
        let alone: Vec<&str> = base.iter().copied().chain([salted.as_str(), "--palw-drill-capacity-step2-at=30"]).collect();
        let why = palw_drill_validate_args_v1(&parse(&alone)).unwrap_err().to_string();
        assert!(why.contains("must start above step 1") || why.contains("does not validate"), "{why}");
        // The marker names the steps, and a stored chain is never reopened under another set.
        let dir = root.path().join("capacity/misaka-testnet-12");
        let set = PalwDrillExtraFencesV1 { capacity_step2_at: Some(30), capacity_step3_at: Some(40), ..Default::default() };
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), None, set).expect("a fresh app dir");
        assert!(marker_of(&dir).contains("capacity_at=room:none,verify:none,step2:30,step3:40,rho100:none\n"), "{}", marker_of(&dir));
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), None, set).expect("the same start");
        let moved = PalwDrillExtraFencesV1 { capacity_step3_at: Some(45), ..set };
        let why = palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), None, moved).unwrap_err();
        assert!(
            why.contains("--palw-drill-capacity-network-room-at/-network-verify-at/-step2-at/-step3-at/-rho100-at (recorded room:none,verify:none,step2:30,step3:40,rho100:none, now"),
            "{why}"
        );
        let none = palw_drill_datadir_guard_v3(&dir, Some(&a), "g", Some(6), Some(10), Some(14), None).unwrap_err();
        assert!(none.contains("recorded room:none,verify:none,step2:30,step3:40,rho100:none, now none"), "dropped over a stored chain: {none}");
        // The keyring names them and the fingerprint the node on the same command line announces.
        let keys = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v4(&a, keys.path(), Some(6), Some(10), Some(14), None, set).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["capacity_network_room_at"], serde_json::Value::Null);
        assert_eq!(manifest["capacity_network_verify_at"], serde_json::Value::Null);
        assert_eq!(manifest["capacity_step2_at"], serde_json::json!(30));
        assert_eq!(manifest["capacity_step3_at"], serde_json::json!(40));
        assert_eq!(manifest["capacity_rho100_at"], serde_json::Value::Null);
        assert_eq!(manifest["consensus_params_id"], both.params.consensus_params_id().to_string().as_str());
    }

    /// **The per-model court window is a dormant drill flag of its own** (`--palw-drill-model-court-at`; the release
    /// arms it nowhere, and the DAA-3,600 flag day — `--palw-drill-tir2-at`, `palw_tir_fence2` alone — leaves it so):
    /// the marker is named (`model_court_at=`), a stored chain is never reopened under another one, the keyring's
    /// manifest names it and announces the fingerprint the node on the same command line announces, and the flag is
    /// refused without the salt.
    #[test]
    fn the_model_court_window_is_a_dormant_drill_flag_of_its_own() {
        use kaspa_consensus_core::config::params::ForkActivation;
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let dir = root.path().join("window/misaka-testnet-12");
        let start = |dir: &Path, model: Option<u64>| {
            palw_drill_datadir_guard_v4(
                dir,
                Some(&a),
                "g",
                Some(40),
                Some(60),
                Some(80),
                Some(100),
                PalwDrillExtraFencesV1 { model_court_at: model, ..Default::default() },
            )
        };
        start(&dir, Some(130)).expect("a fresh app dir");
        assert!(marker_of(&dir).contains("model_court_at=130\n"), "{}", marker_of(&dir));
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        start(&dir, Some(130)).expect("the same start");
        let why = start(&dir, Some(140)).unwrap_err();
        assert!(why.contains("--palw-drill-model-court-at (recorded 130, now 140)"), "{why}");
        let why = start(&dir, None).unwrap_err();
        assert!(why.contains("--palw-drill-model-court-at (recorded 130, now none)"), "dropped over a stored chain: {why}");
        // A marker written by an older start (`guard_v3`) counts as started without the flag.
        let old = root.path().join("older/misaka-testnet-12");
        palw_drill_datadir_guard_v3(&old, Some(&a), "g", Some(40), Some(60), Some(80), Some(100)).expect("an older start");
        assert!(marker_of(&old).contains("model_court_at=none\n"), "{}", marker_of(&old));
        std::fs::create_dir_all(old.join("datadir")).unwrap();
        let why = start(&old, Some(130)).unwrap_err();
        assert!(why.contains("--palw-drill-model-court-at (recorded none, now 130)"), "added over a stored chain: {why}");

        // `apply_to_config` moves it after the IR fences — the drill's own heights (flag days 6/10/14, IR fence 20).
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let salted = format!("--palw-drill-genesis-salt={SALT}");
        let days = ["--palw-drill-fence-at=6", "--palw-drill-fence2-at=10", "--palw-drill-fence3-at=14", "--palw-drill-tir-at=20"];
        let argv = |extra: &[&str]| -> Vec<String> {
            base.iter().copied().chain([salted.as_str()]).chain(days).chain(extra.iter().copied()).map(str::to_owned).collect()
        };
        let parsed = |extra: &[&str]| {
            let v = argv(extra);
            parse(&v.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let release = config_of(&parsed(&[]));
        assert_eq!(release.params.palw_model_court_window, None, "dormant on the release drill");
        assert_eq!(release.params.palw_tir_fence2, Some(ForkActivation::new(3_600)), "and fence2 alone is the DAA-3,600 flag day");
        // The DAA-3,600 flag day's drill flag moves fence2 alone and leaves the window dormant.
        let crossing = config_of(&parsed(&["--palw-drill-tir2-at=30"]));
        assert_eq!(crossing.params.palw_tir_fence2, Some(ForkActivation::new(30)));
        assert_eq!(crossing.params.palw_model_court_window, None, "the DAA-3,600 flag day arms no court window");
        assert_eq!(crossing.palw_drill_fence_moves.len(), release.palw_drill_fence_moves.len() + 1, "fence2, and nothing else");
        palw_drill_validate_args_v1(&parsed(&["--palw-drill-tir2-at=30"])).expect("with the IR fence below it");
        // The window flag arms it alone.
        let model_only = config_of(&parsed(&["--palw-drill-model-court-at=30"]));
        assert_eq!(model_only.params.palw_model_court_window, Some(ForkActivation::new(30)));
        assert!(model_only.params.palw_model_court_window_active_at(30) && !model_only.params.palw_model_court_window_active_at(29));
        assert_eq!(model_only.params.palw_tir_fence2, Some(ForkActivation::new(3_600)), "the window flag moves its own fence alone");
        assert_eq!(model_only.palw_drill_fence_moves.len(), release.palw_drill_fence_moves.len() + 1);
        model_only.params.validate_palw_v2().expect("the dormant window armed in a drill validates");
        palw_drill_validate_args_v1(&parsed(&["--palw-drill-model-court-at=30"])).expect("the model window alone");
        // The keyring names it and the fingerprint the node on the same command line announces.
        let keys = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v4(
            &a,
            keys.path(),
            Some(6),
            Some(10),
            Some(14),
            Some(20),
            PalwDrillExtraFencesV1 { model_court_at: Some(30), ..Default::default() },
        )
        .expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["model_court_at"], serde_json::json!(30));
        assert_eq!(manifest["consensus_params_id"], model_only.params.consensus_params_id().to_string().as_str());
        let keys_without = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v3(&a, keys_without.path(), Some(6), Some(10), Some(14), Some(20)).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["model_court_at"], serde_json::Value::Null);
        assert_eq!(manifest["consensus_params_id"], release.params.consensus_params_id().to_string().as_str());
        // Refused without the salt.
        let unsalted = parse(&["--testnet", "--netsuffix=12", "--palw-drill-model-court-at=130"]);
        let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
        assert!(refused.contains("--palw-drill-model-court-at=130") && refused.contains("--palw-drill-genesis-salt"), "{refused}");
    }

    /// **The keyring export is the binary's own answer**: seeds kaspad accepts (0600, the seat's
    /// bond key), outpoints on the drill premine, the drill genesis beside public testnet-12's; a
    /// directory holding another drill's keyring is refused, the same drill's is rewritten.
    #[test]
    fn t53_the_keyring_export_names_the_drill_and_its_seats() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = palw_drill_write_keyring_v1(&salt(), dir.path(), None).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&manifest_path).unwrap()).unwrap();
        assert_eq!(manifest["format"], PALW_DRILL_KEYRING_FORMAT_V1);
        assert_eq!(manifest["salt_id"], salt().id());
        assert_eq!(manifest["network"], "testnet-12");
        let drill_genesis = kaspa_consensus_core::config::drill::palw_t12_drill_genesis_block_v1(&salt()).hash.to_string();
        assert_eq!(manifest["genesis_hash"], drill_genesis.as_str());
        assert_ne!(manifest["genesis_hash"], manifest["public_genesis_hash"]);
        assert_ne!(manifest["consensus_params_id"], manifest["public_consensus_params_id"]);
        let seats = manifest["seats"].as_array().unwrap();
        assert_eq!(seats.len(), kaspa_consensus_core::config::params::PALW_T12_GENESIS_BONDS.len());
        let premine = manifest["premine_txid"].as_str().unwrap();
        for seat in seats {
            let bond = crate::palw_producer::parse_outpoint(seat["bond_outpoint"].as_str().unwrap()).unwrap();
            assert_eq!(bond.transaction_id.to_string(), premine);
            let seed_path = dir.path().join(seat["seed_file"].as_str().unwrap());
            let seed =
                kaspa_pq_validator_core::load_validator_seed(seed_path.to_str().unwrap()).expect("kaspad accepts the seed file");
            let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
            assert_eq!(key.funding_address(Prefix::Testnet).to_string(), seat["address"].as_str().unwrap(), "the seat's own address");
        }
        assert_eq!(manifest["bonds"].as_array().unwrap().len() + seats.len(), PALW_DRILL_EXPORT_PER_ROLE_V1 as usize);
        assert_eq!(manifest["heartbeat"].as_array().unwrap().len(), PALW_DRILL_EXPORT_PER_ROLE_V1 as usize);
        let validator = &manifest["validator"][0];
        let seed =
            kaspa_pq_validator_core::load_validator_seed(dir.path().join(validator["seed_file"].as_str().unwrap()).to_str().unwrap())
                .expect("a validator seed kaspad accepts");
        assert_eq!(
            PalwDrillKeyringV1::new(salt()).find_validator_pubkey(kaspa_pq_validator_core::ValidatorKey::from_seed(seed).public_key()),
            Some(0)
        );
        #[cfg(feature = "evm")]
        {
            let evm = manifest["evm"].as_array().unwrap();
            assert_eq!(evm.len(), PALW_DRILL_EXPORT_PER_ROLE_V1 as usize);
            let accounts = kaspa_evm::tx::palw_drill_evm_accounts_v1(&salt());
            assert_eq!(evm[4]["address"], format!("0x{}", faster_hex::hex_string(&accounts[4])).as_str());
            let key_path = dir.path().join(evm[4]["key_file"].as_str().unwrap());
            let secret = std::fs::read_to_string(&key_path).unwrap();
            assert_eq!(secret, faster_hex::hex_string(&kaspa_consensus_core::config::drill::palw_drill_evm_secret_v1(&salt(), 4)));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777, 0o600, "a secret is owner-only");
            }
        }

        // A drill node configured from the manifest passes the drill-only key rule.
        let seat0 = &seats[0];
        let args = {
            let argv = [
                "--testnet".to_owned(),
                "--netsuffix=12".to_owned(),
                "--nodnsseed".to_owned(),
                "--addpeer=10.0.0.2:26311".to_owned(),
                format!("--palw-drill-genesis-salt={SALT}"),
                format!("--palw-producer-key={}", dir.path().join(seat0["seed_file"].as_str().unwrap()).display()),
                format!("--palw-producer-bond={}", seat0["bond_outpoint"].as_str().unwrap()),
                format!("--palw-fee-outpoint={}", seat0["fee_float_outpoint"].as_str().unwrap()),
                format!("--palw-heartbeat-miner-address={}", manifest["heartbeat"][0]["address"].as_str().unwrap()),
            ];
            let refs: Vec<&str> = argv.iter().map(|s| s.as_str()).collect();
            parse(&refs)
        };
        assert!(palw_drill_validate_args_v1(&args).is_ok(), "{:?}", palw_drill_validate_args_v1(&args));

        palw_drill_write_keyring_v1(&salt(), dir.path(), None).expect("the same drill rewrites its keyring");
        let other = PalwDrillSaltV1::from_bytes([0x35; 32]).unwrap();
        let why = palw_drill_write_keyring_v1(&other, dir.path(), None).unwrap_err();
        assert!(why.contains("another drill's keyring"), "{why}");

        // The manifest names the fingerprint the node on the same command line announces: the release
        // drill's without the flag, the crossing drill's with it (the int-4 audit of 2026-09-26).
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let salted = format!("--palw-drill-genesis-salt={SALT}");
        let announced = |extra: &[&str]| {
            let argv: Vec<&str> = base.iter().copied().chain([salted.as_str()]).chain(extra.iter().copied()).collect();
            config_of(&parse(&argv)).params.consensus_params_id().to_string()
        };
        assert_eq!(manifest["consensus_params_id"], announced(&[]).as_str(), "the release drill's fingerprint");
        assert!(manifest["fence_at"].is_null(), "no flag day");
        let crossing_dir = tempfile::tempdir().unwrap();
        let crossing_path = palw_drill_write_keyring_v1(&salt(), crossing_dir.path(), Some(40)).expect("written");
        let crossing: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&crossing_path).unwrap()).unwrap();
        assert_eq!(crossing["consensus_params_id"], announced(&["--palw-drill-fence-at=40"]).as_str(), "the crossing drill's");
        assert_ne!(crossing["consensus_params_id"], manifest["consensus_params_id"]);
        assert_eq!(crossing["fence_at"], 40);
        assert_eq!(crossing["genesis_hash"], manifest["genesis_hash"], "the salt's genesis either way");
        let why = palw_drill_write_keyring_v1(&salt(), crossing_dir.path(), Some(0)).unwrap_err();
        assert!(why.contains("--palw-drill-fence-at"), "{why}");
    }

    /// **`--palw-drill-fence-at`: a salted testnet-12 drill only, refused by name everywhere else, and
    /// never beside a params override.** Accepted with the salt; refused without it (public
    /// testnet-12 and every other network alike), with the salt off testnet-12, at 0, at another
    /// fence's height (the fork id would not see it); `--override-params-file` stays refused with it.
    /// Wired into the start-up check every entry point runs, command line only.
    #[test]
    fn the_drill_fence_flag_is_a_salted_drills_alone() {
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        let with = |extra: &[&str]| {
            let mut argv: Vec<&str> = base.to_vec();
            argv.extend_from_slice(extra);
            parse(&argv)
        };
        assert!(palw_drill_validate_args_v1(&with(&[&flag, "--palw-drill-fence-at=40"])).is_ok(), "a drill crossing DAA 40");
        assert_eq!(with(&[&flag, "--palw-drill-fence-at=40"]).palw_drill_fence_at, Some(40));
        assert_eq!(with(&[&flag]).palw_drill_fence_at, None, "no flag, no move");

        for argv in [
            vec!["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311", "--palw-drill-fence-at=40"],
            vec!["--testnet", "--netsuffix=12", "--palw-drill-fence-at=40"],
            vec!["--testnet", "--netsuffix=11", "--palw-drill-fence-at=40"],
            vec!["--devnet", "--palw-drill-fence-at=40"],
            vec!["--simnet", "--palw-drill-fence-at=40"],
            vec!["--palw-drill-fence-at=40"],
        ] {
            let why = refusal(&parse(&argv));
            assert!(why.contains("needs --palw-drill-genesis-salt"), "{argv:?}: {why}");
        }
        let why = refusal(&parse(&["--testnet", "--netsuffix=10", "--nodnsseed", "--addpeer=10.0.0.2:26311", &flag, "--palw-drill-fence-at=40"]));
        assert!(why.contains("testnet-12 only"), "{why}");
        assert!(refusal(&with(&[&flag, "--palw-drill-fence-at=0"])).contains("not genesis"));
        let drill = kaspa_consensus_core::config::params::palw_t12_drill_params_v1(&salt());
        let (other, height) = drill
            .palw_fences_v1()
            .into_iter()
            .filter(|(name, _)| kaspa_consensus_core::config::params::PALW_T12_POST_LAUNCH_FENCES_V1.iter().all(|f| f.name != *name))
            .find_map(|(name, fence)| fence.map(|f| (name, f.daa_score())).filter(|(_, h)| *h != 0 && *h != u64::MAX))
            .expect("testnet-12 schedules a fence past genesis");
        let why = refusal(&with(&[&flag, &format!("--palw-drill-fence-at={height}")]));
        assert!(why.contains(other) && why.contains("fork id"), "{why}");
        let why = refusal(&with(&[&flag, "--palw-drill-fence-at=40", "--override-params-file=/tmp/p.json"]));
        assert!(why.contains("--override-params-file") && why.contains("--palw-drill-fence-at"), "{why}");

        assert!(matches!(
            crate::daemon::validate_args(&parse(&["--testnet", "--netsuffix=12", "--palw-drill-fence-at=40"])),
            Err(ConfigError::PalwDrillRefused(_))
        ));
        let help = crate::args::cli().render_long_help().to_string();
        assert!(help.contains("--palw-drill-fence-at"));
        assert!(!help.contains("KASPAD_PALW_DRILL_FENCE_AT"), "never from the environment");
        let from_file: Result<Args, _> = toml::from_str("palw-drill-fence-at = 40");
        assert!(from_file.is_err(), "never from a config file");
        let cmd = crate::args::cli();
        let text = cmd.get_arguments().find(|a| a.get_id() == "palw-drill-fence-at").and_then(|a| a.get_help()).unwrap().to_string();
        assert!(!text.contains("  "), "--palw-drill-fence-at's help has a run of spaces: {text}");
    }

    /// **The config a drill crossing the flag day runs, and what it prints.** The drill genesis, every
    /// post-launch fence at the flag's height with the moves recorded beside it, a params id and a
    /// schedule id that are not the release drill's; one printed line per fence; and nothing for a node
    /// without the flag — public testnet-12's config never carries a move.
    #[test]
    fn a_drill_crossing_the_flag_day_runs_and_prints_the_moved_fences() {
        use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1};
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        let release_drill = config_of(&parse(&base.iter().copied().chain([flag.as_str()]).collect::<Vec<_>>()));
        let crossing =
            config_of(&parse(&base.iter().copied().chain([flag.as_str(), "--palw-drill-fence-at=40"]).collect::<Vec<_>>()));
        assert_eq!(crossing.params.genesis.hash, release_drill.params.genesis.hash, "the salt's genesis");
        assert_eq!(crossing.palw_drill_genesis_salt, Some(salt()));
        assert_eq!(crossing.palw_drill_fence_moves.len(), PALW_T12_POST_LAUNCH_FENCES_V1.len());
        let fences = crossing.params.palw_fences_v1();
        for fence in PALW_T12_POST_LAUNCH_FENCES_V1 {
            let at = fences.iter().find(|(name, _)| *name == fence.name).unwrap().1;
            assert_eq!(at, Some(ForkActivation::new(40)), "{}", fence.name);
        }
        assert_ne!(crossing.params.consensus_params_id(), release_drill.params.consensus_params_id());
        assert_ne!(crossing.params.consensus_schedule_id(), release_drill.params.consensus_schedule_id());
        crossing.params.validate_palw_v2().expect("the crossing drill validates");

        let lines = palw_drill_fence_lines_v1(&crossing);
        assert_eq!(lines.len(), 1 + PALW_T12_POST_LAUNCH_FENCES_V1.len(), "{lines:#?}");
        assert!(lines[0].contains(&crossing.params.consensus_params_id().to_string()), "{}", lines[0]);
        for (line, fence) in lines[1..].iter().zip(PALW_T12_POST_LAUNCH_FENCES_V1) {
            assert!(line.contains(fence.name) && line.contains("DAA 40"), "{line}");
        }
        assert!(palw_drill_fence_lines_v1(&release_drill).is_empty(), "no flag, nothing printed");
        let public = config_of(&parse(&base));
        assert!(public.palw_drill_fence_moves.is_empty() && palw_drill_fence_lines_v1(&public).is_empty());
        assert_eq!(
            public.params.consensus_params_id(),
            kaspa_consensus_core::config::params::Params::from(palw_drill_network_v1()).consensus_params_id(),
            "public testnet-12 is the release"
        );
    }

    // ---- lane F2 (2026-09-27): the second post-launch flag day, `--palw-drill-fence2-at` ----------------

    /// **`--palw-drill-fence2-at`: a salted testnet-12 drill only, refused by name everywhere else.**
    /// Accepted with the salt, alone or after `--palw-drill-fence-at` at a height of its own; refused
    /// without the salt (public testnet-12 and every other network alike), with the salt off testnet-12,
    /// at 0, and at a height the first flag day's fences take on the same command line (the fork id names
    /// heights, not fences). Wired into the start-up check every entry point runs; command line only.
    #[test]
    fn the_second_flag_day_flag_is_a_salted_drills_alone() {
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        let with = |extra: &[&str]| {
            let mut argv: Vec<&str> = base.to_vec();
            argv.extend_from_slice(extra);
            parse(&argv)
        };
        // Alone below the first flag day's height the second is refused at start-up: lane F2-lock's retroactive
        // lock life needs lane V02's lock life (a DAA-750 fence) at or below it — move the first flag day too.
        let alone_low = palw_drill_validate_args_v1(&with(&[&flag, "--palw-drill-fence2-at=60"])).unwrap_err();
        assert!(alone_low.to_string().contains("palw_final_lock_life at or below it"), "{alone_low}");
        assert!(palw_drill_validate_args_v1(&with(&[&flag, "--palw-drill-fence2-at=800"])).is_ok(), "alone, above the first flag day");
        assert!(
            palw_drill_validate_args_v1(&with(&[&flag, "--palw-drill-fence-at=40", "--palw-drill-fence2-at=60"])).is_ok(),
            "a drill crossing both flag days"
        );
        assert_eq!(with(&[&flag, "--palw-drill-fence2-at=60"]).palw_drill_fence2_at, Some(60));
        assert_eq!(with(&[&flag]).palw_drill_fence2_at, None, "no flag, no move");

        for argv in [
            vec!["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311", "--palw-drill-fence2-at=60"],
            vec!["--testnet", "--netsuffix=12", "--palw-drill-fence2-at=60"],
            vec!["--testnet", "--netsuffix=11", "--palw-drill-fence2-at=60"],
            vec!["--devnet", "--palw-drill-fence2-at=60"],
            vec!["--simnet", "--palw-drill-fence2-at=60"],
            vec!["--palw-drill-fence2-at=60"],
        ] {
            let why = refusal(&parse(&argv));
            assert!(why.contains("--palw-drill-fence2-at=60") && why.contains("needs --palw-drill-genesis-salt"), "{argv:?}: {why}");
        }
        let why = refusal(&parse(&[
            "--testnet",
            "--netsuffix=10",
            "--nodnsseed",
            "--addpeer=10.0.0.2:26311",
            &flag,
            "--palw-drill-fence2-at=60",
        ]));
        assert!(why.contains("testnet-12 only"), "{why}");
        assert!(refusal(&with(&[&flag, "--palw-drill-fence2-at=0"])).contains("not genesis"));
        let why = refusal(&with(&[&flag, "--palw-drill-fence-at=40", "--palw-drill-fence2-at=40"]));
        assert!(why.contains("--palw-drill-fence2-at=40") && why.contains("fork id"), "the first flag day's height: {why}");
        let why = refusal(&with(&[&flag, "--palw-drill-fence2-at=750"]));
        assert!(why.contains("--palw-drill-fence2-at=750") && why.contains("fork id"), "the release's own DAA-750 height: {why}");

        assert!(matches!(
            crate::daemon::validate_args(&parse(&["--testnet", "--netsuffix=12", "--palw-drill-fence2-at=60"])),
            Err(ConfigError::PalwDrillRefused(_))
        ));
        let help = crate::args::cli().render_long_help().to_string();
        assert!(help.contains("--palw-drill-fence2-at"));
        assert!(!help.contains("KASPAD_PALW_DRILL_FENCE2_AT"), "never from the environment");
        let from_file: Result<Args, _> = toml::from_str("palw-drill-fence2-at = 60");
        assert!(from_file.is_err(), "never from a config file");
        let cmd = crate::args::cli();
        let text = cmd.get_arguments().find(|a| a.get_id() == "palw-drill-fence2-at").and_then(|a| a.get_help()).unwrap().to_string();
        assert!(!text.contains("  "), "--palw-drill-fence2-at's help has a run of spaces: {text}");
    }

    /// **The config a drill crossing both flag days runs, and what it prints.** The first flag day's
    /// fences at `--palw-drill-fence-at`, every fence of `PALW_T12_POST_LAUNCH_FENCES_V2` (lane F2's
    /// `palw_floor_refusal_retry` among them, with the fold's mirror) at `--palw-drill-fence2-at`, the
    /// moves of both recorded and printed, a params id that is neither the release drill's nor the
    /// first-crossing drill's — and the keyring manifest names the same fingerprint. Public testnet-12
    /// never carries a move.
    #[test]
    fn a_drill_crossing_the_second_flag_day_runs_and_prints_it() {
        use kaspa_consensus_core::config::params::{ForkActivation, PALW_T12_POST_LAUNCH_FENCES_V1, PALW_T12_POST_LAUNCH_FENCES_V2};
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let flag = format!("--palw-drill-genesis-salt={SALT}");
        let config = |extra: &[&str]| {
            config_of(&parse(&base.iter().copied().chain([flag.as_str()]).chain(extra.iter().copied()).collect::<Vec<_>>()))
        };
        let first = config(&["--palw-drill-fence-at=40"]);
        let both = config(&["--palw-drill-fence-at=40", "--palw-drill-fence2-at=60"]);
        let second = config(&["--palw-drill-fence2-at=800"]);
        assert_eq!(both.params.genesis.hash, first.params.genesis.hash, "the salt's genesis");
        assert_eq!(both.palw_drill_fence_moves.len(), PALW_T12_POST_LAUNCH_FENCES_V1.len() + PALW_T12_POST_LAUNCH_FENCES_V2.len());
        assert_eq!(second.palw_drill_fence_moves.len(), PALW_T12_POST_LAUNCH_FENCES_V2.len());
        let fences = both.params.palw_fences_v1();
        for (list, at) in [(PALW_T12_POST_LAUNCH_FENCES_V1, 40u64), (PALW_T12_POST_LAUNCH_FENCES_V2, 60)] {
            for fence in list {
                let set = fences.iter().find(|(name, _)| *name == fence.name).unwrap().1;
                assert_eq!(set, Some(ForkActivation::new(at)), "{}", fence.name);
            }
        }
        assert_eq!(both.params.palw_floor_refusal_retry, Some(ForkActivation::new(60)), "lane F2's fence rides the second list");
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &both.params.palw_consensus_mode else {
            panic!("testnet-12 is ConsensusV2")
        };
        assert_eq!(bundle.state.floor_refusal_retry_from_daa(), Some(60), "…with the mirror the fold reads");
        both.params.validate_palw_v2().expect("the drill crossing both flag days validates");
        assert_ne!(both.params.consensus_params_id(), first.params.consensus_params_id());
        assert_ne!(both.params.consensus_schedule_id(), first.params.consensus_schedule_id());

        let lines = palw_drill_fence_lines_v1(&both);
        assert_eq!(lines.len(), 1 + both.palw_drill_fence_moves.len(), "{lines:#?}");
        assert!(lines[0].contains("--palw-drill-fence2-at") && lines[0].contains(&both.params.consensus_params_id().to_string()));
        // Public testnet-12 ships the second flag day armed at its own height, so the drill MOVES it to 60.
        assert!(lines.iter().any(|line| line.contains("palw_floor_refusal_retry") && line.contains("DAA 60")), "{lines:#?}");
        assert!(lines.iter().any(|line| line.contains("palw_final_lock_life_retro") && line.contains("DAA 60")), "{lines:#?}");

        let public = config_of(&parse(&base));
        assert!(public.palw_drill_fence_moves.is_empty());
        assert_eq!(
            public.params.palw_floor_refusal_retry,
            kaspa_consensus_core::config::params::PALW_T12_POST_LAUNCH_FENCE_V2_DAA.map(ForkActivation::new),
            "public testnet-12 ships the second flag day at its own height"
        );

        // The keyring export names the fingerprint the node on the same command line announces.
        let dir = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v2(&salt(), dir.path(), Some(40), Some(60), None).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["consensus_params_id"], both.params.consensus_params_id().to_string().as_str());
        assert_eq!((manifest["fence_at"].clone(), manifest["fence2_at"].clone()), (serde_json::json!(40), serde_json::json!(60)));
        let why = palw_drill_write_keyring_v2(&salt(), dir.path(), Some(40), Some(40), None).unwrap_err();
        assert!(why.contains("--palw-drill-fence2-at"), "{why}");
    }

    /// **A stored drill chain is never reopened under another second flag day**, as for the first: the
    /// marker names `fence2_at=`, a chain started with `--palw-drill-fence2-at=60` is refused without it
    /// or at another height, one started without it is refused with it, and a directory with no chain
    /// stored yet takes the new pair. A marker from before the flag counts as started without it.
    #[test]
    fn a_drill_chain_is_never_reopened_under_another_second_flag_day() {
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let crossing = root.path().join("crossing/misaka-testnet-12");
        palw_drill_datadir_guard_v2(&crossing, Some(&a), "g", Some(40), Some(60), None).expect("a fresh app dir takes the drill");
        assert!(marker_of(&crossing).contains("fence_at=40\n") && marker_of(&crossing).contains("fence2_at=60\n"));
        std::fs::create_dir_all(crossing.join("datadir")).unwrap();
        palw_drill_datadir_guard_v2(&crossing, Some(&a), "g", Some(40), Some(60), None).expect("the same pair across restarts");
        for (at2, wording) in [(None, "without --palw-drill-fence2-at"), (Some(61), "with --palw-drill-fence2-at=61")] {
            let why = palw_drill_datadir_guard_v2(&crossing, Some(&a), "g", Some(40), at2, None).unwrap_err();
            assert!(why.contains("with --palw-drill-fence2-at=60") && why.contains(wording), "{why}");
        }
        assert!(palw_drill_datadir_guard_v1(&crossing, Some(&a), "g", Some(40)).is_err(), "the first flag day alone reopens nothing");
        // A marker written before the flag (no `fence2_at=` line) is a chain started without it.
        let old = root.path().join("old/misaka-testnet-12");
        std::fs::create_dir_all(old.join("datadir")).unwrap();
        std::fs::write(old.join(PALW_DRILL_DATADIR_MARKER_V1), format!("salt_id={}\ngenesis=g\nfence_at=40\n", a.id())).unwrap();
        palw_drill_datadir_guard_v2(&old, Some(&a), "g", Some(40), None, None).expect("started and restarted without it");
        let why = palw_drill_datadir_guard_v2(&old, Some(&a), "g", Some(40), Some(60), None).unwrap_err();
        assert!(why.contains("without --palw-drill-fence2-at") && why.contains("with --palw-drill-fence2-at=60"), "{why}");
        // Nothing stored yet: the new pair is taken.
        let fresh = root.path().join("fresh/misaka-testnet-12");
        palw_drill_datadir_guard_v2(&fresh, Some(&a), "g", None, None, None).expect("created");
        palw_drill_datadir_guard_v2(&fresh, Some(&a), "g", None, Some(60), None).expect("no chain stored yet: the new flag day");
        assert!(marker_of(&fresh).contains("fence2_at=60\n"));
    }

    /// **The four later fences ride the marker and the keyring like a flag day** (`--palw-drill-tir2-at`,
    /// `--palw-drill-gen-at`, `--palw-drill-fp-v5-at`, `--palw-drill-improve-at`): each is refused without
    /// the salt, a stored chain is never reopened under another set, and the improvement fence is refused
    /// by name unless its prerequisites stand at or below it.
    #[test]
    fn the_rfc3_and_rfc4_fences_ride_the_marker_and_are_refused_without_their_prerequisites() {
        let a = salt();
        for flag in [
            "--palw-drill-tir2-at=100",
            "--palw-drill-gen-at=100",
            "--palw-drill-decode-rules-at=100",
            "--palw-drill-fp-v5-at=100",
            "--palw-drill-held-chunks-at=100",
            "--palw-drill-improve-at=100",
        ] {
            let unsalted = parse(&["--testnet", "--netsuffix=12", flag]);
            let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
            assert!(refused.contains(flag) && refused.contains("--palw-drill-genesis-salt"), "{flag}: {refused}");
        }
        // The improvement fence alone is refused: its prerequisites are not in force.
        let alone = parse(&[
            "--testnet",
            "--netsuffix=12",
            &format!("--palw-drill-genesis-salt={SALT}"),
            "--nodnsseed",
            "--addpeer=127.0.0.1:1",
            "--palw-drill-improve-at=100",
        ]);
        let why = palw_drill_validate_args_v1(&alone).unwrap_err().to_string();
        assert!(why.contains("--palw-drill-improve-at"), "{why}");
        // The marker names the set and refuses another over a stored chain.
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("x/misaka-testnet-12");
        let set = PalwDrillExtraFencesV1 {
            tir2_at: Some(24),
            model_court_at: Some(26),
            gen_at: Some(28),
            decode_rules_at: Some(32),
            fp_v5_at: None,
            improve_at: Some(36),
            ..Default::default()
        };
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), set).expect("created");
        let marker = std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        // The release line's two flags keep the marker lines that line writes; the capacity ramp's steps share `capacity_at=` (none
        // here); RFC-0003 / RFC-0004's four share `extra_at=`; and the flags after it have a line each (`held_chunks_at=`).
        assert!(
            marker.contains(
                "tir2_at=24\nmodel_court_at=26\ncapacity_at=none\nextra_at=gen:28,decode_rules:32,fp_v5:none,improve:36\nheld_chunks_at=none\n"
            ),
            "{marker}"
        );
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), set).expect("kept");
        let moved = PalwDrillExtraFencesV1 { improve_at: Some(40), ..set };
        let why = palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), moved).unwrap_err();
        assert!(why.contains("recorded gen:28,decode_rules:32,fp_v5:none,improve:36"), "{why}");
        let moved_tir2 = PalwDrillExtraFencesV1 { tir2_at: Some(25), ..set };
        let why = palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), moved_tir2).unwrap_err();
        assert!(why.contains("--palw-drill-tir2-at (recorded 24, now 25)"), "{why}");
        // v3's callers (no extras) keep what they had.
        let v3 = root.path().join("y/misaka-testnet-12");
        palw_drill_datadir_guard_v3(&v3, Some(&a), "g", None, None, None, None).expect("created");
        assert!(std::fs::read_to_string(v3.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap().contains("extra_at=none\n"));
    }

    /// **RFC-0003's held leaf challenge is a drill flag of its own** (`--palw-drill-held-chunks-at`, decision 22,
    /// object tag 90): refused without the salt, and — by the ruleset's own check, never a panic — without
    /// `palw_tir_v1` at or below it; with the IR fence in force it ARMS `palw_held_close_chunks_v1` at its height and
    /// moves nothing else; the marker keeps it on a line of its own (`held_chunks_at=`, `none` where a marker written
    /// before it has none, so no earlier line changes its text) and a stored chain is never reopened under another
    /// height; the keyring's manifest names it and announces the fingerprint the node on the same command line announces.
    #[test]
    fn the_held_leaf_challenge_is_a_drill_flag_of_its_own() {
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let salted = format!("--palw-drill-genesis-salt={SALT}");
        let days = ["--palw-drill-fence-at=6", "--palw-drill-fence2-at=10", "--palw-drill-fence3-at=14"];
        let parsed = |extra: &[&str]| {
            let v: Vec<String> =
                base.iter().copied().chain([salted.as_str()]).chain(days).chain(extra.iter().copied()).map(str::to_owned).collect();
            parse(&v.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let unsalted = parse(&["--testnet", "--netsuffix=12", "--palw-drill-held-chunks-at=140"]);
        let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
        assert!(refused.contains("--palw-drill-held-chunks-at=140") && refused.contains("--palw-drill-genesis-salt"), "{refused}");
        // Below the IR fence the ruleset does not validate: refused by name.
        let why = palw_drill_validate_args_v1(&parsed(&["--palw-drill-held-chunks-at=140"])).unwrap_err().to_string();
        assert!(why.contains("--palw-drill-held-chunks-at"), "{why}");
        // With the IR fence below it the flag arms the fence and moves nothing else.
        let without = config_of(&parsed(&["--palw-drill-tir-at=20"]));
        let with = config_of(&parsed(&["--palw-drill-tir-at=20", "--palw-drill-held-chunks-at=140"]));
        // The release arms the held leaf challenge at H; the flag moves it to the drill's height.
        let h = kaspa_consensus_core::config::params::PALW_T12_INT11_FLAG_DAY_DAA.expect("the int-11 flag day");
        assert_eq!(without.params.palw_held_close_chunks_v1, Some(kaspa_consensus_core::config::params::ForkActivation::new(h)));
        assert_eq!(
            with.params.palw_held_close_chunks_v1,
            Some(kaspa_consensus_core::config::params::ForkActivation::new(140)),
            "armed at its height"
        );
        assert_eq!(with.palw_drill_fence_moves.len(), without.palw_drill_fence_moves.len() + 1, "one fence moved");
        with.params.validate_palw_v2().expect("the drill with the held leaf challenge validates");
        assert_ne!(with.params.consensus_params_id(), without.params.consensus_params_id(), "the fence moves the fingerprint");
        palw_drill_validate_args_v1(&parsed(&["--palw-drill-tir-at=20", "--palw-drill-held-chunks-at=140"])).expect("with the IR fence");
        // The marker's own line, and a stored chain is never reopened under another height.
        let dir = root.path().join("x/misaka-testnet-12");
        let set = PalwDrillExtraFencesV1 { held_chunks_at: Some(140), ..Default::default() };
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), set).expect("created");
        assert!(marker_of(&dir).contains("extra_at=none\nheld_chunks_at=140\n"), "{}", marker_of(&dir));
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), set).expect("kept");
        let moved = PalwDrillExtraFencesV1 { held_chunks_at: Some(150), ..Default::default() };
        let why = palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), moved).unwrap_err();
        assert!(why.contains("--palw-drill-held-chunks-at (recorded 140, now 150)"), "{why}");
        let dropped = PalwDrillExtraFencesV1::default();
        let why = palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), dropped).unwrap_err();
        assert!(why.contains("--palw-drill-held-chunks-at (recorded 140, now none)"), "{why}");
        // A marker written before the flag existed counts as `none`: the release line's chain reopens under this build.
        let old = root.path().join("y/misaka-testnet-12");
        palw_drill_datadir_guard_v3(&old, Some(&a), "g", Some(6), Some(10), Some(14), Some(20)).expect("created");
        let text = marker_of(&old).replace("held_chunks_at=none\n", "");
        std::fs::write(old.join(PALW_DRILL_DATADIR_MARKER_V1), text).unwrap();
        std::fs::create_dir_all(old.join("datadir")).unwrap();
        palw_drill_datadir_guard_v3(&old, Some(&a), "g", Some(6), Some(10), Some(14), Some(20)).expect("an older marker is none");
        // The keyring's manifest names it and announces the fingerprint of the same command line.
        let keys = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v4(&a, keys.path(), Some(6), Some(10), Some(14), Some(20), set).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["held_chunks_at"], serde_json::json!(140));
        assert_eq!(manifest["consensus_params_id"], with.params.consensus_params_id().to_string().as_str());
        let keys_without = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v3(&a, keys_without.path(), Some(6), Some(10), Some(14), Some(20)).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["held_chunks_at"], serde_json::Value::Null);
    }

    /// **RFC-0007's verification vertex is a drill flag of its own** (`--palw-drill-vertex-at`, object tags 100 and 101): refused without
    /// the salt; with the salt it ARMS `palw_verification_vertex_v1` at its height and moves nothing else (its prerequisites are armed at
    /// the drill's genesis); the marker keeps it on a line of its own (`vertex_at=`, `none` where a marker written before it has none) and
    /// a stored chain is never reopened under another height; the keyring's manifest names it.
    #[test]
    fn the_verification_vertex_is_a_drill_flag_of_its_own() {
        let a = salt();
        let root = tempfile::tempdir().unwrap();
        let marker_of = |dir: &Path| std::fs::read_to_string(dir.join(PALW_DRILL_DATADIR_MARKER_V1)).unwrap();
        let base = ["--testnet", "--netsuffix=12", "--nodnsseed", "--addpeer=10.0.0.2:26311"];
        let salted = format!("--palw-drill-genesis-salt={SALT}");
        let days = ["--palw-drill-fence-at=6", "--palw-drill-fence2-at=10", "--palw-drill-fence3-at=14"];
        let parsed = |extra: &[&str]| {
            let v: Vec<String> =
                base.iter().copied().chain([salted.as_str()]).chain(days).chain(extra.iter().copied()).map(str::to_owned).collect();
            parse(&v.iter().map(String::as_str).collect::<Vec<_>>())
        };
        let unsalted = parse(&["--testnet", "--netsuffix=12", "--palw-drill-vertex-at=140"]);
        let refused = palw_drill_validate_args_v1(&unsalted).unwrap_err().to_string();
        assert!(refused.contains("--palw-drill-vertex-at=140") && refused.contains("--palw-drill-genesis-salt"), "{refused}");
        let without = config_of(&parsed(&[]));
        let with = config_of(&parsed(&["--palw-drill-vertex-at=140"]));
        assert_eq!(without.params.palw_verification_vertex_v1, None);
        assert_eq!(
            with.params.palw_verification_vertex_v1,
            Some(kaspa_consensus_core::config::params::ForkActivation::new(140)),
            "armed at its height"
        );
        assert_eq!(with.palw_drill_fence_moves.len(), without.palw_drill_fence_moves.len() + 1, "one fence moved");
        with.params.validate_palw_v2().expect("the drill with the verification vertex validates");
        assert_ne!(with.params.consensus_params_id(), without.params.consensus_params_id(), "the fence moves the fingerprint");
        assert_eq!(
            with.params.palw_verification_vertex_fence().map(|f| f.daa_score()),
            Some(140),
            "and the node's own reader sees it (the seat speaks in vertices from there)"
        );
        palw_drill_validate_args_v1(&parsed(&["--palw-drill-vertex-at=140"])).expect("the flag validates on a salted drill");
        // The marker's own line, and a stored chain is never reopened under another height.
        let dir = root.path().join("x/misaka-testnet-12");
        let set = PalwDrillExtraFencesV1 { vertex_at: Some(140), ..Default::default() };
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), set).expect("created");
        assert!(marker_of(&dir).contains("vertex_at=140\n"), "{}", marker_of(&dir));
        std::fs::create_dir_all(dir.join("datadir")).unwrap();
        palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), set).expect("kept");
        let moved = PalwDrillExtraFencesV1 { vertex_at: Some(150), ..Default::default() };
        let why = palw_drill_datadir_guard_v4(&dir, Some(&a), "g", Some(6), Some(10), Some(14), Some(20), moved).unwrap_err();
        assert!(why.contains("--palw-drill-vertex-at (recorded 140, now 150)"), "{why}");
        // A marker written before the flag existed counts as `none`.
        let old = root.path().join("y/misaka-testnet-12");
        palw_drill_datadir_guard_v3(&old, Some(&a), "g", Some(6), Some(10), Some(14), Some(20)).expect("created");
        let text = marker_of(&old).replace("vertex_at=none\n", "");
        std::fs::write(old.join(PALW_DRILL_DATADIR_MARKER_V1), text).unwrap();
        std::fs::create_dir_all(old.join("datadir")).unwrap();
        palw_drill_datadir_guard_v3(&old, Some(&a), "g", Some(6), Some(10), Some(14), Some(20)).expect("an older marker is none");
        // The keyring's manifest names it.
        let keys = tempfile::tempdir().unwrap();
        let path = palw_drill_write_keyring_v4(&a, keys.path(), Some(6), Some(10), Some(14), None, set).expect("written");
        let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(manifest["vertex_at"], serde_json::json!(140));
        assert_eq!(manifest["consensus_params_id"], with.params.consensus_params_id().to_string().as_str());
    }
}

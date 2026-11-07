//! **RFC-0002 Phase F, step F6 (decision 11): testnet-12's IR ceilings, measured.** The Qwen2.5 A16
//! decoders as IR programs (`a16_mirror_program`, the small history bound), admitted by
//! `tir_admit_v1` with no per-tile ceiling: what each costs in the quantities the fence bounds.
//! Printed as the measurement the ceilings are set from (`--nocapture`), and held against them.

use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_3B};
use kaspa_consensus_core::palw_tir_v1::PALW_T12_TIR_CEILINGS_V1;
use misaka_palw_base0::artifact::Base0ShapeV1;
use misaka_palw_base0::tir_a16::a16_mirror_program;
use misaka_palw_tir::admit::{TirAdmitInputsV1, TirCeilingsV1, tir_admit_v1};

fn shape(g: PalwQwen25GeometryV1) -> Base0ShapeV1 {
    Base0ShapeV1 {
        n_layers: g.layer_count as usize,
        n_heads: g.attn_heads as usize,
        n_kv_heads: g.attn_kv_heads as usize,
        d_head: g.attn_head_dim as usize,
        d_ff: g.ffn_dim as usize,
        vocab: g.vocab_size as usize,
        max_position: g.n_ctx as usize,
        ln_theta_gen_q: 0,
        eps_q: g.rms_eps_q,
    }
}

#[test]
fn the_qwen25_a16_programs_against_testnet_12_s_ir_ceilings() {
    for (name, g) in [("Qwen2.5-1.5B", QWEN25_1_5B), ("Qwen2.5-3B", QWEN25_3B)] {
        let program = a16_mirror_program(&shape(g), misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL).expect("the A16 program");
        let bytes = program.encode();
        let unrolled: u64 = program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len() as u64).sum();
        let ceilings = TirCeilingsV1 {
            max_tile_macs: u64::MAX,
            max_tile_transcendentals: u64::MAX,
            max_tile_opened_bytes: u64::MAX,
            max_tile_operands: u64::MAX,
            max_position_macs: u64::MAX,
            max_position_transcendentals: u64::MAX,
            max_state_bytes: u64::MAX,
            max_step_leaves: u64::MAX,
            max_checkpoint_interval: 1 << 16,
            max_cone_work: 1 << 20,
        };
        let a = tir_admit_v1(&bytes, &TirAdmitInputsV1 { tile_len: 128, h_chunk: 256, ceilings }).expect("admitted without ceilings");
        let h_cones = a.cones.iter().filter(|c| !c.h_reductions.is_empty()).count();
        let worst_whole = a.cones.iter().map(|c| c.tile.macs).max().unwrap_or(0);
        let worst_non_h = a.cones.iter().filter(|c| c.h_reductions.is_empty()).map(|c| c.tile.macs).max().unwrap_or(0);
        eprintln!(
            "{name}: program {} B, unrolled nodes {unrolled}, position MACs {}, state bytes {}, peak live {} B, cone work {}, \
             commit points {} ({h_cones} reduce over H), worst tile MACs {worst_whole} (non-H {worst_non_h})",
            bytes.len(),
            a.position.cost.macs,
            a.position.state_bytes,
            a.position.peak_live_bytes,
            a.cone_work,
            a.cones.len(),
        );
        // Both decoders sit inside testnet-12's ceilings, with the headroom the fence documents.
        let c = PALW_T12_TIR_CEILINGS_V1;
        assert!(bytes.len() as u64 <= c.max_program_bytes as u64, "{name}: the program fits one carrier");
        assert!(unrolled * 4 <= c.max_unrolled_nodes as u64, "{name}");
        assert!(a.position.cost.macs * 3 <= c.max_macs_per_position, "{name}");
        assert!(a.position.state_bytes * 3 <= c.max_state_bytes, "{name}");
        assert!(a.position.peak_live_bytes * 4 <= c.max_peak_live_bytes, "{name}");
        assert!(a.cone_work * 64 <= c.max_cone_work, "{name}");
        assert_eq!(c.max_context, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    }
}

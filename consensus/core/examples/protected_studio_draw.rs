//! Replay the reported private TFO3 draw and signed Court disclosure from a
//! saved native state carriage. The selected-chain facts are sourced from the
//! independent peer reads in the report.
use kaspa_consensus_core::{
    Hash64,
    palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    palw_da_rcore_v1::{
        PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT, PalwDaAnswerV1, PalwDaUnitV1, palw_da_answer_digest_v1, palw_da_disclosure_message_v4,
    },
    palw_fp_beacon_v3::{PalwChainBlockFactV3, derive_beacon_fact_v3},
    palw_freeprompt_v3::{
        fp_draw_slot_v3, fp_network_quantum_v1, fp_pooled_receipt_key_v1, fp_quantum_target_of_compute_v1, fp_quantum_ticket_v3,
    },
    palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2,
    palw_pwu::palw_ticket_admits_v1,
    palw_state_v2::{PalwClaimSourceV2, PalwConsensusObjectV2, PalwStateCarriageV2},
    palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
    palw_tir_court_v1::check_tir_trace_event_disclosure_v1,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{error::Error, fs, io::Read, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    expected: Expected,
    base_class: Hash64,
    certificate_daa: u64,
    maturity_daa: u64,
    draw_slot: u64,
    pinned: Pinned,
    readback_peers: Vec<Peer>,
    tickets: Vec<String>,
    target: String,
    winning_indices: Vec<u32>,
    remaining_window_bound: WindowBound,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WindowBound {
    last_eligible_daa: u64,
    maximum_effective_target: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    claim: Hash64,
    execution_root: Hash64,
    network_domain: Hash64,
}
#[derive(Deserialize)]
struct Pinned {
    bytes: usize,
    sha256: String,
    attempts: Vec<Attempt>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Peer {
    peer: u16,
    carriage_sha256: String,
    court_answer_payload_sha256: String,
}
#[derive(Deserialize)]
struct Attempt {
    block: Hash64,
    daa: u64,
    algo: u8,
}

fn read(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("input exceeds audit bound".into());
    }
    Ok(bytes)
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let [report, state, answer] = args.as_slice() else {
        return Err("expected REPORT.json STATE.borsh COURT-ANSWER.bin".into());
    };
    let r: Report = serde_json::from_slice(&read(report.as_ref(), 128 << 10)?)?;
    let state_bytes = read(state.as_ref(), 16 << 20)?;
    let answer_bytes = read(answer.as_ref(), 1 << 20)?;
    let digest = format!("{:x}", Sha256::digest(&state_bytes));
    let answer_digest = format!("{:x}", Sha256::digest(&answer_bytes));
    let peers: std::collections::BTreeSet<_> = r.readback_peers.iter().map(|p| p.peer).collect();
    if peers.len() != 3
        || r.readback_peers.len() != 3
        || state_bytes.len() != r.pinned.bytes
        || digest != r.pinned.sha256
        || !r.readback_peers.iter().all(|p| p.carriage_sha256 == digest && p.court_answer_payload_sha256 == answer_digest)
    {
        return Err("capture does not match all three peer reads".into());
    }
    let c: PalwStateCarriageV2 = borsh::from_slice(&state_bytes)?;
    let id = r.expected.claim;
    let claim = c.claims.get(&id).ok_or("unknown claim")?;
    let PalwClaimSourceV2::FreePrompt { quanta, .. } = claim.source else {
        return Err("not a free-prompt claim".into());
    };
    if quanta == 0 || claim.execution_root != r.expected.execution_root {
        return Err("unexpected claim".into());
    }
    let slot = fp_draw_slot_v3(r.certificate_daa, r.maturity_daa).ok_or("receipt slot overflow")?;
    if slot != r.draw_slot {
        return Err("wrong draw slot".into());
    }
    let beacon = derive_beacon_fact_v3(
        slot,
        6,
        1,
        r.pinned.attempts.iter().map(|a| PalwChainBlockFactV3 { block: a.block, daa_score: a.daa, pow_algo_id: a.algo }),
    )
    .map_err(|e| format!("private TFO3 beacon: {e:?}"))?;
    let pool = c.receipt_targets.get(&fp_pooled_receipt_key_v1()).ok_or("missing pooled target")?;
    let floor = c.model_lifecycles.get(&r.base_class).ok_or("missing floor model")?;
    let quantum = fp_network_quantum_v1(floor.work.economic_ccu_per_claim, 8);
    if quantum == 0 {
        return Err("missing network quantum".into());
    }
    let target = fp_quantum_target_of_compute_v1(pool.target, u128::from(claim.pwu) / u128::from(quanta), quantum);
    let tickets: Vec<_> = (0..quanta).map(|i| fp_quantum_ticket_v3(r.expected.network_domain, beacon.beacon_block, id, i)).collect();
    let winners: Vec<_> = (0..quanta).filter(|i| palw_ticket_admits_v1(tickets[*i as usize], target)).collect();
    if target.to_string() != r.target || tickets.iter().map(u128::to_string).ne(r.tickets) || winners != r.winning_indices {
        return Err("native draw differs from report".into());
    }
    // This private T12 profile shortened receipt maturity only. Its use window,
    // epoch period and maximum per-epoch retarget factor are the T12 rules.
    let end = beacon.beacon_daa.checked_add(600).ok_or("window overflow")?;
    let start = c.last_point.ok_or("missing native point")?.daa_score;
    if start < beacon.beacon_daa || start > end || end != r.remaining_window_bound.last_eligible_daa {
        return Err("capture is outside the draw window".into());
    }
    let mut upper = pool.target;
    for _ in (start / 1000)..(end / 1000) {
        upper = upper.saturating_mul(4);
    }
    let maximum = fp_quantum_target_of_compute_v1(upper, u128::from(claim.pwu) / u128::from(quanta), quantum);
    if maximum.to_string() != r.remaining_window_bound.maximum_effective_target
        || tickets.iter().any(|&t| palw_ticket_admits_v1(t, maximum))
    {
        return Err("draw could still win under a future retarget".into());
    }
    let payload: PalwLifecycleTxPayloadV2 = borsh::from_slice(&answer_bytes)?;
    let PalwConsensusObjectV2::MaterialDisclosedV2 { claim: answered, unit, answer, discloser, signature } = payload.object else {
        return Err("expected MaterialDisclosedV2".into());
    };
    let msg = palw_da_disclosure_message_v4(&r.expected.network_domain, &id, &unit, &palw_da_answer_digest_v1(&answer), &discloser);
    let signer = c.bonds.get(&discloser).ok_or("unknown discloser")?;
    if answered != id
        || !palw_staged_verify_mldsa87_v1(&signer.pubkey, msg.as_byte_slice(), &signature, PALW_DA_DISCLOSURE_V4_MLDSA87_CONTEXT)
    {
        return Err("wrong claim or invalid disclosure signature".into());
    }
    let (PalwDaUnitV1::Event { row, tile }, PalwDaAnswerV1::TirEvent(event)) = (unit, answer) else {
        return Err("expected a TIR event".into());
    };
    let program = c.tir_classes.get(&claim.class_id).ok_or("unknown registered TIR class")?;
    let disclosure = event.with_program_v1(program).map_err(|e| format!("registered program: {e:?}"))?;
    let verify = |trace, exec| check_tir_trace_event_disclosure_v1(trace, exec, row, tile, &disclosure, PALW_STEP_LEG_MAX_LEAVES);
    verify(claim.trace_root, claim.execution_root).map_err(|e| format!("honest event: {e:?}"))?;
    if verify(Hash64::default(), claim.execution_root).is_ok() || verify(claim.trace_root, Hash64::default()).is_ok() {
        return Err("substituted root was accepted".into());
    }
    println!(
        "claim {id}, beacon DAA {}, {quanta} tickets, winning indices {winners:?}, signed TIR event ({row}, {tile}) verified, both substituted roots rejected",
        beacon.beacon_daa
    );
    Ok(())
}

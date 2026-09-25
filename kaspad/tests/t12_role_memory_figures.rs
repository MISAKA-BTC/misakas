//! **The memory figures `contrib/t12-deploy-kit/PLAN.md` §2 is built from, printed from this build**
//! (release-prep review, 2026-09-25: the table was computed by a scratch test that was never
//! committed, so the shipping commit could not reproduce it).
//!
//! ```text
//! cargo test --locked -p kaspad --test t12_role_memory_figures -- --ignored --nocapture
//! ```
//!
//! It prints, for testnet-12's genesis held rows at the fleet's runtime (A16-KV-i16, the shipped K/V
//! representation; 8 threads, the fleet hosts' core count; the A16 prefill run), what the node's
//! memory ledger reserves for one duty of each role — `holding (the artifact file) + the role's
//! resource-profile working set`, the partial seat's capture re-priced as the streamed fold exactly
//! as the panel does (`palw_partial_seat_streamed_need_v1`) — and then, for every node row of
//! `install-{ibm,113,5104}.sh`, the share its duties need and the unit `MemoryMax` the ledger's
//! cgroup term needs (PLAN.md §2):
//!
//! ```text
//! the ledger starts a duty only if  need ≤ min(share, min(MemAvailable, memory.max − memory.current) − 1 GiB) − reserved
//! so MemoryMax ≥ share + base + artifact page cache + ΣW(the other running duties) + 1 GiB + cache allowance
//! ```
//!
//! `base` (the process without a PALW duty: the consensus caches `--ram-scale` declares plus the
//! process) is an ESTIMATE here and says so; replace it with the RSS `check` prints after `switch`.
//! Nothing is asserted beyond the derivations deriving: this is the operator's worksheet, pinned to
//! the code that reserves.

use kaspa_consensus_core::palw_resource_profile_v1::{
    PalwCaptureRetentionV1, PalwResourceRoleV1, PalwRuntimeLimitsV1, PalwRuntimeProfileV1, palw_attempt_capture_folds_v1,
    palw_profile_max_tile_len_v1, palw_resource_profile_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspad_lib::palw_backends::{PALW_REPLAY_HOST_RESERVE_BYTES_V1, PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1, palw_partial_seat_streamed_fold_bytes_v1};
use std::path::PathBuf;

const MIB: u64 = 1 << 20;
/// What the node's own page cache and kernel objects may add to `memory.current` beyond the terms
/// named (RocksDB blocks, logs, retained captures) — an allowance, not a bound (PLAN.md §2).
const CACHE_ALLOWANCE_MIB: u64 = 2_048;
/// The process without a duty, beyond its declared consensus caches (an estimate; see the module doc).
const PROCESS_BASE_MIB: u64 = 256;
/// .113's b6 also serves gRPC (the explorer's filler/REST), the utxoindex and the EVM RPC.
const EXPLORER_BACKEND_EXTRA_MIB: u64 = 512;

fn repo_file(rel: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn artifact_bytes_of_sidecar(rel: &str) -> u64 {
    repo_file(rel)
        .split("\"artifact_bytes\":")
        .nth(1)
        .and_then(|rest| rest.trim_start().split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("{rel}: no artifact_bytes"))
}

fn mib(bytes: u64) -> u64 {
    bytes.div_ceil(MIB)
}

/// One duty as the ledger reserves it: `need` = holding + working set, `w` = the working set (what
/// the duty touches beyond the shared artifact pages).
#[derive(Clone, Copy, Debug)]
struct Duty {
    name: &'static str,
    need: u64,
    w: u64,
    /// At most this many of it run at once on one node.
    max: usize,
}

/// The largest ΣW of duties that can be running when one more of `duties` asks (`one_more = true`: the
/// MemoryMax term), or that can be running at all (`false`: the node's worst anon), over every multiset
/// the share admits (each duty at most `max` times).
fn worst_running_w(duties: &[Duty], share: u64, one_more: bool) -> (u64, String) {
    let mut best = (0u64, String::new());
    let mut counts = vec![0usize; duties.len()];
    loop {
        let reserved: u64 = counts.iter().zip(duties).map(|(c, d)| *c as u64 * d.need).sum();
        let w: u64 = counts.iter().zip(duties).map(|(c, d)| *c as u64 * d.w).sum();
        // The multiset is running and one more duty of any kind that fits asks now.
        let one_more_fits = duties.iter().enumerate().any(|(i, d)| counts[i] < d.max && reserved + d.need <= share);
        if reserved <= share && (one_more_fits || !one_more) && w > best.0 {
            let what = counts
                .iter()
                .zip(duties)
                .filter(|(c, _)| **c > 0)
                .map(|(c, d)| format!("{c}×{}", d.name))
                .collect::<Vec<_>>()
                .join(" + ");
            best = (w, what);
        }
        let mut i = 0;
        loop {
            if i == counts.len() {
                return best;
            }
            counts[i] += 1;
            if counts[i] <= duties[i].max {
                break;
            }
            counts[i] = 0;
            i += 1;
        }
    }
}

#[test]
#[ignore = "the operator's worksheet for PLAN.md §2 — run with --ignored --nocapture"]
fn t12_role_memory_figures() {
    let params = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("testnet-12 is ConsensusV2")
    };
    let threads: u32 = std::env::var("T12_THREADS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let limits = PalwRuntimeLimitsV1 {
        threads,
        prefill_run_positions: misaka_palw_base0::qwen25_a16_backend::A16_PREFILL_RUN_POSITIONS as u32,
    };
    let runtime = PalwRuntimeProfileV1::A16KvI16;
    let art_8k = artifact_bytes_of_sidecar("consensus/core/src/config/class-manifests/qwen25-1.5b-a16-8k.palwmanifest");
    let art_2m = artifact_bytes_of_sidecar("consensus/core/src/config/class-manifests/qwen25-1.5b-a16-2m.palwmanifest");
    println!("runtime {} · threads {threads} · prefill run {} · 8k artifact {art_8k} B ({} MiB)", runtime.name(), limits.prefill_run_positions, mib(art_8k));

    let mut eight_k: Option<(u64, u64, u64)> = None; // (full need, partial worst need, attempt need), bytes
    for o in bundle.genesis_objects.iter() {
        let PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. } = o else { continue };
        if !kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&c.profile) {
            continue;
        }
        let (profile, job) = (&c.profile, &c.canonical);
        let ladder = kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(
            kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1,
            profile,
        );
        let leaves = kaspa_consensus_core::palw_step::step_leaf_count_capped_v1(profile, job, ladder).expect("the canonical job's step space");
        let fold = PalwCaptureRetentionV1::Fold {
            retain_level: misaka_palw_base0::fp_capture::palw_base0_sparse_retain_level_for_class_v1(profile, ladder),
        };
        let holding = if profile.n_ctx <= 8_192 { art_8k } else { art_2m };
        println!(
            "\nclass {class_id} n_ctx {} canonical ({}, {}) leaves {leaves} capture {fold:?} holding {} MiB",
            profile.n_ctx,
            job.declared_prefill_tokens,
            job.exact_decode_tokens,
            mib(holding)
        );
        let attempt_capture = if palw_attempt_capture_folds_v1(profile) {
            fold
        } else {
            PalwCaptureRetentionV1::DenseTiles { tile_len: palw_profile_max_tile_len_v1(profile) }
        };
        let derive = |role, capture| palw_resource_profile_v1(profile, job, leaves, runtime, role, limits, capture).expect("derives");
        let attempt = derive(PalwResourceRoleV1::Producer, attempt_capture).working_set_bytes();
        let full = derive(PalwResourceRoleV1::FullSeat, fold).working_set_bytes();
        println!("  producer attempt   W {:>6} MiB  need {:>6} MiB", mib(attempt), mib(holding + attempt));
        println!("  full seat / court / DA answer  W {:>6} MiB  need {:>6} MiB", mib(full), mib(holding + full));
        let seats: u16 = std::env::var("T12_PARTIAL_SEATS").ok().and_then(|v| v.parse().ok()).unwrap_or(5);
        let mut worst_partial = 0u64;
        for segment in 0..seats.saturating_sub(1) {
            let p = derive(PalwResourceRoleV1::PartialSeat { seat_count: seats, segment_index: segment }, PalwCaptureRetentionV1::ReplayHashes);
            // The panel re-prices the partial seat's capture as the streamed fold it keeps.
            let w = p.working_set_bytes() - p.capture_retained_bytes + palw_partial_seat_streamed_fold_bytes_v1(p.leaves);
            worst_partial = worst_partial.max(w);
            println!("  partial seat {seats}/{segment}  W {:>6} MiB  need {:>6} MiB", mib(w), mib(holding + w));
        }
        if profile.n_ctx <= 8_192 {
            eight_k = Some((holding + full, holding + worst_partial, holding + attempt));
        }
    }
    let (full_8k, partial_8k, attempt_8k) = eight_k.expect("the 8k genesis row");
    let floor_need = art_8k + PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1;
    println!(
        "\nfloor (BASE-0, no resource profile): holding (the 8k file on an 8k node) + {} MiB scratch estimate = {} MiB",
        mib(PALW_REPLAY_SCRATCH_ESTIMATE_BYTES_V1),
        mib(floor_need)
    );
    let seat_need = full_8k.max(partial_8k);

    // ---- the kit's node rows ----
    let a = mib(art_8k);
    println!("\nnode rows of install-*.sh (share and MemoryMax as staged; PLAN.md §2):");
    for host in ["ibm", "113", "5104"] {
        let file = format!("contrib/t12-deploy-kit/install-{host}.sh");
        let text = repo_file(&file);
        for row in text.lines().map(str::trim).filter(|l| l.starts_with('"') && l.matches('|').count() >= 13) {
            let f: Vec<&str> = row.trim_matches('"').split('|').collect();
            let (id, grpc, share, memmax, produce) = (f[0], f[6], f[8].parse::<u64>().unwrap(), f[9], f[10]);
            // What runs on this node, as the ledger reserves it (a DA answer on its own claim is priced
            // as the class's full-seat replay; a covering signer's is the same figure).
            let mut duties = vec![Duty { name: "seat", need: mib(seat_need), w: mib(seat_need) - a, max: 2 }];
            match produce {
                "8k" => {
                    duties.push(Duty { name: "8k attempt", need: mib(attempt_8k), w: mib(attempt_8k) - a, max: 1 });
                    duties.push(Duty { name: "8k DA answer", need: mib(full_8k), w: mib(full_8k) - a, max: 2 });
                }
                "floor" => {
                    duties.push(Duty { name: "floor attempt", need: mib(floor_need), w: mib(floor_need) - a, max: 1 });
                    duties.push(Duty { name: "floor DA answer", need: mib(floor_need), w: mib(floor_need) - a, max: 2 });
                }
                _ => duties.push(Duty { name: "covering DA answer", need: mib(full_8k), w: mib(full_8k) - a, max: 2 }),
            }
            // The share the plan intends: one of each duty at once (the second seat replay excluded) on a
            // producer; one duty at a time on a seat-only node.
            let intended: u64 = if produce == "none" { duties.iter().map(|d| d.need).max().unwrap_or(0) } else { duties.iter().map(|d| d.need).sum() };
            let (w_running, what) = worst_running_w(&duties, share, true);
            let (w_all, what_all) = worst_running_w(&duties, share, false);
            let ram_scale = kaspad_lib::args::palw_ram_scale_for_share_v1(share * MIB);
            let caches = mib(kaspa_consensus::consensus::storage::declared_cache_budget_bytes_v1(ram_scale));
            let base = caches + PROCESS_BASE_MIB + if grpc != "-" { EXPLORER_BACKEND_EXTRA_MIB } else { 0 };
            let reserve = mib(PALW_REPLAY_HOST_RESERVE_BYTES_V1);
            let need_max = share + base + a + w_running + reserve + CACHE_ALLOWANCE_MIB;
            let staged = match memmax {
                "-" => "infinity".to_string(),
                g => format!("{} MiB", g.parse::<u64>().expect("memmax is GiB or -") * 1024),
            };
            let ok = memmax == "-" || memmax.parse::<u64>().unwrap() * 1024 >= need_max;
            println!(
                "  b{id} ({host}, produce={produce}): share {share} MiB (the duties it is sized for: {intended}) · MemoryMax staged {staged}, \
                 needs ≥ {need_max} = share {share} + base≈{base} (caches {caches} at ram-scale {ram_scale:.3}) + artifact {a} + \
                 ΣW {w_running} ({what}) + reserve {reserve} + cache {CACHE_ALLOWANCE_MIB} → {} · worst anon ≈ base {base} + {w_all} ({what_all}) = {}",
                if ok { "OK" } else { "TOO LOW" },
                base + w_all
            );
        }
    }
}

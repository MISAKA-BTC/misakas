use super::*;
use crate::header::CompressedParents;

#[derive(Default)]
struct RecordingHasher {
    bytes: Vec<u8>,
    calls: usize,
    largest_update: usize,
}

impl HasherBase for RecordingHasher {
    fn update<A: AsRef<[u8]>>(&mut self, data: A) -> &mut Self {
        let bytes = data.as_ref();
        self.calls += 1;
        self.largest_update = self.largest_update.max(bytes.len());
        self.bytes.extend_from_slice(bytes);
        self
    }
}

fn level(count: usize, salt: u8) -> Vec<Hash64> {
    (0..count)
        .map(|i| {
            let mut bytes = [salt; kaspa_hashes::HASH64_SIZE];
            bytes[..8].copy_from_slice(&(i as u64).to_le_bytes());
            Hash64::from_bytes(bytes)
        })
        .collect()
}

fn original_parent_writer<H: HasherBase>(hasher: &mut H, parents: &CompressedParents) {
    for level in parents.expanded_iter() {
        hasher.write_var_array(level);
    }
}

#[test]
fn exec_origin_header_cached_parent_bytes_equal_original_including_scratch_boundary() {
    let shapes = [
        vec![],
        vec![(255, vec![])],
        vec![(255, level(1, 1))],
        vec![(1, level(8, 1))],
        vec![(3, level(8, 1)), (9, vec![]), (255, level(16, 2))],
        vec![(2, level(PARENT_RUN_CACHE_MAX_HASHES - 1, 1))],
        vec![(2, level(PARENT_RUN_CACHE_MAX_HASHES, 1))],
        vec![(2, level(PARENT_RUN_CACHE_MAX_HASHES + 1, 1))],
    ];
    for runs in shapes {
        let parents = CompressedParents::try_from(runs).unwrap();
        let (mut original, mut cached) = (RecordingHasher::default(), RecordingHasher::default());
        original_parent_writer(&mut original, &parents);
        write_cached_parent_runs(&mut cached, &parents);
        assert_eq!(cached.bytes, original.bytes, "every length and hash byte, in expanded level order");
        assert!(cached.largest_update <= PARENT_RUN_CACHE_MAX_HASHES * kaspa_hashes::HASH64_SIZE);
    }
    let parents = CompressedParents::try_from(vec![(255, level(16, 3))]).unwrap();
    let (mut original, mut cached) = (RecordingHasher::default(), RecordingHasher::default());
    original_parent_writer(&mut original, &parents);
    write_cached_parent_runs(&mut cached, &parents);
    assert_eq!(original.calls, 255 * 17);
    assert_eq!(cached.calls, 255 * 2, "two updates per level while preserving all bytes");
}

fn header(parents: CompressedParents, version: u16, algo: u8) -> Header {
    let mut header = Header::from_precomputed_hash(Hash64::from_u64_word(0xBAD), vec![]);
    header.parents_by_level = parents;
    header.version = version;
    header.pow_algo_id = algo;
    header.hash_merkle_root = Hash64::from_u64_word(1);
    header.accepted_id_merkle_root = Hash64::from_u64_word(2);
    header.utxo_commitment = Hash64::from_u64_word(3);
    header.timestamp = 1_234_567;
    header.bits = 0x1D00FFFF;
    header.nonce = 7_654_321;
    header.daa_score = 89;
    header.blue_score = 101;
    header.blue_work = 0xFEDCBA9876543210u64.into();
    header.pruning_point = Hash64::from_u64_word(4);
    header.evm_payload_hash = Hash64::from_u64_word(5);
    header.evm_commitment_root = Hash64::from_u64_word(6);
    header.overlay_commitment_root = Hash64::from_u64_word(7);
    header.palw_state_root = Hash64::from_u64_word(8);
    header.palw_commitment = vec![0xCA; 321];
    header
}

#[test]
fn exec_origin_header_cached_full_digests_equal_original_across_version_and_algo_gates() {
    // Nonzero fields exercise EVM/version, PALW/state-root, commitment include/exclude, blue-work and cached-hash gates.
    for version in 0..=3 {
        for algo in 0..=10 {
            let parents = CompressedParents::try_from(vec![(3, level(8, 1)), (5, level(64, 2)), (255, vec![])]).unwrap();
            let header = header(parents, version, algo);
            assert_eq!(hash_with_parent_run_cache(&header), hash(&header), "identity version {version} algo {algo}");
            assert_eq!(pre_pow_hash_with_parent_run_cache(&header), pre_pow_hash_64(&header), "pre-PoW version {version} algo {algo}");
            assert_ne!(hash(&header), header.hash, "an arbitrary cached hash is not used");
        }
    }
}

#[test]
fn exec_origin_header_above_scratch_bound_still_hashes_without_an_admission_cap() {
    let header = header(
        CompressedParents::try_from(vec![(2, level(PARENT_RUN_CACHE_MAX_HASHES + 1, 4))]).unwrap(),
        crate::constants::EVM_HEADER_VERSION,
        crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3,
    );
    assert_eq!(hash_with_parent_run_cache(&header), hash(&header));
    assert_eq!(pre_pow_hash_with_parent_run_cache(&header), pre_pow_hash_64(&header));
}

#[test]
#[ignore = "explicit timing measurement; compares original and cached writers on the same synthetic headers"]
fn exec_origin_header_parent_run_timing() {
    use std::hint::black_box;
    use std::time::{Duration, Instant};
    let mut largest =
        header(CompressedParents::default(), crate::constants::EVM_HEADER_VERSION, crate::pow_layer0::POW_ALGO_ID_PALW_EXEC_V3);
    largest.palw_commitment = vec![0xCC; crate::pow_layer0::PALW_COMMITMENT_MAX_BYTES];
    let cap = crate::palw_work_slice_v2::PALW_EXEC_ORIGIN_HEADER_MAX_BYTES_V1;
    let max_parents = (cap - borsh::to_vec(&largest).unwrap().len() - 5) / kaspa_hashes::HASH64_SIZE;
    let cases = [(0, 1, 2_000), (1, 255, 100), (10, 8, 1_000), (64, 255, 10), (max_parents, 255, 1)];
    let timed = |header: &Header, rounds: usize, cached: bool| {
        let started = Instant::now();
        for _ in 0..rounds {
            if cached {
                black_box(hash_with_parent_run_cache(black_box(header)));
            } else {
                black_box(hash(black_box(header)));
            }
        }
        started.elapsed() / rounds as u32
    };
    for (count, repeat, rounds) in cases {
        let mut input = largest.clone();
        input.parents_by_level = CompressedParents::try_from(vec![(repeat, level(count, 5))]).unwrap();
        let encoded = borsh::to_vec(&input).unwrap().len();
        assert!(encoded <= cap);
        assert_eq!(hash_with_parent_run_cache(&input), hash(&input));
        assert_eq!(pre_pow_hash_with_parent_run_cache(&input), pre_pow_hash_64(&input));
        let (mut old, mut new) = (Vec::<Duration>::new(), Vec::<Duration>::new());
        for sample in 0..6 {
            // Alternate order to reduce the influence of shared-host changes; no wall-clock PASS threshold.
            if sample % 2 == 0 {
                old.push(timed(&input, rounds, false));
                new.push(timed(&input, rounds, true));
            } else {
                new.push(timed(&input, rounds, true));
                old.push(timed(&input, rounds, false));
            }
        }
        old.sort();
        new.sort();
        eprintln!(
            "[EXEC origin hashing] parents {count}, levels {repeat}, borsh {encoded}, parent preimage {} bytes/digest, original median {:?}, cached median {:?}, ratio {:.3}",
            count * repeat as usize * kaspa_hashes::HASH64_SIZE,
            old[3],
            new[3],
            new[3].as_secs_f64() / old[3].as_secs_f64()
        );
    }
}

//! The RFC-0012 rig supplies its test-only mature work fact. Fork choice,
//! executed headers, settlement generations and withdrawal readback are native.
//! VSS outcomes, the two consumer roots and the original schedule are fixture inputs.
//! Each native database resolves the closure from the same accepted close block.
use super::*;
use kaspa_rpc_core::RpcRawBlock;
use std::{fs::File, path::Path, process::Command};

const READER_DIR: &str = "MISAKA_PRIVATE_TB1_FREEZE_READER";
const READER_SLOT: &str = "MISAKA_PRIVATE_TB1_FREEZE_SLOT";

use kaspa_consensus_core::palw_tb1::{
    Tb1Field,
    freeze::{Tb1MiningClosure, Tb1MiningPurpose, Tb1MiningRound},
    qualification::{Tb1DealerEnd, Tb1PreparationCut, Tb1Qualification, Tb1Ready, Tb1Roster, Tb1VssEnd},
    releases::{Tb1HolderRelease, Tb1ReleaseContext},
    signatures::{READY_CONTEXT, RELEASE_CONTEXT, ready_message},
};
use libcrux_ml_dsa::ml_dsa_65;
use misaka_palw_remote::tb1_exposure::{Tb1ExposureJournal, Tb1HolderFreeze};

fn child_signature(dir: &Path, slot: usize) -> ml_dsa_65::MLDSA65Signature {
    let mut command = Command::new(std::env::current_exe().unwrap());
    let mut child = command
        .arg("native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected")
        .args(["--test-threads=1", "--nocapture"])
        .env(READER_DIR, dir)
        .env(READER_SLOT, slot.to_string())
        .spawn()
        .unwrap();
    let pid = child.id();
    assert!(child.wait().unwrap().success());
    let bytes = std::fs::read(dir.join(format!("holder-{slot}.sig"))).unwrap();
    eprintln!("TB1 native reader pid {pid}, holder slot {slot}, signature bytes {}", bytes.len());
    ml_dsa_65::MLDSA65Signature::new(bytes.as_slice().try_into().unwrap())
}

fn field(v: usize) -> Tb1Field {
    let mut bytes = [0; 66];
    bytes[65] = v as u8;
    Tb1Field::from_bytes(&bytes).unwrap()
}

#[tokio::test]
async fn native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected() {
    let parts = parts(Some(FENCE));
    let mut node = Rig::new(&parts, 0x12_c500_0000);
    let parent = node.chain.sink();
    let template = node.beat();
    let keys: Vec<_> = (1..=4).map(|i| ml_dsa_65::generate_key_pair([i; 32])).collect();
    let public: Vec<_> = keys.iter().map(|kp| kp.verification_key.clone()).collect();
    let coordinates: Vec<_> = (1..=4).map(field).collect();
    let roster = Tb1Roster { coordinates: &coordinates, holder_keys: &public, f: 1 };
    let round = Tb1MiningRound {
        chain_id: node.domain.as_bytes(),
        roster_root: roster.root().unwrap(),
        role_epoch: 1,
        logical_round: 2,
        purpose: Tb1MiningPurpose::Exec,
        schedule_id: [9; 64],
        close_blue_score: template.header.blue_score,
        release_from_daa: template.header.daa_score + 1,
        deadline_exclusive_daa: template.header.daa_score + 11,
    };
    let round_ref = round.id();
    let round_ref = round_ref.as_slice();
    let dealers = [0, 1, 2];
    let ends: Vec<_> =
        (0..4).map(|d| Some(Tb1DealerEnd { end: Tb1VssEnd { end_anchor: [5; 64], transcript_root: [d; 64] }, position: 1 })).collect();
    let mut ready = Vec::new();
    for dealer in dealers {
        for holder_slot in 0..3 {
            ready.push(Tb1Ready {
                round_ref,
                dealer_slot: dealer,
                holder_slot,
                position: 2,
                end: ends[dealer].unwrap().end,
                signature: &[],
            });
        }
    }
    let signatures: Vec<_> = ready
        .iter()
        .map(|r| ml_dsa_65::sign(&keys[r.holder_slot].signing_key, &ready_message(r, 2048).unwrap(), READY_CONTEXT, [9; 32]).unwrap())
        .collect();
    for (r, sig) in ready.iter_mut().zip(&signatures) {
        r.signature = sig.as_slice();
    }
    let cut = Tb1PreparationCut { round_ref, deadline_position: 3, observed_through: 3, dealer_ends: &ends, ready: &ready };
    let q = Tb1Qualification::resolve(
        roster, round_ref, [7; 64], // reserved resources reference
        &dealers, &cut, 9, 2048,
    )
    .unwrap();
    let closure = Tb1MiningClosure { round, qual_root: q.root(), consumer_set_root: [16; 64], policy_root: [17; 64] };
    let freeze_id = closure.commitment(parent);
    let child_dir = std::env::var_os(READER_DIR).map(std::path::PathBuf::from);
    let slot = if child_dir.is_some() { std::env::var(READER_SLOT).unwrap().parse::<usize>().unwrap() } else { 0 };
    let (prefix, work, tip) = if let Some(dir) = &child_dir {
        let raw: [RpcRawBlock; 6] = serde_json::from_reader(File::open(dir.join("blocks.json")).unwrap()).unwrap();
        let [close, e1, b1, e2, work, tip] = raw.map(|r| Block::try_from(r).unwrap());
        let prefix = vec![close, e1, b1, e2];
        for block in &prefix {
            node.arrive(block.clone(), "recorded close prefix").await;
        }
        let close = &prefix[0];
        plant_frontier(&node, close.header.blue_score, close.hash());
        node.arrive(work.clone(), "recorded work").await;
        place_maturing_fact(&node, &work, 10, 0);
        node.vp().pruning_point_store.write().set(close.hash(), 1).unwrap();
        node.arrive(tip.clone(), "recorded settlement tip").await;
        (prefix, work, tip)
    } else {
        let mut miner = new_miner_data();
        miner.extra_data = freeze_id.to_vec();
        let close = node.beat_with_miner(miner, vec![]);
        let close = node.take(close, "committed TB1 close").await;
        let mut prefix = vec![close];
        prefix.extend(extend(&mut node, &[0]).await);
        let beat = node.beat();
        prefix.push(node.take(beat, "b1").await);
        prefix.extend(extend(&mut node, &[1]).await);
        let close = &prefix[0];
        plant_frontier(&node, close.header.blue_score, close.hash());
        let work = extend(&mut node, &[2]).await.remove(0);
        place_maturing_fact(&node, &work, 10, 0);
        node.vp().pruning_point_store.write().set(close.hash(), 1).unwrap();
        let tip = extend(&mut node, &[3]).await.remove(0);
        (prefix, work, tip)
    };
    let anchor = prefix[0].hash();
    let before = snapshot_of(&node).unwrap();
    assert_eq!(before.generation, tip.hash());
    assert_eq!(before.finalized, Some(anchor));
    let context = Tb1ReleaseContext { round_ref, qual_root: q.root(), freeze_id };
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    let api = node.api().as_ref();
    let freeze = Tb1HolderFreeze::resolve(api, anchor, &closure, &q).unwrap();
    let mut other_chain = round.chain_id;
    other_chain[0] ^= 1;
    assert!(!api.palw_coinbase_commits_to(other_chain, anchor, &freeze_id).unwrap());
    for changed in [
        Tb1MiningClosure { consumer_set_root: [65; 64], ..closure },
        Tb1MiningClosure { policy_root: [65; 64], ..closure },
        Tb1MiningClosure { round: Tb1MiningRound { purpose: Tb1MiningPurpose::Panel, ..round }, ..closure },
        Tb1MiningClosure { round: Tb1MiningRound { deadline_exclusive_daa: round.deadline_exclusive_daa + 1, ..round }, ..closure },
    ] {
        assert!(Tb1HolderFreeze::resolve(api, anchor, &changed, &q).is_err());
    }
    assert!(Tb1HolderFreeze::resolve(api, tip.hash(), &closure, &q).is_err());
    let shares = |slot: usize| dealers.map(|d| (d, field(10 + d + slot + 1)));
    let values = shares(slot);
    let row = Tb1HolderRelease { context, holder_slot: slot, shares: &values };
    Tb1ExposureJournal::provision(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir.path(), round_ref, &public[slot], slot as u32).unwrap();
    let msg = journal.reserve_signing_message(api, freeze, &q, &row, 2048).unwrap();
    let sig = ml_dsa_65::sign(&keys[slot].signing_key, &msg, RELEASE_CONTEXT, [71; 32]).unwrap();
    eprintln!(
        "TB1 native acceptance pid {}, slot {slot}, anchor {anchor}, sink {}, qualification {}",
        std::process::id(),
        before.generation,
        Hash64::from_bytes(q.root())
    );
    if let Some(target) = child_dir {
        use std::io::Write;
        File::create_new(target.join(format!("holder-{slot}.sig"))).unwrap().write_all(sig.as_slice()).unwrap();
        return;
    }
    let raw: Vec<_> = prefix.iter().chain([&work, &tip]).map(RpcRawBlock::from).collect();
    serde_json::to_writer(File::create_new(dir.path().join("blocks.json")).unwrap(), &raw).unwrap();
    let signatures = [sig, child_signature(dir.path(), 1), child_signature(dir.path(), 2)];
    for (holder_slot, sig) in signatures.iter().enumerate() {
        let values = shares(holder_slot);
        let row = Tb1HolderRelease { context, holder_slot, shares: &values };
        let bytes = q.holder_release_message(&row, &public[holder_slot], 2048).unwrap();
        ml_dsa_65::verify(&public[holder_slot], &bytes, RELEASE_CONTEXT, sig).unwrap();
    }
    let mut rival = follower(&parts, 0x12_d500_0000, &prefix).await;
    plant_frontier(&rival, prefix[0].header.blue_score, anchor);
    let alternative = extend(&mut rival, &[4, 5, 6]).await;
    for block in &alternative {
        node.arrive(block.clone(), "heavier branch").await;
    }
    let after = snapshot_of(&node).unwrap();
    assert_eq!(after.generation, alternative.last().unwrap().hash());
    assert!(node.api().is_chain_ancestor_of(anchor, after.generation).unwrap());
    assert_eq!(after.finalized, None);
    assert_eq!(readiness_of(&node).finalized.withdrawn_from, Some(anchor));
    assert!(journal.observe_branch(node.api().as_ref()).unwrap_err().to_string().contains("halted"));
    drop(journal);
    assert!(
        matches!(Tb1ExposureJournal::resume(dir.path(),context.round_ref,&public[0],0),Err(err) if err.to_string().contains("halted"))
    );
    eprintln!(
        "TB1 native custody: anchor {anchor}, prior sink {}, replacement {}, stop {:?}",
        before.generation, after.generation, after.stop
    );
}

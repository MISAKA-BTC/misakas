//! The RFC-0012 rig supplies its test-only mature work fact. Fork choice,
//! executed headers, settlement generations and withdrawal readback are native.
//! VSS outcomes and the TB1 freeze-to-block association are fixture inputs.
use super::*;
use kaspa_consensus_core::palw_tb1::{
    Tb1Field,
    qualification::{Tb1DealerEnd, Tb1PreparationCut, Tb1Qualification, Tb1Ready, Tb1Roster, Tb1VssEnd},
    releases::{Tb1HolderRelease, Tb1ReleaseContext},
    signatures::{READY_CONTEXT, ready_message},
};
use libcrux_ml_dsa::ml_dsa_65;
use misaka_palw_remote::tb1_exposure::{Tb1ExposureJournal, Tb1HolderFreeze};

fn field(v: usize) -> Tb1Field {
    let mut bytes = [0; 66];
    bytes[65] = v as u8;
    Tb1Field::from_bytes(&bytes).unwrap()
}

#[tokio::test]
async fn native_withdrawal_halts_an_exposed_holder_while_its_block_is_still_selected() {
    let (mut node, mut rival) = competing_finality_branches(0x12_c500_0000, true).await;
    let before = snapshot_of(&node).unwrap();
    let anchor = before.finalized.expect("fixture native finality covers the original freeze block");
    let daa = node.api().get_header(before.generation).unwrap().daa_score;
    let freeze =
        Tb1HolderFreeze { accepting_block: anchor, freeze_id: [29; 64], release_from_daa: daa, deadline_exclusive_daa: daa + 10 };
    let context = Tb1ReleaseContext { round_ref: b"private native reorg EXEC", qual_root: [7; 64], freeze_id: freeze.freeze_id };
    let keys: Vec<_> = (1..=4).map(|i| ml_dsa_65::generate_key_pair([i; 32])).collect();
    let public: Vec<_> = keys.iter().map(|kp| kp.verification_key.clone()).collect();
    let coordinates: Vec<_> = (1..=4).map(field).collect();
    let dealers = [0, 1, 2];
    let ends: Vec<_> =
        (0..4).map(|d| Some(Tb1DealerEnd { end: Tb1VssEnd { end_anchor: [5; 64], transcript_root: [d; 64] }, position: 1 })).collect();
    let mut ready = Vec::new();
    for dealer in dealers {
        for holder_slot in 0..3 {
            ready.push(Tb1Ready {
                round_ref: context.round_ref,
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
    let cut = Tb1PreparationCut {
        round_ref: context.round_ref,
        deadline_position: 3,
        observed_through: 3,
        dealer_ends: &ends,
        ready: &ready,
    };
    let q = Tb1Qualification::resolve(
        Tb1Roster { coordinates: &coordinates, holder_keys: &public, f: 1 },
        context.round_ref,
        context.qual_root,
        &dealers,
        &cut,
        9,
        2048,
    )
    .unwrap();
    let values = dealers.map(|i| (i, field(10 + i)));
    let row = Tb1HolderRelease { context, holder_slot: 0, shares: &values };
    let dir = kaspa_database::utils::get_kaspa_tempdir();
    Tb1ExposureJournal::provision(dir.path(), context.round_ref, &public[0], 0).unwrap();
    let mut journal = Tb1ExposureJournal::resume(dir.path(), context.round_ref, &public[0], 0).unwrap();
    journal.reserve_signing_message(node.api().as_ref(), freeze, &q, &row, 2048).unwrap();

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

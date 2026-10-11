//! **RFC-0002 P0 residual on the kernel route's own FileProof (lane RFC02-T)** — on the real node, before and past
//! `palw_model_refutation_seal_v1`: an honest finder seals its proof of a covered lie; a copyist re-encodes it (a non-MatMul fault
//! proof's unread `scalar`) and is included first. Frozen GAP-R7: the copyist is paid, the finder's filing is a duplicate. Past the
//! fence (`misaka_palw_kernel::sealed_bounty`): the copyist's conviction stands but its share is held, and the finder's own sealed
//! proof, filed after the conviction, takes it.
use super::*;
use misaka_palw_kernel::public::FaultProofWireV1;

fn sealed_bounty_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (cfg, bundle, premine, floats) = kernel_config();
    let mut params = cfg.params.clone();
    params.palw_model_refutation_seal_v1 = Some(ForkActivation::new(1));
    (Config::new(params), bundle, premine, floats)
}

fn reencode(proof: &ProsecutionV1) -> ProsecutionV1 {
    let ProsecutionV1::Kernel(bytes) = proof else { panic!("a kernel fault proof") };
    let mut wire: FaultProofWireV1 = borsh::from_slice(bytes).unwrap();
    if wire.kind == 2 {
        // A MatMul scalar court reads only the three openings: an extra (unread) prior row is another encoding.
        let extra = wire.output.clone();
        wire.prior_rows.push(extra);
    } else {
        // Every other kind ignores `scalar`.
        wire.scalar = (7, 7, 7);
    }
    ProsecutionV1::Kernel(wire.to_bytes())
}

/// The covered lie, the finder's sealed proof (mature), the copy; returns `(world, claim, finder, copyist, proof, copy)`.
async fn race(w: &mut World) -> (Digest, usize, usize, ProsecutionV1, ProsecutionV1) {
    let job = w.job().await;
    let lie = w.claim(0, &job, true).await;
    let seats = w.seats(&lie.id);
    w.cover(&lie.id).await;
    let picked = w.outsiders(&lie, &seats, 2);
    let (finder, copyist) = (picked[0], picked[1]);
    let da = lie.published(&w.fx, &[]);
    let proof = w.prosecution(&lie.id, &da, 0x5A);
    let copy = reencode(&proof);
    assert_ne!(copy, proof, "two byte encodings");
    let seal = proof_seal_v1(&lie.id, &w.net.kid(finder), &proof);
    let o = w.net.route(finder, &K::SealProof { accuser: w.net.kid(finder), claim: lie.id, seal });
    w.net.send(vec![(finder, o)]).await;
    let daa = w.net.daa();
    w.net.beat_to(daa + 2).await;
    (lie.id, finder, copyist, proof, copy)
}

#[tokio::test]
async fn rfc02t_kernel_fileproof_reencoded_copy_takes_the_bounty_below_the_fence_and_is_held_for_the_sealer_past_it() {
    kaspa_core::log::try_init_logger("warn");
    // ---- below the fence (frozen GAP-R7): the counterexample ----
    let mut w = World::new().await;
    let (id, finder, copyist, proof, copy) = race(&mut w).await;
    let reward = w.policy().claim_collateral * u64::from(w.policy().accuser_reward_permille) / 1000;
    w.proof(copyist, &id, copy.clone()).await;
    assert!(w.net.ledger().claims[&id].convicted);
    w.proof(finder, &id, proof.clone()).await;
    let (cm, co) = minted_and_owed(&w.net, &w.net.chain, copyist);
    let (fm, fo) = minted_and_owed(&w.net, &w.net.chain, finder);
    assert_eq!((cm + co, fm + fo), (reward, 0), "COUNTEREXAMPLE: the re-encoding copyist is paid, the sealed finder gets nothing");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "frozen counterexample replay");

    // ---- past the fence: the conviction stands, the share is held, the earlier sealer takes it ----
    let mut w = World::on(Net::over_cfg(sealed_bounty_config(), TestConsensus::new)).await;
    let (id, finder, copyist, proof, copy) = race(&mut w).await;
    let before = w.net.collateral(0);
    w.proof(copyist, &id, copy.clone()).await;
    let ledger = w.net.ledger();
    assert!(ledger.claims[&id].convicted, "the copy's authentic conviction stands (liveness unchanged)");
    // The held share is not slashed yet: it stays reserved on the producer's real bond until the contest resolves.
    assert_eq!(before - w.net.collateral(0), w.policy().claim_collateral - reward, "slashed but for the held share");
    assert_eq!(w.net.kernel_reserved(0), u128::from(reward), "the held share stays reserved (it cannot leave)");
    let held = ledger.held_bounty_v1(&id).expect("held");
    assert_eq!((held.filer, held.amount), (w.net.kid(copyist), reward), "nobody sealed the copy: its share is held");
    assert_eq!(minted_and_owed(&w.net, &w.net.chain, copyist), (0, 0), "the copyist is not paid");
    w.proof(finder, &id, proof.clone()).await;
    assert!(w.net.ledger().held_bounties_v1().is_empty(), "contested and paid once");
    assert_eq!(before - w.net.collateral(0), w.policy().claim_collateral, "the producer is slashed once, in full");
    assert_eq!(w.net.kernel_reserved(0), 0);
    let (fm, fo) = minted_and_owed(&w.net, &w.net.chain, finder);
    assert_eq!(fm + fo, reward, "the finder, whose seal predates the conviction, takes the held share");
    // The copyist cannot come back for it.
    w.proof(copyist, &id, copy).await;
    assert_eq!(minted_and_owed(&w.net, &w.net.chain, copyist), (0, 0));
    let (fm2, fo2) = minted_and_owed(&w.net, &w.net.chain, finder);
    assert_eq!(fm2 + fo2, reward, "paid once");
    let z = w.net.replay().await;
    w.net.assert_same(&z, "sealed-bounty replay");
}

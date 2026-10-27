//! **Lane KNODE: the kernel route's slice roles, run by kaspad's ACTUAL service** (`PalwPanelService` → `palw_kernel_roles`), over
//! the parent's harness (testnet-12 fixture, the canonical `wide128_v1` class registered through onboarding, Panel-licensed route).
//! Nothing is signed by hand except the harness's own demand and the colluding seats' cover (the parent's `cover`): the job is posted
//! by the poster's service, the claim is decoded/sealed/revealed by the producer's service with its own copy of the registered model,
//! demands are answered by the producer's service or — when the drill keeps it silent — by a relay's service from an exported file,
//! and a drill injector's service files a wrong challenge that the court dismisses. No cryptographic proof of computation is used.
use super::*;

fn roles() -> crate::palw_panel::palw_kernel_roles::PalwKernelRolesConfigV1 {
    Default::default()
}

async fn finish(node: KNode) {
    let mut running = node.running;
    running.finish().await;
}

/// The card's service with `roles`, in its own state dir.
fn service(
    w: &KWorld,
    dir: &std::path::Path,
    card: usize,
    model: &std::path::Path,
    r: crate::palw_panel::palw_kernel_roles::PalwKernelRolesConfigV1,
    keep: &mut Vec<kaspa_database::utils::DbLifetime>,
) -> KNode {
    let sub = dir.join(format!("card-{card}"));
    std::fs::create_dir_all(&sub).unwrap();
    start_kernel_service_with(&w.chain, &sub, w.chain.bonds[card], Some(card), &w.floats[card], model, r, keep)
}

/// The only claim of `producer`'s bond, if any.
fn claim_of(chain: &Chain, producer: usize) -> Option<Digest> {
    let me = palw_kernel_bond_id_v1(&chain.bonds[producer]);
    ledger_of(chain).claims.iter().find(|(_, r)| r.producer == me).map(|(id, _)| *id)
}

/// `Respond` objects about `claim` that `signer` carried.
fn responds_by(chain: &Chain, signer: PalwBondKeyV2, claim: &Digest) -> usize {
    chain
        .history
        .iter()
        .flat_map(|b| b.transactions.iter())
        .filter_map(|t| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.payload).ok())
        .filter(|p| match &p.object {
            PalwConsensusObjectV2::KernelRouteV1 { bytes, signer: s, .. } if *s == signer => {
                matches!(K::decode(bytes), Ok(K::Respond { claim: c, .. }) if c == *claim)
            }
            _ => false,
        })
        .count()
}

fn answered(chain: &Chain, key: &(Digest, u8, u32)) -> bool {
    let ledger = ledger_of(chain);
    ledger.served.contains_key(key) || ledger.demands.get(key).is_some_and(|d| d.last.is_some())
}

/// **Poster → producer → demand answered (T01 honest half, T08 export), then a drill wrong challenge dismissed.** The poster's service
/// posts the single-prompt job; the producer's service decodes it with its own model, seals and reveals; a demand on its claim is
/// answered by its service (and exported for a relay); the colluding Panel covers it; a drill injector's service seals and files a
/// wrong challenge (the honest first token accused with its true logits) and the court dismisses it: the claim is never convicted.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn knode_poster_and_producer_services_carry_an_honest_claim_answer_its_demand_and_a_wrong_challenge_is_dismissed() {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let mut w = KWorld::new().await;
    let dir = tempfile::tempdir().unwrap();
    let model = w.model_file(dir.path());
    let mut keep = Vec::new();

    // Poster (the registrant card's service).
    let mut r = roles();
    r.post_job = Some((vec![3, 17, 9], 3));
    let poster = service(&w, dir.path(), REGISTRANT, &model, r, &mut keep);
    run_until(&mut w.chain, &poster, 300, "the service's job posted", |c| !ledger_of(c).jobs.is_empty()).await;
    finish(poster).await;
    let job = ledger_of(&w.chain).jobs.values().next().unwrap().clone();
    assert_eq!((job.prompt.clone(), job.max_new_tokens, job.class_binding_id), (vec![3, 17, 9], 3, w.class));

    // Producer (card 0), exporting its answers.
    let export = dir.path().join("export");
    let mut r = roles();
    r.producer = true;
    r.export_dir = Some(export.clone());
    let producer = service(&w, dir.path(), 0, &model, r, &mut keep);
    run_until(&mut w.chain, &producer, 300, "the producer's claim committed", |c| claim_of(c, 0).is_some()).await;
    let claim = claim_of(&w.chain, 0).unwrap();
    let carried = carried_by(&w.chain, w.chain.bonds[0], &claim);
    eprintln!("[knode] producer carried {carried:?}");
    // A demand on it (the harness's card 2), answered by the producer's service.
    let key = (claim, 0u8, 1u32);
    let o = route_obj(
        &w.chain,
        2,
        w.chain.bonds[2],
        &K::FileDemand { demander: palw_kernel_bond_id_v1(&w.chain.bonds[2]), claim, stage: 0, position: 1 },
    );
    w.chain.send(2, &o, &mut w.floats[2], &[]).await;
    assert!(ledger_of(&w.chain).demands.contains_key(&key), "the demand is open");
    run_until(&mut w.chain, &producer, 300, "the producer's service answers the demand", |c| answered(c, &key)).await;
    finish(producer).await;
    let file = export.join(crate::palw_panel::palw_kernel_roles::palw_kernel_relay_file_name_v1(&claim, 0, 1));
    assert!(file.exists(), "the answer was exported for a relay: {}", file.display());
    assert!(!ledger_of(&w.chain).claims[&claim].convicted, "an honest claim answered: nothing convicts");

    // The colluding Panel covers it; a drill injector (card 3) files a wrong challenge.
    w.cover(&[claim]).await;
    let mut r = roles();
    r.inject = vec![crate::palw_panel::palw_kernel_roles::PalwKernelInjectV1::WrongChallenge];
    let injector = service(&w, dir.path(), 3, &model, r, &mut keep);
    let accuser = palw_kernel_bond_id_v1(&w.chain.bonds[3]);
    run_until(&mut w.chain, &injector, 400, "the wrong challenge filed", |c| {
        carried_by(c, c.bonds[3], &claim).get("FileProof").copied().unwrap_or(0) >= 1
    })
    .await;
    finish(injector).await;
    w.chain.heartbeat(vec![]).await;
    let ledger = ledger_of(&w.chain);
    assert!(
        ledger.proof_seals.contains_key(&(claim, accuser)) || carried_by(&w.chain, w.chain.bonds[3], &claim).contains_key("SealProof")
    );
    assert!(!ledger.claims[&claim].convicted, "the court dismissed the wrong challenge");
}

/// **A lying, silent producer; a third-party relay answers (T08) — the answer convicts nothing by itself but is carried.** The
/// producer's service (drill `lie` + `silent`) commits a trace with one MatMul value bumped and answers no demand; the answer bytes
/// the producer committed to (recomputed from the model with the same drill lie, as the producer's export would hold) are placed in a
/// relay directory, and the relay's service (card 3) signs and carries them for the open demand.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn knode_a_silent_lying_producer_and_a_relay_service_carries_its_committed_answer() {
    use crate::palw_panel::palw_kernel_roles::{PalwKernelInjectV1, palw_kernel_position_bytes_v1, palw_kernel_relay_file_name_v1};
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let mut w = KWorld::new().await;
    let job = w.job().await;
    let dir = tempfile::tempdir().unwrap();
    let model = w.model_file(dir.path());
    let mut keep = Vec::new();
    let mut r = roles();
    r.producer = true;
    r.inject = vec![PalwKernelInjectV1::Lie, PalwKernelInjectV1::Silent];
    let producer = service(&w, dir.path(), 0, &model, r, &mut keep);
    run_until(&mut w.chain, &producer, 300, "the lying claim committed", |c| claim_of(c, 0).is_some()).await;
    let claim = claim_of(&w.chain, 0).unwrap();
    assert_eq!(ledger_of(&w.chain).claims[&claim].job_id, job.id());
    let key = (claim, 0u8, 1u32);
    let o = route_obj(
        &w.chain,
        2,
        w.chain.bonds[2],
        &K::FileDemand { demander: palw_kernel_bond_id_v1(&w.chain.bonds[2]), claim, stage: 0, position: 1 },
    );
    w.chain.send(2, &o, &mut w.floats[2], &[]).await;
    // The silent producer's service runs on and answers nothing.
    for _ in 0..6 {
        pool_heartbeat(&mut w.chain, &producer.manager, &producer.pool).await;
        tokio::time::sleep(Duration::from_millis(2_200)).await;
    }
    assert!(!answered(&w.chain, &key), "the silent producer answered nothing");
    finish(producer).await;
    // The relay.
    let relay_dir = dir.path().join("relay");
    std::fs::create_dir_all(&relay_dir).unwrap();
    let bytes = palw_kernel_position_bytes_v1(&ledger_of(&w.chain), &claim, &w.fx.params, true, 1).unwrap();
    let item: crate::palw_panel::palw_kernel_roles::PalwKernelRelayItemV1 = (claim, 0, 1, bytes);
    std::fs::write(relay_dir.join(palw_kernel_relay_file_name_v1(&claim, 0, 1)), borsh::to_vec(&item).unwrap()).unwrap();
    let mut r = roles();
    r.relay_dir = Some(relay_dir);
    let relay = service(&w, dir.path(), 3, &model, r, &mut keep);
    run_until(&mut w.chain, &relay, 300, "the relay carried the answer", |c| answered(c, &key)).await;
    finish(relay).await;
    assert_eq!(responds_by(&w.chain, w.chain.bonds[3], &claim), 1, "one relayed Respond, signed by the relay");
}

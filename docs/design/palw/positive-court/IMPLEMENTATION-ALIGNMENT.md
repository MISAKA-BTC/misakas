# Positive Court R2 — 既存実装との対応確認

対象は `misakas-main (1)(1).zip`。静的な限定照合であり、全laneの監査・Rust実行・現在の配備状態の確認ではない。Git commitはZIPから未確定。

**結論:** 「検証側が反証を構築・提出し、producerが資料へ応答する」という役割は既存kernel経路にある。ただしproducerの証拠構築も残り、モデルはverifierが自ら保有する前提。今回の新しいPositive Court・軽量結果commit・Panel witness責任分離が既に実装済みという意味ではない。

## `kaspad/src/palw_kernel_verifier.rs`

検証側の再実行、不正所見からFileProof生成、carrierへ分割送信。正常はこの経路で証明提出されない。

SHA-256: `5d1791e6d3ff9b3a91978acccfd1d774815236c8f43e297cfc0fdbb6b667b4a2`

```text
L11: //! 2. **Re-execute independently** with the operator's own copy of the registered model (ADR-0177 D7: a verifier holds the registered
L12: //!    model; `--palw-kernel-model`, one file a model, matched to a class only by its registered parameter commitments — a model that
L13: //!    does not open to them is never used). A single-program claim is replayed from the ledger's public commitments alone
L14: //!    (`pursuit::kernel_check_program_v1`: no producer value is read, nothing is ever demanded of an honest producer). A segmented claim
L15: //!    is replayed once per process (`seg_detect::prepare_reexecution_v1`) and searched against the `Respond` parts the accepted blocks
L16: //!    carry (`pursuit::KernelBlockMaterialV1`), its prompt from the authenticated tiles (`pursuit::kernel_tiled_prompt_v1`). A typed
L17: //!    `Memory` claim (RFC-0004) is replayed from its public pre-state (the chain's carried post-state, or M0 from the model) by
L18: //!    `pursuit::kernel_check_typed_v1`; pipelines, composites and retrieval snapshots need more than one model and are not checked here.
L19: //! 3. **Pursue** — the finding is the goal of `misaka_palw_remote::pursuit::plan_kernel_pursuit_on_v1`, which returns the step the chain
L20: //!    still needs (seal → wait → file whole or in tag-113 chunks; demand → await the answer → re-check; conviction or the producer's DA
L21: //!    default), dry-run against a clone of the rebuilt ledger before anything is sent: a filing the court would dismiss, a demand past
L22: //!    the scope or the horizon, is reported and never sent.
L23: //! 4. **Send** — the step's objects (`pursuit::kernel_step_sends_v1`) are signed DETERMINISTICALLY (the service's ML-DSA-87 signer is
L24: //!    FIPS 204's deterministic variant, `rnd = 0^32` = `PURSUIT_DETERMINISTIC_RND_V1`: a restarted service rebuilds the same bytes, the
L25: //!    same chunk group) and queued on the court lane (the priority lane's queue, `carry_priority_v1`), which funds each carrier from the
L26: //!    virtual UTXO set at the object's own minimum (mature, unlocked, free; chained through its change, never a deposit spent twice) and
L27: //!    submits it to the PQ mempool, from which the production template and the real block pipeline take it.
```

```text
L420:         for claim in candidates {
L421:             let row = &ledger.claims[&claim];
L422:             let root = kernel_class_model_root_v1(&ledger, &row.class_binding_id).expect("filtered above");
L423:             let model = book.model_for(&root).expect("filtered above");
L424:             let goal = self.kernel_verifier_goal_v1(book, &ledger, &claim, model.as_ref(), salt, &objects);
L425:             let case = book.cases.entry(claim).or_default();
L426:             let goal = match goal {
L427:                 Ok(Some(goal)) => goal,
L428:                 Ok(None) => continue,
L429:                 Err(why) => {
L430:                     if case.unjudged.as_deref() != Some(why.as_str()) {
L431:                         warn!("[{PALW_PANEL}] kernel verifier: claim {} is not judged here: {why} (G14-K)", Hash64::from_bytes(claim));
L432:                         case.unjudged = Some(why);
L433:                     }
L434:                     continue;
L435:                 }
L436:             };
L437:             let pursuit = KernelPursuitV1 { claim, bond: bond_key, goal, synced_collateral: Some(synced) };
L438:             let filing = match &pursuit.goal {
L439:                 KernelGoalV1::Prove(proof) => self.kernel_route_signed_v1(
L440:                     network_domain,
L441:                     bond_key,
L442:                     &KernelRouteObjectV1::FileProof { accuser: me, claim, proof: proof.clone() },
L443:                 ),
L444:                 KernelGoalV1::Demand(_) => None,
L445:             };
```

```text
L469:             let sends = match kernel_step_sends_v1(&step, &pursuit, f.as_ref()) {
L470:                 Ok(sends) => sends,
L471:                 Err(why) => {
L472:                     warn!("[{PALW_PANEL}] kernel verifier: claim {}: {why}", Hash64::from_bytes(claim));
L473:                     continue;
L474:                 }
L475:             };
L476:             for send in sends {
L477:                 let object = match send {
L478:                     KernelSendV1::Route(KernelRouteObjectV1::FileProof { .. }) => filing.clone(),
L479:                     KernelSendV1::Route(k) => self.kernel_route_signed_v1(network_domain, bond_key, &k),
L480:                     KernelSendV1::Chunk(chunk) => {
L481:                         let message = palw_kernel_chunk_message_v1(network_domain, &chunk);
L482:                         self.sign(message.as_byte_slice(), PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1)
L483:                             .map(|signature| PalwConsensusObjectV2::KernelRouteChunkV1 { chunk: Box::new(chunk), signature })
L484:                     }
```

## `misaka/palw/remote/src/pursuit.rs`

CleanはNone、Prosecute/Demandを計画。Positiveを作る新分岐が必要。

SHA-256: `c7b6ad8e77e705f92bcd43d4a6e9a3a73d1312d8bfae6a488b04da3e07481495`

```text
L386: /// The goal a finding sets (`None`: the claim checks — an honest verifier has nothing to send).
L387: pub fn kernel_goal_of_finding_v1(finding: misaka_palw_kernel::ledger::OutsiderFindingV1) -> Option<KernelGoalV1> {
L388:     use misaka_palw_kernel::ledger::OutsiderFindingV1;
L389:     match finding {
L390:         OutsiderFindingV1::Clean => None,
L391:         OutsiderFindingV1::Prosecute(proof) => Some(KernelGoalV1::Prove(proof)),
L392:         OutsiderFindingV1::Demand(positions) => Some(KernelGoalV1::Demand(positions)),
L393:     }
L394: }
L395:
L396: /// **A single-program (K2-TIR v1/v2) or pipeline claim, checked by exact replay** from the ledger's public commitments, the job and
L397: /// the verifier's registered model (`OutsiderV1::check_computation`): no producer value is read, so no position is ever demanded of an
L398: /// honest producer and a lie is proved from what is already public. `Err`: the verifier cannot check (its model does not open to the
L399: /// registered commitments, a typed or segmented claim) — never producer misconduct, never a filing.
L400: pub fn kernel_check_program_v1(
L401:     ledger: &KernelLedgerV1,
L402:     claim: &Digest,
L403:     model: &dyn misaka_palw_kernel::ledger::PublicArtifactV1,
L404:     salt: Digest,
L405: ) -> Result<Option<KernelGoalV1>, String> {
L406:     misaka_palw_kernel::ledger::OutsiderV1 { ledger, claim: *claim, material: &NoProducerValuesV1, artifact: model, salt }
L407:         .check_computation()
L408:         .map(kernel_goal_of_finding_v1)
```

## `kaspad/src/palw_kernel_roles.rs`

producerはbuild_evidenceを実行し、FileDemandへRespond。新しい軽量commitはまだない。

SHA-256: `cf409624bab5668fa746bfcb54a8b091ca1a954f87169362c1f330754dbc9ca0`

```text
L191: /// **The producer's claim of `job`**: greedy (the job's rule) decode with its own model, the evidence, the claim id.
L192: /// Returns `(claim id, plain reveal)`.
L193: pub(crate) fn palw_kernel_produce_v1(
L194:     ledger: &KernelLedgerV1,
L195:     job: &KernelJobV1,
L196:     producer: Digest,
L197:     model: &MapParams,
L198:     lie: bool,
L199: ) -> Result<(Digest, K), String> {
L200:     let row = ledger.classes.get(&job.class_binding_id).ok_or("the job's class is not a single-program class")?;
L201:     let (post, logits) = row.logits_at();
L202:     let mut stream = job.prompt.clone();
L203:     let mut generated = Vec::with_capacity(job.max_new_tokens as usize);
L204:     for _ in 0..job.max_new_tokens {
L205:         let t = trace_v1(&row.program, model, &stream).map_err(|e| format!("decode: {e:?}"))?;
L206:         let tensor = t
L207:             .values
L208:             .get(stream.len() - 1)
L209:             .and_then(|v| v.get(post as usize))
L210:             .and_then(|v| v.get(logits as usize))
L211:             .ok_or("the class's logits node is not in the trace")?;
L212:         let tok = job.decode.select(tensor).ok_or("the decode rule selects no token")?;
L213:         generated.push(tok);
L214:         stream.push(tok);
L215:     }
L216:     let fed_stream = claim_stream(job, &generated);
L217:     let trace = palw_kernel_committed_trace_v1(&row.program, model, &fed_stream, lie)?;
L218:     let w = WiringV1::new(&row.program).map_err(|e| format!("wiring: {e:?}"))?;
L219:     let ev = build_evidence_v1(&w, &trace.evidence(), &fed_stream, row.header(job.class_binding_id), &row.descriptor, 2)?;
L220:     let claim = KernelClaimV1 { job_id: job.id(), producer_bond: producer, generated, evidence_root: ev.root() };
L221:     let id = claim.id();
L222:     Ok((id, K::CommitClaim { claim, evidence: ev, commitments: trace.evidence().commitments }))
L223: }
L224:
L225: /// The seal and the reveal of `reveal` under the ledger's seal rule at the next block (`salt` used only once salted seals apply).
```

```text
L581:                 }
L582:                 let Some(salt_sig) = self.sign(&id, PALW_KERNEL_CLAIM_SALT_CONTEXT_V1) else { continue };
L583:                 let salt = misaka_palw_kernel::hash::id(b"misaka-palw/kaspad/kernel-producer/salt/v1", &salt_sig);
L584:                 let Some((seal, reveal)) = palw_kernel_seal_and_reveal_v1(ledger, me, &reveal, salt) else { continue };
L585:                 let step = if ledger.seals.contains_key(&(*job_id, me)) { (reveal, "reveal") } else { (seal, "SealClaim") };
L586:                 if let Some(o) = sign(&step.0) {
L587:                     out.push((id, PALW_KERNEL_PRODUCER_ROUND_V1, o, step.1));
L588:                 }
L589:             }
L590:             // Answer every open demand on this bond's claims (unless the drill keeps it silent).
L591:             if !cfg.injects(PalwKernelInjectV1::Silent) {
L592:                 for key in ledger.demands.keys().filter(|k| demand_open(ledger, k)) {
L593:                     let (claim, stage, position) = *key;
L594:                     let Some(row) = ledger.claims.get(&claim).filter(|r| r.producer == me) else { continue };
L595:                     let Some(model) = model_of(&row.class_binding_id) else { continue };
L596:                     match palw_kernel_position_bytes_v1(ledger, &claim, &model, lie, position) {
L597:                         Ok(bytes) => {
L598:                             if let Some(dir) = &cfg.export_dir {
L599:                                 let path = dir.join(palw_kernel_relay_file_name_v1(&claim, stage, position));
L600:                                 let item: PalwKernelRelayItemV1 = (claim, stage, position, bytes.clone());
L601:                                 if let Err(e) = std::fs::create_dir_all(dir)
L602:                                     .and_then(|_| std::fs::write(&path, borsh::to_vec(&item).expect("serializes")))
L603:                                 {
L604:                                     warn!("[{PALW_PANEL}] kernel producer: cannot export {}: {e}", path.display());
L605:                                 }
L606:                             }
L607:                             if let Some(o) = sign(&K::Respond { claim, stage, position, bytes }) {
L608:                                 out.push((claim, PALW_KERNEL_PRODUCER_ROUND_V1, o, "Respond"));
L609:                             }
L610:                         }
L611:                         Err(why) => warn!(
L612:                             "[{PALW_PANEL}] kernel producer: claim {}: cannot answer position {position}: {why}",
```

## `consensus/core/src/palw_kernel_route_fold_v1.rs`

Node側foldがledger.apply_object_reserved_v1で検証を実行。Panelの宣言だけを最終裁定として受け取る説明ではない。

SHA-256: `9c8b072ddc51a5931b1d2d26deff797ab320979c69f2af07dc4a99c03e4ce2b6`

```text
L2157:     let proof = matches!(object, KernelRouteObjectV1::FileProof { .. });
L2158:     let mut ledger = load_ledger_with_projection_v9(builder, ctx, projection_paid, proof)?;
L2159:     let original_attested = ledger.attested_artifacts.clone();
```

```text
L2313:     let events =
L2314:         match ledger.apply_object_reserved_v1(object, &AuthV1 { signer_bond: kid }).map_err(PalwStateV2Error::NativeResource)? {
L2315:             Ok(events) => events,
L2316:             Err(_refusal) => {
L2317:                 // A refusal that did court work still spent the block's budget (so junk cannot be tried for free); nothing else is kept.
L2318:                 persist_budget(builder, ctx, &ledger)?;
L2319:                 return Ok(());
L2320:             }
L2321:         };
L2322:     // A class registers only if its worst filing, response and commitments can actually be carried (chunking counted).
```

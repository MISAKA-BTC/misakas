# PALW / RFC18 DAG並列性 — 行番号付きソース証拠
- 対象: `misakas-pre (1).zip`
- SHA-256: `0c356b39f40d57d8c4d4d69321f38d38b766865024255f38897bfe8206c3b8b0`
- 前提: 関連fence有効時のコード分岐を評価。現testnetで無効という点を並列性の不成立理由にはしない。
- 本書はZIP内ソースの抜粋であり、コメント中のPASS/FAILや測定値は今回の再実行結果ではない。
- Rust/cargo・実node・実モデルの実行は行っていない。`rule_check_results.json`は限定したPython独立検査のみ。
## 目次
- D01: RFC18の目標・siblingの要件・認証待ち時の進行
- D02: 複数親・GHOSTDAG・実templateの接続
- D03: 全体一名のproposer選出でなく各attemptの閾値判定
- D04: BLUE/RED merged workの採用とstateへの反映
- D05: 同一mergeset中のmodel round robinと、その範囲
- D06: claimごとのPanel derivation / model-specific readiness
- D07: 適格REDの報酬は本人へ・escrowの控除
- D08: Gate Cのglobal poolと複数claim sweep・抽選予算
- D09: 共有safe frontierとoldest unresolvedの影響
- D10: 共有capacity / per-DAA escrow budget / bond制約
- D11: DAG幅scheduleはstub、既存の2分設定
- D12: genesis固定operatorだけでないpermissionless anchorの接続
- D13: 認証creditゼロ時のGHOSTDAG比較、旧DNS gate退役
- D14: 3モデルsibling試験が実際にassertする範囲
- D15: 実プロセスdriverのPanelチェック不足
- D16: 同梱の過去検証記録：今回再実行した結果ではない
- D17: 一Panel serviceのreplay並列性とmain carrier局所制約
- D18: 参考：並列とは別のFP receipt / Gate C未完了事項

## D01 — RFC18の目標・siblingの要件・認証待ち時の進行

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md:29–41`

```text
    29 | ## 0. 要約と目標状態
    30 | 
    31 | **目標状態:** 「多数のモデルごとにminerとPanelが独立して同時稼働し、それぞれの計算がDAGに取り込まれ、特定のPoS側の生成者選出や確定制御に妨げられず、全体として一つの合意に収束する」。
    32 | 
    33 | DAG(GHOSTDAG、複数親、REDの取引受理)は作り直さない。現状は (a) 複数親・複数tip・RED受理がすでに機能し、(b) PALWの採否は単一proposer選出ではなく計算由来のticketとbond予算で決まり、(c) RED merged workもPALWでは数える。残る障害は四つである。
    34 | 
    35 | | 障害 | 現状 | 本RFC |
    36 | | --- | --- | --- |
    37 | | sibling間の順序依存 | 共有上限(DAA当たり発行予算、network room、per-bond)で競合したとき、consensus order(blue workとhash)の先頭が勝つ。ADR-0170のseed anchorは「最後に採用されたattempt」で決まる | canonical sibling order(モデル間round robin、§4) |
    38 | | Panel anchorの特権 | testnet-12はDAA 750からgenesisの8 operator bondのattemptだけがanchor(lane A)。headerだけで判定し、merged workで拒否されたattemptもanchorになり得る | 許可不要の集約anchor(§5) |
    39 | | 未認証claimの合意credit | `Final`のclaimはsourceを問わずfrontierを進め、settlementはattemptの`Final`を数え、rule Eはvoidされたattemptも数える。閉鎖モデルの自作自演でもfinalityが進む。testnet-12の現在の保護はDNS-BFT gateに依存しており、DNS-BFTは廃止済み | Gate C: `SecurityCertified`だけがcreditを生む(§7)、rule E v2(§6) |
    40 | | DAG幅 | 120 s / k = 1 / parents 10 / mergeset 180。長時間推論(p50 340 s)はk = 1でほぼ全てRED | 休眠schedule仕様とstub(§8) |
    41 | 
```

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md:98–127`

```text
    98 | ## 4. 設計変更1: 多モデル同時生成とRED取り込み(`palw_dag_sibling_attribution_v1`)
    99 | 
   100 | ### 4.1 不変条件
   101 | 
   102 | * **I-S1(保持):** 有効なsibling blockは色に関わらず全て保持され、merge depth以内に後続blockのmergesetへ入る(GHOSTDAGの既存性質)。MUST NOT: PALWの規則でsiblingをparent候補から外す。
   103 | * **I-S2(独立記録):** 各claimは自分のbond・class・carrying blockで記録される(`attempt_id_v2`、`execution_key`、`accepted_block`)。Panelは`claim_id`で鍵付けされ、seedはclaim idを含む(`palw_panel_draw_seed_v1`)。siblingのPanel・escrow・報酬を混同しない(MUST)。
   104 | * **I-S3(順序非依存):** 一つのaccepting blockが採用するmerged attemptの集合は、mergesetの**集合**の関数でなければならない(MUST)。色・blue work・merge順・到着順に依存しない。
   105 | * **I-S4(モデル間公平):** 共有上限で競合したとき、各モデルのsiblingが1件ずつ採用されてから、どのモデルの2件目も採用される(round robin)。
   106 | * **I-S5(seed anchor):** ADR-0170のseed anchorは、同じspanで採用されたattempt(自身・merged、BLUE/RED)のうち`H(domain ‖ execution key)`が最小のもの。last-write-winsではない(`record_round_seed_anchor_of`、`transition_builder.rs`)。
   107 | * **I-S6(selected parentのsibling):** siblingのうちGHOSTDAGがselected parentに選んだものは、自分のblockの遷移で先に採用される(merger のmergesetに入らない)。どれが選ばれるかはGHOSTDAGの決定的な規則(blue work、次にblock hash)で決まり、到着順にもmerge順にも依存しない。block hashを動かすには勝ちticket(推論1回)がもう一度要る。共有上限で負けるのは、このselected parentを除いたmergeset内のsiblingで、それはcanonical順で決まる。
   108 | 
   109 | ### 4.2 規則(fence以後、accepting blockのDAAで判定)
   110 | 
   111 | step 4bの前に、mergesetのattempt workだけを並べ替える。非attempt work(receipt spend等)は元の位置に残す。順序は、モデル(class)ごとに`H(domain ‖ execution key)`の昇順、モデル同士は各モデル先頭の優先度順、1巡につき1モデル1件のround robin(`palw_sibling_canonical_order_v1`、`consensus/core/src/palw_rfc0018_v1.rs:283`、fold `palw_state_v2.rs:14481-14496`)。優先度はsibling自身の推論で決まり、accepting minerは動かせない。siblingのminerが動かすには別の勝ちticket(推論1回)が要る。
   112 | 
   113 | ### 4.3 mergeset上限とREDの期限
   114 | 
   115 | * mergeset 180、parents 10、heartbeat ≤ 4/mergesetは変えない。round robinはこの上限の内側で働く。
   116 | * REDのPALW workの期限はGHOSTDAGのmerge depth(testnet-12で30 block)である。これを超えるREDは`ViolatingBoundedMergeDepth`で失われる。推論時間 + 伝播はmerge depthの時間(約60分)より十分短くなければならない(SHOULD、§8で計測)。
   117 | * coinbaseのcarve差し引きは採用集合の超集合のまま(`palw_v2_merged_escrow_withheld`)。負けたsiblingのcarveは焼却。本RFCは「どれが負けるか」をmerge順の偶然からround robinへ変えるだけで、焼却の規則は変えない。
   118 | 
   119 | ### 4.4 計測と受入試験
   120 | 
   121 | | ID | 試験 | 合格条件(記録する値) | 実装 |
   122 | | --- | --- | --- | --- |
   123 | | T-1a | 3モデル・3 miner・別Panel、同じ親からsibling(in-process、k = 1) | 3 blockともmergerの過去にある(selected parent/BLUE/RED)。3 claimとも自分のbond・class・carrying blockで記録 | `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs` |
   124 | | T-1b | 同じblock群を別nodeへ、3 siblingの到着順を全6通り | sink、selected parent、blues、reds、PALW state root(採用claim・消費した上限・escrow)、safe frontier、ADR-0170 seed anchorが一致 | 同上 |
   125 | | T-1c | sibling fence armで同じ2試験 | 同上 | 同上 |
   126 | | T-1d | canonical順は集合の関数、round robin、非attempt slot不変 | 回転・逆順で同じ列。3モデルの先頭3件は3モデル | `palw_rfc0018_v1/tests.rs` |
   127 | | T-1e | 実process: 3 producer nodeを分離して同じbaseから3モデルを同時採掘、再接続 | 各siblingの色、claim記録、帰属、Panelがclaimごとに別 | `scripts/drills/rfc0018_dag_drill.py siblings` |
```

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md:191–219`

```text
   191 | ## 7. Gate C — `SecurityCertified`(`palw_independently_work_certified_v1`、ユーザー方針により採用)
   192 | 
   193 | ### 7.1 二つの状態
   194 | 
   195 | | 状態 | 意味 | 何が起きるか |
   196 | | --- | --- | --- |
   197 | | `Final` | claimの手続きが完了 | 利用者払いescrowの経済的settlementは既存規則どおり |
   198 | | `SecurityCertified` | 合意creditを持ってよい | safe frontier、safe weight、bounded immature/live total、settlement depth、rule E participation、settled anchor、発行由来(coinbase)報酬legに寄与 |
   199 | 
   200 | 不変条件(fence以後、claimの受理DAAで判定):
   201 | 
   202 | * **I-C1:** `SecurityCertified`でないclaimは、上の7つのどれにも寄与しない(MUST NOT)。
   203 | * **I-C2:** `Final`、Panelのlicence、Panelの過半、モデル保有者の集合、登録者が満たせる集合のいずれも、単独で認証にならない(MUST NOT)。
   204 | * **I-C3:** 認証済みclaimが後でG14の公開告発で反証されたら、voidされ、合意creditは取り消される(MUST)。safe weightは`reverse_convicted_final`で引かれ、participation・settlement depthから消える。
   205 | * **I-C4:** DNS-BFTおよびDNS finality anchorに安全性・収束を依存しない(MUST NOT)。
   206 | * **I-C5:** 何も認証されない間も、DAG、通常取引、heartbeat/BASE-0の時計、DAAは進む。止まるのはfinality creditだけ(MUST)。
   207 | 
   208 | ### 7.2 認証述語(前提に基づく認証)
   209 | 
   210 | 1. **commit:** 支配されたclaimの`Final`で、claim・producer・母集団の切断点を固定する(`commit_at_final_v1`、`consensus/core/src/palw_security_certification_fold_v1.rs:45`)。
   211 | 2. **draw:** commitより厳密に後のexecution laneのseed anchor(ADR-0130)から作るseedで、**global pool**(Active、commitの`verifier_maturity`前に登録、最小担保以上、producerのoperatorを除く)から`m`位置を担保比例・復元抽選する(`palw_certification_pool_admits_v1`、`palw_security_certification_v1.rs:280`、RFC-0017/R7-ECONの抽選を再利用)。**モデル保有の有無で母集団を絞らない**。
   212 | 3. **attest:** 抽選されたbondは期限内に、そのclaimを再実行して判定を署名する(RFC-0017のinner 75を、exam idの代わりにcertification idで使う、`try_attest_claim_certification_v1`、fold `:111`)。
   213 | 4. **decide:** `k`位置以上が`Reproduced`(mismatchなし)で、claimがまだ`Final`、未解決の告発がなく、**G14公開告発window**(`prosecution_window_daa`)が`Final`から経過したら`Certified`(`palw_certification_verdict_v1`、`:320`)。期限までに`k`未満なら`NotCertified`、母集団不足は`NoPopulation`、`certify_by`までに抽選できなければ`Expired`、`Final`でなくなれば`Withdrawn`。**1 claimにつき1回、再試行なし、classごとに数える**(attemptの有界化と計数)。network全体のepoch当たり抽選数にも上限がある。
   214 | 5. **公開check経路:** 決定論的な公開検査(G14完了classの公開material)が固定されたclassは、告発window経過で`PublicCheckCertified`。このbinaryでは固定classは0件(`PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1 = &[]`)。
   215 | 
   216 | **前提(明示):** 認証は抽選検証者の署名付き承認に依る。**「抽選されたverifier stakeのうち正直な割合が閾値(`k/m`)以上である」ことを前提とする。** 署名は再計算の証明ではない。これは無条件の数学的証明ではなく、前提に基づく認証である。RPCはこれを`securityCertificationPremise`として返す(§7.8)。
   217 | 
   218 | 閉鎖モデルは、正直な抽選検証者が入手も再実行もできないので、正直な位置から肯定承認を得られない。攻撃者が`m`位置中`k`以上を引いたときだけ認証される。
   219 | 
```

## D02 — 複数親・GHOSTDAG・実templateの接続

### `consensus/src/processes/ghostdag/protocol.rs:244–271`

```text
   244 |     pub fn find_selected_parent(&self, parents: impl IntoIterator<Item = BlockHash>) -> BlockHash {
   245 |         let sortable =
   246 |             |parent: BlockHash| SortableBlock { hash: parent, blue_work: self.ghostdag_store.get_blue_work(parent).unwrap() };
   247 |         if self.round_lane.is_none() {
   248 |             return parents.into_iter().map(sortable).max().unwrap().hash;
   249 |         }
   250 |         // ADR-0125: a round block is never a selected parent. Where a block names chain parents, the
   251 |         // heaviest of them is its selected parent. Where it names only round blocks — which the header
   252 |         // stage admits for a round block alone — its selected parent is the heaviest of their anchors,
   253 |         // which is not a direct parent: the lane extends itself without re-naming the chain block it
   254 |         // hangs from. The last fallback (every parent) is reached only by a trusted block whose
   255 |         // anchors were pruned away, where the syncer supplies the GHOSTDAG data anyway, and is there so
   256 |         // that path cannot panic.
   257 |         let parents: Vec<BlockHash> = parents.into_iter().collect();
   258 |         let non_round: Vec<BlockHash> = parents.iter().copied().filter(|parent| !self.is_round_block(*parent)).collect();
   259 |         if !non_round.is_empty() {
   260 |             return non_round.into_iter().map(sortable).max().unwrap().hash;
   261 |         }
   262 |         parents
   263 |             .iter()
   264 |             .filter_map(|parent| self.ghostdag_store.get_selected_parent(*parent).ok())
   265 |             .filter(|anchor| !anchor.is_origin() && self.ghostdag_store.get_blue_work(*anchor).is_ok())
   266 |             .map(sortable)
   267 |             .max()
   268 |             .or_else(|| parents.iter().copied().map(sortable).max())
   269 |             .unwrap()
   270 |             .hash
   271 |     }
```

### `consensus/src/processes/ghostdag/protocol.rs:303–387`

```text
   303 |     pub fn ghostdag(&self, parents: &[BlockHash]) -> GhostdagData {
   304 |         assert!(!parents.is_empty(), "genesis must be added via a call to init");
   305 | 
   306 |         // Run the GHOSTDAG parent selection algorithm
   307 |         let selected_parent = self.find_selected_parent(parents.iter().copied());
   308 |         // Handle the special case of origin children first
   309 |         if selected_parent.is_origin() {
   310 |             // ORIGIN is always a single parent so both blue score and work should remain zero
   311 |             return GhostdagData::new_with_selected_parent(selected_parent, 1); // k is only a capacity hint here
   312 |         }
   313 |         let k = self.k;
   314 |         // Initialize new GHOSTDAG block data with the selected parent
   315 |         let mut new_block_data = GhostdagData::new_with_selected_parent(selected_parent, k);
   316 |         // Get the mergeset in consensus-agreed topological order (topological here means forward in time from blocks to children)
   317 |         let ordered_mergeset = self.ordered_mergeset_without_selected_parent(selected_parent, parents);
   318 | 
   319 |         // ADR-0105: the selected parent's blue score, from which each `Weighted` candidate's walk
   320 |         // floor is derived below. Read only where the fence is configured.
   321 |         let transparency_base = self
   322 |             .heartbeat_transparent
   323 |             .map(|transparency| (self.ghostdag_store.get_blue_score(selected_parent).unwrap(), transparency.merge_depth));
   324 | 
   325 |         // ADR-0125: which candidates are round blocks, read once. A round block is red, always, and
   326 |         // is not counted against anybody — so it is added before any coloring walk and never reaches
   327 |         // one, and it is not among the candidates that could still turn blue in the bound below.
   328 |         let round_flags: Vec<bool> = match self.round_lane {
   329 |             Some(_) => ordered_mergeset.iter().map(|candidate| self.is_round_block(*candidate)).collect(),
   330 |             None => vec![false; ordered_mergeset.len()],
   331 |         };
   332 |         let mut uncolored_non_round = round_flags.iter().filter(|round| !**round).count() as u64;
   333 | 
   334 |         for (index, blue_candidate) in ordered_mergeset.iter().cloned().enumerate() {
   335 |             if round_flags[index] {
   336 |                 new_block_data.add_red(blue_candidate);
   337 |                 continue;
   338 |             }
   339 |             let lane = self.lane_coloring(blue_candidate, selected_parent);
   340 |             // ADR-0105: the lowest blue score a `Weighted` candidate's coloring walk may reach once it
   341 |             // has passed over a heartbeat. The new block's blue score is the selected parent's plus
   342 |             // its final mergeset blues, which are at most the blues already added (the selected
   343 |             // parent included) plus every candidate not yet colored (this one included). A candidate
   344 |             // whose selected-chain ancestor sits at or above `that - merge_depth` is in the future of
   345 |             // the new block's merge-depth root, which is where `check_bounded_merge_depth` requires
   346 |             // every red to be. (ADR-0125: round blocks can never be blue, so they are not counted —
   347 |             // without them this is the count it always was.)
   348 |             let weighted_floor = match (lane, transparency_base) {
   349 |                 (LaneColoring::Weighted, Some((selected_parent_blue_score, merge_depth))) => {
   350 |                     let most_blues = new_block_data.mergeset_blues.len() as u64 + uncolored_non_round;
   351 |                     (selected_parent_blue_score + most_blues).saturating_sub(merge_depth)
   352 |                 }
   353 |                 _ => 0,
   354 |             };
   355 |             uncolored_non_round -= 1;
   356 |             let coloring = self.check_blue_candidate(&new_block_data, blue_candidate, k, lane, weighted_floor);
   357 | 
   358 |             if let ColoringOutput::Blue(blue_anticone_size, blues_anticone_sizes) = coloring {
   359 |                 // No k-cluster violation found, we can now set the candidate block as blue
   360 |                 new_block_data.add_blue(blue_candidate, blue_anticone_size, &blues_anticone_sizes);
   361 |             } else {
   362 |                 new_block_data.add_red(blue_candidate);
   363 |             }
   364 |         }
   365 | 
   366 |         let blue_score = self.ghostdag_store.get_blue_score(selected_parent).unwrap() + new_block_data.mergeset_blues.len() as u64;
   367 | 
   368 |         let added_blue_work: BlueWorkType = new_block_data
   369 |             .mergeset_blues
   370 |             .iter()
   371 |             .cloned()
   372 |             .map(|hash| {
   373 |                 let header = self.headers_store.get_header(hash).unwrap();
   374 |                 palw_lane_blue_work_v1(
   375 |                     header.pow_algo_id,
   376 |                     header.bits,
   377 |                     header.daa_score,
   378 |                     self.heartbeat_lane,
   379 |                     self.attempt_work_lane,
   380 |                     self.level_work,
   381 |                 )
   382 |             })
   383 |             .sum();
   384 |         let blue_work: BlueWorkType = self.ghostdag_store.get_blue_work(selected_parent).unwrap() + added_blue_work;
   385 | 
   386 |         new_block_data.finalize_score_and_work(blue_score, blue_work);
   387 | 
```

### `consensus/src/pipeline/virtual_processor/processor.rs:5185–5291`

```text
  5185 |     pub(super) fn pick_virtual_parents(
  5186 |         &self,
  5187 |         selected_parent: BlockHash,
  5188 |         mut candidates: VecDeque<BlockHash>,
  5189 |         pruning_point: BlockHash,
  5190 |         // ADR-0125: the round tips, offered after the chain's own candidates. Empty where the lane
  5191 |         // is not configured.
  5192 |         round_tips: Vec<BlockHash>,
  5193 |     ) -> (Vec<BlockHash>, GhostdagData) {
  5194 |         // TODO (relaxed): additional tests
  5195 | 
  5196 |         // Mergeset increasing might traverse DAG areas which are below the finality point and which theoretically
  5197 |         // can borderline with pruned data, hence we acquire the prune lock to ensure data consistency. Note that
  5198 |         // the final selected mergeset can never be pruned (this is the essence of the prunality proof), however
  5199 |         // we might touch such data prior to validating the bounded merge rule. All in all, this function is short
  5200 |         // enough so we avoid making further optimizations
  5201 |         let _prune_guard = self.pruning_lock.blocking_read();
  5202 |         let max_block_parents = self.max_block_parents as usize;
  5203 |         let mergeset_size_limit = self.mergeset_size_limit;
  5204 |         let max_candidates = self.max_virtual_parent_candidates(max_block_parents);
  5205 | 
  5206 |         // Prioritize half the blocks with highest blue work and pick the rest randomly to ensure diversity between nodes
  5207 |         if candidates.len() > max_candidates {
  5208 |             // make_contiguous should be a no op since the deque was just built
  5209 |             let slice = candidates.make_contiguous();
  5210 | 
  5211 |             // Keep slice[..max_block_parents / 2] as is, choose max_candidates - max_block_parents / 2 in random
  5212 |             // from the remainder of the slice while swapping them to slice[max_block_parents / 2..max_candidates].
  5213 |             //
  5214 |             // Inspired by rand::partial_shuffle (which lacks the guarantee on chosen elements location).
  5215 |             for i in max_block_parents / 2..max_candidates {
  5216 |                 let j = rand::thread_rng().gen_range(i..slice.len()); // i < max_candidates < slice.len()
  5217 |                 slice.swap(i, j);
  5218 |             }
  5219 | 
  5220 |             // Truncate the unchosen elements
  5221 |             candidates.truncate(max_candidates);
  5222 |         } else if candidates.len() > max_block_parents / 2 {
  5223 |             // Fallback to a simpler algo in this case
  5224 |             candidates.make_contiguous()[max_block_parents / 2..].shuffle(&mut rand::thread_rng());
  5225 |         }
  5226 | 
  5227 |         let mut virtual_parents = Vec::with_capacity(min(max_block_parents, candidates.len() + 1));
  5228 |         virtual_parents.push(selected_parent);
  5229 |         let mut mergeset_size = 1; // Count the selected parent
  5230 | 
  5231 |         // ADR-0068 Phase 1 (F3a/F5): the heartbeat width tracking, `None` where the lane is not
  5232 |         // armed (no header can be a heartbeat there, so the set is vacuously empty and the
  5233 |         // per-member header reads are skipped). The selected parent is in the mergeset, so it
  5234 |         // seeds the set. Admissibility is decided over the WHOLE accumulated set — flat bound,
  5235 |         // or the F5 chain exemption — mirroring `check_mergeset_heartbeat_width` exactly, so a
  5236 |         // template never builds what consensus refuses and never refuses what consensus admits.
  5237 |         let track_heartbeats = self.palw_heartbeat_width_fence.is_some();
  5238 |         let mut heartbeat_set: Vec<(u64, BlockHash, u64)> = Vec::new();
  5239 |         // H3: whether the chain exemption must also be paced. Judged at the selected parent's score
  5240 |         // PLUS ONE — the highest score the block this template becomes can carry — so a template at
  5241 |         // the fence applies the rule wherever the header stage might, and never builds a block it
  5242 |         // refuses.
  5243 |         let mut paced = false;
  5244 |         if track_heartbeats {
  5245 |             let sp = self.headers_store.get_header(selected_parent).unwrap();
  5246 |             if sp.pow_algo_id == kaspa_consensus_core::palw_heartbeat_v1::PALW_HEARTBEAT_ALGO_ID {
  5247 |                 heartbeat_set.push((sp.blue_score, selected_parent, sp.timestamp));
  5248 |             }
  5249 |             paced = self.palw_clock_floor.is_some_and(|fence| fence.is_active(sp.daa_score.saturating_add(1)));
  5250 |         }
  5251 | 
  5252 |         // ADR-0125: one parent slot is kept for the round lane while it has a tip to offer, so a DAG
  5253 |         // wide with chain tips cannot starve the lane of every merge.
  5254 |         let round_reserve = usize::from(!round_tips.is_empty() && max_block_parents > 1);
  5255 |         // Try adding parents as long as mergeset size and number of parents limits are not reached
  5256 |         while let Some(candidate) = candidates.pop_front() {
  5257 |             if mergeset_size >= mergeset_size_limit || virtual_parents.len() >= max_block_parents - round_reserve {
  5258 |                 break;
  5259 |             }
  5260 |             match self.mergeset_increase(&virtual_parents, candidate, mergeset_size_limit - mergeset_size, track_heartbeats) {
  5261 |                 MergesetIncreaseResult::Accepted { increase_size, heartbeat_members } => {
  5262 |                     if !heartbeat_members.is_empty() {
  5263 |                         let mut combined = heartbeat_set.clone();
  5264 |                         combined.extend_from_slice(&heartbeat_members);
  5265 |                         if !self.heartbeat_set_admissible(&mut combined, paced) {
  5266 |                             // Over the flat bound and not one chain: this candidate widens the
  5267 |                             // heartbeat lane past what consensus admits. Nothing to substitute —
  5268 |                             // skip it; a later template absorbs it against a fresh mergeset.
  5269 |                             continue;
  5270 |                         }
  5271 |                         heartbeat_set = combined;
  5272 |                     }
  5273 |                     mergeset_size += increase_size;
  5274 |                     virtual_parents.push(candidate);
  5275 |                 }
  5276 |                 MergesetIncreaseResult::Rejected { new_candidate } => {
  5277 |                     // If we already have a candidate in the past of new candidate then skip.
  5278 |                     if self.reachability_service.is_any_dag_ancestor(&mut candidates.iter().copied(), new_candidate) {
  5279 |                         continue; // TODO (optimization): not sure this check is needed if candidates invariant as antichain is kept
  5280 |                     }
  5281 |                     // Remove all candidates which are in the future of the new candidate
  5282 |                     candidates.retain(|&h| !self.reachability_service.is_dag_ancestor_of(new_candidate, h));
  5283 |                     candidates.push_back(new_candidate);
  5284 |                 }
  5285 |             }
  5286 |         }
  5287 |         assert!(mergeset_size <= mergeset_size_limit);
  5288 |         assert!(virtual_parents.len() <= max_block_parents);
  5289 |         self.palw_add_round_parents(&mut virtual_parents, selected_parent, round_tips, max_block_parents);
  5290 |         self.remove_bounded_merge_breaking_parents(virtual_parents, pruning_point)
  5291 |     }
```

### `consensus/src/pipeline/virtual_processor/processor/template.rs:853–866`

```text
   853 |         let parents_by_level = self.parents_manager.calc_block_parents(pruning_point, &virtual_state.parents);
   854 |         let hash_merkle_root = calc_hash_merkle_root(txs.iter());
   855 | 
   856 |         let accepted_id_merkle_root = self
   857 |             .calc_accepted_id_merkle_root(virtual_state.accepted_tx_ids.iter().copied(), virtual_state.ghostdag_data.selected_parent);
   858 |         let utxo_commitment = virtual_state.multiset.clone().finalize();
   859 |         let header = Header::new_finalized(
   860 |             version,
   861 |             parents_by_level,
   862 |             hash_merkle_root,
   863 |             accepted_id_merkle_root,
   864 |             utxo_commitment,
   865 |             timestamp,
   866 |             virtual_state.bits,
```

## D03 — 全体一名のproposer選出でなく各attemptの閾値判定

### `consensus/core/src/palw_admission_v2.rs:1152–1173`

```text
  1152 | pub fn check_palw_class_lottery_v4(
  1153 |     state: &PalwChainStateV2,
  1154 |     state_params: &PalwStateParamsV2,
  1155 |     attempt: &crate::palw_attempt_v2::PalwAttemptUnsignedV2,
  1156 |     execution_anchor: Hash64,
  1157 |     work_target_floor: Option<u128>,
  1158 | ) -> Result<(), PalwAdmissionV2Error> {
  1159 |     let Some(floor) = work_target_floor else { return check_palw_class_lottery_v3(state, attempt, execution_anchor) };
  1160 |     if attempt.class_id == state_params.base_class_id() {
  1161 |         return check_palw_class_lottery_v3(state, attempt, execution_anchor);
  1162 |     }
  1163 |     let ccu = state
  1164 |         .model_lifecycle(&attempt.class_id)
  1165 |         .map(|row| row.work.economic_ccu_per_claim)
  1166 |         .ok_or(PalwAdmissionV2Error::ClassWorkUnknown(attempt.class_id))?;
  1167 |     let target = palw_effective_class_target_v1(state, state_params, &attempt.class_id, Some(floor))?;
  1168 |     let ticket = crate::palw_attempt_v2::class_ticket_v3(attempt, execution_anchor);
  1169 |     if ticket > target {
  1170 |         return Err(PalwAdmissionV2Error::ClassTicketAboveWorkTarget { class_id: attempt.class_id, ticket, target, ccu, floor });
  1171 |     }
  1172 |     Ok(())
  1173 | }
```

### `consensus/core/src/palw_work_target_v1.rs:94–125`

```text
    94 | /// **One epoch step of `W`**: `W · model_blocks / expected_blocks`, clamped to `[W / f, W · f]`,
    95 | /// never below `floor`. An epoch that expected nothing leaves `W` where it is (floored); an epoch
    96 | /// the models sat out eases `W` by the whole clamp toward the floor — silence is not evidence of
    97 | /// trying, but the floor is the price the network states, and `W` may not sit above it on nothing.
    98 | pub fn palw_work_target_step_v1(current: u128, floor: u128, model_blocks: u64, expected_blocks: u64, max_factor: u32) -> u128 {
    99 |     let f = max_factor.max(2) as u128;
   100 |     let current = current.max(1);
   101 |     if expected_blocks == 0 {
   102 |         return current.max(floor);
   103 |     }
   104 |     let low = current / f;
   105 |     let high = current.saturating_mul(f);
   106 |     if model_blocks == 0 {
   107 |         return low.max(floor).max(1);
   108 |     }
   109 |     let scaled = mul_div_u128(current, model_blocks as u128, expected_blocks as u128);
   110 |     scaled.clamp(low, high).max(floor).max(1)
   111 | }
   112 | 
   113 | /// **A class's ticket target from its CCU and `W`**: `MAX · min(1, CCU / W)`, never zero. A class
   114 | /// with no counted work is priced at the hardest target — it may not be handed the ticket space by
   115 | /// the arithmetic that prices the ones that declared work.
   116 | pub fn palw_work_ticket_target_v1(ccu: u128, work: u128) -> u128 {
   117 |     if ccu == 0 {
   118 |         return 1;
   119 |     }
   120 |     let work = work.max(1);
   121 |     if ccu >= work {
   122 |         return u128::MAX;
   123 |     }
   124 |     mul_div_u128(u128::MAX, ccu, work).max(1)
   125 | }
```

## D04 — BLUE/RED merged workの採用とstateへの反映

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:9860–9914`

```text
  9860 |     pub(in crate::pipeline::virtual_processor) fn palw_v2_merged_works(
  9861 |         &self,
  9862 |         ghostdag_data: &GhostdagData,
  9863 |         state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
  9864 |         state_params: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
  9865 |         // The accepting block's mergeset non-DAA set — passed in rather than read from a stored
  9866 |         // block, so the TEMPLATE path (whose virtual block has no hash yet) can supply
  9867 |         // `virtual_state.mergeset_non_daa` and the two paths compute one identical work list.
  9868 |         mergeset_non_daa: &BlockHashSet,
  9869 |         point: &kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
  9870 |     ) -> (Vec<PalwMergedOwnedWorkV1>, Vec<(BlockHash, String)>) {
  9871 |         let mut works = Vec::new();
  9872 |         let mut skips = Vec::new();
  9873 |         if ghostdag_data.unordered_mergeset_without_selected_parent().next().is_none() {
  9874 |             return (works, skips);
  9875 |         }
  9876 |         let non_daa = mergeset_non_daa;
  9877 |         // Lane accept-order: past its fence, parents-first at the accepting block's DAA score.
  9878 |         let merged: Vec<BlockHash> = self.acceptance_ordered_mergeset_without_selected_parent(ghostdag_data, point.daa_score);
  9879 |         for blue in merged.iter() {
  9880 |             if non_daa.contains(blue) {
  9881 |                 continue;
  9882 |             }
  9883 |             let header = match self.headers_store.get_header(*blue) {
  9884 |                 Ok(header) => header,
  9885 |                 Err(missing) => {
  9886 |                     skips.push((*blue, format!("no header for a mergeset block: {missing}")));
  9887 |                     continue;
  9888 |                 }
  9889 |             };
  9890 |             match self.palw_v2_check_attempt_admission(&header, state, state_params, point, None) {
  9891 |                 Ok(Some(envelope)) => works.push(PalwMergedOwnedWorkV1::Attempt(
  9892 |                     *blue,
  9893 |                     envelope,
  9894 |                     // B-1: the merged block's own subsidy, the pool its escrow is carved from past
  9895 |                     // the deep fence. An attempt block is never a heartbeat, so this equals the
  9896 |                     // coinbase-declared subsidy body validation pinned.
  9897 |                     self.coinbase_manager.calc_block_subsidy(header.daa_score),
  9898 |                     // ADR-0126: and the carve, at the lower of the attempt block's score and the
  9899 |                     // accepting block's — the block whose coinbase pays and withholds it.
  9900 |                     self.palw_escrow_for_carrier_at(*blue, header.daa_score, point.daa_score),
  9901 |                     // ADR-0132 Upgrade C: the merged block's own `bits`, the lottery its forward faced.
  9902 |                     header.bits,
  9903 |                     header.clone(),
  9904 |                 )),
  9905 |                 Ok(None) => match self.palw_v2_check_receipt_spend(&header, state, state_params, point) {
  9906 |                     Ok(Some(envelope)) => works.push(PalwMergedOwnedWorkV1::Spend(*blue, envelope)),
  9907 |                     Ok(None) => {}
  9908 |                     Err(why) => skips.push((*blue, why)),
  9909 |                 },
  9910 |                 Err(why) => skips.push((*blue, why)),
  9911 |             }
  9912 |         }
  9913 |         (works, skips)
  9914 |     }
```

### `consensus/core/src/palw_state_v2.rs:14795–14935`

```text
 14795 |                     // RFC-0016 (dormant): V+ block rights and the pending queue's bond share — checked in the class gate and beside the
 14796 |                     // per-bond share, before `apply_attempt`'s first write; another block may take the last right or queue slot.
 14797 |                     | PalwStateV2Error::ModelBlockRightsRefused { .. }
 14798 |                     | PalwStateV2Error::FinalQueueRefused { .. }),
 14799 |                 ) => {
 14800 |                     builder.register_network_demand_on_v1(&refused, &envelope.attempt, ctx.daa_score)?;
 14801 |                     merged_skips.push((ctx.block, refused.to_string()));
 14802 |                 }
 14803 |                 Err(other) => return Err(other),
 14804 |             }
 14805 |         }
 14806 |         // A receipt spend is NOT skipped when this block's own objects convicted its execution
 14807 |         // (`WrongPhase` from #8's reversal, `ReceiptRightsForfeited` from #5): the processor
 14808 |         // admitted it against the parent state, and the fold refuses it here, so the block is
 14809 |         // disqualified on every node alike. Deliberately — unlike an attempt, a spend escrows
 14810 |         // nothing: the coinbase pays a chain block's worker share whatever its work turned out to be
 14811 |         // (the selected-parent exemption in the processor's `palw_v2_unentitled_blues`), so skipping would
 14812 |         // pay one receipt block for a convicted execution, which is what #5 closed.
 14813 |         PalwBlockWorkV3::ReceiptSpend(spend) => apply_receipt_spend(&mut builder, ctx, spend)?,
 14814 |     }
 14815 | 
 14816 |     // 4b. The mergeset's work — blues and reds alike — in consensus order, selected parent
 14817 |     //     excluded (ADR-0058).
 14818 |     //
 14819 |     //     Each attempt passes the full stateful admission against the LIVE fold state — not the
 14820 |     //     parent state the processor's per-blue pre-checks ran against — because the works ahead
 14821 |     //     of it in this very block consume budget and exposure, and two blues must not both take
 14822 |     //     the last slot. A refusal SKIPS (checkpoint, try, restore): the accepting block did not
 14823 |     //     author its anticone, so nothing about a merged blue may disqualify it. The skip list is
 14824 |     //     returned, never folded — logs are not state.
 14825 |     // **RFC-0018 §4 (`palw_dag_sibling_attribution_v1`, at THIS accepting block's DAA): the canonical sibling order.** Past the fence
 14826 |     // the attempts of the mergeset are admitted in a round robin over model classes, each class by `H(domain ‖ execution key)` —
 14827 |     // a function of the mergeset SET, not of GHOSTDAG colour or merge order — so concurrent siblings of different models share any
 14828 |     // contended per-block cap round by round and the ADR-0170 "last admitted" anchor is canonical. Non-attempt works keep their
 14829 |     // slots. Below the fence (every shipped network) the identity permutation: the consensus order, byte for byte.
 14830 |     let sibling_order: Vec<usize> =
 14831 |         if builder.params.rfc0018_v1().is_some_and(|mirror| mirror.sibling_attribution_at(ctx.daa_score)) {
 14832 |             let keys: Vec<crate::palw_rfc0018_v1::PalwSiblingKeyV1> = merged_work
 14833 |                 .iter()
 14834 |                 .map(|merged| match merged.work {
 14835 |                     PalwBlockWorkV3::Attempt(envelope) => {
 14836 |                         crate::palw_rfc0018_v1::palw_sibling_key_of_attempt_v1(envelope.attempt.class_id, merged.execution_key)
 14837 |                     }
 14838 |                     _ => crate::palw_rfc0018_v1::PalwSiblingKeyV1 { class_id: None, execution_key: merged.execution_key },
 14839 |                 })
 14840 |                 .collect();
 14841 |             crate::palw_rfc0018_v1::palw_sibling_canonical_order_v1(&keys)
 14842 |         } else {
 14843 |             (0..merged_work.len()).collect()
 14844 |         };
 14845 |     for merged in sibling_order.iter().map(|&index| &merged_work[index]) {
 14846 |         let refusal: Option<String> = match merged.work {
 14847 |             PalwBlockWorkV3::None => None,
 14848 |             PalwBlockWorkV3::Attempt(envelope) => match admission {
 14849 |                 None => Some("a merged attempt on a caller that supplied no admission params".to_string()),
 14850 |                 Some(admission) => {
 14851 |                     // **the boundary-budget fence is false, and its value is inert here**
 14852 |                     // (ADR-0045 D2; mainnet audit 2026-09-06, M-2). This runs against the LIVE fold state, and
 14853 |                     // step 3b's `ensure_epoch_budgets` has already written the table for
 14854 |                     // `ctx.daa_score`'s epoch — so the stored snapshot matches and the fence's
 14855 |                     // branch is unreachable. Passing `true` would be a rule change on every
 14856 |                     // network, including the two whose validity may not move; passing the fold a
 14857 |                     // fence it cannot resolve would be a second reading of one block's DAA.
 14858 |                     match crate::palw_admission_v2::check_palw_attempt_admission_v2(
 14859 |                         &builder.state,
 14860 |                         params,
 14861 |                         admission,
 14862 |                         ctx,
 14863 |                         envelope,
 14864 |                         // ADR-0045's boundary fence is inert here: `ensure_epoch_budgets` has
 14865 |                         // already installed the live epoch's table. ADR-0123's release is
 14866 |                         // resolved once for this block and must remain armed in the fold.
 14867 |                         //
 14868 |                         // **ADR-0149 §6: and the canonical-work height rides too.** Left at its
 14869 |                         // default this re-run judged every merged attempt past the fence on the
 14870 |                         // DECLARED rule while the node's pre-check (`palw_v2_merged_works`, which
 14871 |                         // passes the height) judged it on the derived one — no merged attempt could
 14872 |                         // pass both, so past the bundle every merged blue's work was skipped here:
 14873 |                         // claimed by nobody, paid to nobody, weighing nothing. The two readings of
 14874 |                         // one attempt now take the same height and the same floor draw.
 14875 |                         crate::palw_admission_v2::PalwEpochBudgetFencesV1 {
 14876 |                             budget_release_active: builder.extras.epoch_budget_release_active,
 14877 |                             work_target_floor: builder.work_target_floor(ctx),
 14878 |                             canonical_work_daa: builder.extras.canonical_work_daa,
 14879 |                             base_known_draw: builder.base_known_draw(),
 14880 |                             audit_2026_09_23_active: builder.extras.audit_2026_09_23_active,
 14881 |                             // ADR-0152 SR-7: the raw second-clock depth the fold's own ceiling reads.
 14882 |                             settled_anchor_depth: builder.read().settled_anchor_depth(),
 14883 |                             // Lane V02: the pair `held_charge_block_active_v1` reads, so item 8 counts a held
 14884 |                             // session at its charge past the fence as `gate_room` does.
 14885 |                             held_accuser_charge_active: builder.extras.offence_attribution_active
 14886 |                                 && builder.extras.held_context_ladder.is_some(),
 14887 |                             ..Default::default()
 14888 |                         },
 14889 |                     ) {
 14890 |                         Err(refused) => Some(refused.to_string()),
 14891 |                         Ok(_) => {
 14892 |                             let checkpoint = builder.try_checkpoint()?;
 14893 |                             // B-1: past the deep fence a merged claim escrows its carve, carved from the
 14894 |                             // MERGED block's own subsidy — the accepting coinbase withholds that same
 14895 |                             // carve. Below the fence, false → escrows nothing (ADR-0058 D5),
 14896 |                             // byte-identical to before. Read before the &mut borrow of `builder`.
 14897 |                             let escrows_reward = builder.extras.audit_2026_09_11_deep_active;
 14898 |                             // ADR-0165: its colour in THIS block's mergeset, read before the &mut borrow of `builder`.
 14899 |                             let floor_event =
 14900 |                                 PalwFloorEventSourceV1::of_merged_block(builder.extras.merged_reds.contains(&merged.carrying_block));
 14901 |                             match apply_attempt(
 14902 |                                 &mut builder,
 14903 |                                 ctx,
 14904 |                                 envelope,
 14905 |                                 PalwAttemptOriginV1 {
 14906 |                                     rider: false,
 14907 |                                     carrying_block: merged.carrying_block,
 14908 |                                     escrows_reward,
 14909 |                                     escrow_subsidy: merged.subsidy,
 14910 |                                     // ADR-0126: resolved at the MERGED block's DAA, never this block's.
 14911 |                                     escrow_carve: merged.escrow_carve,
 14912 |                                     execution_key: merged.execution_key,
 14913 |                                     carrying_bits: merged.bits,
 14914 |                                     job_anchor: merged.job_anchor,
 14915 |                                     floor_event,
 14916 |                                     budget_block_units: crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1,
 14917 |                                 },
 14918 |                             ) {
 14919 |                                 Ok(()) => {
 14920 |                                     // **ADR-0170 M1 (`palw_anchor_window_v1`): an admitted attempt this block MERGES, blue or red, is
 14921 |                                     // a seed anchor as the block's own is.** Under the floor reserve those are the only attempts
 14922 |                                     // there are (a REAL attempt is merged, almost never a chain block). Own work first (step 4),
 14923 |                                     // then the mergeset in consensus order: the last admitted one wins, as the last chain block of
 14924 |                                     // a span always did. Past the fence only, and only where the execution lane records anchors.
 14925 |                                     if let Some(lane) = extras.round_lane
 14926 |                                         && builder.params.anchor_window_records_merged_at(ctx.daa_score)
 14927 |                                     {
 14928 |                                         builder.record_round_seed_anchor_of(
 14929 |                                             ctx,
 14930 |                                             lane.schedule_span_daa,
 14931 |                                             merged.carrying_block,
 14932 |                                             merged.execution_key,
 14933 |                                         )?;
 14934 |                                     }
 14935 |                                     None
```

## D05 — 同一mergeset中のmodel round robinと、その範囲

### `consensus/core/src/palw_rfc0018_v1.rs:291–345`

```text
   291 | /// One merged work as the canonical order reads it.
   292 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   293 | pub struct PalwSiblingKeyV1 {
   294 |     /// `Some(class)` for an attempt; `None` for any other work (keeps its slot).
   295 |     pub class_id: Option<Hash64>,
   296 |     pub execution_key: Hash64,
   297 | }
   298 | 
   299 | /// `H(domain ‖ execution key)` — fixed by the sibling's own inference (the accepting miner cannot move it; the sibling's miner moves it
   300 | /// only by another winning inference).
   301 | pub fn palw_sibling_priority_v1(execution_key: &Hash64) -> Hash64 {
   302 |     let mut h = keyed(PALW_RFC0018_SIBLING_ORDER_DOMAIN_V1);
   303 |     h.update(execution_key.as_byte_slice());
   304 |     finish(h)
   305 | }
   306 | 
   307 | /// **The canonical order of one accepting block's merged works** — a permutation of `0..works.len()`.
   308 | ///
   309 | /// Non-attempt works keep their positions (their order is the consensus order, as before). The attempt positions are refilled in a
   310 | /// round robin over model classes: classes ordered by their first member's priority, each class's attempts by priority, one attempt per
   311 | /// class per round. The result depends only on the SET of `(class, execution key)` pairs at the attempt positions — not on GHOSTDAG
   312 | /// colour, blue work or merge order — so concurrent siblings of different models share a contended per-block cap round by round, and
   313 | /// the same mergeset yields the same admitted set on every node and every arrival order. Ties (equal priority: the same execution key,
   314 | /// which the fold dedups anyway) fall back to the original index.
   315 | pub fn palw_sibling_canonical_order_v1(works: &[PalwSiblingKeyV1]) -> Vec<usize> {
   316 |     let attempt_slots: Vec<usize> = works.iter().enumerate().filter(|(_, w)| w.class_id.is_some()).map(|(i, _)| i).collect();
   317 |     let mut by_class: BTreeMap<Hash64, Vec<(Hash64, usize)>> = BTreeMap::new();
   318 |     for &i in &attempt_slots {
   319 |         let w = works[i];
   320 |         by_class.entry(w.class_id.expect("attempt slot")).or_default().push((palw_sibling_priority_v1(&w.execution_key), i));
   321 |     }
   322 |     let mut classes: Vec<Vec<(Hash64, usize)>> = by_class
   323 |         .into_values()
   324 |         .map(|mut members| {
   325 |             members.sort();
   326 |             members
   327 |         })
   328 |         .collect();
   329 |     classes.sort_by(|a, b| a[0].cmp(&b[0]));
   330 |     let mut ordered = Vec::with_capacity(attempt_slots.len());
   331 |     let mut round = 0usize;
   332 |     while ordered.len() < attempt_slots.len() {
   333 |         for class in &classes {
   334 |             if let Some((_, i)) = class.get(round) {
   335 |                 ordered.push(*i);
   336 |             }
   337 |         }
   338 |         round += 1;
   339 |     }
   340 |     let mut out: Vec<usize> = (0..works.len()).collect();
   341 |     for (slot, index) in attempt_slots.iter().zip(ordered) {
   342 |         out[*slot] = index;
   343 |     }
   344 |     out
   345 | }
```

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md:100–117`

```text
   100 | ### 4.1 不変条件
   101 | 
   102 | * **I-S1(保持):** 有効なsibling blockは色に関わらず全て保持され、merge depth以内に後続blockのmergesetへ入る(GHOSTDAGの既存性質)。MUST NOT: PALWの規則でsiblingをparent候補から外す。
   103 | * **I-S2(独立記録):** 各claimは自分のbond・class・carrying blockで記録される(`attempt_id_v2`、`execution_key`、`accepted_block`)。Panelは`claim_id`で鍵付けされ、seedはclaim idを含む(`palw_panel_draw_seed_v1`)。siblingのPanel・escrow・報酬を混同しない(MUST)。
   104 | * **I-S3(順序非依存):** 一つのaccepting blockが採用するmerged attemptの集合は、mergesetの**集合**の関数でなければならない(MUST)。色・blue work・merge順・到着順に依存しない。
   105 | * **I-S4(モデル間公平):** 共有上限で競合したとき、各モデルのsiblingが1件ずつ採用されてから、どのモデルの2件目も採用される(round robin)。
   106 | * **I-S5(seed anchor):** ADR-0170のseed anchorは、同じspanで採用されたattempt(自身・merged、BLUE/RED)のうち`H(domain ‖ execution key)`が最小のもの。last-write-winsではない(`record_round_seed_anchor_of`、`transition_builder.rs`)。
   107 | * **I-S6(selected parentのsibling):** siblingのうちGHOSTDAGがselected parentに選んだものは、自分のblockの遷移で先に採用される(merger のmergesetに入らない)。どれが選ばれるかはGHOSTDAGの決定的な規則(blue work、次にblock hash)で決まり、到着順にもmerge順にも依存しない。block hashを動かすには勝ちticket(推論1回)がもう一度要る。共有上限で負けるのは、このselected parentを除いたmergeset内のsiblingで、それはcanonical順で決まる。
   108 | 
   109 | ### 4.2 規則(fence以後、accepting blockのDAAで判定)
   110 | 
   111 | step 4bの前に、mergesetのattempt workだけを並べ替える。非attempt work(receipt spend等)は元の位置に残す。順序は、モデル(class)ごとに`H(domain ‖ execution key)`の昇順、モデル同士は各モデル先頭の優先度順、1巡につき1モデル1件のround robin(`palw_sibling_canonical_order_v1`、`consensus/core/src/palw_rfc0018_v1.rs:283`、fold `palw_state_v2.rs:14481-14496`)。優先度はsibling自身の推論で決まり、accepting minerは動かせない。siblingのminerが動かすには別の勝ちticket(推論1回)が要る。
   112 | 
   113 | ### 4.3 mergeset上限とREDの期限
   114 | 
   115 | * mergeset 180、parents 10、heartbeat ≤ 4/mergesetは変えない。round robinはこの上限の内側で働く。
   116 | * REDのPALW workの期限はGHOSTDAGのmerge depth(testnet-12で30 block)である。これを超えるREDは`ViolatingBoundedMergeDepth`で失われる。推論時間 + 伝播はmerge depthの時間(約60分)より十分短くなければならない(SHOULD、§8で計測)。
   117 | * coinbaseのcarve差し引きは採用集合の超集合のまま(`palw_v2_merged_escrow_withheld`)。負けたsiblingのcarveは焼却。本RFCは「どれが負けるか」をmerge順の偶然からround robinへ変えるだけで、焼却の規則は変えない。
```

## D06 — claimごとのPanel derivation / model-specific readiness

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:10460–10567`

```text
 10460 |     pub(super) fn palw_v2_derive_panel_binding_v1(
 10461 |         &self,
 10462 |         state: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2,
 10463 |         block: BlockHash,
 10464 |         block_daa: u64,
 10465 |         binding_extras: &kaspa_consensus_core::palw_state_v2::PalwTransitionExtrasV1,
 10466 |         panel_params: &kaspa_consensus_core::palw_panel_v2::PalwPanelParamsV2,
 10467 |         min_collateral: u64,
 10468 |         claim_id: &kaspa_hashes::Hash64,
 10469 |         claim: &kaspa_consensus_core::palw_state_v2::PalwClaimStateV2,
 10470 |     ) -> Result<kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2, Option<String>> {
 10471 |         use kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2;
 10472 |         if !matches!(claim.phase, PalwClaimPhaseV2::Provisional) {
 10473 |             return Err(None);
 10474 |         }
 10475 |         // The anchor is the first chain block at or past `accepted_daa + anchor_delay`. Until
 10476 |         // one exists the claim simply waits — that delay is what stops a producer from
 10477 |         // mining until it likes its own jury.
 10478 |         // Same base as the validator's, for the same reason — see the sibling call site.
 10479 |         // RFC-0010: through the versioned door — a V3-rule claim has no lane-A anchor (and the list this binder walks never offers one).
 10480 |         let Some(state_params) = self.palw_state_params_v2.as_ref() else { return Err(None) };
 10481 |         let Some(anchor) = self.palw_v2_anchor_fact_for_claim_v1(block, claim, state_params, panel_params) else {
 10482 |             return Err(None);
 10483 |         };
 10484 |         // C-02 (deep fence) and ADR-0124 (the panel economy): the whole draw policy, from the
 10485 |         // same anchor as the acceptance layer's sibling call — so this assembler builds the
 10486 |         // exact panel that layer recomputes.
 10487 |         let mut policy = self.palw_panel_draw_policy_at(anchor.anchor_daa);
 10488 |         // **ADR-0152 SW-8: under the stake draw a panel binds only in its own anchor block**
 10489 |         // (`validate_panel_bound_v2_with_policy` refuses any other), so no later block draws it again.
 10490 |         if policy.stake.is_some() && anchor.anchor_block != block {
 10491 |             return Err(None);
 10492 |         }
 10493 |         if let Some(state_params) = self.palw_state_params_v2.as_ref() {
 10494 |             policy.valid_lock = Self::palw_panel_valid_lock_of_v1(
 10495 |                 state,
 10496 |                 state_params,
 10497 |                 binding_extras,
 10498 |                 claim_id,
 10499 |                 claim,
 10500 |                 block_daa,
 10501 |                 panel_params.seat_count() as usize,
 10502 |             );
 10503 |         }
 10504 |         // A registry too small to seat a panel yields nothing rather than a short one: a
 10505 |         // partial jury is `derive_panel_v2`'s fail-closed refusal, and it stays that.
 10506 |         // ADR-0065 D1, from the claim's own anchor — the same input the acceptance layer
 10507 |         // uses, through the same one-place subtraction.
 10508 |         let maturity_floor = kaspa_consensus_core::palw_panel_v2::palw_seat_maturity_floor_v1(
 10509 |             anchor.anchor_daa,
 10510 |             self.palw_bond_maturity_window_at(state, anchor.anchor_daa),
 10511 |         );
 10512 |         // ADR-0071 SA-3, from the same anchor as the acceptance layer's sibling call.
 10513 |         let capability_bound = self.palw_capability_bound_at(anchor.anchor_daa);
 10514 |         // ADR-0100 Decision 4: a class with a plan draws per shard, or not at all — a flat
 10515 |         // panel of a sharded class would ask shard seats to judge a whole model.
 10516 |         // Lane F1, the panel seed: for a claim anchored past `Params::palw_panel_seed_execution`
 10517 |         // `H(anchor attempt's execution ‖ claim)`, below it the anchor block — the one value the gate
 10518 |         // recomputes and the fold stores.
 10519 |         let seed = anchor.panel_seed(claim_id);
 10520 |         let drawn = match (
 10521 |             self.palw_tir_shard_count(state, &claim.class_id, anchor.anchor_daa),
 10522 |             self.palw_stratified_shard_count(state, &claim.class_id, anchor.anchor_daa),
 10523 |         ) {
 10524 |             // RFC-0006: an IR class with a layer-shard plan draws per shard, with the whole draw policy (stake race, readiness,
 10525 |             // route-matrix lock) the flat draw reads — the same function the acceptance layer recomputes it with.
 10526 |             (Some(s_l), _) => kaspa_consensus_core::palw_panel_v2::derive_tir_shard_panel_v1(
 10527 |                 state,
 10528 |                 panel_params,
 10529 |                 claim_id,
 10530 |                 seed,
 10531 |                 min_collateral,
 10532 |                 maturity_floor,
 10533 |                 capability_bound,
 10534 |                 policy,
 10535 |                 s_l,
 10536 |             ),
 10537 |             (None, Some(shard_count)) => kaspa_consensus_core::palw_panel_v2::derive_stratified_panel_v2(
 10538 |                 state,
 10539 |                 panel_params,
 10540 |                 claim_id,
 10541 |                 seed,
 10542 |                 min_collateral,
 10543 |                 maturity_floor,
 10544 |                 capability_bound,
 10545 |                 shard_count,
 10546 |             ),
 10547 |             (None, None) => kaspa_consensus_core::palw_panel_v2::derive_panel_v2_with_policy(
 10548 |                 state,
 10549 |                 panel_params,
 10550 |                 claim_id,
 10551 |                 seed,
 10552 |                 min_collateral,
 10553 |                 maturity_floor,
 10554 |                 capability_bound,
 10555 |                 policy,
 10556 |             ),
 10557 |         };
 10558 |         let seats = drawn.map_err(|why| policy.stake.map(|_| why.to_string()))?;
 10559 |         // ADR-0147: nothing to ask here. The outsider is IN the derived panel — the draw policy
 10560 |         // carries the fence — and the acceptance layer demands that panel exactly, so a node
 10561 |         // proposes only what its own fold will take. The identity test that stood here
 10562 |         // compared fields a registrant writes for itself, and is gone with the rule it served.
 10563 |         Ok(kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::PanelBound { claim: *claim_id, anchor: seed, seats })
 10564 |     }
 10565 | 
 10566 |     /// **RFC-0003 §I.4.4: the tensor claims of an accepted set** — the free-prompt walk's twin for
 10567 |     /// version-10 payloads ([`kaspa_consensus_core::palw_gen_claim_v1::palw_fp_gen_objects_from_accepted_txs_v1`]),
```

## D07 — 適格REDの報酬は本人へ・escrowの控除

### `consensus/src/processes/coinbase.rs:196–219`

```text
   196 |         palw_unentitled_blues: &BlockHashSet,
   197 |         // **ADR-0058: on a `ConsensusV2` network an entitled, in-DAA-window red is paid its
   198 |         // worker share to ITS OWN miner script, exactly as a blue is** — not lumped into the
   199 |         // merging miner's red reward. At the frozen 120 s cadence `ghostdag_k = 1`, so any block
   200 |         // whose anticone holds two or more blocks is a red BY CONSTRUCTION — which is every
   201 |         // block of every class slower than the floor. Its claim (created by the accepting
   202 |         // block's transition, ADR-0058) carries the slash exposure; paying the includer instead
   203 |         // would put the stake on one key and the reward on another. `false` outside V2 keeps the
   204 |         // legacy lump byte-identical.
   205 |         palw_pay_entitled_reds_to_their_miner: bool,
   206 |         // **MISAKA B-1 (mainnet audit 2026-09-11, deep fence): the PALW escrow WITHHELD from each
   207 |         // OTHER merged block's worker reward** — `{ merged block hash → worker_carve(its subsidy) }`.
   208 |         // `palw_escrow_withheld` above handles the one chain block a mergeset has (the selected
   209 |         // parent, whose claim a PRIOR transition already committed); this handles every other merged
   210 |         // blue and every entitled in-window red, whose claim THIS accepting block's transition
   211 |         // creates — so the value cannot be read from committed state and is instead sized by the
   212 |         // processor with the same `PalwStateParamsV2::worker_carve` the fold escrows with
   213 |         // (`palw_v2_merged_escrow_withheld`), keeping the carve recorded and the carve withheld one
   214 |         // number. Withholding a superset of what the fold escrows is safe (the excess is simply not
   215 |         // minted); a shortfall would release an unfunded escrow, which the superset construction
   216 |         // rules out. **Empty below the deep fence and on every network without a V2 bundle**, where
   217 |         // every merged carve is paid in full at acceptance (ADR-0058 Decision 5), byte-identical.
   218 |         palw_merged_escrow_withheld: &BlockHashMap<u64>,
   219 |         // **ADR-0125: the merged round blocks.** A round block is a red outside the DAA set, so the
```

### `consensus/src/processes/coinbase.rs:313–354`

```text
   313 |         for red in classified.genuine_reds() {
   314 |             // **The same skip the blues loop has, for the same reason.** It was missing here
   315 |             // while the set was built from blues alone, so the two halves agreed only by never
   316 |             // meeting: an unentitled red was paid its full worker share to this block's miner.
   317 |             // At the frozen cadence `ghostdag_k = 1` against a mergeset limit of 180, so the
   318 |             // blues this filtered were at most ONE block and the reds it did not were
   319 |             // everything else — which is where the value actually was.
   320 |             if palw_unentitled_blues.contains(red) {
   321 |                 continue;
   322 |             }
   323 |             let reward_data = mergeset_rewards.get(red).unwrap();
   324 |             // Reds ∩ DAA earn subsidy + fees; non-DAA reds earn fees only (both fee classes kept).
   325 |             let (eff_subsidy, eff_fees) = if mergeset_non_daa.contains(red) {
   326 |                 (0, reward_data.total_fees)
   327 |             } else {
   328 |                 (reward_data.subsidy, reward_data.total_fees)
   329 |             };
   330 |             // ADR-0058: see the parameter — an entitled in-window red's share goes to the red's
   331 |             // own script, through the same carve arithmetic as the lump below, so moving a block
   332 |             // between the two pay paths never changes the amount, only the payee.
   333 |             if palw_pay_entitled_reds_to_their_miner && !mergeset_non_daa.contains(red) {
   334 |                 let value = match carve {
   335 |                     Some(fs) => {
   336 |                         let s = split_block_subsidy(eff_subsidy, fs);
   337 |                         worker_inclusion_pool = worker_inclusion_pool.saturating_add(s.worker_inclusion_sompi);
   338 |                         let finality = reward_data.finality_fees.min(eff_fees);
   339 |                         s.worker_base_sompi
   340 |                             .saturating_add(split_normal_tx_fees(eff_fees - finality, fs).worker_sompi)
   341 |                             .saturating_add(split_finality_fees(finality, fs).worker_sompi)
   342 |                     }
   343 |                     None => eff_subsidy + eff_fees,
   344 |                 };
   345 |                 // B-1: withhold this entitled red's carve past the deep fence — its claim, created by
   346 |                 // this block's transition, now escrows it (empty map below the fence → paid in full).
   347 |                 // The carve is `worker_carve(subsidy) ≤ worker_base_sompi ≤ value`, so this never
   348 |                 // underflows into fees; if it consumes the whole worker share the output is dropped.
   349 |                 let value = value.saturating_sub(palw_merged_escrow_withheld.get(red).copied().unwrap_or(0));
   350 |                 Self::push_worker_output(&mut outputs, value, reward_data, carve, palw_receipt_v4_payouts.get(red));
   351 |                 continue;
   352 |             }
   353 |             // §F carve: accumulate the Worker share EXCLUDING the §D inclusion sub-pool; else full.
   354 |             // Per-class fee split mirrors the blues loop above (and `split_block_reward`).
```

## D08 — Gate Cのglobal poolと複数claim sweep・抽選予算

### `consensus/core/src/palw_security_certification_v1.rs:373–446`

```text
   373 | /// **The pool predicate**: Active, registered at least `verifier_maturity_daa` before the commit, at least the minimum collateral,
   374 | /// and not the producer's operator. The GLOBAL registry — no model-holding filter (a holder set is the registrant's to fill).
   375 | #[allow(clippy::too_many_arguments)]
   376 | pub fn palw_certification_pool_admits_v1(
   377 |     policy: &PalwSecurityCertificationPolicyV1,
   378 |     committed_daa: u64,
   379 |     producer_principal: &EligDigestV1,
   380 |     active: bool,
   381 |     registered_daa: u64,
   382 |     collateral: u64,
   383 |     operator: &EligDigestV1,
   384 | ) -> bool {
   385 |     active
   386 |         && registered_daa.saturating_add(policy.verifier_maturity_daa) <= committed_daa
   387 |         && collateral >= policy.verifier_min_collateral_sompi.max(1)
   388 |         && operator != producer_principal
   389 | }
   390 | 
   391 | /// `m` stake-weighted positions with replacement over the pool (the RFC-0017 / R7-ECON draw). `None` on an empty pool.
   392 | pub fn palw_certification_draw_v1(
   393 |     seed: &EligDigestV1,
   394 |     id: &EligDigestV1,
   395 |     pool: &[PalwEligibilityVerifierV1],
   396 |     m: u16,
   397 | ) -> Option<Vec<EligDigestV1>> {
   398 |     crate::palw_model_eligibility_v1::palw_eligibility_draw_v1(seed, id, pool, m)
   399 | }
   400 | 
   401 | /// Whether a class has a pinned deterministic public check (the additional path).
   402 | pub fn palw_certification_public_check_v1(class_id: &EligDigestV1) -> bool {
   403 |     PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1.contains(class_id)
   404 | }
   405 | 
   406 | /// The facts about the claim the verdict reads, at `now`.
   407 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   408 | pub struct PalwCertificationClaimFactsV1 {
   409 |     pub still_final: bool,
   410 |     pub accusation_pending: bool,
   411 | }
   412 | 
   413 | /// **The verdict** at `now` (`None`: undecided — wait). Order: a claim no longer `Final` withdraws; a draw that never happened by
   414 | /// `certify_by_daa` expires; the veto; `k` positives with the prosecution window elapsed and nothing pending certify; the deadline
   415 | /// fails the rest. A public-check class certifies once the prosecution window has elapsed with nothing pending.
   416 | pub fn palw_certification_verdict_v1(
   417 |     policy: &PalwSecurityCertificationPolicyV1,
   418 |     row: &PalwClaimCertificationRowV1,
   419 |     facts: PalwCertificationClaimFactsV1,
   420 |     now: u64,
   421 | ) -> Option<PalwCertificationOutcomeV1> {
   422 |     use PalwCertificationOutcomeV1 as O;
   423 |     if row.outcome.is_some() {
   424 |         return None;
   425 |     }
   426 |     if !facts.still_final {
   427 |         return Some(O::Withdrawn);
   428 |     }
   429 |     let prosecution_elapsed = now >= row.final_daa.saturating_add(policy.prosecution_window_daa);
   430 |     if palw_certification_public_check_v1(&row.class_id) {
   431 |         return (prosecution_elapsed && !facts.accusation_pending).then_some(O::PublicCheckCertified);
   432 |     }
   433 |     if row.anchor.is_none() {
   434 |         return (now > row.committed_daa.saturating_add(policy.certify_by_daa)).then_some(O::Expired);
   435 |     }
   436 |     if policy.veto > 0 && row.negative_positions() >= policy.veto as u32 {
   437 |         return Some(O::Vetoed);
   438 |     }
   439 |     if row.positive_positions() >= policy.k as u32 {
   440 |         if prosecution_elapsed && !facts.accusation_pending {
   441 |             return Some(O::Certified);
   442 |         }
   443 |         // k positives are in hand: the decision waits for the prosecution window (and any open accusation) — no deadline failure.
   444 |         return None;
   445 |     }
   446 |     (now > row.deadline_daa).then_some(O::NotCertified)
```

### `consensus/core/src/palw_security_certification_fold_v1.rs:411–505`

```text
   411 | pub(super) fn sweep_security_certification_v1(
   412 |     builder: &mut TransitionBuilder<'_>,
   413 |     ctx: &PalwBlockContextV2,
   414 | ) -> Result<(), PalwStateV2Error> {
   415 |     let now = ctx.daa_score;
   416 |     let Some(mirror) = builder.params.rfc0018_v1().copied() else { return Ok(()) };
   417 |     if !mirror.gate_c_at(now) {
   418 |         return Ok(());
   419 |     }
   420 |     let policy = mirror.certification;
   421 |     let Some(route) = builder.state.kernel_route.as_ref() else { return Ok(()) };
   422 |     let table = PALW_SECURITY_CERTIFICATION_TABLE_V1;
   423 |     let rows: Vec<(Vec<u8>, Vec<u8>)> =
   424 |         route.aux.range((table, Vec::new())..(table + 1, Vec::new())).map(|((_, key), bytes)| (key.clone(), bytes.clone())).collect();
   425 |     let anchor = builder.state.round_seed_anchor.clone();
   426 |     let epoch = now / policy.epoch_daa.max(1);
   427 |     let mut network = row_of::<PalwCertificationNetworkRowV1>(&builder.state, &certification_network_key_v1()).unwrap_or_default();
   428 |     if network.epoch != epoch {
   429 |         network = PalwCertificationNetworkRowV1 { epoch, draws: 0 };
   430 |     }
   431 |     let network_before = network.clone();
   432 |     for (key, bytes) in rows {
   433 |         let native = key.first().copied() == Some(CERTIFICATION_NATIVE_MINING_PREFIX_V39);
   434 |         if !(key.first().copied() == Some(CERTIFICATION_CLAIM_PREFIX_V1) || native) || key.len() != 65 {
   435 |             continue;
   436 |         }
   437 |         let mut row: PalwClaimCertificationRowV1 = borsh::from_slice(&bytes).map_err(|e| refuse(e.to_string()))?;
   438 |         let claim_id = Hash64::from_bytes(row.claim);
   439 |         let claim = if native { None } else { builder.state.claims.get(&claim_id).cloned() };
   440 |         // An owning39 mining claim stands while its native claim row and its complete Master escrow Final stand.
   441 |         let native_live = native && super::palw_native_mining_fold_v39::native_mining_claim_live_v39(&builder.state, &claim_id);
   442 |         // **G-15: a certified reward deferred to the liability horizon** is named here, once, the first sweep strictly past it — and
   443 |         // only if the claim still stands certified (a conviction in the window voids it, and the reward is never named: burned).
   444 |         if let (Some((outcome, _)), Some(c)) = (row.outcome, claim.as_ref())
   445 |             && outcome.certifies()
   446 |             && row.held_reward > 0
   447 |             && crate::palw_g14_gates_v1::palw_certified_reward_held_until_v1(builder.params, c).is_some_and(|until| now > until)
   448 |             && crate::palw_rfc0018_v1::palw_independently_work_certified_v1(
   449 |                 &claim_id,
   450 |                 c,
   451 |                 &crate::palw_rfc0018_v1::PalwGateCViewV1::of(builder.params, &builder.state),
   452 |             )
   453 |         {
   454 |             name_certified_reward_v1(builder, now, &claim_id, c, &row)?;
   455 |             row.held_reward = 0;
   456 |             write_row(builder, key.clone(), Some(&row))?;
   457 |             continue;
   458 |         }
   459 |         if row.outcome.is_some() {
   460 |             // A decided native row is retired by the native fold's own collection (its receipt reads it), never here.
   461 |             if claim.is_none() && !native {
   462 |                 write_row::<PalwClaimCertificationRowV1>(builder, key, None)?;
   463 |             }
   464 |             continue;
   465 |         }
   466 |         let mut changed = false;
   467 |         // (Round 5, user decision 2026-10-15: U-2's Probation bar has no subject — model review is abolished — and is removed; every
   468 |         // tier's claim is drawn and certified alike.)
   469 |         if row.anchor.is_none()
   470 |             && !palw_certification_public_check_v1(&row.class_id)
   471 |             && network.draws < policy.draws_per_epoch
   472 |             && let Some(a) = anchor.as_ref()
   473 |             && a.span >= row.committed_span.saturating_add(policy.anchor_delay_spans.max(1))
   474 |         {
   475 |             let id = if native { palw_native_certification_id_v39(&claim_id) } else { palw_certification_id_v1(&claim_id) };
   476 |             let execution_key = a.execution_key.as_bytes();
   477 |             let seed = palw_certification_seed_v1(&id, a.span, &execution_key);
   478 |             row.anchor = Some((a.span, execution_key));
   479 |             row.drawn_daa = now;
   480 |             row.deadline_daa = now.saturating_add(policy.attest_window_daa);
   481 |             network.draws = network.draws.saturating_add(1);
   482 |             match palw_certification_draw_v1(&seed, &id, &pool_v1(&builder.state, &policy, &row), policy.m) {
   483 |                 Some(draws) => row.draws = draws,
   484 |                 None => row.outcome = Some((PalwCertificationOutcomeV1::NoPopulation, now)),
   485 |             }
   486 |             changed = true;
   487 |         }
   488 |         if row.outcome.is_none() {
   489 |             let facts = if native {
   490 |                 // Native courts settle inside the component jobs before their escrow Final; the Master Final row's presence is
   491 |                 // the standing fact, and no chain accusation can name a native mining claim.
   492 |                 PalwCertificationClaimFactsV1 { still_final: native_live, accusation_pending: false }
   493 |             } else {
   494 |                 PalwCertificationClaimFactsV1 {
   495 |                     still_final: claim.as_ref().is_some_and(|c| matches!(c.phase, PalwClaimPhaseV2::Final { .. })),
   496 |                     // G14 R6: an ORIGINAL TIR frame's conviction door is a pending prosecution for Gate C until it closes — the
   497 |                     // decision waits (never a deadline failure), so no SecurityCertified credit precedes the last possible verdict.
   498 |                     accusation_pending: builder.state.palw_accusation_pending_v1(&claim_id)
   499 |                         || builder.state.palw_tir_public_prosecution_open_v1(&claim_id, now),
   500 |                 }
   501 |             };
   502 |             if let Some(outcome) = palw_certification_verdict_v1(&policy, &row, facts, now) {
   503 |                 row.outcome = Some((outcome, now));
   504 |                 changed = true;
   505 |             }
```

## D09 — 共有safe frontierとoldest unresolvedの影響

### `consensus/core/src/palw_state_v2.rs:15020–15091`

```text
 15020 |     //    below it.**
 15021 |     //
 15022 |     // What stood here asked `unresolved.is_empty()` — whether the chain, tip included, held no
 15023 |     // open claim — and named THIS block. Two failures fell out of it (audit C2), and they pull in
 15024 |     // opposite directions, which is why one rule has to answer both:
 15025 |     //
 15026 |     // * On a chain that produces work the condition is never true. Step 4 above just inserted
 15027 |     //   this block's own `Provisional` claim, so at every block forever the frontier stayed
 15028 |     //   wherever the chain last went idle. `pruning_ceiling_v2` froze with it (the node never
 15029 |     //   prunes again, and the carriage grows one claim per block without bound), and the
 15030 |     //   comparator's first key stopped discriminating between honest chains.
 15031 |     // * On a chain that produces NOTHING the condition is trivially true, so a fork carrying no
 15032 |     //   attempts at all advanced its frontier once per block and OUTRANKED a chain that had
 15033 |     //   matured real work. Reproduced: 60 attempt-less blocks beat a 60-block honest chain on
 15034 |     //   key 1, and the deep-reorg gate allowed it. That is precisely the fabrication that
 15035 |     //   ordering by frontier before weight exists to refuse.
 15036 |     //
 15037 |     // Both close by measuring MATURED WORK rather than the absence of open claims. `unresolved`
 15038 |     // is ordered by `accepted_blue_score`, so the resolved prefix ends immediately below its
 15039 |     // first entry; inside that prefix the frontier is the deepest `Final` claim — a claim, so it
 15040 |     // names its own accepting block, and a chain with no matured work has no frontier to stand
 15041 |     // on however long it grows. `Voided` claims are resolved (they do not hold the prefix back)
 15042 |     // but confer no frontier: a block whose work was thrown out matured nothing.
 15043 |     let old_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
 15044 |     let resolved_through = match builder.state.unresolved.iter().next() {
 15045 |         // Nothing open anywhere: the whole chain including this block is a resolved prefix.
 15046 |         None => u64::MAX,
 15047 |         // Everything strictly below the oldest open claim is resolved. Its own point is not —
 15048 |         // pruning there would delete history a live claim still needs.
 15049 |         Some((oldest_open, _)) => oldest_open.saturating_sub(1),
 15050 |     };
 15051 |     // **RFC-0018 Gate C's frontier site** (adopted 2026-10-15): past the fence the frontier is the deepest SecurityCertified ATTEMPT in
 15052 |     // the resolved prefix — the one consensus-credit rule settlement depth shares (`palw_consensus_credit_anchor_v1`). A `Final`
 15053 |     // claim not (yet) certified is resolved (it holds no prefix back) but matures nothing; a Free-Prompt `Final` never stands as the
 15054 |     // frontier. The identity below its fence (keyed on each claim's acceptance DAA) and on every shipped network.
 15055 |     if let Some(claim) = builder
 15056 |         .state
 15057 |         .claims
 15058 |         .iter()
 15059 |         .filter(|(id, c)| {
 15060 |             matches!(c.phase, PalwClaimPhaseV2::Final { .. })
 15061 |                 && c.accepted_blue_score <= resolved_through
 15062 |                 && crate::palw_rfc0018_v1::palw_gate_c_advances_frontier_v1(builder.params, &builder.state, id, c)
 15063 |         })
 15064 |         .map(|(_, c)| c)
 15065 |         .max_by_key(|c| c.accepted_blue_score)
 15066 |         && claim.accepted_blue_score > old_frontier.0
 15067 |     {
 15068 |         builder.state.safe_frontier_blue_score = claim.accepted_blue_score;
 15069 |         // **This hash is the claim's CARRYING block, which since ADR-0058 need not be on the
 15070 |         // selected chain** (audit M2-27). For a merged work it is the merged block — at
 15071 |         // `ghostdag_k = 1` typically a RED. The pair is still coherent (both halves describe that
 15072 |         // same block), and every consumer today reads only the blue score, which is what fork
 15073 |         // choice compares. What is NOT true any more is the old sentence "the deepest block on this
 15074 |         // chain": a consumer that resolves this hash must not assume chain membership, and the
 15075 |         // field's own definition is corrected to say so rather than the value being changed —
 15076 |         // the carrying block is what the job anchor is derived from, so it is load-bearing here.
 15077 |         builder.state.safe_frontier = claim.accepted_block;
 15078 |     }
 15079 |     // **RFC-0018 §7.5a (Gate C in force at this block): the frontier is RECOMPUTED** — the deepest frontier-eligible Final in the
 15080 |     // resolved prefix, never below the floor (seeded with the frontier above at the first such block, raised by retirements). So a
 15081 |     // certified claim later convicted stops standing as the frontier and the frontier falls back to the deepest remaining eligible
 15082 |     // claim; the delta's `Frontier` entry carries any move, so a reorg reverts it exactly. Below the fence the incremental rule above.
 15083 |     if builder.params.rfc0018_v1().is_some_and(|mirror| mirror.gate_c_at(ctx.daa_score)) {
 15084 |         let seeded = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
 15085 |         palw_security_certification_fold_v1::ensure_frontier_floor_v1(&mut builder, ctx, seeded)?;
 15086 |         let (blue_score, block) =
 15087 |             crate::palw_security_certification_v1::palw_gate_c_frontier_v1(&builder.state, builder.params, resolved_through);
 15088 |         builder.state.safe_frontier_blue_score = blue_score;
 15089 |         builder.state.safe_frontier = block;
 15090 |     }
 15091 |     let new_frontier = (builder.state.safe_frontier_blue_score, builder.state.safe_frontier);
```

## D10 — 共有capacity / per-DAA escrow budget / bond制約

### `consensus/core/src/palw_capacity_s567_v1.rs:61–88`

```text
    61 | /// One claim-bearing block's whole carve, in the ledger's unit (milli-carves).
    62 | pub const PALW_EMISSION_UNIT_MILLI_V1: u64 = 1_000;
    63 | 
    64 | /// **The budget, in claim-bearing blocks a DAA.** testnet-12's lane measured 5.3 claims a DAA at the worst healthy mix
    65 | /// (DAA ~3,000, with the five external seats), so 16 is three times the live rate and binds only where the lane's retarget
    66 | /// has collapsed and a burst of blocks would otherwise mint a burst of carves. A change is a new fence.
    67 | pub const PALW_EMISSION_BLOCKS_PER_DAA_V1: u64 = 16;
    68 | 
    69 | /// **A DAA's budget** in milli-carves.
    70 | pub const fn palw_emission_budget_milli_v1() -> u64 {
    71 |     PALW_EMISSION_BLOCKS_PER_DAA_V1 * PALW_EMISSION_UNIT_MILLI_V1
    72 | }
    73 | 
    74 | /// Why F-EM refuses a claim (non-fatal for the block's own attempt: it is skipped and its carve burned).
    75 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    76 | pub struct PalwEmissionRefusalV1 {
    77 |     pub spent_milli: u64,
    78 |     pub budget_milli: u64,
    79 | }
    80 | 
    81 | /// **Does the DAA's budget take one more claim-bearing block's carve?** `spent` is what the ledger holds for the accepting
    82 | /// block's DAA (0 for no row). A pure function of that number.
    83 | pub fn palw_emission_admits_v1(spent_milli: u64) -> Result<u64, PalwEmissionRefusalV1> {
    84 |     let budget_milli = palw_emission_budget_milli_v1();
    85 |     match spent_milli.checked_add(PALW_EMISSION_UNIT_MILLI_V1) {
    86 |         Some(after) if after <= budget_milli => Ok(after),
    87 |         _ => Err(PalwEmissionRefusalV1 { spent_milli, budget_milli }),
    88 |     }
```

### `consensus/core/src/palw_state_v2.rs:21682–21745`

```text
 21682 |     // **ADR-0160 stage 2 (F-Q §5.9 (f)): a credited claim waits for room in the audit backlog** — a
 21683 |     // queue bound, never the safety (an unaudited claim is never paid). Before any write, so step 4
 21684 |     // skips a refused own attempt (`AuditBacklogFull` joins the finding-17 arm).
 21685 |     if crate::palw_audit_door_v1::palw_capacity_claim_credited_v1(builder.params, &claim) {
 21686 |         let backlog = crate::palw_audit_door_v1::palw_capacity_audit_backlog_v1(&builder.state, builder.params);
 21687 |         let max = crate::palw_audit_door_v1::palw_capacity_audit_backlog_max_v1(&claim.class_id);
 21688 |         if backlog >= max {
 21689 |             return Err(PalwStateV2Error::AuditBacklogFull { class: claim.class_id, backlog, max });
 21690 |         }
 21691 |     }
 21692 |     // **ADR-0160 stage 2 (F-S): the issuance slot and the bucket** — `outstanding < N_out` and a whole
 21693 |     // token, read by the one function admission and the producer's facts read (S.4). Before any write.
 21694 |     let issuance_bond = claim.bond;
 21695 |     let issuance = match builder.state.bonds.get(&issuance_bond) {
 21696 |         Some(record) => crate::palw_issuance_slots_v1::palw_issuance_read_at_v1(
 21697 |             &builder.state,
 21698 |             builder.params,
 21699 |             &issuance_bond,
 21700 |             record.collateral,
 21701 |             ctx.daa_score,
 21702 |         ),
 21703 |         None => None,
 21704 |     };
 21705 |     if let Some(read) = &issuance {
 21706 |         read.admits_v1()
 21707 |             .map_err(|refusal| PalwStateV2Error::IssuanceCapped { bond: issuance_bond, refusal: format!("{refusal:?}") })?;
 21708 |     }
 21709 |     // **ADR-0164 F-EM: the DAA's PALW reward budget** — a claim-bearing block's whole carve is charged to the accepting block's DAA,
 21710 |     // 16 carves at most; a rider (paid out of its lead's carve) charges nothing. Before any write, like the slots: step 4 skips a
 21711 |     // refused own attempt (its carve burned) and 4b a merged one. `None` below the fence.
 21712 |     let emission_after = if builder.params.capacity_emission_active_at(ctx.daa_score)
 21713 |         && origin.escrows_reward
 21714 |         && !origin.rider
 21715 |         && claim.escrowed_reward > 0
 21716 |     {
 21717 |         let spent = builder.state.emission_spent_milli_v1(ctx.daa_score);
 21718 |         Some(crate::palw_capacity_s567_v1::palw_emission_admits_v1(spent).map_err(|refusal| {
 21719 |             PalwStateV2Error::EmissionBudgetExhausted { spent_milli: refusal.spent_milli, budget_milli: refusal.budget_milli }
 21720 |         })?)
 21721 |     } else {
 21722 |         None
 21723 |     };
 21724 |     // **ADR-0160 stage 4 (F-N): the network level and the work-conserving share** — the last check, so
 21725 |     // only an attempt every other rule admits is refused for want of a network unit (and registers its
 21726 |     // bond's demand: step 4 skips a refused own attempt, 4b a merged one); before any write.
 21727 |     builder.check_network_room_v1(&claim.bond, &claim, ctx.daa_score)?;
 21728 |     // **Lane BUDGET (ADR-0176 D1/D3): the producer bond's budget** — the claim's reservation (Q 1, B its block or its rider share, R its
 21729 |     // escrow, F its Final contribution) against the bond's window: the LAST check before any write, so step 4 skips a refused own attempt
 21730 |     // (its carve withheld as a skipped attempt's, never minted) and 4b a merged one; the claim's immature weight is held within the F it
 21731 |     // reserves. `None` below `palw_bond_budget_v1`: nothing read, nothing written.
 21732 |     // Lead default 2026-10-15 (pending user review): the BASE floor class is exempt from native custody (no paid material; provider
 21733 |     // absence must not halt floor liveness). Below the monetary fence both calls below are no-ops for every class, so skipping
 21734 |     // them for the floor is byte-identical there.
 21735 |     let floor_exempt = claim.class_id == builder.params.base_class_id();
 21736 |     let monetary_ready = if floor_exempt { None } else { palw_monetary_material_fold_v1::plan_native_ready(builder, ctx, claim_id)? };
 21737 |     if monetary_ready.is_none() && !floor_exempt {
 21738 |         builder.state.require_native_monetary_material_scope_v1(builder.params, claim_id, &claim)?;
 21739 |     }
 21740 |     let consumer_plan = builder.plan_paid_consumer_v1(claim_id, &claim)?;
 21741 |     // **RFC-0017 (`palw_model_eligibility_v1`, REQ-ELIG-2): a `Probation` model's attempt** is admitted whole inside the model's stage
 21742 |     // cap and the network's fixed probation budget (the bond's own ADR-0176 `B` cap is the plan below), or refused — before any write.
 21743 |     // Admitted, it is planned with no Final weight (F = 0) and pinned after the claim is written. `None` below the fence.
 21744 |     let probation_ticket = palw_model_eligibility_fold_v1::eligibility_probation_preview_v1(
 21745 |         builder,
```

### `consensus/core/src/palw_rfc0016_fold_v1.rs:286–335`

```text
   286 |     pub(super) fn rfc0016_capacity_read_v1(
   287 |         &self,
   288 |         class_id: &Hash64,
   289 |         fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
   290 |         now_daa: u64,
   291 |         extra: Option<(Hash64, PalwGatedClaimV1)>,
   292 |     ) -> (u128, u128, u32) {
   293 |         let g = &fold.globals;
   294 |         let seat = palw_panel_shipped_per_span_v1(1, g).saturating_mul(crate::palw_work_target_v1::PALW_PANEL_DEMAND_SCALE_V1);
   295 |         let base = self.params.base_class_id();
   296 |         let classes: Vec<Hash64> = self.state.model_lifecycles.keys().filter(|c| **c != base).copied().collect();
   297 |         let mut verifiers: Vec<PalwVerifierSupplyV1> = Vec::new();
   298 |         let mut ready_here: u32 = 0;
   299 |         for (bond_key, bond) in self.state.bonds.iter() {
   300 |             let ready: BTreeSet<Hash64> =
   301 |                 classes.iter().filter(|c| self.model_registry_seat_is_ready(bond_key, bond, c, now_daa, fold)).copied().collect();
   302 |             if ready.contains(class_id) {
   303 |                 ready_here = ready_here.saturating_add(1);
   304 |             }
   305 |             if !ready.is_empty() {
   306 |                 verifiers.push(PalwVerifierSupplyV1 { per_span: seat, classes: ready });
   307 |             }
   308 |         }
   309 |         let terms = self.with_inflight_index(|index| {
   310 |             let owed = palw_panel_owed_v1(self.state, self.params, index, self.own_attempt_class, extra);
   311 |             palw_panel_terms_v1(self.state, g.seat_count as u128, &owed)
   312 |         });
   313 |         let capacity = palw_waterfill_capacity_v1(&verifiers, &terms);
   314 |         (capacity.get(class_id).copied().unwrap_or(0), terms.get(class_id).copied().unwrap_or(0), ready_here)
   315 |     }
   316 | 
   317 |     /// **The elastic room** (RFC-0016 §10): `None` below `palw_elastic_final_capacity_v1` (the shipped room decides). Past it:
   318 |     /// capacity 0 admits nothing; demand within capacity admits; otherwise the claim is admitted into the class's bounded pending queue
   319 |     /// while it has room (it is queued at acceptance), or refused (queue full). A C7 (long-window) class is held to the concurrency
   320 |     /// derived from its capacity instead of its static `max_inflight_claims`.
   321 |     pub(super) fn rfc0016_elastic_room_v1(
   322 |         &self,
   323 |         class_id: &Hash64,
   324 |         row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
   325 |         fold: &crate::palw_model_registry_v1::PalwModelRegistryFoldV1,
   326 |         now_daa: u64,
   327 |         whole: u64,
   328 |         incoming: PalwGatedClaimV1,
   329 |     ) -> Option<Result<(), PalwStateV2Error>> {
   330 |         if !self.params.rfc0016_capacity_active_at(now_daa) {
   331 |             return None;
   332 |         }
   333 |         let mirror = self.params.rfc0016_v1()?;
   334 |         let (capacity, demand, _ready) = self.rfc0016_capacity_read_v1(class_id, fold, now_daa, Some((*class_id, incoming)));
   335 |         let window = row.profile.verification_window_spans as u64;
```

## D11 — DAG幅scheduleはstub、既存の2分設定

### `consensus/core/src/palw_rfc0018_v1.rs:463–538`

```text
   463 | // -------------------------------------------------------------------------------------------------
   464 | // The DAG width schedule (RFC-0018 §8) — SPEC AND STUB
   465 | // -------------------------------------------------------------------------------------------------
   466 | 
   467 | /// **One step of the DAG width schedule.** No reader exists in this binary: GHOSTDAG's `k`, the parents bound and the mergeset bound are
   468 | /// constructor scalars of the GHOSTDAG manager and the header pipeline, and the block time scales every DAA-denominated PALW window
   469 | /// (RFC-0018 §8.4). A per-block reader (ForkedParam-style, keyed on the selected parent's DAA) is the follow-up the user decides on.
   470 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   471 | pub struct PalwDagWidthScheduleV1 {
   472 |     pub ghostdag_k: KType,
   473 |     pub target_time_per_block_ms: u64,
   474 |     pub max_block_parents: u16,
   475 |     pub mergeset_size_limit: u64,
   476 | }
   477 | 
   478 | impl PalwDagWidthScheduleV1 {
   479 |     /// The schedule's identity (hashed into both fingerprints beside the height).
   480 |     pub fn id(&self) -> Hash64 {
   481 |         let mut h = keyed(PALW_RFC0018_DAG_WIDTH_DOMAIN_V1);
   482 |         h.update(&self.ghostdag_k.to_le_bytes());
   483 |         h.update(&self.target_time_per_block_ms.to_le_bytes());
   484 |         h.update(&self.max_block_parents.to_le_bytes());
   485 |         h.update(&self.mergeset_size_limit.to_le_bytes());
   486 |         finish(h)
   487 |     }
   488 | 
   489 |     /// Structural refusals (the values themselves are the user's).
   490 |     pub fn refusal(&self) -> Option<&'static str> {
   491 |         if self.ghostdag_k == 0 || self.max_block_parents < 2 || self.target_time_per_block_ms == 0 {
   492 |             return Some("palw_dag_width_schedule_v1: k ≥ 1, at least two parents and a positive block time");
   493 |         }
   494 |         if self.mergeset_size_limit < self.ghostdag_k as u64 * 10 {
   495 |             return Some("palw_dag_width_schedule_v1: the mergeset bound must exceed 10·k (Kaspa's own relation)");
   496 |         }
   497 |         None
   498 |     }
   499 | }
   500 | 
   501 | /// **The fence**: the height and the schedule step.
   502 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   503 | pub struct PalwDagWidthScheduleFenceV1 {
   504 |     pub activation: ForkActivation,
   505 |     pub schedule: PalwDagWidthScheduleV1,
   506 | }
   507 | 
   508 | impl PalwDagWidthScheduleFenceV1 {
   509 |     /// Structural fixture values for the fork-id probe and the A-2 harness. NOT a proposal: validation refuses every armed value.
   510 |     pub fn unapproved_probe_at(activation: ForkActivation) -> Self {
   511 |         Self {
   512 |             activation,
   513 |             schedule: PalwDagWidthScheduleV1 {
   514 |                 ghostdag_k: 3,
   515 |                 target_time_per_block_ms: 120_000,
   516 |                 max_block_parents: 16,
   517 |                 mergeset_size_limit: 360,
   518 |             },
   519 |         }
   520 |     }
   521 | }
   522 | 
   523 | /// **The width analysis (RFC-0018 §8.2)**: the expected fraction of honest blocks a selected chain colours RED at GHOSTDAG `k`, for
   524 | /// blocks whose effective publication delay is `delay_ms` (propagation plus, for an attempt, the inference time since its parent) at a
   525 | /// network block interval of `interval_ms` — the Poisson tail `P(anticone > k)` with `λ = 2·delay/interval`, in permille. A planning
   526 | /// estimate (independent arrivals), not a security proof.
   527 | pub fn palw_dag_width_red_permille_v1(delay_ms: u64, interval_ms: u64, k: KType) -> u32 {
   528 |     if interval_ms == 0 {
   529 |         return 1000;
   530 |     }
   531 |     let lambda = 2.0 * delay_ms as f64 / interval_ms as f64;
   532 |     let mut term = (-lambda).exp();
   533 |     let mut cdf = term;
   534 |     for i in 1..=k as u32 {
   535 |         term *= lambda / i as f64;
   536 |         cdf += term;
   537 |     }
   538 |     ((1.0 - cdf).clamp(0.0, 1.0) * 1000.0).round() as u32
```

### `consensus/core/src/config/params.rs:228–260`

```text
   228 |     pub const fn new_two_minute_bps() -> Self {
   229 |         Self::new_seconds_per_block(120, 1)
   230 |     }
   231 | 
   232 |     /// Shared constructor for sub-1-bps networks: the same formulas `Bps<BPS>` encodes, evaluated
   233 |     /// at λ = 1/`secs` (which `Bps` cannot express — its const generic is an integer bps).
   234 |     ///
   235 |     /// Per-block sample rates floor at 1 (a rate below one block means "sample every block"), and
   236 |     /// `max_block_parents` / `mergeset_size_limit` sit at the formula floors for every k this is
   237 |     /// used with (k/2 < 10, 2k < 180). `ghostdag_k` is a parameter because `calculate_ghostdag_k`
   238 |     /// is not a const fn; each caller's value is asserted against the real formula in tests.
   239 |     pub const fn new_seconds_per_block(secs: u64, ghostdag_k: KType) -> Self {
   240 |         use crate::config::constants::consensus::{
   241 |             COINBASE_MATURITY_SECONDS, FINALITY_DURATION, MERGE_DEPTH_DURATION, PRUNING_DURATION,
   242 |         };
   243 |         let mergeset_size_limit: u64 = 180;
   244 |         let finality_depth = FINALITY_DURATION / secs;
   245 |         let merge_depth = MERGE_DEPTH_DURATION / secs;
   246 |         // The prunality lower bound, exactly as `Bps::pruning_depth` decomposes it.
   247 |         let lower_bound = finality_depth + merge_depth * 2 + 4 * mergeset_size_limit * ghostdag_k as u64 + 2 * ghostdag_k as u64 + 2;
   248 |         let duration_term = PRUNING_DURATION / secs;
   249 |         let pruning_depth = if lower_bound > duration_term { lower_bound } else { duration_term };
   250 |         // Ceiling division: maturity must never round DOWN below its wall-clock intent (at
   251 |         // 120 s/block, 100 s of maturity is one block, not zero).
   252 |         let coinbase_maturity = COINBASE_MATURITY_SECONDS.div_ceil(secs);
   253 |         Self {
   254 |             target_time_per_block: secs * 1000,
   255 |             ghostdag_k,
   256 |             past_median_time_sample_rate: 1,
   257 |             difficulty_sample_rate: 1,
   258 |             max_block_parents: 10,
   259 |             mergeset_size_limit,
   260 |             merge_depth,
```

### `consensus/core/src/config/constants.rs:75–85`

```text
    75 |     /// Merge depth bound duration (in seconds). For 1 BPS networks this equals the legacy depth
    76 |     /// bound in block units. For higher BPS networks this should be scaled up.
    77 |     ///
    78 |     /// This number should be roughly equal to DAA window duration in order to prevent merging
    79 |     /// low-difficulty side-chains (up to ~2x over DAA duration is still reasonable since creating
    80 |     /// a mergeable low-difficulty side-chain within this bound requires a significant hashrate fraction)
    81 |     pub const MERGE_DEPTH_DURATION: u64 = 3600;
    82 | 
    83 |     /// The value of the pruning proof `M` parameter
    84 |     pub const PRUNING_PROOF_M: u64 = 1000;
    85 | 
```

## D12 — genesis固定operatorだけでないpermissionless anchorの接続

### `consensus/core/src/palw_rfc0018_v1.rs:347–456`

```text
   347 | // -------------------------------------------------------------------------------------------------
   348 | // The permissionless Panel anchor (RFC-0018 §5)
   349 | // -------------------------------------------------------------------------------------------------
   350 | 
   351 | /// One candidate contribution to a permissionless anchor: an attempt the anchor block is or merges (blue or red).
   352 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   353 | pub struct PalwAnchorContributionV1 {
   354 |     pub daa_score: u64,
   355 |     pub block: Hash64,
   356 |     pub bond: PalwBondKeyV2,
   357 |     /// The attempt's execution commitment (lane F1's seed unit).
   358 |     pub execution: Hash64,
   359 |     /// The fold admitted the attempt. Contributions are read from COMMITTED fold output
   360 |     /// ([`palw_permissionless_contributions_from_state_v1`]: the claims of the anchor block's selected-parent state), so this is
   361 |     /// `true` for every contribution the node builds; header-only operator anchoring had no such check (RFC-0018 F-6).
   362 |     pub admitted: bool,
   363 |     /// Kept for the pure tests' fixtures; the committed-output reading needs no maturity input (admission already ran on the bond).
   364 |     pub bond_matured: bool,
   365 | }
   366 | 
   367 | /// **The permissionless anchor's contributions, from committed fold output (RFC-0018 §5.2, exact)**: every ATTEMPT claim of `state`
   368 | /// (the anchor block's selected-parent state — fold output committed before the anchor block) accepted by a chain block below
   369 | /// `before_blue_score`, BLUE or RED alike (`accepted_block` is the carrying block). Phase-independent — a claim the fold admitted was
   370 | /// admitted, whatever happened to it later — so the processor (reading the selected parent's state) and the fold's step 4c (reading
   371 | /// its own state, which adds only claims accepted in the anchor block itself, excluded by the blue-score bound) read one set.
   372 | /// Recognition is therefore one chain block later than the attempt's acceptance.
   373 | pub fn palw_permissionless_contributions_from_state_v1(state: &PalwChainStateV2, before_blue_score: u64) -> Vec<PalwAnchorContributionV1> {
   374 |     state
   375 |         .claims_iter()
   376 |         .filter(|(_, claim)| matches!(claim.source, PalwClaimSourceV2::Attempt) && claim.accepted_blue_score < before_blue_score)
   377 |         .map(|(id, claim)| PalwAnchorContributionV1 {
   378 |             daa_score: claim.accepted_daa,
   379 |             block: claim.accepted_block,
   380 |             bond: claim.bond,
   381 |             execution: claim.work_id.unwrap_or(*id),
   382 |             admitted: true,
   383 |             bond_matured: true,
   384 |         })
   385 |         .collect()
   386 | }
   387 | 
   388 | /// **Whether these contributions anchor a claim of `producer` whose slot is `slot`**: at least the policy's minimum of OTHER bonds
   389 | /// contribute at or past the slot (the claim's own producer never seeds its own Panel). The one predicate the processor's walk, its
   390 | /// binding derivation and the fold's step 4c ask.
   391 | pub fn palw_permissionless_anchors_claim_v1(contributions: &[PalwAnchorContributionV1], slot: u64, producer: &PalwBondKeyV2) -> bool {
   392 |     palw_permissionless_anchor_seed_v1(contributions, slot, Some(producer), &PalwPermissionlessAnchorPolicyV1::design_unapproved_v1())
   393 |         .is_some()
   394 | }
   395 | 
   396 | /// The permissionless anchor's design values (UNAPPROVED; RFC-0018 §11).
   397 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   398 | pub struct PalwPermissionlessAnchorPolicyV1 {
   399 |     /// Fewest distinct bonds whose contributions the aggregate must hold (≥ 2: one honest unpredictable contribution must be inside).
   400 |     pub min_distinct_bonds: u32,
   401 | }
   402 | 
   403 | /// Unapproved design value (RFC-0018 §11): at least one bond OTHER than the claim's producer (the producer is excluded per claim).
   404 | pub const PALW_RFC0018_ANCHOR_MIN_DISTINCT_BONDS_DESIGN_V1: u32 = 1;
   405 | 
   406 | impl PalwPermissionlessAnchorPolicyV1 {
   407 |     pub const fn design_unapproved_v1() -> Self {
   408 |         Self { min_distinct_bonds: PALW_RFC0018_ANCHOR_MIN_DISTINCT_BONDS_DESIGN_V1 }
   409 |     }
   410 | }
   411 | 
   412 | /// **The aggregate seed source of a permissionless anchor for a claim whose slot is `slot`** — `(draw DAA, aggregate)`, or `None` where
   413 | /// the anchor does not (yet) reach the slot.
   414 | ///
   415 | /// Eligible: admitted, matured-bond contributions at or past the slot, excluding the claim's own producer bond. One contribution per
   416 | /// bond — its earliest `(DAA, block)` — so a bond's extra attempts neither add weight nor re-roll (its later wins are ignored). The
   417 | /// aggregate is `H(domain ‖ slot ‖ Σ sorted (bond, execution))` over those per-bond contributions; it needs at least
   418 | /// `min_distinct_bonds`. The draw DAA is the least eligible DAA (the draw's clock-keyed inputs resolve there, as lane A's do at its
   419 | /// seed attempt). Order-independent: blue and red alike, and the result is a function of the contribution SET.
   420 | ///
   421 | /// **Grinding bound (RFC-0018 §5.3):** an adversary controlling `b` of the contributing bonds can choose, per bond, among withholding
   422 | /// and its own admitted attempts in the window; the honest contributions (unpredictable execution commitments) are inside the hash, so
   423 | /// its choice set is at most `Π_{its bonds}(1 + q_b)` where `q_b` is the bond's admitted attempts in the window — bounded by ADR-0176's
   424 | /// per-bond claim budget, each costing a winning inference. The panel-capture probability is multiplied by at most that factor.
   425 | pub fn palw_permissionless_anchor_seed_v1(
   426 |     contributions: &[PalwAnchorContributionV1],
   427 |     slot: u64,
   428 |     producer: Option<&PalwBondKeyV2>,
   429 |     policy: &PalwPermissionlessAnchorPolicyV1,
   430 | ) -> Option<(u64, Hash64)> {
   431 |     let mut first: BTreeMap<PalwBondKeyV2, (u64, Hash64, Hash64)> = BTreeMap::new();
   432 |     for c in contributions.iter().filter(|c| c.admitted && c.bond_matured && c.daa_score >= slot && Some(&c.bond) != producer) {
   433 |         let entry = first.entry(c.bond).or_insert((c.daa_score, c.block, c.execution));
   434 |         if (c.daa_score, c.block) < (entry.0, entry.1) {
   435 |             *entry = (c.daa_score, c.block, c.execution);
   436 |         }
   437 |     }
   438 |     if (first.len() as u64) < policy.min_distinct_bonds.max(1) as u64 {
   439 |         return None;
   440 |     }
   441 |     let draw_daa = first.values().map(|(daa, _, _)| *daa).min()?;
   442 |     let mut h = keyed(PALW_RFC0018_ANCHOR_AGGREGATE_DOMAIN_V1);
   443 |     h.update(&slot.to_le_bytes());
   444 |     for (bond, (_, _, execution)) in &first {
   445 |         h.update(bond.0.transaction_id.as_bytes().as_slice());
   446 |         h.update(&bond.0.index.to_le_bytes());
   447 |         h.update(execution.as_byte_slice());
   448 |     }
   449 |     Some((draw_daa, finish(h)))
   450 | }
   451 | 
   452 | /// The highest slot a permissionless anchor reaches: the latest eligible contribution's DAA, capped at the anchor's own DAA (as lane A).
   453 | pub fn palw_permissionless_anchor_reach_v1(contributions: &[PalwAnchorContributionV1], anchor_daa: u64) -> Option<u64> {
   454 |     // Claim-independent: whether THIS claim is anchored (a non-producer contribution at or past its slot) is
   455 |     // `palw_permissionless_anchors_claim_v1`, asked per claim by the walk, the derivation and step 4c.
   456 |     let distinct: BTreeSet<PalwBondKeyV2> = contributions.iter().filter(|c| c.admitted && c.bond_matured).map(|c| c.bond).collect();
```

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:7022–7054`

```text
  7022 |     fn palw_permissionless_seed_source_v1(
  7023 |         &self,
  7024 |         anchor: &PalwChainAnchorV1,
  7025 |         slot: u64,
  7026 |         producer: Option<&kaspa_consensus_core::palw_state_v2::PalwBondKeyV2>,
  7027 |     ) -> Option<(u64, kaspa_consensus_core::Hash64)> {
  7028 |         let contributions = anchor.permissionless.as_ref()?;
  7029 |         kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_anchor_seed_v1(
  7030 |             contributions,
  7031 |             slot,
  7032 |             producer,
  7033 |             &kaspa_consensus_core::palw_rfc0018_v1::PalwPermissionlessAnchorPolicyV1::design_unapproved_v1(),
  7034 |         )
  7035 |     }
  7036 | 
  7037 |     /// **RFC-0018 §5: the contributions chain block `block` is or merges** — every attempt header (the block itself and its blue and
  7038 |     /// genuine red mergeset, not its selected parent) whose envelope decodes and whose executor bond the registry at the block's
  7039 |     /// selected parent holds under the very key the envelope carries (the header stage verified the signature over that key), each
  7040 |     /// with the bond's maturity at the attempt's DAA. `admitted` is the header-and-registry approximation of admission (GAP-A1: the
  7041 |     /// fold's own acceptance of a merged attempt is decided in this same block's transition, so reading it here would be circular).
  7042 |     /// Reads only chain data every node holds. `None` where a store cannot serve what it reads.
  7043 |     fn palw_permissionless_contributions_of_chain_block_v1(
  7044 |         &self,
  7045 |         block: BlockHash,
  7046 |         header: &Header,
  7047 |     ) -> Option<Vec<kaspa_consensus_core::palw_rfc0018_v1::PalwAnchorContributionV1>> {
  7048 |         // RFC-0018 §5.2 (exact): the contributions are the ADMITTED attempts of committed fold output — the claims of the block's
  7049 |         // selected-parent state, blue and red carriers alike — so recognition runs one chain block behind admission and no header
  7050 |         // the fold refused ever contributes.
  7051 |         let data = self.ghostdag_store.get_data(block).ok()?;
  7052 |         let committed = self.palw_candidate_state_v2(data.selected_parent)?;
  7053 |         Some(kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_contributions_from_state_v1(&committed, header.blue_score))
  7054 |     }
```

### `consensus/src/pipeline/virtual_processor/processor/palw.rs:7124–7137`

```text
  7124 |         // **RFC-0018 §5: past `palw_panel_anchor_permissionless_v1` (at the block's own DAA, as lane A) the anchor is permissionless** —
  7125 |         // it takes precedence over lane A's operator list, which it replaces. A block anchors the slots up to its latest eligible
  7126 |         // contribution (capped at its own DAA) once at least two distinct bonds contribute; the seed is their aggregate
  7127 |         // (`palw_permissionless_seed_source_v1`). Below the fence (every shipped network) this branch is never taken.
  7128 |         if self.palw_panel_anchor_permissionless_at(daa) {
  7129 |             let contributions = self.palw_permissionless_contributions_of_chain_block_v1(block, header)?;
  7130 |             let reach = kaspa_consensus_core::palw_rfc0018_v1::palw_permissionless_anchor_reach_v1(&contributions, daa);
  7131 |             return Some(reach.map(|reach| PalwChainAnchorV1 {
  7132 |                 reach,
  7133 |                 operator_attempts: Some(contributions.iter().map(|c| (c.daa_score, c.block)).collect()),
  7134 |                 permissionless: Some(contributions),
  7135 |             }));
  7136 |         }
  7137 |         match self.palw_operator_anchor.as_ref().filter(|rule| rule.active_at(daa)) {
```

## D13 — 認証creditゼロ時のGHOSTDAG比較、旧DNS gate退役

### `consensus/core/src/palw_fork_choice_rule_e_v1.rs:826–865`

```text
   826 | pub fn palw_rule_e_decide_v2(
   827 |     incumbent: &PalwRuleESideV1,
   828 |     challenger: &PalwRuleESideV1,
   829 |     participation_counts: bool,
   830 |     pair: &PalwRuleEPairV1,
   831 |     capital_v2: bool,
   832 |     ghostdag_heavier: impl FnOnce() -> bool,
   833 |     shallow_ghostdag_win: impl FnOnce() -> bool,
   834 | ) -> PalwDeepReorgV2 {
   835 |     if !capital_v2 {
   836 |         return palw_rule_e_decide_v1(
   837 |             incumbent,
   838 |             challenger,
   839 |             participation_counts,
   840 |             pair.even_split_min,
   841 |             ghostdag_heavier,
   842 |             shallow_ghostdag_win,
   843 |         );
   844 |     }
   845 |     let allow = |yes: bool| if yes { PalwDeepReorgV2::Allow } else { PalwDeepReorgV2::Refuse };
   846 |     // RFC-0018 §6.4: where NEITHER exclusive past holds any certified consensus credit (no certified participation, no frontier, no
   847 |     // weight), no PALW finality is at stake on either side — the DAG's own order decides, so two uncertified partitions converge
   848 |     // instead of each keeping its incumbent. Uncertified history carries no PALW settlement finality (§7.8), so nothing is reversed
   849 |     // that the chain ever reported final.
   850 |     let no_credit = |s: &PalwRuleESideV1| s.participation_capital_v2 == 0 && s.economic() == (0, 0, 0);
   851 |     if no_credit(incumbent) && no_credit(challenger) {
   852 |         return allow(ghostdag_heavier());
   853 |     }
   854 |     match palw_rule_e_order_v2(challenger, incumbent, participation_counts, true) {
   855 |         Ordering::Greater => PalwDeepReorgV2::Allow,
   856 |         Ordering::Less => PalwDeepReorgV2::Refuse,
   857 |         Ordering::Equal
   858 |             if participation_counts
   859 |                 && incumbent.participation_capital_v2 >= pair.even_split_min_capital_v2
   860 |                 && challenger.participation_capital_v2 >= pair.even_split_min_capital_v2 =>
   861 |         {
   862 |             allow(ghostdag_heavier())
   863 |         }
   864 |         Ordering::Equal => allow(shallow_ghostdag_win()),
   865 |     }
```

### `consensus/src/pipeline/virtual_processor/palw_rule_e.rs:202–230`

```text
   202 |     /// **Rule E's deep-reorg gate** (ADR-0178): may `candidate` replace the incumbent `prev_sink`? Unweighable refuses.
   203 |     pub(crate) fn palw_rule_e_gate_v1(
   204 |         &self,
   205 |         candidate: BlockHash,
   206 |         prev_sink: BlockHash,
   207 |         incumbent_daa: u64,
   208 |     ) -> Result<PalwDeepReorgV2, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
   209 |         let mut states = PalwRuleEStatesV1::default();
   210 |         let Some((pair, counts)) = self.palw_rule_e_pair_checked_v1(&mut states, candidate, prev_sink)? else {
   211 |             info!(
   212 |                 "deep reorg refused (rule E): candidate {candidate} or the incumbent {prev_sink} cannot be weighed over their exclusive pasts"
   213 |             );
   214 |             return Ok(PalwDeepReorgV2::Refuse);
   215 |         };
   216 |         let heavier = || match (self.ghostdag_store.get_blue_work(candidate), self.ghostdag_store.get_blue_work(prev_sink)) {
   217 |             (Ok(c), Ok(p)) => SortableBlock::new(candidate, c) > SortableBlock::new(prev_sink, p),
   218 |             _ => false,
   219 |         };
   220 |         // RFC-0018 §6: v2 participation past its fence at the incumbent's DAA; below it `palw_rule_e_decide_v1` exactly.
   221 |         let capital_v2 = self.palw_rule_e_participation_v2_at(incumbent_daa);
   222 |         let decision = palw_rule_e_decide_v2(&pair.b, &pair.a, counts, &pair, capital_v2, heavier, || {
   223 |             self.palw_reorg_shallow_ghostdag_win_v1(candidate, prev_sink, incumbent_daa)
   224 |         });
   225 |         debug!(
   226 |             "rule E gate: candidate {candidate} {:?} against the incumbent {prev_sink} {:?} (participation counts: {counts}, even split at {}) — {decision:?}",
   227 |             pair.a, pair.b, pair.even_split_min
   228 |         );
   229 |         Ok(decision)
   230 |     }
```

### `consensus/src/pipeline/virtual_processor/processor/dns.rs:148–164`

```text
   148 |         // **ADR-0128 Decision 5: the stake reorg gate follows the BFT vote, and it is asked FIRST.**
   149 |         //
   150 |         // Past the fence at the incumbent sink, a candidate that abandons the DNS-final anchor is
   151 |         // refused before the PALW comparator below is consulted — a reorg and an extension alike,
   152 |         // which is what the early return below used to skip for a reorg. A candidate this does not
   153 |         // refuse (it contains the anchor, the anchor is stale on this node's own chain, or nothing
   154 |         // is confirmed) goes on exactly as it does without the fence. See `dns_bft_gate_refusal`.
   155 |         let incumbent_daa = match self.headers_store.get_daa_score(prev_sink) {
   156 |             Ok(daa) => daa,
   157 |             Err(_) => return Ok(DnsReorgOutcome::DominanceViolation),
   158 |         };
   159 |         let dns_retired = self.dns_retired_at(incumbent_daa);
   160 |         if !dns_retired && let Some(refusal) = self.dns_bft_gate_refusal(candidate, prev_sink) {
   161 |             return Ok(refusal);
   162 |         }
   163 |         // **Unit D, site 4: on a V2 network the deep-reorg gate IS the one comparator.**
   164 |         //
```

## D14 — 3モデルsibling試験が実際にassertする範囲

### `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs:1–58`

```text
     1 | //! **RFC-0018 §10 T-1/T-2, in process, on testnet-12's own GHOSTDAG (k = 1): three models, three miners, sibling attempts from the
     2 | //! same parents — all retained, merged, and each claim recorded and attributed on its own, whatever order the blocks arrive in.**
     3 | //!
     4 | //! The chain is testnet-12 as launched (the harness cards). Every genesis model row that is not the floor is planted `Active` on eight
     5 | //! ready seats (`t12_real_share`'s plant), so three classes are open: the floor and the two model rows. Three cards each build an
     6 | //! attempt of a DIFFERENT class on the SAME virtual (three templates before any is inserted), the three land as siblings, and the next
     7 | //! heartbeat merges them — one is the selected parent, the others are merged blue or red (k = 1).
     8 | //!
     9 | //! * `…all_retained_and_each_recorded…` — every sibling is in the merging block's past (selected parent, blue or red) and every claim
    10 | //!   is in the state with its own bond, class and carrying block (attribution is per claim, never per round or per parent).
    11 | //! * `…arrival_order_does_not_matter…` — a second node receives the same blocks in the reverse order: the same sink, the same
    12 | //!   GHOSTDAG colouring and the same PALW state root (DAG interpretation and adopted claims converge).
    13 | //! * `…with_the_sibling_fence…` — the same two runs with `palw_dag_sibling_attribution_v1` armed from genesis (a FIXTURE: the fence is
    14 | //!   refused by validation on every network; the harness does not validate): unchanged retention, and the canonical order yields the
    15 | //!   same state on both arrival orders.
    16 | //!
    17 | //! **Unverified source (2026-10-15): written, not built and not run.** Real-process versions: `scripts/drills/rfc0018_dag_drill.py`.
    18 | use super::OnetimeTxSelector;
    19 | use super::t12_round_lane_e2e::{T12Chain, card_payout_spk, stamp_harness_time, t12_genesis_chain, t12_with_harness_cards};
    20 | use crate::consensus::test_consensus::TestConsensus;
    21 | use crate::model::stores::ghostdag::GhostdagStoreReader;
    22 | use kaspa_consensus_core::BlockHash;
    23 | use kaspa_consensus_core::api::ConsensusApi;
    24 | use kaspa_consensus_core::block::{Block, MutableBlock, TemplateBuildMode};
    25 | use kaspa_consensus_core::coinbase::MinerData;
    26 | use kaspa_consensus_core::config::params::ForkActivation;
    27 | use kaspa_consensus_core::config::{Config, ConfigBuilder};
    28 | use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
    29 | use kaspa_consensus_core::palw_model_registry_v1::{PalwModelLifecycleV1, PalwSeatReadinessRowV1};
    30 | use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    31 | use kaspa_hashes::Hash64;
    32 | 
    33 | type Premine = Vec<(kaspa_consensus_core::tx::TransactionOutpoint, kaspa_consensus_core::tx::UtxoEntry)>;
    34 | type Parts = (Config, PalwConsensusParamsV2, Premine, Premine);
    35 | 
    36 | fn card_pubkey(i: usize) -> Vec<u8> {
    37 |     TestConsensus::palw_v2_registry_keypair(i as u64).verification_key.as_ref().to_vec()
    38 | }
    39 | 
    40 | /// testnet-12 as launched; with `sibling_fence`, RFC-0018's sibling attribution armed from genesis (fixture only).
    41 | fn parts(sibling_fence: bool) -> Parts {
    42 |     let (config, _bundle, premine, floats) = t12_with_harness_cards();
    43 |     let mut params = config.params.clone();
    44 |     if sibling_fence {
    45 |         params.palw_dag_sibling_attribution_v1 = Some(ForkActivation::always());
    46 |         params.sync_palw_rfc0018_v1();
    47 |     }
    48 |     let config = ConfigBuilder::new(params).skip_proof_of_work().build();
    49 |     let bundle = match &config.params.palw_consensus_mode {
    50 |         PalwConsensusMode::ConsensusV2(b) => b.clone(),
    51 |         _ => unreachable!("testnet-12 is ConsensusV2"),
    52 |     };
    53 |     assert_eq!(
    54 |         bundle.state.rfc0018_v1().is_some_and(|m| m.sibling_attribution_at(0)),
    55 |         sibling_fence,
    56 |         "the fold's mirror follows the fixture fence"
    57 |     );
    58 |     (config, bundle, premine, floats)
```

### `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs:92–193`

```text
    92 |     fn finish(c: T12Chain, bundle: &PalwConsensusParamsV2, history: Vec<Block>) -> Rig {
    93 |         let mut classes = vec![bundle.base_class_id];
    94 |         {
    95 |             let (_, tip) = c.tip_state();
    96 |             classes.extend(tip.model_lifecycles_iter().map(|(id, _)| *id).filter(|id| *id != bundle.base_class_id).take(2));
    97 |         }
    98 |         assert_eq!(classes.len(), 3, "testnet-12's genesis holds the floor and two model rows");
    99 |         let mut rig = Rig { c, classes, nonce: 1 << 46, history };
   100 |         rig.plant();
   101 |         rig
   102 |     }
   103 | 
   104 |     /// Every non-floor class `Active` on eight ready seats at the sink (`t12_real_share`'s plant, for each row).
   105 |     fn plant(&mut self) {
   106 |         let vp = self.c.vp();
   107 |         let (sink, tip) = vp.palw_state_v2_store.read().load_tip(&self.c.bundle.state).unwrap().expect("the tip loads");
   108 |         let now = self.c.ctx.consensus.get_virtual_daa_score();
   109 |         let mut carriage = PalwStateCarriageV2::from_state(&tip);
   110 |         for class in self.classes.iter().skip(1) {
   111 |             let row = carriage.model_lifecycles.get_mut(class).expect("the model row");
   112 |             row.state = PalwModelLifecycleV1::Active;
   113 |             row.profile.max_inflight_claims = row.profile.max_inflight_claims.max(16);
   114 |             for bond in &self.c.bonds {
   115 |                 carriage.seat_readiness.insert(
   116 |                     (*bond, *class),
   117 |                     PalwSeatReadinessRowV1 { proved_daa: now, proved_span: now, leaf_index: 0, proof_version: 2, chunks: 8 },
   118 |                 );
   119 |             }
   120 |         }
   121 |         let planted = carriage.into_state(&self.c.bundle.state, None).expect("the planted tip is a consistent state");
   122 |         vp.palw_state_v2_store.write().set_tip_for_tests(sink, &planted).expect("the planted tip becomes the tip");
   123 |     }
   124 | 
   125 |     /// **An attempt of `class` by `card` on the CURRENT virtual, built and not inserted** (`t12_real_share::Rig::build_of`).
   126 |     fn build_of(&mut self, class: Hash64, card: usize) -> (MutableBlock, Hash64) {
   127 |         use kaspa_consensus_core::palw_attempt_v2::{
   128 |             PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
   129 |             PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
   130 |             palw_network_domain_v2_for,
   131 |         };
   132 |         self.c.ctx.simulated_time += 1_000;
   133 |         self.nonce += 1;
   134 |         let bond = self.c.bonds[card];
   135 |         let mut t = self
   136 |             .c
   137 |             .ctx
   138 |             .consensus
   139 |             .build_block_template(
   140 |                 MinerData::new(card_payout_spk(card), vec![]),
   141 |                 Box::new(OnetimeTxSelector::new(Vec::new())),
   142 |                 TemplateBuildMode::Standard,
   143 |             )
   144 |             .expect("a template");
   145 |         stamp_harness_time(&self.c.config.params, &mut t.block.header, self.c.ctx.simulated_time);
   146 |         t.block.header.nonce = self.nonce;
   147 |         let facts = self.c.ctx.consensus.palw_producer_facts_v2(class, Some(bond.0)).expect("a V2 network answers");
   148 |         let bond_facts = facts.bond.as_ref().expect("a genesis card is a registered bond").clone();
   149 |         let network_domain =
   150 |             palw_network_domain_v2_for(self.c.config.params.net.to_string().as_bytes(), Some(self.c.config.params.genesis.hash));
   151 |         let header = &t.block.header;
   152 |         let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
   153 |         let mut attempt = PalwAttemptUnsignedV2 {
   154 |             version: PALW_ATTEMPT_V2_VERSION,
   155 |             network_domain,
   156 |             challenge: challenge_v2(network_domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
   157 |             class_id: facts.class_id,
   158 |             executor_bond: bond.0,
   159 |             executor_pubkey: card_pubkey(card),
   160 |             operator_id: bond_facts.operator_id,
   161 |             artifact_root: facts.artifact_root,
   162 |             trace_root: Hash64::default(),
   163 |             output_root: Hash64::from_u64_word(0x0018_0000_0000_0000 | self.nonce),
   164 |             execution_root: Hash64::from_u64_word(0x18EC_0000_0000_0000 | self.nonce),
   165 |             pwu: facts.pwu,
   166 |             trace_manifest_root: Hash64::default(),
   167 |             trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
   168 |             trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
   169 |         };
   170 |         let anchor = execution_anchor_v3(network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
   171 |         let mut won = false;
   172 |         for draw in 0u64..4_000_000 {
   173 |             attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x1800_0000_0000_0000);
   174 |             attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
   175 |             if class_ticket_v3(&attempt, anchor) <= facts.class_target {
   176 |                 won = true;
   177 |                 break;
   178 |             }
   179 |         }
   180 |         assert!(won, "class {class}'s lottery is winnable");
   181 |         let claim_id = attempt_id_v2(&attempt);
   182 |         let signature = libcrux_ml_dsa::ml_dsa_87::sign(
   183 |             &TestConsensus::palw_v2_registry_keypair(card as u64).signing_key,
   184 |             claim_id.as_byte_slice(),
   185 |             PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
   186 |             [0x5Au8; 32],
   187 |         )
   188 |         .expect("ML-DSA-87 sign over a 64-byte attempt id")
   189 |         .as_ref()
   190 |         .to_vec();
   191 |         t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
   192 |         t.block.header.finalize();
   193 |         (t.block, claim_id)
```

### `consensus/src/pipeline/virtual_processor/tests/rfc0018_dag_siblings.rs:216–303`

```text
   216 | /// Three models × three miners from the same parents, inserted in `order`, merged by one heartbeat. Returns the siblings and the merger.
   217 | async fn three_models_from_the_same_parents(rig: &mut Rig) -> (Vec<Sibling>, Block) {
   218 |     let mut built = Vec::new();
   219 |     for (card, class) in [(1usize, rig.classes[0]), (2, rig.classes[1]), (3, rig.classes[2])] {
   220 |         let (block, claim) = rig.build_of(class, card);
   221 |         built.push(Sibling { block: block.to_immutable(), claim, card, class });
   222 |     }
   223 |     let parents: Vec<Vec<BlockHash>> = built.iter().map(|s| s.block.header.direct_parents().to_vec()).collect();
   224 |     assert!(parents.windows(2).all(|w| w[0] == w[1]), "the three siblings share their parents");
   225 |     for s in &built {
   226 |         rig.insert(&s.block).await;
   227 |     }
   228 |     let merger = rig.c.heartbeat(1_000, Vec::new()).await;
   229 |     (built, merger)
   230 | }
   231 | 
   232 | fn assert_retained_and_attributed(tag: &str, rig: &Rig, siblings: &[Sibling], merger: BlockHash) {
   233 |     let gd = rig.c.vp().ghostdag_store.get_data(merger).unwrap();
   234 |     for s in siblings {
   235 |         let hash = s.block.header.hash;
   236 |         let merged = gd.selected_parent == hash || gd.mergeset_blues.contains(&hash) || gd.mergeset_reds.contains(&hash);
   237 |         assert!(merged, "{tag}: sibling {hash} (card {}) is in the merger's past as selected parent, blue or red", s.card);
   238 |     }
   239 |     let (_, state) = rig.c.tip_state();
   240 |     let mut seen_classes = std::collections::BTreeSet::new();
   241 |     for s in siblings {
   242 |         let claim = state.claim(&s.claim).unwrap_or_else(|| panic!("{tag}: card {}'s claim {} is recorded", s.card, s.claim));
   243 |         assert_eq!(claim.bond, rig.c.bonds[s.card], "{tag}: attributed to its own miner's bond");
   244 |         assert_eq!(claim.class_id, s.class, "{tag}: recorded under its own model");
   245 |         assert_eq!(claim.accepted_block, s.block.header.hash, "{tag}: carried by its own block, blue or red");
   246 |         seen_classes.insert(claim.class_id);
   247 |     }
   248 |     assert_eq!(seen_classes.len(), 3, "{tag}: three models, three claims");
   249 | }
   250 | 
   251 | async fn run_all_retained(sibling_fence: bool) {
   252 |     let tag = if sibling_fence { "sibling fence" } else { "launched" };
   253 |     let mut rig = Rig::new(parts(sibling_fence)).await;
   254 |     let (siblings, merger) = three_models_from_the_same_parents(&mut rig).await;
   255 |     assert_retained_and_attributed(tag, &rig, &siblings, merger.header.hash);
   256 | }
   257 | 
   258 | /// The same blocks, delivered to fresh nodes in EVERY permutation of the three siblings' arrival (3! = 6), give the same sink,
   259 | /// colouring, PALW state root (adopted claims, caps consumed, escrows), safe frontier and ADR-0170 seed anchor (RFC-0018 §4.2: under
   260 | /// the fence the anchor is the least-priority admitted attempt of the span, never the last write).
   261 | async fn run_arrival_order(sibling_fence: bool) {
   262 |     let tag = if sibling_fence { "sibling fence" } else { "launched" };
   263 |     let mut a = Rig::new(parts(sibling_fence)).await;
   264 |     let start = a.c.sink();
   265 |     let (siblings, merger) = three_models_from_the_same_parents(&mut a).await;
   266 |     let (ga, sa) = (a.c.vp().ghostdag_store.get_data(merger.header.hash).unwrap(), a.c.tip_state().1);
   267 |     for order in [[0usize, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
   268 |         let mut b = Rig::replica(parts(sibling_fence), &a).await;
   269 |         // The two rigs start byte-identical (same genesis, the same replayed blocks, the same plant).
   270 |         assert_eq!(b.c.sink(), start, "{tag}: identical starts");
   271 |         for i in order {
   272 |             b.insert(&siblings[i].block).await;
   273 |         }
   274 |         b.insert(&merger).await;
   275 |         let tag = format!("{tag}, arrival {order:?}");
   276 |         assert_eq!(a.c.sink(), b.c.sink(), "{tag}: one sink");
   277 |         let gb = b.c.vp().ghostdag_store.get_data(merger.header.hash).unwrap();
   278 |         assert_eq!(ga.selected_parent, gb.selected_parent, "{tag}: one selected parent");
   279 |         assert_eq!(ga.mergeset_blues, gb.mergeset_blues, "{tag}: one colouring (blues)");
   280 |         assert_eq!(ga.mergeset_reds, gb.mergeset_reds, "{tag}: one colouring (reds)");
   281 |         let sb = b.c.tip_state().1;
   282 |         assert_eq!(sa.state_root(), sb.state_root(), "{tag}: the adopted claims, caps and escrows are one");
   283 |         assert_eq!(sa.safe_frontier(), sb.safe_frontier(), "{tag}: one fork-choice standing");
   284 |         assert_eq!(sa.round_seed_anchor(), sb.round_seed_anchor(), "{tag}: one ADR-0170 seed anchor");
   285 |         assert_retained_and_attributed(&tag, &b, &siblings, merger.header.hash);
   286 |     }
   287 | }
   288 | 
   289 | #[tokio::test]
   290 | async fn rfc18_three_models_three_miners_from_the_same_parents_are_all_retained_and_each_recorded() {
   291 |     run_all_retained(false).await;
   292 | }
   293 | 
   294 | #[tokio::test]
   295 | async fn rfc18_arrival_order_does_not_matter_for_dag_claims_or_state() {
   296 |     run_arrival_order(false).await;
   297 | }
   298 | 
   299 | #[tokio::test]
   300 | async fn rfc18_with_the_sibling_fence_retention_and_arrival_order_independence_hold() {
   301 |     run_all_retained(true).await;
   302 |     run_arrival_order(true).await;
   303 | }
```

## D15 — 実プロセスdriverのPanelチェック不足

### `scripts/drills/rfc0018_dag_drill.py:45–82`

```text
    45 | import subprocess
    46 | import tempfile
    47 | import threading
    48 | import time
    49 | 
    50 | from misaka_wrpc_json import WsRpc
    51 | 
    52 | 
    53 | class Blocked(RuntimeError):
    54 |     pass
    55 | 
    56 | 
    57 | def norm(value):
    58 |     return value.removeprefix('0x').lower() if isinstance(value, str) else value
    59 | 
    60 | 
    61 | def field(row, *names, default=None):
    62 |     """The first present field of `row` among `names` (RPC JSON spellings vary by op version)."""
    63 |     for name in names:
    64 |         if isinstance(row, dict) and name in row and row[name] is not None:
    65 |             return row[name]
    66 |     return default
    67 | 
    68 | 
    69 | class DelayedLink:
    70 |     """Opaque loopback P2P link with a fixed per-direction delivery delay; `cut` closes only its own sockets."""
    71 | 
    72 |     def __init__(self, target, delay_ms):
    73 |         self.target, self.delay = target, delay_ms / 1000.0
    74 |         self.enabled, self.closed = threading.Event(), threading.Event()
    75 |         self.lock, self.connections = threading.Lock(), set()
    76 |         self.listener = socket.socket()
    77 |         self.listener.bind(('127.0.0.1', 0))
    78 |         self.listener.listen()
    79 |         self.listener.settimeout(.2)
    80 |         self.address = '127.0.0.1:%d' % self.listener.getsockname()[1]
    81 |         threading.Thread(target=self.accept, daemon=True).start()
    82 | 
```

### `scripts/drills/rfc0018_dag_drill.py:301–371`

```text
   301 |     def scenario_siblings(self):
   302 |         roles = sorted(self.plan['nodes'])
   303 |         for role in roles:
   304 |             self.start(role)
   305 |         for role in roles:
   306 |             self.wait(lambda role=role: self.probe(role), role + ' RPC startup')
   307 |         self.ruleset(roles)
   308 |         self.mesh(roles)
   309 |         self.fixture('bootstrap')
   310 |         base = self.wait(lambda: self.converged(roles), 'a common tip before the siblings')[0]['dag']['sink']
   311 |         miners = self.plan['miners']
   312 |         assert len({m['model'] for m in miners.values()}) >= 3, 'three distinct models'
   313 |         assert len({m['bond'] for m in miners.values()}) >= 3, 'three distinct miners (bonds)'
   314 |         assert len({m['node'] for m in miners.values()}) >= 3, 'three distinct producer nodes'
   315 |         # Isolate the producer nodes so each mines on the SAME base, then run the three miners concurrently.
   316 |         producers = sorted({m['node'] for m in miners.values()})
   317 |         for a in producers:
   318 |             self.cut_between([a], [r for r in roles if r != a])
   319 |         threads, failures = [], []
   320 |         for name, miner in sorted(miners.items()):
   321 |             def go(name=name, miner=miner):
   322 |                 try:
   323 |                     self.command(self.argv(miner['argv']), 'miner-' + name)
   324 |                 except Exception as error:  # noqa: BLE001 — recorded, then raised below
   325 |                     failures.append((name, repr(error)))
   326 |             threads.append(threading.Thread(target=go))
   327 |         for t in threads:
   328 |             t.start()
   329 |         for t in threads:
   330 |             t.join()
   331 |         if failures:
   332 |             raise Blocked('a miner fixture failed: %r' % failures)
   333 |         siblings = {}
   334 |         for name, miner in miners.items():
   335 |             tips = self.call(miner['node'], 'getBlockDagInfo')['tipHashes']
   336 |             mined = [t for t in tips if t != base]
   337 |             assert len(mined) == 1, '%s: exactly one new tip on its own node' % name
   338 |             header = self.block(miner['node'], mined[0])['header']
   339 |             assert base in (field(header, 'parentsByLevel', default=[[]])[0] or []), '%s mined on the common base' % name
   340 |             siblings[name] = mined[0]
   341 |         for key in list(self.links):
   342 |             self.links[key].enabled.set()
   343 |         self.mesh(roles)
   344 |         self.fixture('merge')  # e.g. one heartbeat that merges the tips
   345 |         rows = self.wait(lambda: self.converged(roles), 'convergence after the siblings')
   346 |         sink = rows[0]['dag']['sink']
   347 |         report = {'base': base, 'siblings': siblings, 'colour': {}, 'claims': {}}
   348 |         verbose = field(self.block(roles[0], sink), 'verboseData', default={})
   349 |         chain = [field(verbose, 'selectedParentHash')]
   350 |         blues, reds = field(verbose, 'mergeSetBluesHashes', default=[]) or [], field(verbose, 'mergeSetRedsHashes', default=[]) or []
   351 |         claims = self.claim_rows(rows[0])
   352 |         by_block = {norm(field(r, 'acceptedBlock', 'carryingBlock')): r for r in claims.values()}
   353 |         for name, block in siblings.items():
   354 |             colour = 'SELECTED_PARENT' if block in chain else 'BLUE' if block in blues else 'RED' if block in reds else 'IN_PAST'
   355 |             report['colour'][name] = colour
   356 |             row = by_block.get(norm(block))
   357 |             assert row is not None, '%s: its claim is recorded (blue or red alike)' % name
   358 |             miner = miners[name]
   359 |             assert norm(field(row, 'bond', 'executorBond')) == norm(miner['bond']), '%s: attributed to its own bond' % name
   360 |             assert norm(field(row, 'classId', 'class')) == norm(miner['model']), '%s: recorded under its own model' % name
   361 |             report['claims'][name] = row
   362 |         # Panels: each claim binds its own Panel (seed keyed on the claim), never a sibling's.
   363 |         self.fixture('advance_to_binding')
   364 |         panels = {}
   365 |         for name in siblings:
   366 |             claim_id = norm(field(report['claims'][name], 'claimId', 'id'))
   367 |             panel = self.call(roles[0], 'getPalwPanelAssignments', {'claimId': claim_id})
   368 |             panels[name] = json.dumps(field(panel, 'seats', 'assignments', default=[]), sort_keys=True)
   369 |             assert field(panel, 'claimId', default=claim_id) in (claim_id, '0x' + claim_id), '%s: the panel answers for its own claim' % name
   370 |         report['panelsDistinctByClaim'] = len(set(panels.values())) == len(panels)
   371 |         self.report = report
```

## D16 — 同梱の過去検証記録：今回再実行した結果ではない

### `docs/audit/palw/design-reviews/consolidated-validation-2026-10-15.md:51–68`

```text
    51 | 242 `0x3afba613f2ab70a6`; 170 `0xb68d62123647ff28` and 171 `0x68623b7dc7d30b55` equal the Python port.
    52 | 
    53 | ## Still failing (not fixed here)
    54 | 
    55 | - **Kernel-route beacon cannot lock (≈45 consensus tests, `g14_kernel_route_e2e::{conformance,opv_bootstrap}`, core
    56 |   `rfc0010_production_fold::paid_panel_sources…`).** Since `0a11986ed` no kernel Final exports a beacon event
    57 |   (`palw_kernel_route_v1.rs` ~808: every path `None` "unless an authenticated selected-approval adapter supplies an event").
    58 |   The fixtures need that adapter (RFC07 remaining work); no fixture change can lock the beacon.
    59 | - RFC-0018 `rfc0018_dag_siblings` ×3 (unbuilt lane source: the armed fence is refused by the binary; card 2's claim unrecorded).
    60 | - `rfc9_v4_chain_e2e` offline miner: hash-dependent count of winning quanta (1, test needs 2).
    61 | - `rfc0016_e2e::t12_rfc0016_zero_rights_heartbeat_keeps_the_cadence` (floor-class harness, exempt by design).
    62 | - Pre-existing on base `64699e960`: rfc12 ×4, `t12_a2u_mixed_verdicts` ×2, SDK ×3 (qwen25 pilot bytes, recorded 32-position,
    63 |   corpus preflight), tir-lower ×4 (three_way, corpus_v2, admission, quant_decode_pins), base0 float_free ×2 and prefix cache.
    64 | - kaspad service E2E cluster (16: pruned-import mismatch, view None, honest job flagged, socket/wall deadline; SIGABRTs are a
    65 |   DB-lifetime panic during cleanup after those).
    66 | - Lane-open: base0 `gdn_evidence_v2_hybrid` forged checkpoint (now reaches `PromptIdsAreNotTheJobs`), kernel
    67 |   `native_invocation32_stream` (program violates TIR NF-15/NF-19).
    68 | - Debug-runtime, stopped: SDK `k2s_v5_the_worst_geometry…`, `adv_c4_conformance` byte flips, `gen_sd3_class`, one preflight.
```

## D17 — 一Panel serviceのreplay並列性とmain carrier局所制約

### `kaspad/src/palw_panel.rs:1504–1544`

```text
  1504 |     /// Whether a replay (`heavy`: of a C7 class) may start now. A heavy one: while no other C7 replay
  1505 |     /// is held, running or detached, and a slot is free. A light one: while no light replay runs, or
  1506 |     /// a slot is free — the running and the detached together under twice the slots. A running replay
  1507 |     /// that is overdue ([`Self::mark_overdue`]) holds no slot, but it still counts toward the total
  1508 |     /// held, so a thrashing host cannot pile up more than twice the slots.
  1509 |     pub(crate) fn has_room(&self, heavy: bool) -> bool {
  1510 |         let slots = self.in_flight();
  1511 |         let light_running = self.running.values().filter(|run| !run.heavy && !run.overdue).count();
  1512 |         let slot_holders = self.running.values().filter(|run| !run.overdue).count();
  1513 |         let heavy_held = self.running.values().chain(self.detached.values()).filter(|run| run.heavy).count();
  1514 |         let open = slot_holders < slots && self.running.len() + self.detached.len() < 2 * slots;
  1515 |         if heavy { heavy_held < Self::HEAVY_IN_FLIGHT && open } else { light_running == 0 || open }
  1516 |     }
  1517 | 
  1518 |     /// **Whether a replay of `class` started at `now_daa` returns by `deadline`**, as far as this host
  1519 |     /// has measured: the last replay of the class that returned roots took what it took, and a class
  1520 |     /// not yet timed here is started (its first run is the measurement).
  1521 |     pub(crate) fn fits(&self, class: &Hash64, now_daa: u64, deadline: u64) -> bool {
  1522 |         self.timed.get(class).is_none_or(|took| now_daa.saturating_add(*took) <= deadline)
  1523 |     }
  1524 | 
  1525 |     /// Start `work` on `backend` in a blocking task that holds `reservation` for its life.
  1526 |     #[allow(clippy::too_many_arguments)]
  1527 |     pub(crate) fn start<W>(
  1528 |         &mut self,
  1529 |         key: PalwSeatReplayKeyV1,
  1530 |         class: Hash64,
  1531 |         heavy: bool,
  1532 |         now_daa: u64,
  1533 |         reservation: Option<crate::palw_memory_ledger::PalwMemoryReservationV1>,
  1534 |         backend: Box<dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1>,
  1535 |         work: W,
  1536 |     ) where
  1537 |         W: FnOnce(&dyn kaspa_consensus_core::palw_backend::PalwExecutionBackendV1) -> PalwSeatReplayResultV1 + Send + 'static,
  1538 |     {
  1539 |         let handle = tokio::task::spawn_blocking(move || {
  1540 |             let _held_for_the_replay = reservation;
  1541 |             PalwSeatTaskOutV1::Replay(work(backend.as_ref()))
  1542 |         });
  1543 |         self.running.insert(key, PalwSeatReplayRunV1 { handle, class, started_daa: now_daa, heavy, overdue: false });
  1544 |     }
```

### `kaspad/src/palw_panel.rs:3006–3022`

```text
  3006 | /// Bounding the in-flight depth converts that silent loss into back-pressure. It also decides
  3007 | /// WHICH work gets the scarce slots, because court moves are built and submitted before receipt
  3008 | /// quorums: a rung has a deadline and a quorum does not.
  3009 | ///
  3010 | /// **One, since ADR-0135's devnet drill (2026-09-17).** At eight, a chain of receipts took fifteen to
  3011 | /// twenty-five minutes to clear on an eight-node devnet — a child of an unconfirmed parent is dropped
  3012 | /// in relay, so only the origin's own blocks (one in eight) could mine the tail — and every
  3013 | /// possession proof queued at that tail; six of eight seats never got one out. A carrier that
  3014 | /// spends a confirmed output relays everywhere and is mined by anyone's next block: one at a time
  3015 | /// is the depth relay honours, and a seat's throughput becomes one carrier a block, which testnet-11's
  3016 | /// cadence never approaches.
  3017 | const MAX_INFLIGHT_CARRIERS: usize = 1;
  3018 | 
  3019 | /// **How long a submitted court move is assumed to be in flight.**
  3020 | ///
  3021 | /// Long enough that an accepted carrier normally reaches a block inside it — otherwise the panel
  3022 | /// pays a second fee for a move that was going to land anyway — and short enough that a LOST move
```

### `kaspad/src/palw_panel.rs:3073–3089`

```text
  3073 | pub(crate) fn palw_stuck_carrier_opens_slot_v1(unconfirmed_daa: u64) -> bool {
  3074 |     unconfirmed_daa >= PALW_CARRIER_STUCK_DAA_V1
  3075 | }
  3076 | 
  3077 | /// **V07 (the pre-t12 sweep of 2026-09-25): the readiness lane — possession proofs get a carrier slot
  3078 | /// of their own.** One carrier is in flight per panel on one funding chain (`MAX_INFLIGHT_CARRIERS`),
  3079 | /// and a possession proof had no slot but that one: behind a DA storm it took every other slot at
  3080 | /// best, behind a carrier the stuck opening may not replace (a class registration, a claim's
  3081 | /// commitment) none at all, and the drill measured proofs 365 s from submit to block. So a seat that
  3082 | /// holds a second spendable output under its own script runs a second chain on it, for possession
  3083 | /// proofs alone: a proof the main slot cannot take this tick ([`palw_readiness_lane_takes_v1`]) rides
  3084 | /// it, one in flight, spending a CONFIRMED output — never a child of the main chain's carrier, which
  3085 | /// relay drops (why the main cap is one) — and the main slot is left to every other duty. A seat
  3086 | /// with one output runs as before; the lane scans for a second at most once a DAA.
  3087 | pub(crate) const PALW_READINESS_LANE_INFLIGHT_V1: usize = 1;
  3088 | 
  3089 | /// **Does a due possession proof ride the readiness lane?** (V07) Exactly when the main slot cannot
```

## D18 — 参考：並列とは別のFP receipt / Gate C未完了事項

### `consensus/core/src/palw_fp_admission_v3.rs:171–198`

```text
   171 | /// Items 1-5 of ADR-0044 Decision 6: the claim, the quantum, the beacon, the use window and the lottery. Returns the claim.
   172 | pub(crate) fn receipt_items_1_to_5<'a>(
   173 |     state: &'a PalwChainStateV2,
   174 |     ctx: &PalwBlockContextV2,
   175 |     receipt_maturity_daa: u64,
   176 |     receipt_use_window_daa: u64,
   177 |     beacon: &PalwBeaconFactV3,
   178 |     spend: &ReceiptSpendFacts,
   179 |     pricing: Option<&crate::palw_state_v2::PalwFpPricingV1>,
   180 | ) -> Result<&'a crate::palw_state_v2::PalwClaimStateV2, PalwFpAdmissionV3Error> {
   181 |     // 1. The claim: exists, free-prompt, certified.
   182 |     let claim = state.claim(&spend.claim_id).ok_or(PalwFpAdmissionV3Error::ClaimMissing(spend.claim_id))?;
   183 |     let PalwClaimSourceV2::FreePrompt { quanta, spent } = &claim.source else {
   184 |         return Err(PalwFpAdmissionV3Error::NotFreePrompt(spend.claim_id));
   185 |     };
   186 |     let PalwClaimPhaseV2::Final { final_daa } = claim.phase else {
   187 |         return Err(PalwFpAdmissionV3Error::NotCertified(spend.claim_id));
   188 |     };
   189 | 
   190 |     // 2. The quantum: exists, unspent on this chain.
   191 |     if spend.quantum_index >= *quanta {
   192 |         return Err(PalwFpAdmissionV3Error::QuantumOutOfRange { claim: spend.claim_id, index: spend.quantum_index, quanta: *quanta });
   193 |     }
   194 |     if spent.contains(&spend.quantum_index) {
   195 |         return Err(PalwFpAdmissionV3Error::QuantumAlreadySpent { claim: spend.claim_id, index: spend.quantum_index });
   196 |     }
   197 | 
   198 |     // 3. The beacon is the claim's draw beacon — the fact holds for THIS claim's slot, and the
```

### `docs/rfc/0018-dag-parallel-multi-model-consensus.md:350–360`

```text
   350 | ## 12. ユーザー判断が必要な事項
   351 | 
   352 | * **U-1** 各fenceの有効化と順序(Gate C ≥ RFC-0017 eligibility・kernel route、rule E v2 ≥ rule E、DNS退役 ≥ rule E)。
   353 | * **U-2** 発行由来報酬legの保留を、producerへの1 legで支払うか(本実装)、Panel share・buyback・vestingへ分けるか(GAP-C4)。
   354 | * **U-3** free-promptの認証前spendのcoinbase carve(GAP-C3、本実装はweightだけ止める)。
   355 | * **U-4** DAG幅scheduleの値と、block単位readerを実装するか。
   356 | * **U-5** `m`/`k`と、認証1件あたり`m`回の再実行費用の負担。
   357 | * **U-6** veto案の採否。
   358 | * **U-7** creditなし同士のtieをGHOSTDAG順にする規則(§6.4)の承認。
   359 | * **U-8** 公開check経路に固定するclassと、その計測記録。
   360 | 
```

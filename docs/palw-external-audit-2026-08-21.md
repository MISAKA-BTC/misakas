# External PALW audit — received 2026-08-21

**Provenance.** A static code audit commissioned against a ZIP snapshot of four *unintegrated*
lanes (`palw-base0-depth`, `palw-freeprompt-v3`, `palw-cross-class-v1`, `palw-rc-audit-fixes`). The
snapshot carried PALW files only — the root `Cargo.toml`'s other workspace members and
`consensus/Cargo.toml` were absent — so the auditor could not build or re-run any test. Every
finding below is from reading.

Recorded here verbatim because it is the document the fix work of 2026-08-21 answers, and because
"the audit said X" is a claim somebody should be able to check. **Appendix A** records what was
independently verified, refuted, or fixed against the integration branch afterwards; the audit text
itself is unedited.

---

## 総合判定

**現状は `NO-GO` です。**

許容できるのは、経済的weightを持たない内部開発・shadow実行までです。
**ConsensusV2 testnetをweight-bearingで公開する段階には達していません。**

設計思想自体はかなりよくできています。しかし現在は、優秀な部品がそれぞれ別の宇宙で正しく動いている状態です。コンセンサスは部品の平均点ではなく、接合部の最悪点で壊れるので、そこは容赦がありません。

特に訂正すべき点があります。

> 「35BでもFull Nodeが1 tileだけ検証する仕組みは実装済み」

という以前の評価は、このソース一式を監査した結果、**言い過ぎでした**。正確には、

> **1 step裁定のデータ構造と合成テストは存在する。だが、実workerによるstep trace生成、tile単位のweight opening、実ブロックcarriageまでつながった35B対応E2Eは未実装**

です。

---

# Critical findings

## C-01 実workerがcourt用のstep legを生成していない

最も大きい穴です。`misaka-palw-worker/src/main.rs:1644-1671` に、次の事実がコードコメントとして明記されています。

* workerはstep legを取得していない
* shimが公開するのはtapとlogitsで、kernelごとのtile出力ではない
* attemptの`execution_root`を実行結果から再構成する経路がない
* free-prompt側は`execution_root = Hash64::default()`を返す

さらに `palw_state_v2.rs:3014-3028` は、zero execution rootを正しく拒否します。つまり
「実モデル実行 → 各kernel tileをcapture → `PalwStepBindingV2` → `execution_root`」という本番経路が
ありません。Qwen2.5の「condition 9・10」テストは、実workerの出力ではなく、テストコードが全leafを
合成して `PalwStepLegBuilderV1` に投入しています。

**影響**: 実minerがcourtで検証可能なcommitmentを作れない／合成テストは通るが実ネットワークで同じ
proofを生成できない／不正な `execution_root` を実行から再導出できない／Qwen3.6を載せても
challenge可能な実行証明が存在しない。

**必須修正**: 実際のinteger engineに、全canonical coordinate `(call, node_slot, position,
tile_index)` の出力をcaptureするinstrumentationを入れ、実行と同時に `PalwStepBindingV2` を構築する。

## C-02 artifact operand APIが「要素数」と「バイト数」を混同している

`palw_step_refute.rs:613-619` では、oracleの `elements` は「tensor dtype上の要素数」と定義されて
います。しかしproduction oracleである `PalwProvenOperandsV1` は `palw_artifact.rs:181-188` で
`elements` **バイト**だけを返しています。

| op | courtの要求 | 実際に返る量 | courtが期待する量 |
|---|---:|---:|---:|
| Rescale | 1 element | 1 byte | 5 bytes |
| Requantize | N elements | N bytes | 9N bytes |
| RoPE | N elements | N bytes | 約4N bytes |

`Rescale` は `palw_step_refute.rs:418-432`、Requantize/RoPEは同 `:501-544`。テスト用 `FixedRow` は
`:1222-1228` で要求サイズを無視して全bytesを返すため、この欠陥を隠しています。

**影響**: 実際のMerkle openingを使用すると Rescale・Requantize・RoPE は
`InputSetNotCanonical → DoesNotAdjudicate` になる。catalog coverageが100%と表示されても実裁判は
閉じません。

**必須修正**: oracleを `byte_offset + byte_len` か `dtype + element_count + canonical encoded size`
に変更し、全10 opを `PalwProvenOperandsV1` 経由で検証するE2Eテストを置く。`FixedRow` だけでは駄目。

## C-03 「1 tile裁定」が実際には全weight matrixの開示と再計算になっている

`palw_step_refute.rs:440-499` のMatMul裁定は `wanted = out_dim * input_len` を計算し、weight matrix
全体を要求します。その後 `:874-924` で全出力を再計算してから、最後にchallenge対象tileだけ切り出し
ています。

| matrix | 必要なweight opening |
|---|---:|
| BASE-0 output 4096×256 | 1 MiB |
| Qwen2.5 unembed 151,936×1,536 | 約222.6 MiB |

`palw_qwen25_profile.rs:418-437` も、courtテストでは実1.5B geometryではなく1層・hidden 32・vocab 48
の小型probeを使っていると明記しています。一方 ADR-0046 はcourt closeを「152KBより十分小さい」と
想定しています。現実のコードと二桁から三桁違います。人類の「one tile」という言葉が、またしても
全体を意味していました。

**影響**: Qwen規模の有効なcourt proofをtransactionに載せられない／proof deserialization DoS／
terminal adjudicationがモデルサイズ非依存ではない／35B Full Node軽量裁定という中心的主張が
成立しない。

**必須修正**: challengeされた出力範囲 `tile_start..tile_end` に対応するweight rowだけをopenし、その
出力laneだけを再計算する。さらにclass admission時に最大opening bytes／最大terminal MAC／最大
operand数／最大Merkle path数を実geometryから導出し、ruleset上限以下であることを検証する。

## C-04 decode時のEmbeddingが明示的に `Unadjudicable`

`palw_step_refute.rs:387-410` は `if coord.call_index != 0 { return Err(Unadjudicable); }` で、
Embedding gatherはprefillでしか裁定できません。しかしBASE-0のcanonical jobは
`palw_base0_profile.rs:442-444` で **prefill 8 / decode 4** です。

**影響**: 生成tokenごとの最初のEmbedding stepを裁定できない。Qwenだけでなく現在のBASE-0 floor自身に
当たるので、BASE-0も「全reachable step adjudicable」ではありません。

**必須修正**: decode tokenを前回logitsから決定的に導出する — argmax／最小token IDによるtie-break／
前回logits commitment／選択tokenのproof／そのtokenに対するembedding opening を、一つの裁定可能な
連鎖にする。

## C-05 engine・profile・courtが同じ計算を表していない

* **RMSNorm** — engine `engine.rs:288-292` は `RMSNorm → norm_requant → int8`、profile
  `palw_base0_profile.rs:165-174` は RMSNormのみ、court `palw_step_refute.rs:353-360` は
  RMSNormのi32出力のみ。
* **Q/K/V projection** — engine `engine.rs:198-211` は `MatMul → per-channel/tensor requant`、
  profileはMatMul 1ノードとしてしか表していない。
* **RoPE** — engine `engine.rs:213-229` は `RoPE → CODE_CLAMPでrequant`、profile/courtはRoPEのみ。
* **residual** — engine `engine.rs:266-280` は `AddElem → residual_requant`、profile
  `palw_base0_profile.rs:246,287` は AddElemのみ。
* **norm gain** — profile inventory `palw_base0_profile.rs:84-99` に `attn_norm.weight` /
  `ffn_norm.weight` / `output_norm.weight` があるが、engineもinteger courtも使用しない。

**影響**: workerが実engineの値をstep legへcommitできるようになった瞬間、courtが別の値を再計算する
可能性がある — honest producerの誤slash／不正producerの誤無罪／artifact rootに実行されないdead data／
同じprofileが異なる実行意味論を持つ。コンセンサス破壊級です。

**必須修正**: engine・profile・court・artifact inventoryを、**同一のcanonical execution IRから生成**
する。手書きの4実装を「たぶん同じ」と扱う段階は終わりです。

## C-06 実artifactに対するopenable `artifact_root` 生成が未完成

`palw_artifact.rs:21-27` 自身が、canonical inventoryは未実装と書いています。存在するのはMerkle leaf
形式／Merkle root計算／opening検証だけ。「checkpoint → deterministic PTQ → canonical operand
inventory → `artifact_root`」とするproduction builderは見当たりません。`qwen25-convert.rs:69-96` が
出力しているのは全artifactをflat hashした `execution_class_id()` であり、courtが部分openingできる
Merkle `artifact_root` ではありません。

**必須修正**: canonical inventory manifestを実装する。各entryに tensor name／layer／dtype／shape／
row_start／byte_len／quantization record／canonical order を含め、重複・欠落・overlap・余分なbytesを
拒否する。

## C-07 ConsensusV2が実ネットワークに配線されていない

コード自身が明記しています — `palw_rc_identity_v2.rs:45-50`（RC identityはまだshipped presetでない）、
`palw_mode_v2.rs:853-875`（shipped networkはDisabledまたはLegacy、ConsensusV2はfinalizer不在でboot拒否）、
`consensus/src/processes/palw_state_v2_sync.rs:8-29`（state syncはdormant、callerはPR-10）、
`palw_chain_weight.rs:43-44`（chain weightはconsensus-inert）、
`palw_fork_authority_v2.rs:11-15`（tip/IBD/pruning/finalityへの配線は将来）、
ADR-0043 `:8-10`（headerの `palw_state_root` fieldはPR-10）。

**影響**: PALW stateがheaderにcommitされない／tip selectionがPALW weightを使わない／IBD・pruning・
deep reorgがPALW authorityを使わない／restart・reorg時にV2 stateを追従させるlive callerがない／
ConsensusV2 testnet presetがない。

## C-08 V2 bondの経済的ロックが未実証

`palw_carriage_v2.rs:334-345` ではoutput 0を担保として登録します。しかしADR-0046 `:100-106` が要求する
「retirementまでoutpointをspend不能にする／slash分をcanonical burn scriptへ送る／slash分をfeeとして
minerへ戻さない」というV2 spend gateは、提供されたsnapshotでは確認できません。ADR-0046 `:171-174` も、
このwiringは後でlandすると明記しています。

**影響**: state上ではbondが存在していても、UTXOを先に使えてしまうならslashは帳簿上の数字になります。
weight-bearingネットワークではactivation blockerです。

---

# High findings

| ID | 問題 | 影響 |
|---|---|---|
| H-01 | post-genesis class登録方針が矛盾 | `palw_class_admission_v2` は最小1‰、`palw_lifecycle_objects_v2` はgenesis限定、state machineはweightless clock activationを実装 |
| H-02 | 強いprofile coverage gateがadmissionから呼ばれていない | `verify_profile_coverage_v1` は存在するが、`verify_class_admission_v2` はkernel ID集合だけを検査 |
| H-03 | CourtClose payloadにPALW固有のサイズ上限がない | `palw_carriage_v2.rs:291-294` はversionしか検証しない |
| H-04 | Qwen2.5の公開geometryがadmissibleでない | `tile_len=128, n_ctx=4096` は 132,354,910 leaves で上限 4,194,304 を超える。テストだけ16,384へ差し替えている |
| H-05 | Qwen2.5の量子化品質がactivation条件未達 | artifactコメント自身がconstant-token degenerationとresidual range collapseを記録 |
| H-06 | Qwen3.6 deterministic classは未実装 | 現在あるGDN裁定はNEON/AVX2・glibc・llama commitを焼いたfloat方式。integer MoE/GDN/SSM profile/converter/full-forwardはない |
| H-07 | artifact canonical validationが弱い | public field、per-layer fallback、extra entryがidentityを変えても実行されない可能性 |
| H-08 | class identityの意味が二重化 | chain側は `shape_profile_id` をclass IDとし、engine/converterはartifact全体digestを `execution_class_id` と呼ぶ |

特にH-01は、後からQwen3.6を登録する計画に直結します。現在のコードには「genesis-only」「1‰登録」
「weightless登録」という3つの物語が同居しています。コンセンサスに多様性は要りません。答えは1つで
十分です。

---

# Medium findings

### M-01 不正署名がmalformationではなくstateful skip

`palw_carriage_v2.rs:351-370` では operator / retire / challenger の signature invalid がいずれも
skip扱いです。race由来のstale transactionはskipでよいですが、不正署名は送信者が作ったmalformation
です。現在の分類は無効署名spamを安価にします。

### M-02 state-root ADRと実コードがずれている

ADR-0043のroot preimageには class shares／epoch budgets／receipt targets／pending payouts などが
ありませんが、実コード `palw_state_v2.rs:1345-1389` には含まれています。pre-liveなので直せますが、
ruleset fingerprintを固定する前にADRとgolden vectorを一致させる必要があります。

### M-03 Qwenのcourtテストが実geometryではない

`palw_qwen25_profile.rs:418-437` 自身が、小型probe geometryであると認めています。構造テストとしては
有用ですが、opening size／transaction mass／terminal latency／full matrix問題／actual artifact
inventory を証明するテストではありません。

---

# 良かった点

悪い箇所だけ並べると文明が終わるので、良い設計も明記します。

* Merkle treeのodd leafをduplicateせずpromoteしており、`[a,b,c]` と `[a,b,c,c]` のroot衝突を避けている
* courtがclaimのtrace rootとfull execution rootの両方を照合している
* `Unadjudicable` を有罪として扱わず、誤slashを避けている
* no-show slashとhonest non-convictionの鏡像テストを入れている
* negative integer laneをfloat NaN扱いしていた欠陥を明示laneで閉じている
* candidate-chain scoped state、derive-never-declare、ruleset identityへの意識は強い
* BASE-0整数primitiveの独立順序differentialという方向性は正しい

したがって、作り直しではありません。**接続と意味論の一本化が必要**です。

---

# 修正の優先順位

**P0: courtと実行を成立させる** — ①4レーンを1本のauthoritative integration branchへ統合
②ADR番号衝突を解消 ③完全repo上でworkspace build/testを再現 ④engine/profile/court/inventoryの単一
execution IRを作る ⑤実workerでstep legをcapture ⑥operand APIのbyte/element不整合を修正
⑦MatMulをtile-local openingへ変更 ⑧decode token導出を裁定可能にする ⑨production artifact
inventory/root builderを実装 ⑩CourtClose proofのbytes/MAC/opening数を制限

**P1: ConsensusV2を本当にチェーンへ接続** — ⑪V2 bond spend gateをcandidate-chain stateに配線
⑫ClassRegistered方針を1つに固定し実carrierへ接続 ⑬header `palw_state_root` を実装 ⑭block validation
前のstateful admission gateを実装 ⑮state sync・reorg・restart・IBD・pruningを接続 ⑯tip/finality/
deep-reorgを単一PALW fork authorityへ接続 ⑰実ConsensusV2 presetとgenesis artifactを作成

**P2: practical classes** — ⑱BASE-0 artifactをcanonical root付きでfreeze ⑲Qwen2.5のadmissible
geometryと品質基準を固定 ⑳Qwen2.5を最小shareでmulti-node soak ㉑その後にQwen3.6のinteger
MoE/GDN/SSM primitiveを実装 ㉒Qwen3.6実checkpointでfull forward・state bound・court opening・
性能を測定

---

# クラス別の最終評価

| 対象 | 評価 |
|---|---|
| BASE-0 4-layer | integer engineはあるが、実trace/court semanticsが一致せず**weight-bearing不可** |
| Qwen2.5-1.5B | converterと実forward実験は有望。ただしartifact root、品質、geometry、実worker接続が未完 |
| Qwen3.6-35B-A3B | 現状はllama.cpp float runtime。目標とするdeterministic integer practical classは**未実装** |
| 1 tileだけのFull Node裁定 | 合成toy testの骨格はあるが、実際は全matrix openingで**モデルサイズ非依存になっていない** |
| ConsensusV2 testnet | preset、finalizer、state root、sync、fork authorityが未接続で**起動不可** |
| 経済的slash | state処理はあるがbond spend gateが未実証で**担保性未成立** |

## 最終判断

現段階で公開してよい表現は

> PALW ConsensusV2の状態機械、整数primitive、step dispute、class economyの主要部を実装中。内部shadow試験段階。

まだ使ってはいけない表現は

> Qwen35Bを全ノードが再実行せず1 tileだけで安全に検証できるweight-bearing testnetが完成した。

このsnapshotからの最短経路は、Qwen3.6実装を先に膨らませることではありません。まず**BASE-0で、実worker
実行から実ブロック、実challenge、実artifact opening、実court closeまでを一本で通す**ことです。そこが
閉じれば、35Bはモデル規模の問題になります。閉じる前は、35Bを足しても未接続の部品が35倍立派になるだけ
です。

---

# Appendix A — verification against the integration branch

The audit read four unintegrated lanes. `palw-mainnet-rc-integration` is further along, so each
finding was re-checked against it before any fix was written. Status as of 2026-08-21.

| # | verified on the integration branch? | state |
|---|---|---|
| C-01 | **reproduces** | partially fixed — `ForwardProbe::steps` + `legs::base0_step_tiles_v1` (`e5d7a69c`) give rows → canonical tiles → leg root. Per-head steps and the full refutation round trip remain |
| C-02 | **reproduces**, and sharper than reported: the `Rescale` arm asks the oracle for ONE element and requires FIVE bytes, so op 9 had never adjudicated through a production oracle | **fixed** — `operand_bytes(tensor, layer, byte_offset, byte_len)` (`dc9934cf`), mutation-checked, with `op_nine_adjudicates_through_a_real_artifact_opening` |
| C-03 | **reproduces** | **fixed** — tile-local opening (`317e397a`), mutation-checked. Five court E2E tests had to open only the tile; one of them was named `..._from_one_tile_and_no_model` while opening the whole block |
| C-04 | **reproduces** | **fixed** — the decode token is pinned by the claim's own `full_logits_trace_root`, which already bound `output_token_ids_hash_v2` (`fbe22c64`). No new commitment |
| C-05 | **reproduces, and understates it**: the engine performs 36 steps a layer and the profile declared 18. K was never rotated; the cache roles sat on the raw projections; a leftover slot-indexed patch silently rewrote two unrelated nodes | **fixed (2026-08-26)** — the profile was generated from `BASE0_LAYER_IR` first (`aace6691`), and the engine's op sequence is now compiled from it too (`09bd647f`/`df63a916`), with one name-to-bytes binding shared with the inventory. All three tables project in both classes; `engine.rs` calls no kernel |
| C-06 | **reproduces** | partially fixed — `PalwArtifactInventoryV1` with its rejection rules (`f3f4ca4b`). The production builder from a real checkpoint remains |
| C-07 | **STALE on this branch.** `header.palw_state_root` is a real field (`header.rs:234`), the pipeline disqualifies on `header.palw_state_root != parent_root` (`processor.rs:1161`), and IBD consults `decide_ibd_commit_v2` (`protocol/flows/src/ibd/flow.rs:1809`) | three real blockers remain instead of six: `current_attempt` hardwired to `None` (`processor.rs:1211`), subnetworks `0x4a`/`0x4b` unrouted at tx validation, and no `NetworkId` mapping to a ConsensusV2 network |
| C-08 | **reproduces** — `slash_bond` (`palw_state_v2.rs:1908`) decrements a `u64` and touches no output | not fixed |
| H-01 | reproduces | decided by ADR-0049 H; code not unified |
| H-02 | reproduces | coverage now sweeps coordinates (`0bf60aa1`) but is still not called from `verify_class_admission_v2` |
| H-03 | points at `palw_carriage_v2.rs`, **absent from this branch** | admission-side ceilings landed (`0a7eb1e4`); the carriage-side limit is a different lane |
| H-04 | reproduces | measured and pinned by a tripwire; the shipped constants are unchanged |
| H-05 – H-08 | reproduce | not fixed. H-08's naming settlement is decided in ADR-0049 G and `execution_class_id` is still a flat digest |
| M-01 | points at `palw_carriage_v2.rs`, **absent from this branch** (it lives on `palw-rc-audit-fixes`, `c1425406`). The equivalent here is `palw_lifecycle_objects_v2`, which carries no signature check | not applicable as written; unexamined here |
| M-02 | **reproduces** — `state_root()` covers `class_shares` (`palw_state_v2.rs:1450`) and `receipt_targets` (`:1460`); ADR-0043 mentions neither. A code comment (`:101`) attributes the addition to ADR-0045, so it is ADR-0043 that has not followed | not fixed |
| M-03 | **reproduces** — the probe geometry is `layer_count: 1, hidden_dim: 32, vocab_size: 48` (`palw_qwen25_profile.rs:487`) | not fixed |

**Two findings the audit could not have made from the snapshot**, both raised by the same
adversarial sweep and both verified here:

* **the attempt envelope's ML-DSA-87 signature was verified on no consensus path.** The pipeline
  called `check_palw_attempt_admission_v2`, which takes no verifier; admission item 2 compares two
  public values. Anyone could mine under anyone's bond, and one solved PoW minted unlimited distinct
  valid blocks. Fixed in `9e047dc4`, mutation-checked — reverting it makes a forged block become the
  sink. ADR-0042 Decision 3c's deferral rested on a premise this removed, and needs re-deciding.
* **`MAX_DOT_LEN` was derived from `127²` while `requantize` emits `-128`**, so ADR-0040 Decision E's
  free-reduction-order premise held over a subset of the operand type. Fixed, with a `const`
  assertion so the bound cannot be raised past what an `i32` accumulator survives.

**The audit's verdict stands.** What the fix work changed is the composition of the evidence, not
the conclusion: the wiring turned out to be further along than reported (46 of 55 modules have a
real caller, the pipeline seam is complete end to end) and the **seam semantics** turned out to be
worse — a signature nobody checked, a unit mismatch invisible to an all-`int8` class, a graph
declaring half its own computation.

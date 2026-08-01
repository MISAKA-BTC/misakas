# PALW PCPB LeafV2 wiring — design memo (ADR-0045 D3-b)

- **Status:** Design memo + 実装記録 — D3-b「部分ゲート禁止(全 clause 同時有効)」の実装可能形の
  確定と、その consensus 側の着地。ADR-0040 §5.14.7(2026-07-23 確定)を **2026-08-01 時点の
  実コード**と突合し、field 一式と clause 一覧を固定した上で §11 まで実装した。
  **DoD 達成(producer・bridge 配線・§8・G7・testnet-21 再 genesis 込み)** — 意図的な赤 1 件
  (ADR-0044 harness timeline = D3-c と同時)のみ残る。§11 の DoD を参照。
- **Date:** 2026-08-01
- **Inputs:** ADR-0045 D3; ADR-0040 §5.14(.1–.7); `docs/palw-beacon-seed-history-design.md`(D3-a);
  監査 2026-07-19 H-10 / P1-10; 実測(下 §0)

## 0. §5.14.7 執筆後に動いた前提(実測 2026-08-01)

§5.14.7 は 2026-07-23 の実測に立つ。以後に動いたものを先に固定する — **本メモの差分は全て
ここから導かれる**。

| §5.14.7 の前提 | 現状(実測) |
|---|---|
| DB version 9(§5.14.7.8 は 8→9 と記す) | **15**(`factory.rs:124`)。daemon arm は `<= 14`。本スライスは **15→16** |
| leaf に challenge 系 field 無し | **Receipt-v3 slice が部分的に先行**: `receipt_v3_job_challenge` / `receipt_v3_issued_epoch` / `receipt_v3_expires_epoch` / `receipt_v3_compute_set_id` が `PalwPublicLeafV1` に存在(LEAF_LEN 964)。job 単位 challenge の 3 欠落要素(`scheduler_job_id` / `requester_credential` / `request_commitment`)は Seam 1 の `derive_job_challenge`(`mil/bridge/src/challenge.rs`)が**導出に含めて実装済み** |
| clause 6/7/9 は非強制(C5 未 flip) | **C5 は safe subset で flip 済み**: body 検証が clause 1–5 + 6(buried-anchor chain_commit)+ 9(eligibility draw)+ 10(halt)を強制、clause 7(lane bits)/8(compute cap)は header/GHOSTDAG 段。**ADR-0039 番号 1–10 は全て生きている** |
| provider registry 在庫ゼロ(H-03) | **provider bond は実在**: `PalwProviderBondRecord`(value-lock + spend gate + SEL-01 floor + unbond/slash 座標)、reward 座標は `active_provider_bond_at(&leaf.provider_{a,b}_bond)` を既に強制 |
| epoch 別 seed が引けない | **D3-a 着地**(working tree): `DbPalwBeaconStore::beacon_seed_at(epoch)` fail-closed、境界 writer、sweep、snapshot carry、窓 = `palw_beacon_seed_history_window_epochs` |
| PCPB primitives | §5.14.7.9 のとおり INERT 着地済み: `PalwDispatchEvidence` / `palw_dispatch_evidence_valid` / snapshot builder(`palw_build_snapshot_witnesses` → `PalwSnapshotCommitment {snapshot_root, assignment_root, total_bond, provider_count}`)/ `palw_epoch_seed_at` / `PalwParams` の `w/k/Δ`(inert placeholder w=6, k=Δ=2) |

**帰結 1(番号の訂正)。** §5.14.7.6 の「clause 6=freshness / 7=dispatch / 9=DA presence」は
C5 flip **前**の空き番号を仮に使った表記であり、現コードでは 6/7/9 は別の規則が占有している。
PCPB 規則は **clause 11 / 12 / 13** として ADR-0039 系列の続番に確定する(§4)。部分ゲート禁止の
実体(同一コミット・全 clause 同時)は不変。

**帰結 2(field の再利用)。** §5.14.7.2 の `job_challenge_commitment` / `challenge_epoch` は
**新設しない** — `receipt_v3_job_challenge` / `receipt_v3_issued_epoch` がその実体である
(§5.14.3 が要求した job 単位性は Seam 1 の導出が既に満たす)。leaf が今日 sentinel 0 を許すのは
Object-V1 互換のためであり、PCPB clause 群は sentinel を**構造的に**拒否する(§4 clause 11 —
zero challenge は再導出不一致、zero epoch は窓外)。

## 1. Leaf v2 — field 一式(確定)

`PalwPublicLeafV1` に追加する field は **5 本**。全て `leaf_hash → leaf_root → content_id() ==
batch_id` に入り、M2/item-7 の封印(acceptance 座標の epoch pin)に同乗する。

```rust
// 追加(borsh 末尾、この順)
pub a_commit: Hash64,               // self 枝: A の receipt commitment(external 枝: 0 sentinel)
pub a_commit_epoch: u64,            // self 枝: A-commit の on-chain 登録 epoch(external: 0)
pub provider_snapshot_root: Hash64, // anchor−k epoch の bond 加重 snapshot 根
pub assignment_proof_root: Hash64,  // snapshot から決定的導出される assignment tree 根
pub dispatch_kind: u8,              // 0 = BeaconAssigned(external) / 1 = SelfSerial(self)
```

再利用(改名しない): `receipt_v3_job_challenge`(= §5.14.7.2 の challenge commitment)、
`receipt_v3_issued_epoch`(= challenge epoch)、`registered_epoch`(item 7 が acceptance epoch に
pin 済み — freshness が grind 不能である根拠そのもの)。

**anchor epoch の定義(branch 別)** — snapshot と post-commit beacon の座標:

- `dispatch_kind = 0`(external): `anchor = receipt_v3_issued_epoch`。scheduler は challenge 発行の
  Δ epoch 後に R_{anchor+Δ} で 2 slot を抽選し、**その後**計算が走る。抽選 seed が job を含まないのは
  §5.14.7.1 の設計どおり(per-epoch pair)— job を seed に入れると「無料で challenge を乱発し
  R_{E+Δ} を見てから sybil pair の job だけ提出する」grind が開くため、**入れないことが正しい**。
  残る grind は「どの epoch の pair に条件付けるか」の w 通り選択のみ(freshness 窓が上界)。
- `dispatch_kind = 1`(self): `anchor = a_commit_epoch`。計算完了後に A_commit を登録し、
  R_{anchor+Δ} が B を抽選する(計算時間は Δ に縛られない — §4.1 の「A_commit 以前に確定した
  E−k」の E が A_commit epoch である理由)。

**§5.14.7.2 との差分は 2 点、いずれも根拠つき:**

1. `a_commit_epoch` は §5.14.7.2 に無い。必要な理由: self 枝の R_{E+Δ} が grind 不能であるには
   「a_commit が R_{E+Δ} 判明**前**に on-chain 確定した」ことを consensus が検査できなければ
   ならない(§2 の A-commit registry と clause 12 の等値検査)。leaf 側に epoch を持たせるのは
   M2 の原則(検証器が使う値は leaf content に封じる)による — registry 行だけを真実にすると
   fork 間で leaf の意味が浮動する。
2. `dispatch_kind` は §5.14.7.2 どおり維持。chunk 側 evidence の enum tag と二重だが、leaf 側を
   commitment として固定し、不一致は acceptance で拒否する(evidence を後から別枝に差し替える
   自由度を消す)。

**載せないもの(prior 議論の「reveal / escrow / reroll round」の帰結):**

- **reveal(r_blind)** — leaf には載らない。a_commit の開示は audit/arbitration 帯(DA object)の
  仕事で、consensus の mint 検証は commitment 等値と B の署名 receipt(a_commit 埋め込みの実検査)
  までを見る。security model(qwen-8.0)の「opening 配送は out-of-crate」を維持。
- **escrow(価値)** — v1 の A-commit 登録は **commitment-only**(§2)。価値 escrow は ADR-0045 D1/D4 が
  fence した経済帯(settlement/premium)に属し、D3-b の consensus 健全性には不要。ordering anchor
  だけが必要で、それは登録 epoch が担う。
- **reroll round** — v1 では**載せない**。reroll を wire に入れると「B no-show」を consensus が
  検証できないまま A に再抽選の自由度を与える(r の範囲だけ grind が開く)。v1 は no-show 時に
  **challenge から再発注**(新 a_commit)で liveness を回復する。no-show penalty 機構(ADR-0040 §938)
  が別スライスで入った時に初めて re-検討する。

サイズ: LEAF_LEN 964 → **1189**(実測。borsh の可変長 field を含むため単純な field 幅の和ではない)。
`LEAF_FNV` も再導出済み。leaf は bincode 永続化されるため **DB cutover 15→16 が必須**(§6)。

## 2. 新規 on-chain 面 — A-commit registry と per-epoch snapshot

### 2.1 `PalwACommitV1` payload(新 overlay kind)

```rust
pub struct PalwACommitV1 { pub version: u16, pub a_commit: Hash64 }
```

- 無署名・content-keyed。**登録者の身元は意味を持たない** — 検査対象は「いつ chain に載ったか」
  だけであり、front-run は M2 と同じ論法で無害(同一内容の再送は冪等、他人の a_commit を先に
  登録しても被害者の leaf はそのまま検証を通る)。spam は tx 手数料と窓 sweep が抑える。
- acceptance arm は `PalwACommitRegistry`(store)へ `a_commit → accept_epoch` を書く。
  **first-accept-wins**: 既存行があれば no-op(冪等)。epoch は accepted block の
  consensus-derived epoch(自己申告ではない)。
- **sweep**: `accept_epoch < current − (window)` の行は pruning pass で削除
  (窓は §3 の snapshot 窓と同じ)。

### 2.2 `PalwProviderSnapshotHistory`(新 store、D3-a と同型)

`epoch → PalwSnapshotCommitment`(既存 struct をそのまま永続化: snapshot_root / assignment_root /
total_bond / provider_count)。

- **writer 座標:** epoch 境界を跨ぐ最初の chain block の virtual commit — **D3-a の seed writer と
  同一座標・同一 WriteBatch**。入力は当該時点の provider-bond view の active 集合
  (`effective_provider_bond_status == Active`、SEL-01 floor 適用後)を canonical 化した
  `palw_build_snapshot_witnesses(entries).commitment`。
  - `provider_id := blake2b_512_keyed(PALW_PROVIDER_ID_DOMAIN, bond_outpoint)`(新 domain 1 本)。
    bond 単位の interval は線形なので同一 owner の複数 bond は 1 本の大 bond と等価、分割耐性は
    既存 floor が担う。
  - `ml_dsa_pk_hash := record.owner_pubkey_hash`、`bond_sompi := record.amount_sompi`、
    `reward_script_commitment := record.reward_key_root`。
- **読み出し:** `snapshot_at(epoch) -> Option<PalwSnapshotCommitment>` — fail-closed。
- **保持窓:** `palw_beacon_seed_history_window_epochs + w + k`(§3)。sweep / pruning-snapshot carry /
  coherence 検査(窓内・単調・重複なし)は D3-a と同じ規律。carry は
  `PalwPruningPointSnapshotPayloadV1` に `provider_snapshot_history: Vec<(u64, PalwSnapshotCommitment)>`
  を追加する(**再 genesis と同時なので version 分岐は作らない** — D3-a と同じ理由で `new()` 経由の
  canonical 行のみ、`Default` 禁止)。

entries 本体は保持しない(検証は membership witness が chunk に同乗するため根と total_bond で足りる)。

## 3. 窓の算術(D3-a への追補 1 件を含む)

mint 検証は leaf の active window 全域で走る。最古の読みは:

- beacon seed: `R_{issued_epoch}`(clause 11 の challenge 再導出)と `R_{anchor+Δ}`(clause 12)。
  最古 = `registered − w` 起点 → **D3-a の窓 `palw_beacon_seed_history_window_epochs` は
  `+ w` 拡張が必要**(現行値は lifecycle + inclusion + 2 で、監査 lookback しか覆っていない)。
  これは D3-a 実装への追補であり、`beacon_seed_history_window_covers_the_verification_lookback`
  テストの拡張として固定する。
- snapshot: `anchor − k` → 窓 = beacon 窓 + k。

`PalwParams::is_structurally_valid` の既存 invariant(`k≥1, Δ≥1, w≥Δ, k+Δ ≤ evidence_window_epochs`)
は維持。freshness の下限(§4 clause 11)が空にならないことは `w ≥ Δ` が保証する。

## 4. Clause 一覧(確定)— ADR-0039 系列の続番 11/12/13

現行の生存 clause: 1–5(store facts、body)/ 6(chain_commit、body)/ 7(lane bits、header)/
8(compute cap、header/GHOSTDAG)/ 9(eligibility draw、body)/ 10(halt、body)。
PCPB は以下 3 本を**同一コミットで**追加する。検査座標は 2 つ — **acceptance**(leaf-chunk arm、
`insert_leaf` の前、M2 membership gate の直後)と **mint**(`check_palw_ticket`)。

### Clause 11 — challenge 束縛 + freshness

- **acceptance(重い半分):** chunk が leaf ごとに運ぶ challenge preimage
  `{scheduler_job_id, requester_credential, request_commitment}` から
  `derive_job_challenge(network_id, issued_epoch, beacon_seed_at(issued_epoch)?, …, shape_id)
  == leaf.receipt_v3_job_challenge` を再導出検査(seed 窓外 = fail-closed 拒否)。
  これが無いと issued_epoch は自由申告になり freshness が空洞化する(cached-activation replay、
  H-10 の本体)。
- **acceptance + mint(pure な半分):** `palw_challenge_fresh_v2(issued, anchor, registered, w, Δ)` =
  `issued ≤ anchor ∧ anchor + Δ ≤ registered ∧ registered − issued ≤ w`
  (external は anchor = issued として同式)。既存 helper `palw_challenge_fresh` は Δ 下限を持たない
  ため **v2 形へ仕様変更**(弱体化ではない — 検査は増える。既存 unit test は書き換え)。
  mint 側は binding(leaf field のみ)で再評価できる pure 検査として残す — content_id に封じられて
  いるので acceptance 通過後は不変式だが、fail-closed の形を mint にも置く(clause 13 と同じ規律)。

### Clause 12 — dispatch evidence(acceptance)

`palw_dispatch_evidence_valid` を本線化する。呼び出しは:

- `resolved_* := snapshot_at(anchor − k)?`(fail-closed)、
  `post_commit_beacon := beacon_seed_at(anchor + Δ)?`(fail-closed)、
  `leaf_*_root / leaf_a_commit := leaf の committed 値`(clause 0 が等値を検査)。
- **branch ↔ `dispatch_kind` の一致**を要求(不一致 = 拒否)。
- **provider 束縛(本メモで確定する追加検査、§5.14.7.1 への追補):** 抽選が当てた provider と
  leaf が申告する provider の等値 —
  `slot_a.entry.provider_id == provider_id(leaf.provider_a_bond)`、slot_b/B も同様。
  reward script は `entry.reward_script_commitment == bond.reward_key_root` 経由で bond に固定され、
  leaf.script ↔ bond の突合は既存 reward 座標(`active_provider_bond_at`)の検査系に属する
  (実装時に script↔reward_key_root の強制有無を確認し、無ければ**本スライスで acceptance に足す**)。
- **self 枝の追加 2 検査:**
  1. `PalwACommitRegistry[a_commit] == Some(a_commit_epoch)`(**等値**。より古い登録に後から
     epoch を当てる／後から登録して過去を名乗る、の両方向を潰す)。
  2. A 自身の snapshot membership(`SelfSerialProof` に `a_entry + a_snapshot_membership` を追加する
     仕様変更)— unbonded な A の自己発注を消す。B と同一 provider の自己ペアは
     `a_entry.provider_id != b_entry.provider_id` で拒否。
- 評価コスト(ML-DSA-87 verify ×1 + merkle ~20 hash)は chunk payload の byte 質量に既に比例課金
  される(§5)。

### Clause 13 — mint 時の文脈 presence + 等値(fail-closed)

`check_palw_ticket` に追加する軽量検査:

```text
snapshot_at(anchor − k) == Some(c) ∧ c.snapshot_root == leaf.provider_snapshot_root
                        ∧ c.assignment_root == leaf.assignment_proof_root
beacon_seed_at(anchor + Δ).is_some()
self 枝: PalwACommitRegistry[leaf.a_commit] == Some(leaf.a_commit_epoch)
```

evidence の再実行はしない(witness は wire にのみ存在し永続化されない)。これで健全なのは
**境界深度の論証**による: anchor−k / anchor+Δ / a_commit 登録は全て chunk acceptance より深い。
これらを跨ぐ reorg は必然的に chunk acceptance 自体を巻き戻し、新 chain 上で chunk が再処理される
際に clause 11/12 が新文脈で再評価される。acceptance を巻き戻さない reorg は文脈を動かせない
(D3-a store の境界再 resolve 上書きと同じ座標論)。clause 13 はその防御線の**等値形**であり、
窓外に出た古い ticket を fail-closed で止める役も兼ねる(D3-a の「honest ticket は常に窓内」)。

### 部分ゲート禁止の充足(D3-b の判定基準)

- leaf field 追加(§1)、chunk v3(§5)、clause 11/12/13、producer(§7)、DB 16(§6)は
  **単一コミット**で入る。field 先行(宣言 bool の受理)も検証先行(正直 block の全滅)も作らない —
  §5.14.4 の 2 角の回避がそのまま判定基準。
- `palw_algo4_accept = false`(全 preset)/ activation lever 不変。wiring は re-genesis 後も
  G12 e2e(D3-c)が green になるまで INERT preset の背後にある。
- 旧 `PalwDispatchProof` / `palw_dispatch_proof_valid` / 旧 `palw_challenge_fresh` 形は本スライスで
  **削除**(§5.14.7.9 が予告した除去)。

## 5. Chunk v3(wire のみ、非永続)

`PalwLeafChunk` version 3(v2 拒否は validate 側 — M2 の「寛容 parse は穴の再開」と同じ規律で
**v2 も v3 検証器は拒否**し、v3 は 専用 version 検査を持つ):

```rust
// per-leaf、leaves と同順・同数
pub struct PalwLeafPcpbWitnessV1 {
    pub scheduler_job_id: Hash64,
    pub requester_credential: Hash64,
    pub request_commitment: Hash64,
    pub dispatch: PalwDispatchEvidence,
}
```

文脈非依存 `validate_leaf_chunk`: `witnesses.len() == leaves.len()`、membership proof 検査は v2 と
同じ、witness の静的上界(sibling ≤ 8、`b_receipt_preimage` ≤ 既存 receipt 上界、pk/sig は
ML-DSA-87 固定長)。

**payload 予算(実測で pin する)。** ML-DSA-87: pk 2592 B / sig 4627 B。SelfSerial witness ≈
pk + sig + preimage(≈2.6–5.5 KiB、session key 含む)+ membership 2 本(≤1 KiB)+ entry ≈
**≈ 13 KiB/leaf**。BeaconAssigned ≈ 2.6 KiB/leaf。`PALW_MAX_OVERLAY_PAYLOAD_BYTES = 512 KiB` の
内側に**最悪系列(全 leaf SelfSerial)**が収まるよう、v3 chunk の leaf 上限を
`PALW_MAX_LEAVES_PER_CHUNK_V3 = 24`(≈ 24 × (1.2 + 13 + 0.6) KiB ≈ 355 KiB + ヘッダ余裕)へ引き下げる。
正確な上界は §8 のテストが式ではなく **実 encode の byte 長**で表明する(§5.15.2 の「推測ではなく
計算」の規律)。`chunk_count` 上限・manifest の chunk 算術は `PALW_MAX_BATCH_LEAVES_V1 = 256` を
不変のまま 256/24 = 11 chunk を許すよう再確認する。

## 6. DB / format 規律

- `LATEST_DB_VERSION` **15 → 16**(`factory.rs`)、pin 改名、`daemon.rs` hard-reset arm を
  `<= 15` へ。双方向表明(`factory.rs` の新旧境界 assert)は既存テストが強制。
- `LEAF_LEN = 1165` / `LEAF_FNV` 再 pin。**LIFECYCLE / VIEW / CERT / MANIFEST の pin は動いては
  ならない**(動いたら範囲外を触っている — §5.15.10 と同じ柵)。
- 新 domain 定数(`PALW_PROVIDER_ID_DOMAIN` ほか)は pairwise distinctness テストと
  `domain_strings_are_pinned_and_fit_key_limit` へ登録。
- 新 store prefix 2 本(`PalwProviderSnapshotHistory` / `PalwACommitRegistry`)を
  `database/src/registry.rs` へ(D3-a の `PalwBeaconSeedHistory` の隣)。
- genesis hash は動かない(header preimage 不変)。**ただし leaf format が動くため testnet-20 は
  再 genesis 配布**(ADR-0041/0048 の cutover 手順に同乗)。

## 7. Producer(mil 側)

- **self 枝(bridge):** 計算完了 → `a_commit = H(job_descriptor ‖ receipt_fields ‖ r_blind)` →
  `PalwACommitV1` tx 送出 → 登録 epoch 確定(= `a_commit_epoch`)→ `R_{anchor+Δ}` 待ち →
  `derive_b` で B 確定 → B へ receipt 送付・`a_commit` 埋め込み署名回収 → witness 組立。
  r_blind / opening は DA object 側(既存 Seam 経路)。
- **external 枝(bridge/scheduler):** challenge 発行(既存 Seam 1)→ `R_{issued+Δ}` で pair 確定 →
  実行 → witness 組立(`PalwSnapshotWitnessSet::select()` が producer 側 API として既在)。
- **miner:** `build_leaf_chunk` v3(witness 同梱、leaf 上限 24)、`build_batch_manifest` は不変
  (manifest format は動かない)。fixture は §5.15.9 の教訓どおり**導出**で作り直す(リテラル禁止)。
  producer 棚卸しは「miner / auditor / palw_demo / virtual_processor tests harness」の 4 本 +
  bridge(新規)— **grep で数えず全経路を再走査する**(2 回続けて漏れた記録がある)。

## 8. テスト計画(最小集合)

- **G7(恒真化回帰):** evidence 検証が「宣言 bool を一切読まない」ことの回帰 — 偽 root 差し替え
  (clause 0)、非抽選 provider、pair 同一、署名偽造、a_commit 非埋め込み、branch↔kind 不一致。
  `palw_prod_findings_all_covered` へ登録。
- **grind 系 negative:** a_commit 事後登録(registry epoch ≠ leaf epoch の両方向)、issued_epoch
  自由申告(challenge 再導出不一致)、freshness 境界(`anchor+Δ = registered` 丁度 / `w` 丁度 /
  ±1)、窓外 anchor の fail-closed(seed / snapshot / registry の各 None)。
- **provider 束縛:** 抽選勝者 ≠ leaf 申告 provider の拒否(A/B 両席、両枝)。unbonded A の
  self-order 拒否。
- **sentinel 拒否:** Object-V1 型 leaf(challenge/epoch = 0)が clause 11 で落ちる。
- **payload 上界:** 全 leaf SelfSerial の満杯 v3 chunk の実 encode が 512 KiB の内側、
  上限 +1 leaf が文脈非依存検証で落ちる。
- **fork/reorg:** epoch 境界跨ぎ reorg 後の snapshot/seed 再 resolve と clause 13 の整合
  (`palw_algo4_sink_reorg_…` の隣)。acceptance 巻き戻し → 再処理で新文脈により拒否される系。
- **窓 assert:** D3-a harness の bounded assert を `+w`(beacon)/`+w+k`(snapshot)へ拡張。
- **cross-crate golden:** bridge/miner の witness 組立 → consensus 検証器を両枝で通す
  (INERT テストの builder→verifier 経路を production 型で再固定)。
- **params:** 6 preset の flat mirror(`palw_freshness_window_epochs` /
  `palw_snapshot_lag_epochs` / `palw_post_commit_delta_epochs`)一貫性 + invariant 反証テスト。

## 9. 範囲外(D3-c 以降へ)

- **G12 e2e**(正/否/境界を ADR-0044 harness 上で)— D3-c。本スライスの単体/統合テストは
  その前提を作るが、判定は harness green のみが行う。
- no-show penalty / reroll(§1)、価値 escrow / settlement(D1/D4 fence)、
  §5.14.7.7 `runtime_class_id → implementation_id`(同じ re-genesis 列車に載る**別コミット** —
  それ自身の 5 箇所同時規律は §4.4 のとおり)、adaptive m(`replica_count` 系 — §4.2 の残り半分。
  二者 leaf の m は事実上 2 で固定継続)。

## 10. 実装中に確定した 2 件(設計への追補、2026-08-01)

### 10.1 Object-V1 leaf は D3-b 以降 **格納不能**になる(spec change)

clause 11 は `receipt_v3_job_challenge` を `R_{issued}` の下で再導出する。`validate_public_leaf` の
Object-V1 arm は **その 2 field が zero sentinel であることを要求する**(`leaf.receipt_v3_legacy_sentinel`)。
両者は同時に満たせない — したがって **v3 chunk に載せられる leaf は Object V2 のみ**である。

これは弱体化ではなく、D3-b が要求する帰結である(V1 は「challenge が存在しない」形式であり、
freshness を掛ける対象が無い)。§4 の「sentinel を構造的に拒否する」を、実装では
**Object-V1 leaf 自体が leaf-chunk 経路から消える**という形で満たす。`validate_public_leaf` の
V1 arm は leaf-chunk 経路では到達不能になる(他経路の互換のため削除はしない)。

### 10.2a A-commit registry 検査は「等値」ではなく `row ≤ declared`(spec change、両座標)

§4 clause 12 は `PalwACommitRegistry[a_commit] == Some(a_commit_epoch)`(等値)と書いたが、
実装(acceptance / clause 13 の両座標)は **`row ≤ declared`** に確定した。根拠:

- **危険な方向は一方だけ**: 拒否すべきは「登録が leaf 申告より**遅い**」(既知の beacon を借りる)
  方向で、これは `row ≤ declared` が正しく殺す。
- 「古い登録に後から epoch を当てる」(row < declared)は無害: commitment は `R_{declared+Δ}`
  判明前に on-chain 確定しており、B が commit を後追いする保証は保たれる。残る自由度
  (どの epoch の pair に条件付けるか)は freshness 窓 `w` に上界される — external 枝の
  issued_epoch 選択と**同じ**残余 grind であり、等値にしても複数 anchor の登録(tx 手数料)で
  同じ自由度が買えるため、等値は grind を消さず価格を付け替えるだけ。
- 両座標の実装コメントに同じ論証を残し、`pcpb_clause12_acceptance_selfserial_registry_and_seats`
  (acceptance)と `palw_pcpb_ticket_binding_enforced`(mint)が **row 不在 / row > declared 拒否・
  row ≤ declared 受理**の両方向を pin する。

### 10.2b lease challenge = leaf challenge(byte-parity は load-bearing)

`PALW_JOB_CHALLENGE_DOMAIN` は bridge の `BRIDGE_JOB_CHALLENGE_DOMAIN` と**同一文字列**で確定
(帰結 2 の実体)。clause 11 が再導出する leaf の `receipt_v3_job_challenge` は Seam 1 の lease
challenge **そのもの**であり、これが崩れると発行済み lease が on-chain で解決しなくなる。
parity は `job_challenge_parity_with_consensus_is_pinned`(bridge)が cross-crate で固定する。

### 10.2 mint 開始 epoch の下界 = `k + Δ`(genesis 境界条件)

clause 12 は `anchor − k` の snapshot を、clause 11/12 は `anchor + Δ` の seed を要求する。
`anchor ≥ issued ≥ 0` かつ `anchor − k` が非負でなければならないので、**最初に mint 可能な
`registered_epoch` は `k + Δ` 以上**(現行 inert 値で 4)。再 genesis 直後の数 epoch は
algo-4 が構造的に空になる — これは fail-closed として正しく、テスト fixture(現行
`FIXTURE_REGISTRATION_EPOCH = 1`)を作り直す際の必須の制約でもある。運用手順(§7 producer)は
「epoch ≥ k+Δ になるまで batch を登録しない」を明記すること。

## 11. 着地状況(2026-08-01 時点)

**着地済み(workspace 全体が `cargo check --workspace --tests` green):**

- §1 leaf 5 field / §5 chunk v3 + `PalwLeafPcpbWitnessV1` + shape 検証 / 旧 `PalwDispatchProof` 系の除去
- §4 clause 11・12 を acceptance arm(`insert_leaf` の前)に、clause 13 を `check_palw_ticket` に配線。
  `PalwPcpbAcceptanceCtx` は `None` = **全 leaf chunk 拒否**(検証不能は waiver ではなく拒否)
- §2 `PalwACommitV1`(subnet `0x45`)+ `DbPalwPcpbStore`(prefix 68/69)+ 選択鎖 reconcile writer
  (provider-bond registry と同一座標・同一 batch;epoch 境界ごとに snapshot を導出)+ sweep
- §3 窓の `+w` / `+w+k` 拡張、pruning snapshot の 2 carry(writer/validate/import)
- §6 DB 15→16、daemon hard-reset arm `<= 15`、`LEAF_LEN 964→1189` / `LEAF_FNV` 再導出、
  cross-crate golden(leaf hashes / root)を miner・core の両側で再導出、
  新 domain 2 本(`PALW_PROVIDER_ID_DOMAIN` / `PALW_JOB_CHALLENGE_DOMAIN`)を 3 つの domain テストへ登録
- §5.14.7.1 の evidence verifier を D3-b 仕様へ更新(branch↔`dispatch_kind` 束縛、
  座席束縛 `provider_{a,b}_id`、self 枝の A membership + A≠B)。`pcpb_evidence_tests` は
  実 ML-DSA-87 / 実 Merkle / 実抽選のまま全緑

**tripwire — 発火 → 再 genesis で解決(2026-08-01)。**

`compute_registry_palw_network_selection` の `consensus_identity_hash` pin が落ちた。§3 の
`w`/`k`/`Δ` を `Params` に足したので testnet-20 の consensus identity が動いたためで、テスト doc の
2 択のうち (b)「無条件の変更 = 採掘済み履歴を無効化する」に該当。規定どおり **新 suffix への
再 genesis で解決した**: `testnet-21`(`pcpb-palw`、`PCPB_PALW_PARAMS` /
`PCPB_PALW_GENESIS`、port 26531、tag "misaka-pcpb-palw")。tripwire(threshold pin + identity
pin)は `pcpb_palw_network_selection` へ移設し、PCPB 窓 3 本も identity の一部として pin。
testnet-20 は deprecated(seeders 空、ledger 保持者のためにコンパイル可能なまま)。移行手順は
`docs/testnet-21-migration.md`。なお同日着地の bystander-wedge 修正(6ff40d4、work margin の
難易度連動化)は t21 が genesis から継承し、preset の絶対 addend ゼロも新テストで pin した。

**fixture 再構築(§5.15.9 (vi) 型)— 34 件中 33 件着地:**

- `FIXTURE_REGISTRATION_EPOCH` を 1 → 4(§10.2 の `k + Δ` 下界)。fixture leaf は Object-V2 へ
  移行し(§10.1)、`processes::palw` の共有 `pcpb_test_support` から stamp される — 引いた
  provider 座席・committed roots・`R_{issued}` の下で**再導出できる** challenge まで込みで、
  リテラルではなく導出で組む
- acceptance を駆動する全テストが staged PCPB context を渡す。context 無しの呼び出しは
  「clause を飛ばす」のではなく chunk 全体を拒否するので、テストは gate を迂回できない
- algo-4 harness(`virtual_processor::tests`)と参照 mint(`palw_demo.rs`)は leaf を store へ
  直接 seed するため clause 11/12 を通らないが、**clause 13 は通る** — 両者に
  `stage_palw_pcpb_context` / PCPB seed 書き込みを追加した。`palw_demo` の方は production 経路で
  あり、これが無いと `--palw-mine` が `NotReady` で**無言に**止まる(§5.15.9 が
  このファイルについて名指ししている failure mode そのもの)
- cross-crate golden(leaf hashes / leaf root)は miner・consensus-core の**両側**を再導出。
  片側だけ直すと seam が黙って壊れる(テスト自身の doc が「片方だけ落ちたら、それが探していた
  drift」と書いている通り)。`LEAF_LEN 964→1189` / `LEAF_FNV` と同じ再 genesis 級の移動
- **残り 1 件: `palw_full_lifecycle_prune_then_replay_e2e`(ADR-0044 harness)。** これは
  fixture の直しでは閉じない — harness は manifest を **epoch 0** で登録するが、§10.2 の下界は
  `registration_epoch ≥ k + Δ` を要求する。したがって harness の timeline 全体
  (`AUDIT_EPOCH 3` / `ACTIVATION_EPOCH 4` / `EXPIRY_EPOCH 14` と、:110-121 が記録している
  pruning/finality の margin)を **再導出**する必要がある。margin は「walk_bound < pruning_depth」
  「mint は全て pp より下」等の関係で成立しているので、定数を +4 ずらす作業ではない。
  D3-c(G12 e2e)が同じ harness を拡張するので、その設計と一緒に扱うのが正しい

**§7 producer(bridge)— 着地:**

- `mil/bridge/src/pcpb.rs`(Seam 5)。external 枝は `R_{anchor+Δ}` から 2 slot を再抽選して witness と
  leaf binding を返し、self 枝は `SelfSerialFlow` が **commit → anchor(0x45)→ 抽選 beacon 待ち →
  B receipt 回収 → witness** を明示的な状態として進める。順序が安全性そのものなので、`step()` は
  「何を待っているか」を型で返す(`AwaitDrawBeacon` を握り潰さない)
- **producer 自身が consensus の検査を先に全部やる**: B が抽選された provider か、鍵が committed
  `ml_dsa_pk_hash` に落ちるか、preimage が `a_commit` を埋めているか、署名が実 ML-DSA-87 で通るか。
  ここで落とせば chunk 登録料を払う前に止まる — clause 12 で落ちると acceptance の error は
  virtual processor に捨てられるので**無言**になる
- **producer 側の clause 0**: node が返した entry set を `palw_build_snapshot_witnesses` で組み直し、
  served commitment と一致しなければ `SnapshotRootMismatch` で拒否する。改竄・stale をここで捕まえる
- テストは実 verifier(`palw_dispatch_evidence_valid`)+ 実 ML-DSA-87 + 実 Merkle。6 件緑

**§2.2 への訂正(実装で判明):** 「entries 本体は保持しない」は**検証器については正しいが producer に
ついては誤り**だった。producer は membership proof を**作る**ので entry set が要り、現行 registry から
「epoch e 時点の bonded 集合」は再構成できない。よって node が prefix 70 に canonical entry set を
保持し、`getPalwState` の PCPB selector(wire v6)で供給する。**pruning carry には載せない** —
pruned joiner は prefix 68 で**検証**でき、見たことのない epoch の**生産**を助ける義務はない。
この非対称性(検証データは carry、生産データは best-effort)が正しい分界である。

**bridge HTTP/journal 配線 — 着地(2026-08-01、本スライス後半):**

- `ChainFacts` に `pcpb_context(anchor_epoch, a_commit?)` を昇格(`RpcChainFacts` は wire v6、
  `PinnedChainFacts` は pinned PCPB fixture — entries から commitment を**再構築**するので
  「存在し得ない entry/commitment 対」を pin できない)。
- journal 3 イベント: `PcpbSelfFlowOpened`(anchor 前に永続化 — crash が on-chain anchor を
  孤児にしない/re-anchor は B re-roll なので忘却は安全性違反)/ `PcpbAnchorObserved`
  (epoch は**チェーンが報告した値**のみ、自己申告不可)/ `PcpbWitnessProduced`(borsh witness
  同梱、miner の chunk builder がそのまま消費)。
- HTTP 5 ルート(BRIDGE-AUTH-01 準拠): `POST/GET /palw/v1/pcpb/self-flows`(open/poll —
  `SubmitAnchor{0x45 payload}` → `AwaitDrawBeacon` → `AwaitPartnerReceipt{partner_bond,
  preimage}` → `Ready`)、`POST …/self-flows/receipt`(B 受領書 → `finish` → witness)、
  `POST /palw/v1/pcpb/witnesses`(external: **lease 駆動** — lease の triple/shape/epoch が
  そのまま clause 11 の入力; node の `R_anchor` ≠ lease seed は拒否)、`GET …/witnesses`。
- external の idempotency は leaf challenge 直引き(§10.2b の parity による)。
- テスト: 実 ML-DSA-87 + 実 verifier で self 全周(open → wait → receipt → 実 clause-12 通過)
  + 再起動復元、external の lease 束縛・seed 分岐拒否・idempotency、抽選一致
  (`mil/bridge/src/state.rs::tests::pcpb`)。bridge 48/48。

**§8 negative 群 + G7 — 着地(同):**

- acceptance 座標(`processes::palw::tests::pcpb_clause_negatives`、全 fixture が
  「manifest を改竄 leaf 自身の Merkle 投影から導出」する self-consistent 形 — membership gate
  で死ぬ fixture は clause の証明にならない):
  - clause 11: issued 自由申告(challenge 不再導出)/challenge 改竄/Object-V1 sentinel
    (構造的に導出不能)/freshness `anchor+Δ≤registered` ±0/±1・`w` 丁度/w+1(丁度 2 件は受理側で pin)
  - clause 12 fail-closed: context 無し(chunk 全拒否)・issued seed 無し・snapshot 無し・
    draw seed 無し・anchor < k(§10.2 の genesis 境界)
  - 座席束縛: external A/B 席の非抽選 provider 差し替え、self の B 差し替え、unbonded A
    (snapshot 外 bond + 捏造 entry)
  - a_commit registry: 不在/row>declared 拒否、row≤declared 受理(§10.2a)
- payload 上界: 満杯 24-leaf worst-case の実 encode < 512 KiB は既存
  `palw_leaf_chunk_v3_is_mandatory_and_proofs_are_arity_bounded` が表明済み — **上限+1 leaf の
  文脈非依存拒否**を同テストへ追加(`InvalidCount{leaf_chunk.leaves, 25, 1, 24}`)。
- mint 座標: `palw_pcpb_ticket_binding_enforced`(virtual_processor tests)— 実 block +実
  `check_palw_ticket` で clause 13 の root 不一致/snapshot 窓外 fail-closed/self の registry
  不在・row>declared 拒否・row=declared 受理、各否定の後に**同一構成の正常 mint**(帰責可能性)。
- G7: `PALW_PROD_FINDINGS` の **PCPB-01 を `Unimplemented` → `Covered`** へ昇格。名指しは
  mint 検証器 + evidence 恒真化回帰 4 本(偽 root/非抽選/pair 同一/署名偽造/a_commit 非埋め込み/
  branch↔kind — `pcpb_evidence_tests`)+ acceptance 2 本。`palw_prod_findings_all_covered` /
  `palw_gate_table_verifiers_all_resolve` とも green(G12 `palw_pcpb_e2e` は D3-c のまま
  Unimplemented — 検証器名は未 resolve を維持)。

**未着地(次スライス):**

- ADR-0044 harness timeline の再導出(上記)— D3-c(G12 e2e)と同時に扱う
- ADR-0040 §5.14.7.7(`runtime_class_id → implementation_id`)は同じ再 genesis 列車の別コミット
- ADR-0040 本文の §2 PROD 表(PCPB-01 行)と §7.2 G7 表の文言 reconcile(kaspa-pq リポジトリ側)

## Definition of done(本スライス)

- [x] §1 の 5 field + §5 chunk v3 + §4 clause 11/12/13 + §6 DB16 が同一スライス
- [x] 旧 3 helper(`PalwDispatchProof` / `palw_dispatch_proof_valid` / 旧 freshness 形)の除去
- [x] §7 producer(bridge)— `mil/bridge/src/pcpb.rs` + PCPB context の store/RPC 供給
      (§5.15.9 の brick 論証を満たす: 検証器と producer が同じスライスに揃った)
- [x] §8 テスト(grind 系・座席束縛・窓外 fail-closed・sentinel・payload 上界の実 encode +
      上限+1 拒否)+ mint 座標の clause-13 検証器
- [x] 再 genesis: testnet-21(`pcpb-palw`)。identity tripwire は規定手順 (b) で解決(新 preset へ
      pin 移設)。残る赤は **1 件のみ** — ADR-0044 harness の timeline 再導出(D3-c と同時に扱う、
      §11 参照)
- [x] G7: PCPB-01 → `Covered`(`palw_pcpb_ticket_binding_enforced` ほか 6 本)
- [x] bridge の HTTP/journal 配線(Seam 5 — 3 journal イベント + 5 ルート + Pinned PCPB fixture)
- [x] ADR-0045 D3-b へ本メモの pointer 追記

# ADR-0041 — Mainnet は PALW を GENESIS から有効化する(Header-v4 re-genesis 方針の確定)

- **Status:** Accepted(方針として確定。ただし本 ADR は `palw_algo4_accept` を一切動かさない —
  acceptance flip と public/value activation は従来どおり
  `docs/adr-palw-public-value-activation-readiness.md` の A/B ゲート台帳に従う別個のレビュー済み変更である)
- **Date:** 2026-07-26
- **Supersedes / amends:** なし(ADR-0039〔replica-GEMM audited-compute lane、予約番号・未着地〕と
  ADR-0040〔単一プール整数正準化 remediation〕の帰結を mainnet 形状に落とす)
- **Consumes:** `docs/palw-public-value-header-v4-antispam.md`(v4 deployment fence)、
  `docs/adr-permissionless-snapshot-authentication.md`(StopShip)、
  `docs/adr-palw-public-value-activation-readiness.md`(activation 台帳)、
  qwen-8.0 `docs/mainnet-readiness-ledger.md` / `docs/model-genesis-candidate.md`(外部ゲート)

## Context

PALW(algo-4 audited-compute lane)の有効化様式には二系統が実装済みである:

1. **fence 方式** — 既存 chain identity の上で `palw_activation_daa_score` を将来の DAA に置き、
   その高さ以降に lane を「存在」させる。mainnet / testnet-10 / simnet / devnet は
   `u64::MAX`(= 永遠に inert)を出荷している(`params.rs:371-375`)。
2. **genesis-active 方式** — `palw_activation_daa_score = 0`。testnet-palw-110 と devnet-palw-111 が
   この形で出荷済みであり、header processor は `palw_activation_daa_score <= genesis.daa_score` の
   re-genesis-root 境界を明示的に扱う(`header_processor/processor.rs:220, 500-514`)。
   genesis-active の経路だけが実運転・実テストで踏まれている
   (`palw_activated_presets_bound_the_view` が正確に 2 preset を pin する)。

一方、public/value に必須の anti-spam(Header-v4)は **re-genesis 専用の境界**として設計されている。
v4 deployment fence(`palw-public-value-header-v4-antispam.md:15-29`、
`header_processor/processor.rs:217-223` の construction assert)は、非 inert な `palw_spam` を
受理する条件として次の三点 **全て** を要求する:

- `palw_spam.is_structurally_valid()`
- `palw_activation_daa_score <= genesis.daa_score`(= PALW は遅くとも genesis から active)
- `genesis.version == PALW_ANTISPAM_HEADER_VERSION (= 4)`

すなわち「public/value の mainnet」は定義上 **v4 genesis + genesis-active PALW** であり、
既存 mainnet identity への後付け fence では到達できない。

後付けの困難は理論だけではない。2026-07-26 に devnet-111 の稼働中チェーンへ pruning 深度を
後付けした際、歴史ヘッダに pruning sample が存在しないため pruning point の初回前進が
約 21,600 ブロック遅延することを実測した(§Consequences 参照)。パラメータの後付けは
「新しい規則を知らない歴史」という一回性コストを常に伴う。genesis から正しい形で始める方が
単純で、検証済み経路の再利用になる。

## Decision

**Mainnet は PALW を genesis から有効化した Header-v4 の新 identity として発行する。**
既存 mainnet identity への fence 方式(選択肢 1)は採用しない。具体的に:

1. **新 genesis / 新 network identity。** mainnet 相当の最終 identity は、新しい
   `MAINNET_PALW_GENESIS`(`version = PALW_ANTISPAM_HEADER_VERSION = 4`、
   `palw_spam_accumulator_commitment` を genesis で finalize)を持つ。suffix・ports・seeds・
   datadir は全て新規(v4 は re-genesis 境界であり、途中変換・DB 移行は提供しない)。
2. **`palw_activation_daa_score = 0`。** lane は genesis から「存在」する。
   testnet-palw / devnet-palw で実証済みの genesis-active 経路をそのまま使う。
3. **`palw_spam` は非 inert。** 値は `PalwSpamParams::PUBLIC_REGENESIS_CANDIDATE` を出発点とし、
   本 roadmap 項目 6(非ゼロ経済パラメータの実測較正)の完了値で確定する。
   構造妥当性は construction fence が強制する。
4. **`palw_algo4_accept` は本 ADR の範囲外(false のまま)。** 「land / accept / weight」の
   三段のうち本 ADR が決めるのは land の形状のみ。accept の flip は activation 台帳
   (A: code ゲート、B: 72h soak・独立レビュー・re-genesis ceremony 等の外部ゲート)を
   全て通過した後の、別個のレビュー済み変更である。`palw_compute_work_scale`(weight)も
   同様に 0 のまま(Stage-A: accept+measure)。
5. **mint は `eligible=false, weight=0` のまま。** qwen-8.0 `mint.rs` の 12 ゲート
   (ExternalNetwork ×5、ExternalHardware、ExternalModelGenesis を含む)は本 ADR で動かない。
6. **段階投入は staging-mainnet で行う(roadmap 項目 8)。** 最終 identity の前に、同一形状
   (v4 genesis、genesis-active、非 inert spam、accept=false)の **staging-mainnet** を
   新 genesis で起動し、re-genesis ceremony・多ノード pruning/catch-up/reorg soak・
   DA/certificate/払い出しの全 vertical を演習する。staging で確定した genesis 形状が
   そのまま最終 identity の雛形になる。

### 採らなかった選択肢

- **(a) 既存 mainnet identity への fence 後付け** — v4 fence が構造的に禁止
  (`genesis.version == 4` 要求)。仮に v3 のまま fence したとしても G6(sibling flood)が
  unbounded のまま public に出ることになり、activation 台帳の StopShip に抵触する。却下。
- **(b) v3 genesis-active(testnet-palw の形状を mainnet へ)** — anti-spam commitment を持たず、
  public/value の前提を満たさない。閉域 testnet 専用形状である。却下。
- **(c) 方針決定の先送り(`u64::MAX` 維持)** — roadmap 全 8 項目が「mainnet の形」に依存する
  (経済較正は spam/bond パラメータの入る genesis を、snapshot auth は v4 の pruned-IBD を、
  staging-mainnet は genesis 形状を前提とする)。形を先に固定しないと後続項目が全て仮定の上に
  積み上がる。却下。

## Consequences

### 正の帰結

- **検証済み経路の再利用。** genesis-active は 2 preset で実運転済み。re-genesis-root 境界の
  取り扱い(nullifier fold、lane bits、DA 状態の genesis 例外)は実装・テスト済み。
- **後付けコストの根絶。** 2026-07-26 実測: 稼働中 devnet-111 へ pruning 深度
  (finality 7,200 / pruning 21,600)を後付けした結果、歴史ヘッダに pruning sample が無く
  初回 pruning point 前進が sink ≈ 122,400(約 9 時間)まで遅延した。新 genesis なら
  サンプルは最初から載り、この種の遅延は発生しない。
- **アーカイブ強制の不要化。** devnet-111 で per-block PALW ストア(DA 250/254、
  search 189/190)の pruning walk 組み込みと boundary-anchor スキップ則を実装済み
  (commit `ffc9fb8`)。mainnet は `palw_requires_archival=false` の pruned 運用を既定とできる。

### 負の帰結・警戒事項(設計で吸収する)

- **genesis 直後のウォームアップ窓。** DA 方針 `min_beacon_burial_daa = 100` により、
  genesis+0..100 には buried beacon が存在せず **DA obligation は登録できない**。
  eligibility draw も finality-buried な lagged `R_E` を要するため初期 epoch では空である。
  `certificate_allowed` は「空 obligation 集合は成功ではない」(da.rs:1318)を保つ —
  つまり **最初の leaf/batch は構造的に genesis+ウォームアップ後**にしか成立しない。
  これは欠陥ではなく仕様であり、staging-mainnet の演習項目に「genesis からの最短 mint 到達」を
  含めて実測する(devnet-111 の実測では beacon warm→bond→batch→mint の全 vertical は
  DAA 数千のオーダーで完了している)。
- **premine 分配が先行依存。** provider bond(`min_provider_bond_sompi`)と leaf bond は
  資金化された UTXO を要する。genesis は単一 premine UTXO であるため、
  最初の leaf の前に「分配 → 成熟 → bond」の運用手順が必須。staging-mainnet の
  ceremony 手順書に分配計画(faucet 方針含む)を織り込む。
- **pruned 運用と late-join。** genesis-active + pruned mainnet では、pruning point より
  後方から参加するノードに trustless な snapshot import が必要になる。現状の
  permissionless snapshot authentication は **StopShip**(primitives 済み、配線未了)。
  → **roadmap 項目 2 は本方針の hard dependency** である(完了まで public 参加形態は
  full-history same-genesis 参加に限られる)。
- **G6 が unbounded のままなら v4 の意味が無い。** 非 inert `palw_spam` は stamp を課すが、
  reachability reindex の O(部分木) 書き換えは stamp では払われない。
  → **roadmap 項目 3(G6 の bounded 化)も hard dependency**。
- **Header-v4 は一方通行。** 新 identity 発行後の遡及修正は re-genesis しかない。
  だからこそ staging-mainnet(項目 8)を最終 identity と同形状で先行させ、
  ceremony・soak・全 vertical を演習してから確定する。

### 依存グラフ(roadmap との対応)

```
ADR-0041(本書: 形状の確定)
  ├─ 項目2 permissionless snapshot auth      … pruned mainnet の late-join に必須(StopShip 解除)
  ├─ 項目3 G6 bounded 化                     … 非 inert spam を意味あるものにする(StopShip 解除)
  ├─ 項目4 nullifier prune-then-replay E2E   … pruned 運用での nullifier 健全性の実証
  ├─ 項目5 PCPB / fraud / audit model 固定    … ECON-03「fraud-proof なき高額報酬の禁止」の解決
  ├─ 項目6 経済パラメータ実測較正             … PUBLIC_REGENESIS_CANDIDATE → 確定値
  ├─ 項目7 ModelGenesisManifest 二者再現      … ExternalModelGenesis ゲート(mint 側)
  └─ 項目8 staging-mainnet 起動               … 上記を束ねた同形状演習 → 最終 genesis の雛形
```

## Definition of done(本 ADR 自体の)

- [x] 方針の文書化と番号付け(本書)
- [~] `MAINNET_PALW_GENESIS` / staging-mainnet 用 preset の追加(項目 8 で着地)
  — **staging 半分は着地済み(ADR-0048 / commit a861606)**: `STAGING_PALW_GENESIS`
  (`consensus/core/src/config/genesis.rs:321`、`version: PALW_ANTISPAM_HEADER_VERSION` at `:336`)、
  `STAGING_MAINNET_PALW_PARAMS`(`consensus/core/src/config/params.rs:1719`、`palw_activation_daa_score: 0`、
  `palw_algo4_accept: false`、`palw_compute_work_scale: 0`、`skip_proof_of_work: false`)、
  network 選択(`params.rs:945-946` / `network.rs:280` port 26511)。
  — **`MAINNET_PALW_GENESIS` は未着地(外部依存)**: `MAINNET_PARAMS` は今も
  `palw_activation_daa_score: u64::MAX`(`params.rs:1397`)+ v3 genesis。着地の前提は本 repo の外にある
  (ADR-0046 の経済パラメータ確定 = 現在 `palw_spam` は `PUBLIC_REGENESIS_CANDIDATE` プレースホルダ、
  ADR-0048 の 30 日 staging soak、re-genesis ceremony の統治判断)。`params.rs:1692-1694` の指示どおり
  staging 成功後に verbatim コピーする。**先に定数を書くことは統治成果物の捏造にあたるため行わない。**
- [~] 上記 preset が v4 fence の三条件を construction 時に満たすことの test pin
  — **staging preset についてのみ達成。`MAINNET_PALW_*` は対象が存在しないため未達**(直上の `[~]` と同じ
  antecedent を共有する。半分しか存在しない対象に `[x]` を付けないため `[~]`)。
  fence 本体 `consensus/src/pipeline/header_processor/processor.rs:217-222`(assert)。staging について三重に pin 済:
  (1) 静的ミラー test `params.rs:1989` `palw_header_v4_antispam_is_inert_on_every_shipped_preset_except_the_staging_regenesis`
  (`:2015` "v4 fence (1/3)"、`:2017-2018` "(2/3)"、`:2021-2023` "(3/3)")、
  (2) 実 runtime construction `virtual_processor/processor.rs:8796` `v4_fixture_params()` が
  無改変の `STAGING_MAINNET_PALW_PARAMS` を返し、`TestConsensus::new` 経由で上記 assert を実走行(`:8979-8981`)、
  (3) CLI/daemon matrix `kaspad/src/args.rs:1809-1853`。
  mainnet preset 着地時に同じ三条件を pin して初めて `[x]` になる。
- [~] `palw_activated_presets_bound_the_view` の期待集合更新(staging preset 追加時)
  — **staging 追加分は反映済。mainnet preset 追加時に再更新が必要**(同上の理由で `[~]`)。
  — `params.rs:2118` で `[(&str, Params); 7]` へ拡張、`:2126` に `("staging-mainnet-palw", STAGING_MAINNET_PALW_PARAMS)`、
  期待集合は `:2156` で `vec!["testnet-palw-110", "devnet-palw-111", "staging-mainnet-palw"]`。
  同 test は 7 preset 全てに `!p.palw_algo4_accept` を再断言(`:2133`)しており、本 ADR §4「accept は false のまま」を
  機械的に強制している。sibling pin は `params.rs:2234` / `:2310` / `:2226`。

本 ADR は「形」を確定する。それ以上のいかなるレバー(accept / weight / mint)も動かさない。

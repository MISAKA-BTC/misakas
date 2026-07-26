# ADR-0048 — Header-v4 staging-mainnet: 新 genesis での起動計画

- **Status:** Accepted(構成と起動条件の確定。起動実施は先行 ADR の DoD 到達後)
- **Date:** 2026-07-26
- **Consumes:** ADR-0041(形状)、0042(snapshot auth)、0043(G6 bound)、0044(harness)、
  0045(security model)、0046(経済較正)、0047(model genesis — 非ブロッカー)

## Context

ADR-0041 は mainnet の形(v4 genesis、genesis-active PALW、非 inert spam、accept=false)を確定した。
Header-v4 は一方通行の re-genesis 境界であるため、最終 identity の前に**同形状のリハーサル網**が要る。
既存の "staging-mainnet" という語は DNS/PoS-v2 overlay の文脈(test-plan-kaspa-pq.md)でのみ使われており、
PALW 用 staging はここで初めて定義する。

## Decision

1. **Preset 定義(確定):** `STAGING_MAINNET_PALW_PARAMS` を新設する。
   - `net`: 新 suffix(mainnet でも devnet-111 でもない; 例 `testnet-200` 系の独立 identity)
   - `genesis`: 新規 `STAGING_PALW_GENESIS` — `version = 4`、`palw_spam_accumulator_commitment` を
     genesis で finalize、独自 coinbase_payload、新 timestamp
   - `palw_activation_daa_score = 0`、`palw_algo4_accept = false`(flip は別変更)、
     `palw_compute_work_scale = 0`
   - `palw_spam`: `PUBLIC_REGENESIS_CANDIDATE`(ADR-0046 L1 較正後に更新)
   - `MAX_DIRECT_CHILDREN_PER_PARENT`(ADR-0043)有効
   - **実 PoW**(`skip_proof_of_work = false`、testnet-palw と同じ「algo-3 実 PoW + algo-4 は
     hash-floor 免除」形状)— devnet の skip-pow は staging では使わない
   - `palw_requires_archival = false`(pruned 運用が既定; 2026-07-26 の pruning 実装が前提)、
     `palw_requires_peer_allowlist = true`(初期は closed → snapshot auth 完成後に開放)
   - 縮小しない実物大パラメータ(finality/pruning depth は mainnet 想定値)
2. **起動条件(gate):** 以下が全て green であること。
   - ADR-0043 の sibling bound 実装 + 単体/e2e(閾値凍結は staging 上で行うので実装のみ)
   - ADR-0044 harness + prune-then-replay E2E + 0042-1c fixture
   - ADR-0045 D1(SS-04 非遡及修正)着地
   - genesis ceremony 手順書(premine 分配計画・faucet 方針・鍵管理 — qwen-8.0 の
     release-signing / faucet 文書を staging 用に具体化)
   - `palw_activated_presets_bound_the_view` 等の preset pin テスト更新
3. **staging で実施する演習(ADR-0041 の負の帰結の実測):**
   genesis からの最短 mint 到達(warm-up 窓の実測)、premine 分配→bond→batch 全 vertical、
   pruning 初回パス(genesis からサンプルが載るので遅延なしを確認)、
   ADR-0046 L1/L2 再実測、multi-node soak(0042 の要件)、
   その後 allowlist 開放 → 30-day permissionless public soak(mainnet-readiness ledger の C 項)。
4. **成功 = 最終 mainnet genesis の雛形化:** staging で凍結された params/genesis 形状を
   そのまま `MAINNET_PALW_*` に写す(値の写経のみ、新設計なし)。

## Consequences

- 起動は先行 ADR の code-DoD に依存(0043 実装 / 0044 harness / 0045 D1)。これらは
  次の実装スロットの作業列であり、本 ADR は列の**終点**を固定する。
- 実 PoW 採掘・複数ホスト・30-day soak は外部リソース(経過時間を含む)。
- devnet-111 は staging 起動後も「速い実験網」として並存する(用途分離)。

## Definition of done

- [x] 構成・起動条件・演習項目の確定(本 ADR)
- [ ] `STAGING_MAINNET_PALW_PARAMS` + `STAGING_PALW_GENESIS` 実装 + preset pin テスト
- [ ] ceremony 手順書(分配・faucet・鍵)
- [ ] 起動 → 演習列の完走 → 30-day public soak(外部)

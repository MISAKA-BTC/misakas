# ADR-0042 — Permissionless snapshot authentication: completion contract

- **Status:** Accepted(方針確定。lever は既定 `false` のまま。本 ADR は配線を有効化しない)
- **Date:** 2026-07-26
- **Supersedes / amends:** `docs/adr-permissionless-snapshot-authentication.md`(StopShip)を amend し、
  完成の定義(DoD)と設計上の確定事項を固定する
- **Consumes:** ADR-0041(mainnet は pruned 運用 → permissionless late-join が hard dependency)

## Context

pruned mainnet(ADR-0041)では pruning point より後方から参加するノードに **trustless**
な snapshot import が必要になる。現状は operator-pin を信頼根とする v4 import しか許されず、
chain-derived(permissionless)経路は二層(provenance admission + `Config::palw_permissionless_snapshot_auth`
lever、既定 false)で fenced。crypto core と fenced importer call-site は着地済み
(`consensus/core/src/palw_pruned_frontier.rs`、nodes 2b8139c/82d2330/5bf0a8a/1afedd6)。

**本 ADR 執筆時の再検査で確定した事実(元 ADR の記述を更新する):**
元 ADR は「install-before-verify(durable write が `c==v` 検証に先行する)gap 1」を
未解決の StopShip として記述していたが、**現行コードでは既に解消済み**である。
`prepare_pruning_point_palw_snapshot_import`(`consensus/src/pipeline/virtual_processor/processor.rs:7801`)は
read-only で、chain-derived 経路では `verify_chain_derived_pruning_boundary_from_payload`(:7889)と
全 collision preflight を行って `Prepared…` を返すだけ。実際の `stage_prepared…` + `self.db.write(batch)`
は別関数 `import_pruning_point_palw_snapshot`(:8225-8251)にあり、**検証は書き込みに先行する**。
したがって残る作業は安全性バグの修正ではなく、**信頼根の transport とその検証の統合**である。

## Decision

permissionless snapshot auth の **完成の定義(DoD)を以下に固定**し、4 点すべてを満たすまで
lever は既定 false のまま出荷しない。

1. **信頼根は新しい PoW 暗号ではなく既存の pruning-proof accumulated-work 検証を再利用する。**
   transported Header-v4 bundle(PP selected-parent state を `overlay_commitment_root` で commit する
   子ヘッダ + below-PP support-row ヘッダ commitment)は、**既存の trusted-data / headers-proof パッケージ
   の内側**(または暗号的にそれに束縛された bounded addendum)で運ぶ。独立した PoW validator は作らない。
   → 元 ADR の 1d 選択肢のうち「同一パッケージ内 transport」を確定採用する。
2. **1c 導出等価性を full-lifecycle TestConsensus fixture で証明する。**
   payload 由来の `palw_pruning_payload_paid_work_nullifiers` / `palw_pruning_payload_da_state_root` が、
   live commitment が使う store 由来の `palw_paid_work_window(pp)` / `palw_da_parent_state(pp).state_root()`
   と**一致**することを、**実 batch lifecycle(manifest→leaf-chunk→cert→mint carrier)で coherent に
   構築した状態**の上で assert する。mint 専用の hand-seed 環境(`palw_algo4_env`)は overlay view と
   manifest が pruning 用に coherent でないため不可 — この fixture は ADR-0044 の long-chain harness を
   共有する(keystone)。手で builder を再現する fixture は builder のバグを検出できないため**禁止**。
3. **信頼上のレビュー点は 3 つに限定される** — (a) transported ヘッダが proof-validated 集合の部分集合、
   (b) descendant が正しい first-post-pruning-point child、(c) support-row ヘッダが anti-spam closure に
   一致 — これを独立レビュアーが承認する。
4. **新 v4 re-genesis identity で多ノード pruning/catch-up/reorg soak** を通す。

## Consequences

- lever は既定 false。全 6 preset は archival/closed-network policy を保つ。DoD の 1〜4 を満たすまで
  public late-join は full-history same-genesis 参加に限られる(ADR-0041 の pruned-mainnet 制約と一致)。
- 2(fixture)は ADR-0044 の harness に依存する = **ADR-0044 が先行 keystone**。
- 1(transport)と 4(soak)は multi-node かつ外部レビューを要し、in-session では閉じない。
- 安全性の観点では現行コードは fail-closed(検証が書き込みに先行、誤導出は valid boundary を
  拒否するだけ)であり、lever off の限り regression リスクは無い。

## Definition of done

- [ ] transport 統合(既存 pruning-proof パッケージ内)+ IBD flow が lever on 時に bundle を構築し importer へ渡す
- [ ] 1c full-lifecycle fixture(ADR-0044 harness 共有)green
- [ ] 3 レビュー点(a/b/c)の独立レビュー完了
- [ ] 新 v4 re-genesis での多ノード soak 完了
- [x] install-before-verify 順序の確認(現行コードで既に fail-closed)

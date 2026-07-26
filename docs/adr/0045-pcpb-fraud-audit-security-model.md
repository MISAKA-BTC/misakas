# ADR-0045 — PCPB / fraud / audit security model の固定

- **Status:** Accepted(security model と意味論の確定。PCPB 配線と fraud-carrier 実装は fenced のまま)
- **Date:** 2026-07-26
- **Supersedes / amends:** ADR-0040 §5.14(PCPB do-not-wire-now)を「配線条件付き確定」へ更新;
  `consensus/core/src/palw.rs` SS-04 注記(:4162-4177)の意味論を決定
- **Consumes:** qwen-8.0 `docs/security-model.md`(境界宣言)、ADR-0041

## Context

三つの独立した「正しさの柱」が異なる成熟度で存在する:

1. **Audit rounds(built)** — 重み付き committee 選出 + sample root + certificate attestation。
   G13 = VerifierExists(honest-accept + 6 種の reject が e2e 済み; multi-node withhold/reorg は統合待ち)。
2. **PCPB = post-commitment provider binding(specced, INERT)** — ticket と provider の遅延束縛。
   primitives(domain、derive_b、dispatch proof、receipt merkle)は全て pure で**production 呼び出しゼロ**。
   G12 = Unimplemented。ADR-0040 §5.14 が「今は配線するな」(P1-7 型 `true && true` tautology 回避、
   per-epoch beacon seed 履歴の欠如)を規定。
3. **Fraud(unwired + 未解決の意味論分岐)** — `PalwBatchStatus::FraudEvidence→Revoked` は
   production call site ゼロ。**SS-04**: `revoked_from_daa` は非遡及だが `next(FraudEvidence)` 経由の
   Revoked は遡及 — 「配線する者はまず此れを解決せよ」とコードが明記。
   経済側の拘束: ADR-0040:700「fraud-proof が無い段階で高額報酬を有効化してはならない」。

## Decision

### D1. Fraud 意味論(SS-04)の確定: **非遡及(revoked_from_daa)を正とする**

Revoked は **`revoked_from_daa` 以降の新規 credit/settlement を止める**効果とし、
既に確定した支払いを遡及的に無効化しない。理由:

- 遡及 Revoked は coinbase 済み UTXO の巻き戻しを要求し、UTXO モデルでは不可能
  (支払いは merging block の coinbase で即時確定 — 2026-07-26 の実測 PASS が示す通り)。
  「遡及」を選ぶと必然的に settlement 遅延(escrow/maturity)の導入になり、
  §17.1 の即時分配設計と矛盾する。
- 非遡及でも経済的抑止は **bond slash**(遡及可能な担保没収)が担う。fraud の損害は
  「future revenue の停止 + collateral の slash」で回収する、という分業を確定する。
- 従って `next(FraudEvidence)` の遡及セマンティクスを非遡及に修正し、SS-04 分岐を消す
  (これは fenced コードの整合修正であり、consensus 挙動の変更は fraud 配線時に初めて現れる)。

### D2. 三本柱の役割分担(security model として固定)

| 柱 | 検出するもの | 効果 | 時点 |
|---|---|---|---|
| Audit rounds | 標本化された leaf の再検証不一致 | certificate 拒否(事前)/ FraudEvidence(事後) | batch lifecycle 内 |
| PCPB | ticket と provider の束縛偽装(委譲・横流し) | ticket 無効 = mint 不能(事前) | mint 時 |
| Fraud + slash | 事後に発覚した不正(withhold、虚偽 receipt) | Revoked(非遡及)+ bond slash(遡及的担保) | いつでも |

**信頼境界は qwen-8.0 `docs/security-model.md` の宣言を正とする**: 単一 self-reported receipt は
独立証明ではない; host/GPU-firmware 級の敵は TEE/ZK 域で v1 の範囲外; auditor
scheduling / model 再実行 / opening 配送は out-of-crate。

### D3. PCPB 配線の前提条件(ADR-0040 §5.14 の「いつなら良いか」を確定)

PCPB を ticket 検証へ配線してよいのは次が**全て**揃った時のみ:
(a) per-epoch beacon seed の**履歴**が store に保持される(`retain_future_of` が past epoch を
捨てる現状の解消 — 保持窓は ADR-0044 harness で検証する retention と同じ規律で bounded)、
(b) 部分ゲート禁止(全 clause 同時有効 — grindable な中間状態を作らない)、
(c) G12 e2e(正/否/境界)が ADR-0044 harness 上で green。

### D4. 経済拘束の固定

ADR-0040:700 を invariant に昇格する:
**`palw_compute_work_scale > 0`(weight)や非ゼロ高額報酬の有効化は、D1 の fraud 配線 +
bond slash 経路の e2e が green であることを前提条件とする。** ADR-0046(経済較正)は
この前提の下でのみ数値を凍結できる。

## Consequences

- SS-04 の分岐が消え、fraud 配線者の設計自由度が「非遡及 + slash」に固定される(良いこと:
  escrow/maturity の再設計論争を打ち切る)。
- PCPB は引き続き INERT。ただし「何が揃えば配線できるか」が測定可能な 3 条件になった。
- audit の multi-node withhold/reorg e2e は ADR-0044 harness の拡張として実装する
  (同一 harness 第 3 の用途)。
- security model 文書(qwen-8.0)とノード実装の間の参照が双方向に固定される。

## Definition of done

- [x] SS-04: `next(FraudEvidence)` を非遡及に修正(fenced、テスト付き)— 244deae、
  `palw_batch_referenceable` が `revoked_from_daa` 由来 bool のみを時間座標とし `Revoked` を
  非遡及ラベル化; test `revoked_batch_is_referenceable_non_retroactively`
- [x] 本 ADR の表(D2)を `docs/security-model.md` に反映(qwen-8.0 側)— "Consensus 側の三本柱"
  節を追加、Revoked 非遡及と「単一 Receipt は独立証明でない」の双方向参照を固定
- [x] beacon seed 履歴 store の設計メモ(D3-a)— node `docs/palw-beacon-seed-history-design.md`
  (bounded per-epoch `PalwBeaconSeedHistory`、writer=epoch 境界 commit、保持窓=lifecycle 全長、
  pruning snapshot carry、fail-closed 読み出し;ADR-0044 の version/dup 教訓を明記)
- [x] G13 withhold/reorg e2e を ADR-0044 harness に追加 — **withhold** 次元を実装
  (`palw_full_lifecycle_prune_then_replay_e2e`: quorum 不足 cert(1/3 票 < 2/3 stake、withholding は
  slate 全体を分母に算入)が acceptance の attestation gate で store に到達せず拒否される — live-devnet
  CertAbsent 根因の再現)。**reorg** 次元は権威座標(accepted-lifecycle/reward)の cross-fork 挙動が
  既存 `palw_algo4_sink_reorg_cross_fork_nullifier_replay_e2e` で pin 済みであり、v3 body-stage view の
  `cert_hash` は非権威(CERT-TRUST: `apply_certificate` は検証しない)ため本 harness の surface では
  健全に表現できない旨を test コメントに記録

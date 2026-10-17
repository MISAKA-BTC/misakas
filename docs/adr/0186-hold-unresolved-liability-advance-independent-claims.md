# ADR-0186 — 未解決 claim の責任を保持し、それに依存しない claim を進める

- **Decision:** Accepted — ユーザー指示 2026-10-17。
- **Scope:** RFC-0018 の共有 safe frontier と、それを読む決済・fork choice・pruning。
- **Implementation:** 段階 1・段階 2 とも休眠 fence(実装中)。
- **Related:** [RFC-0023](../rfc/0023-unresolved-claim-liability-and-independent-progress.md)、[RFC-0018](../rfc/0018-dag-parallel-multi-model-consensus.md)、[ADR-0181](0181-design-freeze-and-controlled-change.md)(設計凍結の手続き)。

## 決定

**遅い claim A を正しいことにして追い越さない。A の責任を未解決のまま確実に保持し、A に依存しない B だけを進める。**

- 段階 1(次回 fence の候補):現行の frontier 式を保ち、claim ごとの未解決期限 `受理 DAA + L_class` を受理時の資源・期限の整合で保証する。期限到達は不正の証明ではなく、係争中の担保・証拠を解放しない。B の各段階(Final・認証・保留解除・支払い出力・`settled`・native safe)を別々に測る。
- 段階 2(別の休眠 fence):未解決集合を担保・証拠付きで checkpoint に拘束し、依存の単位を claim とその依存関係にして、独立した claim の認証・決済・`settled` を進める。checkpoint・fork choice・settlement・native lifecycle・pruning/IBD を一緒に変える。

## 理由

DAG による並列生成と、個別の仕事・送金を独立に確定できることは別問題である。現行規則は安全だが、計算上独立な B・C まで共有の確定で A を待たせる。モデル別 frontier の最小・最大・多数決や時間経過による無視は、待ち続けるか責任を失うかのどちらかになる。

## 帰結

- 段階 1 は既存の bounded admission・detection policy・retention の規則の上に置く。
- 段階 2 は G14(未解決 A を外部の正直な検証者が後から反証できること)を checkpoint の要件に含める。
- fence 前の動作と testnet-12 の identity は変えない。

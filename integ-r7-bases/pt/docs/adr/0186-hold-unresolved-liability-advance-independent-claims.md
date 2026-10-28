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

## 追記(2026-10-18):段階 1 の測定と RFC-0018 (ii)(iv) の変更

ADR-0181 の設計凍結に従い、設計の変更をここに記録する。値はどれも変えていない。

- **段階 1 の測定。** 6 つの原因(資料応答の遅延、正当な court、虚偽告発、再抽選、court 容量の飽和、遅延 claim の継続投入)ごとに定常遅れを測った。どれも min(保持時間, L) + 投入間隔 + 認証時間 以内だった(G14 完全性証明 §13.8)。node の restart と genesis からの replay は、fence をまたいで resolved prefix まで一致した。
- **RFC-0018 (ii):mergeset の class 順(休眠の `palw_dag_sibling_attribution_v1` の中)。** class を最初の member の priority で並べると、attempt の多い model ほど先頭に来る。16 carve の予算の下では、1 attempt の model が公平な取り分の 13 分の 1 しか入らなかった。class の順を `H(domain ‖ set seed ‖ class)` に変えた(set seed は mergeset の attempt priority の hash)。SET の関数である性質、round robin、class 内の順、16 carve は変えていない(§13.9)。
- **RFC-0018 (iv):Panel の own receipt の class lane(node policy、consensus の変更なし)。** 1 本の in-flight carrier では、court の queue が空にならない間、OwnReceipts の site に slot が回らない。own receipt は class 数に関係なく窓を外した。V07 の readiness lane と同じ型の lane を class ごとに置いた。own receipt は、main slot が取れないときだけ、その class の lane(自分の確認済み output、1 本ずつ in flight)に乗る。lane は 1 seat あたり 16 本まで。他の chain の carrier の子は作らないので、relay の性質は V07 と同じである(§13.10)。

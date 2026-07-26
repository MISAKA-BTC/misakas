# ADR-0047 — ModelGenesisManifest の二者独立再現: 手順の確定

- **Status:** Accepted(手順と受け入れ基準の確定。実施は外部依存 — 第三者・RTX box・公式重み)
- **Date:** 2026-07-26
- **Consumes:** qwen-8.0 `docs/model-genesis-candidate.md`(4 disqualifiers と flip path a–d)、
  ADR-0041(ExternalModelGenesis は mint 側ゲート)

## Context

mint の `model.official_genesis` ゲート(ExternalModelGenesis)は開いていない。
qwen-8.0 `docs/model-genesis-candidate.md` が現候補の 4 つの失格事由を verbatim 記録している:
(1) provenance が公式重みに到達しない(第三者 "abliterated" 派生で終端)、
(2) 変換が誰にも再現されていない(sha256 一致の第二者なし)、
(3) 署名付き manifest が無い、
(4) 35B float は committed compute-set ではない(receipt は整数正準 0.5B 較正セットに束縛)。

「二者独立再現」はこのうち (2) を直接、(3) を付随的に閉じる作業である。
(1) は重み取得経路の問題、(4) は compute-set の付け替えで、別軸。

## Decision

1. **再現プロトコル(確定):**
   - 二者 = **Mac(Metal、本機)** と **RTX box(CUDA、`tadas`@Tailscale)**。同一人物の
     二台は「二者」の弱形であることを honest に記録する(強形は組織的に独立な第三者;
     staging 公開後に公募する)。
   - 入力: 公式重み(provenance 連鎖が Alibaba 公式 checkpoint に到達するもの)のみ。
     現候補(abliterated 派生)は**再現しても genesis に昇格しない**(disqualifier (1) が残るため)。
   - 各者は独立に: 重み取得 → sha256 記録 → 整数正準変換(committed 手順)→
     `ModelGenesisManifest` 構築 → keyed-BLAKE2b(palw-k1)ハッシュ → ML-DSA 署名。
   - **受け入れ = manifest ハッシュの bit-exact 一致 + 双方の署名 + 変換ログの突合。**
     不一致時は diff を evidence 化してから原因(非決定性 or 手順差)を潰す —
     proof-of-llm verifier の Metal/CUDA 非対称(f151cd16→60a662e2)の教訓により、
     **backend 非依存の manifest**(重み由来のみ、実行トレース非含有)とする。
2. **成果物の置き場:** qwen-8.0 `docs/evidence/model-genesis-repro-<date>/`(両者のログ・ハッシュ・署名)
   + `docs/model-genesis-candidate.md` の disqualifier 表の更新。
3. **順序:** 公式重みの取得(disqualifier (1) の解消)が先行条件。それまで本 ADR は
   「手順確定済み・実施待ち」で停止する。実施自体はネットワーク外の作業なので
   staging-mainnet(ADR-0048)をブロックしない(mint ゲートは staging でも閉のまま)。

## Consequences

- 実施は外部依存 3 点(公式重みへの provenance / RTX box 稼働 / 望ましくは第三者)。
  in-session では閉じない。
- disqualifier (4)(35B vs 0.5B compute-set)は本 ADR の範囲外 — Receipt v3 / compute-set
  付け替えの track(qwen-8.0 側)で扱う。

## Definition of done

- [x] 再現プロトコルと受け入れ基準の確定(本 ADR)
- [ ] 公式重み provenance の確保(外部)
- [ ] Mac / RTX の独立再現 → manifest ハッシュ一致 + 双方署名(外部実行)
- [ ] evidence 格納 + candidate 文書の disqualifier 更新

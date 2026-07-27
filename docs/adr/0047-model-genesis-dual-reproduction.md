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

## Prerequisites (in-repo) — 2026-07-27 追記

本 ADR は「手順確定済み・実施待ち」だったが、**実施しても成果を突き合わせられない**状態だった:
§1 が要求する `ModelGenesisManifest` が両リポジトリに 1 件も存在せず(`.rs`/`.py`/`.sh` 全 grep で 0 hit)、
二者が各自変換を走らせても比較対象が無かった。受け入れ基準「manifest ハッシュの bit-exact 一致」が
**評価不能**だったということで、これは外部依存ではなく in-repo の欠落である。ここを閉じた。

### 実装済み(qwen-8.0 側)

| 前提条件 | 場所 | 内容 |
|---|---|---|
| `ModelGenesisManifest` 型・正準符号化・keyed-BLAKE2b-512 ハッシュ | `runtime-palw/src/model_genesis.rs` | §1 の比較対象そのもの。backend 非依存・重み由来のみ。整数は big-endian、可変長は全て `u64` 長前置(prefix-free ⇒ 単射)。ハッシュ domain は `misaka-palw-v1/model-genesis` で `palw-k1/file` および `misaka-palw-v3/*` の全 domain と分離 |
| フィールド差分 | `ModelGenesisManifest::first_mismatch` | §1 の不一致時 diff 要求。ハッシュ比較だけでは「どこが違うか」が出ないため、dotted field 名を返す |
| backend 非依存の機械的強制 | `ModelGenesisManifest::validate` | 変換手順に backend/host トークン(`metal`/`cuda`/`gpu`/`ngl`/`thread`/`arm64` …)が現れたら reject。絶対パス・`..` を含むパス(操作者の home が digest に混入する)、非 ASCII、可変 revision pin(branch 名は再現不能)も reject。proof-of-llm verifier の Metal/CUDA 非対称(f151cd16→60a662e2)が根拠 |
| 正直な達成度評価 | `model_genesis::assess_genesis` | 失格事由 4 件をデータ化。`eligible` は構造上 true になり得ない(`ManifestUnsigned` は無条件)ことを unit test で固定。同一操作者の 2 台は `TwoPartyForm::Weak` として §1 の弱形を明示的に記録 |
| 決定的変換 + manifest スクリプト | `scripts/model_genesis_manifest.py`(`selftest`/`convert`/`manifest`/`verify`) | 二者が走らせる同一手順。変換環境を固定(`LC_ALL=C`, `TZ=UTC`, `PYTHONHASHSEED=0`, `SOURCE_DATE_EPOCH=0`)し `--model-name` を必須化 — 未指定だと converter が `general.name` をローカルディレクトリ名から導出し、重みと無関係な理由で不一致になるため |
| Rust↔Python parity | golden vector `b747790722bdf626…f756d9b3` を両実装に pin | 符号化器が乖離すると二者間不一致の原因が「重み」か「道具」か切り分け不能になる。`selftest` と `scripts/tests/test_model_genesis_manifest.py` で束縛 |

fail-closed は現物で確認済み: `models/Qwen3.6-35B-A3B-Claude-4.7-base-meta/` は pin された
メタデータ 7 ファイルのみで safetensors shard を持たず、`llama-quantize` も
`config/runtime-pins.sh` の `PALW_BUILD_TARGETS` に含まれない。`convert`/`manifest` は
どちらも**欠けているものを名指しして exit 1** し、manifest を出力しない。

### 署名: LOCKED テーブルにより実装不可(人間の unlock 判断が必要)

§1 の「ML-DSA 署名」は**コードでは閉じられない**。`consensus/core/src/signature_domains.rs` は
2026-07-25 に LOCKED、golden test が全行を二重化しており、行追加は明示的な人間の判断である。
したがって `model_genesis.rs` には署名関数を一切置いていない。必要になる行は次の 1 行:

```rust
SignatureDomain {
    object: "PALW model genesis manifest",
    context: crate::palw::PALW_MODEL_GENESIS_V1_MLDSA87_CONTEXT, // b"misaka-palw-v1/model-genesis/mldsa87"
    defined_in: "misaka_palw::model_genesis::ModelGenesisManifest::manifest_hash",
},
```

同一 commit で更新が必要になるテスト(この 3 点が lock の審査面):

1. `signature_domain_table_is_locked` — golden 行の追加。
2. `palw_naming_divergence_is_pinned_not_forgotten` — PALW 行を 15 と固定し、allow-list 外の
   PALW 行に `/` が無いことを表明している。slash 規約の新規行は allow-list に明示追加が必要
   (= 命名規約を惰性で継承せず明示的に決める)。
3. `signature_domains_are_prefix_free` — 提案 context は `misaka-palw-v3/receipt/mldsa87` /
   `misaka-palw-v3/jobspec/mldsa87` と相異かつ prefix 関係に無い(qwen-8.0 側
   `model_genesis::tests::proposed_context_is_distinct_and_prefix_free` で事前確認済み)。

qwen-8.0 側の定数名は `PALW_MODEL_GENESIS_V1_MLDSA87_CONTEXT_UNREGISTERED`(unlock 前に
黙って使えない名前)。署名 context と keyed-hash domain は別 namespace(同ファイルの module note)
なので、上記ハッシュ domain はテーブルを必要とせず、この判断を先取りもしていない。

### 依然として外部(本追記で 1 つも閉じていない)

- **公式重み provenance**: pin は今も `huihui-ai/Huihui-Qwen3.6-35B-A3B-Claude-4.7-Opus-abliterated`
  = **第三者 abliterated 派生**で終端する。manifest は provenance tier を**ハッシュ対象の中に**
  持つため、一方が "official"、他方が "derivative" と記録して一致することはあり得ない。
  道具は派生物を「再現可能」にはできるが「公式」にはできない。
- **Mac / RTX の独立再現**: 未実施。上記公式重み・RTX box 稼働・(望ましくは)組織的に独立な
  第二者が要る。
- **evidence 格納 + candidate 文書更新**: 上記の下流。

DoD のチェックボックスは 3 つとも未達のまま(本追記では 1 つも tick していない)。
mint 側 `model.official_genesis`(`MainnetBlocker::ExternalModelGenesis`)も閉じていない。

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

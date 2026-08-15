# PALW full-logits trace scheme v2 — 設計根拠・安全条件・段階導入

Status: **EXPERIMENTAL / devnet・shadow mode・zero-credit のみ GO**
Date: 2026-08-12
Relates to: ADR-0007, ADR-0008, ADR-0021, ADR-0024, `misaka-palw-worker`

Companion documents:

- [`MISAKA PALW VPS Canonical Worker 経路設計書 v0.1`](misaka-palw-vps-canonical-worker-design-v0.1-ja.md)
- [`MISAKA PALW側PoW 詳細設計書 v0.1`](misaka-palw-pow-detailed-design-v0.1-ja.md)
- [`MISAKA PALW セキュアOTA設計書 v0.1`](misaka-palw-ota-secure-update-design-v0.1-ja.md)

> [!IMPORTANT]
> 本書の `v2` は **PALW内部のtrace scheme version** であり、block headerの
> `pow_algo_id = 2` を意味しない。header-levelのalgo 2はhistorical Argon2idとして
> 認識されるため再利用しない。数値IDのnamespaceを混同してはならない。

## 1. 判定と適用範囲

full logitsへ束縛する方向で実装と検証を進めてよい。ただし、現在許可するのは
`ShadowSidecar`、`LocalReceiptOnly`、`ConsensusVisibleZeroCredit`までとする。

| 段階 | 判定 | 条件・理由 |
|---|---:|---|
| worker実装と異機種再現試験 | **GO** | 出力文字列ではなくfull logitsをcommitする |
| VLT overlay試験運用 | **条件付きGO** | portable x86 classの候補が2台・3 vectorsで成立した |
| consensus-visible / 報酬0 | **GO** | 実ネットワーク条件で不一致、停止、容量を観測する |
| 専用testnetの低credit cap | **NO-GO（gate待ち）** | 本書§12のactivation gateをすべて満たすこと |
| PALW報酬・work加算 | **NO-GO** | commitmentの受理は監査経済と委員会安全性に依存する |
| mainnet・fork choiceへの影響 | **NO-GO** | 外部監査、長期soak、攻撃試験、経済評価が未完了 |

設計原則は次のとおり。

1. 永久hash floorを残す。
2. PALW障害時は `PALW credit = 0` とし、hash ordering/livenessを継続する。
3. zero-creditの観測を経ずに報酬、work、fork-choice weightを与えない。
4. fail-openでPALW creditを与えない。runtime不明・再現不能・委員会不足はzero-creditとする。

## 2. 識別子と上位アーキテクチャ

### 2.1 ID namespaceを分離する

採用する識別は次のとおり。

```text
block header pow_algo_id       = 既存ADRが定義する値（2を再利用しない）
palw_trace_scheme_name         = "misaka-palw/full-logits-trace/v2"
palw_trace_scheme_id           = Hash64(palw_trace_scheme_name)
runtime_manifest_version       = 2
trace_commitment_version       = 2
```

`algo_id = 2` という略記は廃止する。実装中に数値versionが必要な場合も、
`pow_algo_id` とは別の型・別のfield・別のnamespaceに置く。型変換による混入を許さない。

### 2.2 推奨ブロック条件

PALWを単独PoWにはしない。上位条件は次を推奨する。

```text
Valid block = valid permanent hash PoW
              AND (PALW certificate is absent
                   OR PALW certificate is valid under its activation stage)
```

shadow/zero-credit段階ではPALW certificateはfork choice、blue work、DAA、block levelへ
影響しない。PALW固有のcreditを導入する場合は、別ADRと明示的activationが必要である。

## 3. 旧Ollama方式を廃止した測定根拠

以下は期待値ではなく実測値である。

| measurement | result |
|---|---|
| Ollama greedy continuation、uniform-random 10 seeds、出荷値N=16 | **1 distinct** — 定数 |
| `POW_L1_PALW_OLLAMA_CALIBRATION_V1`の先頭64 bytes | 定数文字列のBLAKE2b-512と一致 |
| seed由来24-word prompt、60 seeds、N=16 | 40 distinct、`p_max=0.117`、min-entropy約3.1 bits、top-5が38% |
| 同prompt、N=32 | min-entropy約3.3 bits — budget増加では解決しない |
| Ollama 0.32.8 `/v1/completions`の`logprobs` | absent/null — API経由でlogitsへ束縛できない |

出力文字列だけのcommitmentは、その文字列の実効entropyを超える仕事を証明しない。
512-bit hashは約3 bitsの入力entropyを増やさない。このためprompt修正やdecode予算増加ではなく、
commit対象をruntime内部のfull logits sequenceへ変更する。

## 4. commitmentの意味と安全性の限界

### 4.1 `Receipt != Proof`

本方式を `cryptographic proof of work` とは呼ばない。正確な分類は
**Proof of Audited Compute** または **replay-verified work** である。

workerは計算せず任意の64-byte rootを申告できる。公開固定keyを使うkeyed-BLAKE2bは
domain separationであり、秘密鍵MACではない。したがってroot単体は計算実行の証明ではない。

> workerは任意のrootを申告できる。ただし、誠実なverifierによるcanonical replayを
> 通過するには、canonical logits列を再現するか、監査プロセスを回避・支配しなければならない。

受理安全性は次の組合せに依存する。

- canonical full replay
- 独立したhonest verifierの存在
- committee selectionとclass membership gate
- bond、slashing、challenge window、reward maturity
- no-showと誤署名への罰則
- 監査・challengeの検閲耐性

「512-bit rootだから安全」「rootを得るには必ずモデルを実行する」という説明は禁止する。
未知のshortcutまたは合法cache最適化が見つかった場合、creditは観測された最低攻撃コストまで
下げ、必要なら即時zero-creditへ戻す。

### 4.2 commit対象

現schemeがcommitするのは各decode call後の**最終full logits vector**であり、各GEMMの
intermediate、layer activation、operation scheduleそのものではない。そのため規範名を
`full_logits_sequence_root`（field名は `decode_logits_trace_root`）とする。

既存コードの `gemm_trace_root` は移行用aliasに限定し、新規wire/documentationでは使用しない。
wire互換が必要ならrenameはversion境界で行う。

## 5. 現在までの実測と、その主張可能範囲

`misaka-palw-worker`は各decode call後のfull logits vector
（測定時 `n_vocab = 248_320`、f32）からordered event commitmentを構成する。旧Ollama方式が
定数化したdecode=16相当条件で、固定2B GGUFの入力感応性を確認した。

```text
seed 00112233 prefill=68 decode=16  out=c306e02f779de0a6d70c  root=ecb8dee6a03008d5db14
seed ffeeddcc prefill=69 decode=16  out=8208ff8e5f9522c2872e  root=d4ecd6f507932afca384
seed a1b2c3d4 prefill=81 decode=16  out=6011beed44d86b060682  root=3695392f91729bdaebb2
```

また、h1（AMD EPYC 6c）とh2（Intel Broadwell 8c、F16C masked）で次を確認した。

- llama.cpp commit: `030ebb558`
- `GGML_NATIVE=OFF`
- `GGML_METAL=OFF`
- `GGML_BLAS=OFF`
- `GGML_ACCELERATE=OFF`
- `GGML_OPENMP=OFF`
- `MISAKA_PALW_CPU=1`
- 同一GGUF（SHA-256 prefix `aaf42c8b…`、両hostで照合）
- 同一promptに対するroot prefix一致: 3/3
- 異なる3 promptに対するroot prefix: 3 distinct

この結果が証明したのは、**この2台、このbuild/profile、このGGUF、この3 promptで、
入力感応性とcross-machine一致を同時に観測した**ことだけである。

これはportable x86 determinism classの有力候補を示すsmoke testだが、heterogeneous x86
全体、任意prompt、長期運用、mainnet安全性を証明しない。掲載値は表示用prefixであり、
試験assertとgolden vectorは必ずfull 64-byte（128 hex chars）を使う。

> [!CAUTION]
> 現在の本文には3 vectorsの短縮prefixしか掲載されていない。元のfull root、入力bytes、
> manifest、worker stdout/stderr、各hostのartifact hashを改変不能な測定artifactとして保存し、
> full 64-byte比較を再実行するまで、この3件をactivation用golden vectorとして扱わない。

## 6. TraceCommitmentV2

trace objectは外側receiptに依存せず、job、network、runtime、token budgetへ自己完結して
bindする。可変長値はcanonical length-prefix encodingを使い、文字列連結を使わない。

> [!IMPORTANT]
> **正規実装とセキュリティ訂正(2026-08-13)。** 本節のpreimage規範は
> `consensus/core/src/palw_v2.rs` であり、レイアウトは同モジュールのgolden vector
> テストで凍結されている。初版の本節には次の3点の欠陥があり、詳細設計書§10・
> VPS設計書§7と実装に合わせて訂正した。
>
> 1. **domain keyの表記衝突** — 初版は `misaka/palw/…`、詳細設計§10.5とVPS設計§7.3は
>    `misaka-palw/…` を使っていた。2実装が別文書に従うと同一jobで異なるrootを計算し、
>    honest-refutes-honestの恒常forkになる。**`misaka-palw/…` に統一する。**
> 2. **event hashがjob contextへ束縛されていなかった** — 初版のevent式にはjob束縛が
>    なく、event粒度で別job・別network・別runtimeのtraceへ流用できた。全eventは
>    `job_context_hash` を先頭へ束縛する(詳細設計§10.5と同じ、より強い形)。
> 3. **ordered_event_hashesの直接連結** — 将来のinterval challenge(TraceVM)がevent
>    単位の開示証明を要求するため、ordered listはdomain分離Merkle root(leaf/nodeを
>    別keyで分離、奇数nodeは複製せずpromote、event_countは外側で束縛)としてcommitし、
>    そのrootを外側keyed hashへ束縛する。初版が根に直接並べていた識別field群は
>    `job_context_hash` 経由ですべて束縛が維持される。

```text
TraceCommitmentV2
├─ version
├─ network_id
├─ job_nullifier
├─ execution_seed
├─ model_profile_id
├─ runtime_manifest_hash
├─ runtime_class_id
├─ tokenizer_id
├─ prompt_token_ids_hash
├─ declared_prefill_tokens
├─ exact_decode_tokens
├─ max_context_tokens
├─ vocab_size
├─ logits_dtype
├─ event_count
├─ ordered_full_logits_event_hashes
└─ full_logits_sequence_root: [u8; 64]
```

概念式は次のとおり。

```text
job_context_hash = BLAKE2b-512(
    key = "misaka-palw/job-context/v2",
    input = canonical_encode(
        version || network_id || job_id || job_nullifier || assignment_id
        || execution_seed || model_profile_id || runtime_manifest_hash
        || runtime_class_id || shape_profile_id || trace_scheme_id || cu_ruleset_id
        || tokenizer_id || prompt_token_ids_hash
        || declared_prefill_tokens || exact_decode_tokens || max_context_tokens
    )
)

event_hash_i = BLAKE2b-512(
    key = "misaka-palw/full-logits-event/v2",
    input = canonical_encode(
        job_context_hash || phase || phase_step || n_vocab || logits_dtype
        || logits_count || canonical_logits_bytes
    )
)

ordered_event_commitment = MerkleRoot(
    leaf_i = BLAKE2b-512(key = "misaka-palw/trace-merkle-leaf/v2", index_i || event_hash_i),
    node   = BLAKE2b-512(key = "misaka-palw/trace-merkle-node/v2", left || right),
    奇数nodeは複製せずpromote(duplicate-leaf ambiguityの排除)
)

full_logits_sequence_root = BLAKE2b-512(
    key = "misaka-palw/full-logits-trace/v2",
    input = canonical_encode(
        version || job_context_hash || vocab_size || logits_dtype
        || declared_prefill_tokens || exact_decode_tokens
        || event_count || first_event_kind || last_event_kind
        || ordered_event_commitment || output_token_ids_hash || stop_reason
    )
)
```

eventの削除、重複、並べ替え、phase解釈差、vocab長差、別job・別network・別runtimeからの
root再利用は、root不一致として検出されなければならない。

## 7. RuntimeManifestV2とdeterminism class

`GGML_NATIVE=OFF`だけでは十分でない。class membershipは自己申告CPU名ではなく、exact artifactと
固定実行条件で定義する。

```text
RuntimeManifestV2
├─ target_arch / target_triple
├─ compiler_name / compiler_version
├─ linker_version
├─ cmake_cache_sha256
├─ worker_binary_sha256
├─ llama_static_library_sha256
├─ llama_cpp_commit
├─ patchset_root
├─ exact_cpu_isa_baseline
├─ runtime_cpu_feature_mask
├─ GGML_NATIVE / GGML_OPENMP / GGML_BLAS / GGML_ACCELERATE
├─ GGML_SSE42 / GGML_AVX / GGML_AVX2 / GGML_FMA / GGML_F16C
├─ GGML_CPU_ALL_VARIANTS
├─ thread_count / thread_affinity_policy
├─ floating_point_environment
├─ GGUF_sha256
├─ tokenizer_sha256
├─ prompt_template_sha256
├─ trace_scheme_id
└─ golden_vector_root
```

可能な限り各machineで個別compileせず、bit-identical static worker artifactを配布する。
当面のclass定義は次より広げない。

```text
x86_64-linux
+ exact worker/llama artifacts
+ fixed ISA baseline and runtime feature mask
+ OpenMP disabled
+ fixed thread count and affinity policy
+ exact GGUF/tokenizer/template
+ fixed floating-point environment
```

「EPYCとBroadwellは同一class」と断定せず、「両者を含められるportable x86 class候補が
3 vectorsで成立した」と表現する。F16C maskingがbuild optionかruntime CPUID maskか、EPYC側と
同じkernel pathかはmanifestと試験logに明記する。

## 8. 浮動小数点のcanonical policy

起動時に次を検証し、manifestと異なる場合はPALW execution/verifierを開始しない。

```text
rounding_mode = round-to-nearest-ties-to-even
fast_math = false
fp_contract = off
FTZ = manifestで固定
DAZ = manifestで固定
NaN / +/-Inf = execution invalid（fail closed）
finite f32 = IEEE-754 little-endian bytesをそのままcommit
signed zero = bitを保持（FTZ/DAZとともに固定）
```

buildでは `-ffast-math` を禁止し、FMA contractionを無効にする。正常有限値を丸めたり
量子化して「canonicalize」してはならない。low-bit bindingを弱める変更は新scheme versionと
再activationを要求する。

## 9. TokenBudgetV2とseed導出

workerの旧 `--n-predict` はprefillを含むtotal budgetであり、Ollamaのdecode-only設定と意味が
異なる。曖昧な名称をconsensus境界から除去する。

```text
PalwTokenBudgetV2
├─ declared_prefill_tokens: u32
├─ exact_decode_tokens: u32
├─ max_context_tokens: u32
├─ tokenizer_profile_id: Hash64
├─ add_bos: bool
└─ add_eos: bool
```

検証条件:

```text
actual_prefill_tokens == declared_prefill_tokens
actual_prefill_tokens + exact_decode_tokens <= max_context_tokens
generated_decode_tokens == exact_decode_tokens
```

CLI/APIは `--max-total-tokens`、`--exact-decode-tokens`、
`--declared-prefill-tokens` を分離する。固定decode量に対してearly EOGとなったjobは
**invalidまたはzero-credit**とし、短時間seed grindingへcreditを与えない。

seedをworker/minerに選ばせない。job commit後に判明するfuture chain randomnessを使う。

```text
execution_seed = H(
    committed_job_id || future_chain_randomness || model_profile_id
    || runtime_class_id || epoch
)
```

future randomnessの確定時点、reorg時の扱い、job commit deadline、domain separationは別ADRで
凍結する。固定prefixのKV cache再利用は強制的に禁止できないため、seedをprompt前方へ配置し、
seed-derived部分を増やし、最速の合法cache実装をdifficulty/credit calibrationの基準にする。

## 10. 委員会・監査・経済上の注意

同一classの自己申告だけでcommitteeへ参加させない。class登録には最低限次を要求する。

1. exact RuntimeManifestV2の登録
2. 公開full-length golden vectorsへの合格
3. unpredictable random challengeへの合格
4. bond lock
5. no-show slash
6. 誤rootへの署名slash
7. operator credential単位の重複排除
8. executorのself-verification除外とnon-replacement sampling

committee 5 / confirmations 3を採る場合、executorとは独立した十分なcredential数をactivation
条件とする。eligible independent credentialsがthresholdを満たさないときcommitteeを縮小して
mintしてはならず、PALW creditを0にする。stake分割Sybilへoperator aggregationとconcentration capを
適用する。

full replayはprimary executionと同程度のコストを持つ。committee 5が全件replayするなら、概ね
1 jobにつきprimary 1回 + verifier 5回である。「軽い検証」と表現しない。

報酬導入前に次の不等式を保守的仮定で満たす必要がある。

```text
最大不正利益
< 監査される確率 × slash額
  + 失う未成熟報酬
  + 手数料
  + 将来参加権の喪失価値
```

## 11. 必須試験

### 11.1 entropy・実行コスト試験

最低10,000件のchain-derived seedで次を測る。

- full 64-byte rootのdistinct数、collision、最大出現頻度 `p_max`
- min-entropy推定値と信頼区間
- prefill/decode event count分布とearly EOG率
- 実行時間のp1 / p50 / p99
- 最短実行seedと低頻度attractorの有無
- prefix/KV cacheを使う最速合法実装のコスト

### 11.2 cross-machine determinism corpus

最低1,000 canonical promptsを各machineで5回以上実行し、full 64-byte equalityをassertする。
corpusには次を含める。

- prompt長の境界値、1 token、最大prefill付近
- ASCII、日本語、絵文字、結合文字、Unicode normalization差
- binary inputを許す場合のNUL相当とlength-prefix境界
- early EOGを誘発する入力
- cold/warm cache、process再起動後
- 同時実行1/2/5/10 process
- CPU affinity変更、メモリ圧迫
- 少なくとも3 CPU microarchitectures

### 11.3 negative controls

次を1項目ずつ意図的に変更し、rootまたはruntime class IDが必ず変わることを確認する。

- OpenMP ON
- thread count / affinity変更
- FMA、F16C、FTZ、DAZの変更
- fast-math ON
- GGUFの1 bit変更
- tokenizer、BOS/EOS、prompt normalizationの変更
- decode数、vocab size、dtypeの変更
- runtime binary変更
- event 1件の削除、重複、順序入れ替え

### 11.4 容量・障害試験

次をclassごとにp50 / p95 / p99で記録する。

- primary/replay実行時間とroot hashing overhead
- job/sec/machine、5 concurrent replay時の遅延
- committee response、no-show率、challenge window内capacity
- CPU時間・電力・verifier報酬・必要slash額
- runtime crash、timeout、OOM、manifest mismatch、NaN/Inf時のfail-closed挙動

## 12. ClassActivationGate

低credit testnetへ進む前に、すべて満たすこと。

```text
[ ] at least 3 CPU microarchitectures tested
[ ] at least 1,000 canonical prompts × 5 reruns/machine
[ ] full 64-byte equality; prefix比較を判定に使用していない
[ ] cold/warm/restart/concurrent/affinity/memory-pressure tests passed
[ ] at least 10,000 chain-derived seedsのentropy/cost report completed
[ ] negative controls produce mismatch or a different class ID
[ ] exact artifacts and floating-point environment are launch-verified
[ ] minimum independent bonded credentials are continuously available
[ ] measured replay capacity fits the challenge window at p99
[ ] sustained zero-mismatch shadow/zero-credit soak period completed
[ ] adversarial test and external review completed
[ ] emergency zero-credit rollback exercised
```

一つでも未達ならPALW reward、work、fork-choice weightを有効化しない。

## 13. 段階導入

導入順を固定する。

1. `ShadowSidecar`
2. `LocalReceiptOnly`
3. `ConsensusVisibleZeroCredit`
4. 専用testnetで低いcredit cap（§12通過後）
5. adversarial test / fault injection / economic simulation
6. 外部監査後の限定報酬
7. mainnet判断は別ADR、別activation、緊急rollback実証後

各段階の昇格は自動化せず、測定artifact、full golden vectors、mismatch/no-show統計、runtime
manifest、経済パラメータを添付した明示的decision recordで行う。

## 14. 実装中に発見・修正済みの事項

- Linux buildでAppleの`-lc++`を無条件linkしていた問題を修正した（`4be9cad`）。
- Linuxではggml-cpuがOpenMPを取り込む差を発見し、`CPU_BUILD_PROFILE`へ
  no-OpenMPを追加した（`2825d99`）。
- cross-machine smoke testと入力感応性の測定根拠を記録した（`4101713`）。

**2026-08-13 Land段階実装（consensus挙動不変、devnet/shadow/zero-credit範囲）:**

- v2コア型・domain・preimage layoutを `consensus/core/src/palw_v2.rs` に実装し、
  golden vectorテストで凍結した。§6の3点のセキュリティ訂正はこの実装で確定した。
  header pipeline・fork choice・emissionからは一切参照されない（既存の
  consensus fingerprint pinテストが不変のままパスすることで確認）。
- `palw-worker` へ `--mode v2-job` / `--mode v2-manifest` を実装した:
  - framed Borsh `PalwJobEnvelopeV2`（u32-le長prefix、256 KiB上限、途中/末尾の余剰バイト拒否）
  - token-ID入力（workerはこの経路でtokenize・normalize・template適用をしない）
  - prefill/decode予算の分離検証（checked arithmetic、`max_context_tokens`のprofile一致必須、
    `--n-predict`はv2経路で拒否）
  - exact decode: early EOGはtelemetryのみで、停止しない
  - non-finite logitsはevent受理前に検出しfail closed（stdout無出力+非零exit、部分結果なし）
  - FP環境検証（MXCSR/FPCR: rounding=RNE, FTZ=0, DAZ=0）を起動時とモデルロード後の2回実施
  - モデルSHA-256は毎回全読で再計算（(path,size,mtime) cacheを信頼しない — VPS設計§4.4）
  - envelope宣言の6 identity（model / runtime-manifest / class / shape / trace-scheme / cu）と
    自己導出値の完全一致を実行前に要求
  - `RuntimeManifestV2` はbuild.rsが実測したCMakeCache SHA-256・静的library結合SHA-256・
    GGML_*フラグ（CMakeCacheの実値）と、実行時の自バイナリlive hashから構成。未検証fieldは
    literal `"unpinned"` として可視化し、class登録時の必須拒否対象とする
- closed-replay smoke harness `scripts/misaka-palw-v2-worker-smoke.py`: 実モデルで
  Execute×2+Replay×1のprojection byte一致、seed/prompt入力感応性、fail-closed 10 probe
  （改竄manifest・予算違反・不正frame・vocab範囲外token・`--n-predict`混入等）を確認した。
- **golden vector登録とboot self-test**（`v2-golden-gen` / `v2-selftest` /
  `MISAKA_PALW_GOLDEN`）: manifest hashとの循環はgolden job contextの
  sentinel manifest hash（全zero）で解決し、set headerがclass/model/shapeを束縛
  （別classのsetはロード拒否 — CPU buildがMetal setを拒否することを実証）。登録すると
  manifest hashが変わるため、gate付きruntimeとgate無しruntimeはhashレベルで別物になる。
  ローカル2 profile（aarch64 Metal / aarch64 CPU）のdev goldenを生成・全PASS。
  ただしこれは**開発機のgolden**であり、x86 fleet classのgoldenはfleet実機で
  同じtoolingにより生成する（§5のCAUTIONは未解消のまま）。
- **`palw-agent` Phase A**（`misaka-palw-agent`）: boot golden gate（selftest失敗で
  QUARANTINED=全job拒否で稼働継続、golden未登録は起動拒否）、model load前のadmission、
  per-job supervision（deadline/timeout kill、部分結果破棄）、応答の再束縛検証
  （request hash echo・CU再導出・job_context_hash独立再計算）。
  smoke: `scripts/misaka-palw-v2-agent-smoke.py`。

これらは必要な前進だが、§12のgateを満たしたことを意味しない。特にgolden vector
（full 64-byte）の正式登録、3 microarchitectures×1,000 prompts×5回、10,000 seedsの
entropy/cost試験、`"unpinned"` fieldの解消、独立bonded credential条件は未達のまま残る。

## 15. 禁止する安全性表現

レビュー、README、運用資料で次を使用しない。

- 「trace rootは計算を暗号学的に証明する」
- 「512-bitなので偽造不能」
- 「EPYCとBroadwellは一般に同じclass」
- 「heterogeneous x86 fleetで決定性が証明済み」
- 「full replayは軽い」
- 「promptが短いのでtoken budgetは固定できる」
- 「cacheを禁止すれば同じ計算量を強制できる」
- 「1 tokenの検証だから1 token分のFLOPsで済む」（ADR-0026 §4: KV cacheを持たないfresh
  verifierは challenged positionまでのprefillを払う。replay costは必ず cold/no-KV で
  class毎にp99実測する）
- 「1点challengeで十分」（ADR-0026 §5: `P_detect = 1-(1-f)^q`。局所的不正では
  `q` を bond/期待利得から動的に決める）

許容する表現:

> 固定されたportable CPU profileの2台・3 vectorsで、入力感応性とfull-logits rootの
> cross-machine一致を観測した。rootはaudited commitmentであり、その受理安全性はcanonical
> replay、独立committee、bond/slashing、challenge window、reward maturityに依存する。

## 16. CapabilityDeclarationV2 — v2 capability宣言（2026-08-13追加）

v2 runtimeのconsensus可視化はcapability宣言から始める。設計はv1
（`ComputeCapabilityPayload` / `misaka-vlt-v1/compute-capability`）が実運用で確立した
lifecycleを踏襲し、identityブロックだけをv2の完全な識別へ置き換える。正規実装は
`consensus/core/src/palw_v2.rs`（golden vectorテストでpreimage凍結）。

### 16.1 Object

```rust
PalwCapabilityDeclarationV2 {
    version: u16,                       // = 2
    validator_id: Hash64,               // bondのvalidator_pubkey_hashと一致必須
    bond_outpoint: TransactionOutpoint, // slashable claimの担保
    model_profile_id: Hash64,
    runtime_manifest_hash: Hash64,      // golden_vector_rootをpreimageに含む
    runtime_class_id: Hash64,
    shape_profile_id: Hash64,
    trace_scheme_id: Hash64,
    cu_ruleset_id: Hash64,
    expiry_daa_score: u64,
    signature: Vec<u8>,                 // ML-DSA-87
}
```

設計判断:

- **network_idはpayloadに持たず、署名メッセージにのみ束縛する**（v1と同じ。payloadは
  network-boundなtransactionに乗る）。
- **golden_vector_rootをpayloadに持たない。** `runtime_manifest_hash` のpreimageが既に
  束縛しており、別fieldで運ぶと「宣言されたrootとmanifest内のrootが食い違う」という
  検証不能な表面を作るだけである。ungated runtimeの排除は2層で行う:
  ローカルは宣言gate（§16.3）がagent healthのsentinel検査で拒否し、
  ネットワーク（Accept段階）はv2 registryの行がmanifest hashをexactにpinし、
  registry登録ceremonyがreal golden rootを要求する。
- **宣言はhardware自己申告を一切運ばない。** 運ぶのはpinned identity hashだけである。

### 16.2 署名メッセージ

```text
palw_capability_message_v2 = BLAKE2b-256(
  key = "misaka-palw/capability-message/v2",
  network_id || validator_id || bond_outpoint(txid || index)
  || model_profile_id || runtime_manifest_hash || runtime_class_id
  || shape_profile_id || trace_scheme_id || cu_ruleset_id
  || expiry_daa_score
)
ML-DSA-87 signing context = "misaka-palw/capability/mldsa87/v2"
```

### 16.3 宣言gate — agent capability handleの消費規範

```text
may_declare =
      agent reachable（health probeに応答）
  AND health.state != Quarantined
  AND health.selftest_passed == true
  AND health.golden_vector_root != unpopulated sentinel
  AND health.runtime_manifest_hash != all-zero sentinel
```

一つでも欠ければ**宣言しない**。既に宣言済みならrenewを止め、expiryで失効させる
（**withdraw-by-silence**: 明示的withdrawal objectは持たない。撤回のlivenessを
別objectの検閲耐性へ依存させず、expiryを撤回の床にする）。gateの評価はagentの
health frame（identityブロック入り）に対して行い、workerを直接触らない。

### 16.4 v1から継承する受理規則（Accept段階でそのまま適用）

- accepted/expiry**両端**bound（`is_live_at` — 宣言前の視点から宣言が見えてはならない。
  後出し宣言によるcommittee挿入・pool水増しre-rollの防止）
- `declaration_block` によるancestry検査（DAAは時計であって祖先証明ではない）
- expiry cap: `min(declared, accepted + max_capability_validity_blocks)`
- validator単位dedup（latest expiry勝ち — 1 operatorが複数committee slotを占めない）
- bond束縛（実行できないclassの宣言はslashable claim）

### 16.5 段階と現在の状態

**現在（Land）**: 型・署名メッセージ・gateを凍結し、kaspadの`--compute-endpoint`監視が
gate評価結果と宣言identityをログへ出す。**署名もchain受理もまだ行わない。**

**Accept段階へ進むためのchecklist**（一つでも欠ければ進まない）:

```text
[ ] v2 registry（manifest hash exact-pin行）と登録ceremony（real golden root必須）
[ ] 新しいDnsTxKind（v1 ComputeCapability kindを再利用しない — 別scheme別pool）
[ ] stateless/stateful検証（署名・bond・registry membership・expiry cap）
[ ] store prefix + reorg-exact削除 + pruning bundle包含
[ ] credit walkでのv2 candidate pool構築（v1 poolと混合しない）
[ ] activation fence（devnet先行。fingerprint移動を伴う明示的flag day）
[ ] 宣言のみではcommitteeに入れない（§10: golden vectors + random challenge + bond）
```

# MISAKA PALW algo_id=2 VPS Canonical Worker 経路設計書

> **This describes the WITHDRAWN float lane, and no longer applies to any network.** The CPU
> determinism class exists because llama.cpp ships hand-written per-ISA kernels whose reductions sum
> in different orders — a real property of that runtime. The execution family replaced it
> (ADR-0053): pinned integer arithmetic in this tree's own Rust, with **no `target_arch` branch on
> the execution path** and `runtime_class_id` left at zero, because the integer family's identity is
> its graph and not its host. **There is no CPU class today, and arm and x86 hosts are not
> separated** — for verifiers or producers. Kept as the record of what the float lane cost and of
> why the network left it. See `testnet11-node-operator.md` §2.


**文書ID:** MISAKA-PALW-VPS-RUNTIME-0001  
**版:** v0.1  
**日付:** 2026-08-12  
**状態:** Draft / Testnet-Only / Ollama経路失効後  
**対象:** `algo_id=2 (PALW LLM, logits-bound)`、`misaka-palw-worker`、VLT executor / verifier fleet

> [!IMPORTANT]
> 本書はVPS canonical workerの配置・IPC・運用設計であり、安全モデル、識別子、activation gateの
> 規範は[`PALW full-logits trace scheme v2`](palw-full-logits-trace-v2-design.md)に従う。
> 本書中の`algo_id=2`はheader-levelの`pow_algo_id`ではなく、型とnamespaceを分離したPALW内部の
> execution/scheme versionを指す。historical Argon2idの`pow_algo_id=2`を再利用してはならない。
> 両文書が衝突する場合は、より保守的な規則と基礎設計書のactivation gateを優先する。

関連する運用・consensus詳細:

- [`MISAKA PALW側PoW 詳細設計書 v0.1`](misaka-palw-pow-detailed-design-v0.1-ja.md)
- [`MISAKA PALW セキュアOTA設計書 v0.1`](misaka-palw-ota-secure-update-design-v0.1-ja.md)

---

## 0. 結論

Ollama経路は廃止し、VPSでは次の経路を正規経路とする。

```text
MISAKA P2P / Chain
        |
        v
kaspad + VLT compute service
        |
        | Unix Domain Socket（同一VPS、標準）
        v
palw-agent
        |
        | canonical Borsh frame + private stdio
        v
pinned palw-worker process / worker pool
        |
        | direct C FFI
        v
pinned llama.cpp CPU build
        |
        v
read-only pinned GGUF
```

採用判断は以下である。

| 項目 | 判断 |
|---|---|
| Ollama `/v1/completions` | 失効。新規receiptを生成してはならない |
| `llama-server`互換HTTP API | 不採用。consensus経路に入れない |
| `palw-worker`からllama.cppを直接呼ぶ | 採用 |
| 同一VPS内の通信 | Unix Domain Socketを採用 |
| VPS間の公開HTTP | 禁止 |
| validatorとcomputeを別VPSに分ける構成 | 専用1対1、private network、相互認証時のみ許可 |
| 複数validatorが同じ中央LLM APIを共有 | 禁止 |
| workerがvalidator秘密鍵を保持 | 禁止 |
| worker停止時のOllama fallback | 禁止。abstainまたはcompute capability停止 |

最初の実装では、現在のローカルsubprocess方式を正しさの基準として残す。その後、モデル再ロードのコストを削減するため、モデルを常駐させる`palw-agent`とworker poolへ移行する。ただし、移行前後で`MatchProjection`が完全一致しなければならない。

---

## 1. 設計目的

本設計の目的は次の5点である。

1. Ollamaの出力文字列・API仕様に依存せず、full logits commitmentを生成する。
2. VPSごとの差異をruntime manifestと起動時conformance testでfail closedにする。
3. executorとverifierが同じcanonical jobを独立再実行できるようにする。
4. workerをkaspadのアドレス空間・validator signing key・公開ネットワークから分離する。
5. worker障害がMISAKA本体のvalidator、DNS finality、hash laneのlivenessを止めないようにする。

本設計は、logits root単体を暗号学的な計算証明とは扱わない。任意のrootを申告すること自体は可能であり、正しさは独立再実行、committee、bond、maturity、auditによって判定する。同じ偽結果へA/Bが一致しても、その値が正しい計算結果であるとは限らないためである。

---

## 2. スコープと識別子

### 2.1 対象スコープ

本書は、2026-08-12に測定されたportable x86 CPU profile上のfull-logits経路を対象とする。

対象外は次のとおり。

- Ollama text-output calibration
- token-only proof
- Metal、CUDA、ROCmとの同一determinism class化
- canonical integer Compute Setへの移行
- mainnet positive creditの承認
- LLM出力内容の意味的正しさ

### 2.2 `algo_id=2`のnamespace

`algo_id=2`が既存のheader-level PoW IDと衝突し得る実装では、次のようにnamespaceを分離する。

```text
palw_execution_algo_id = 2
palw_trace_scheme_id    = Hash64("misaka-palw/full-logits-trace/v2")
pow_header_algo_id      = 既存consensus定義を維持
```

過去のheader algo IDをPALW runtime IDとして再利用してはならない。

### 2.3 新しい識別子

```text
job_schema:
  misaka.palw.job.v2

submission_schema:
  misaka.palw.testnet-submission.v4

runtime_profile:
  misaka-palw-llamacpp-cpu-portable/v1

trace_scheme:
  full-logits-per-canonical-decode-call/keyed-blake2b-512/v2

transport_protocol:
  misaka-palw-agent-borsh/v1
```

既存Ollama runtime hashとtrace schemeは、activation DAA以後`Retired`として扱う。historical dataの読取りは許すが、新しいjob、receipt、verdict、creditには使用してはならない。旧receiptを新schemeとして再解釈することも禁止する。

---

## 3. 推奨VPSトポロジ

### 3.1 標準構成: validatorとworkerを同一VPSに配置

```text
VPS-Validator-A
├─ kaspad
├─ validator service / remote signer client
├─ palw-agent
├─ palw-worker CPU process pool
└─ pinned GGUF
```

この構成を初期testnetの標準とする。

利点は次のとおり。

- public networkを経由せず、promptとjobをUDSで渡せる
- transport latencyとpacket lossを除外できる
- worker endpointをインターネットへ公開しない
- validatorごとに独立workerを持たせやすい
- 現行のlocal subprocess実装から段階移行しやすい

初期運用プロファイルは以下を推奨する。

```text
CPU: dedicated x86_64 vCPU 8以上
RAM: 16 GiB以上
Disk: NVMe、model・binary・log用に30 GiB以上
palw worker threads: 4固定
worker concurrency: 1固定
kaspad用CPU余白: 2 vCPU以上
OS余白: 2 vCPU程度
```

これはconsensus定数ではなく、初期運用値である。性能測定後に変更できるが、worker thread数と実行scheduleを変更する場合は新しいshape/runtime profile IDが必要になる。

### 3.2 分離構成: validator VPSとcompute VPSを1対1で分離

```text
VPS-Validator-A                        VPS-Compute-A
┌────────────────────┐                 ┌────────────────────┐
│ kaspad / validator │=== private ===> │ palw-agent         │
│ validator signer   │  mTLS/WireGuard │ pinned worker      │
└────────────────────┘                 │ pinned GGUF        │
                                       └────────────────────┘
```

この構成は次の条件を全て満たす場合だけ許可する。

- validator A専用のcompute Aであり、他validatorと共有しない
- private networkだけで到達可能
- validator session keyとworker session keyを相互pinする
- requestとresponseのcanonical bytesを双方が署名する
- worker identity、runtime manifest、capacityをchain capabilityへ結び付ける
- timeout時はabstainし、別の共有workerへ自動fallbackしない

一つの中央worker APIをA/B/C複数validatorが共有すると、process数だけ増えてfailure domainは一つのままである。committee independenceを偽装するため、この構成は禁止する。

---

## 4. コンポーネント責務

### 4.1 `kaspad` / VLT compute service

`kaspad`は次を担当する。

- chain stateとfuture randomnessからjob contextを構築する
- executorまたはverifierの役割を決定する
- verifierへexecutorのclaimed rootを渡さない
- canonical `PalwJobEnvelopeV2`を生成する
- agentのresponseをschema、profile、job binding、token count、root長で検査する
- `MatchProjection`と`ComputeReceipt`を構築する
- validator signing keyまたはremote signerでreceipt/verdictを署名する
- agentが失敗した場合、computeだけを停止しvalidator本体を継続する

`kaspad`はlogitsを計算せず、workerのraw stdoutをそのままchainへ流さない。

### 4.2 `palw-agent`

`palw-agent`はVPS上のruntime supervisorであり、LLM計算結果の意味を決めない。

責務は次のとおり。

- UDS listener
- peer credential確認
- canonical frame lengthとschema検査
- bounded queue
- deadline admission control
- duplicate job防止
- runtime manifestとmodel hashの確認
- worker process起動、監視、timeout kill
- stdout/stderrの同時drain
- worker responseの再parseとjob binding確認
- health state公開
- metrics公開

禁止事項は次のとおり。

- promptの追記、normalization、chat template適用
- expected output/rootのworkerへの注入
- hidden retryで異なるruntimeへ切替
- cross-job KV cache
- prefix cacheを秘密裏に利用してCUを過大申告
- outputを書き換えて成功扱いにする

### 4.3 `palw-worker`

workerは唯一、canonical LLM executionを行う。

責務は次のとおり。

- pinned modelを直接llama.cpp APIでロード
- runtime policyを起動後に再読取し、不一致ならexit
- canonical token IDsを受理
- jobごとに新しいllama contextを生成
- fixed prefill scheduleを実行
- greedy argmax、first-index tie breakを実行
- exact decode countを実行
- full logitsをcanonical f32 little-endian bytesへ変換
- per-event digestとfinal trace rootを生成
- output token IDs、schedule、trace、CUをcommit
- canonical responseだけをstdout/IPCへ出力

workerはvalidator秘密鍵、bond key、P2P keyを持たない。worker outputへのvalidator署名はkaspad側で行う。

### 4.4 model store

modelは次の形で配置する。

```text
/var/lib/misaka-palw/models/
└─ <gguf_sha256>.gguf
```

要件は次のとおり。

- root所有、workerからread-only
- 起動時に全SHA-256を再計算
- path、size、mtimeだけのcacheを信頼しない
- hash確認後にopenしたfile descriptorまたはread-only mountを維持
- 不一致時はworker READYにならない

### 4.5 release manifest

signed manifestの自己申告だけでは、悪意あるVPSが本当にそのbinaryを動かしている証明にはならない。したがってmanifestは主にaccidental driftの防止として扱い、network correctnessは独立再実行に依存する。

それでもoperator errorを防ぐため、起動時には必須binary setのmissing、extra、mismatchを全てfatalにし、実binaryをその場でhashする。

---

## 5. IPC設計

### 5.1 同一VPS

通信方式はUnix Domain Socketとする。

```text
/run/misaka-palw/agent.sock
```

- filesystem permission: `0660`
- owner: `palw`
- group: `misaka-validator`
- transport framing: `u32-le length || Borsh payload`
- 最大request sizeとresponse sizeを固定
- 1 connectionあたり1 requestを初期仕様とする
- **half-close契約（2026-08-13追加）**: 送信側はframe送信後に書き込み側を
  `shutdown(SHUT_WR)` する。受信側はframe後の余剰バイト不在をEOFで検証するため、
  half-closeしないclientはtimeoutまでblockし応答を得られない。subprocess stdinは
  pipe closeで同じ効果を得る
- unknown versionはfail closed

JSONはCLI、debug、evidence表示に限り、production IPCのcanonical identityには使わない。

### 5.2 `PalwJobEnvelopeV2`

```rust
pub struct PalwJobEnvelopeV2 {
    pub version: u16,
    pub network_id: Vec<u8>,
    pub job_id: Hash64,
    pub job_nullifier: Hash64,
    pub mode: PalwJobModeV2, // Execute | Replay

    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
    pub cu_ruleset_id: Hash64,

    pub execution_seed: [u8; 32],
    pub prompt_token_ids: Vec<u32>,
    pub exact_decode_tokens: u32,
    pub max_context_tokens: u32,

    pub assignment_id: Hash64,
    pub assignment_epoch: u64,
    pub deadline_unix_ms: u64,
}
```

### 5.3 raw textではなくtoken IDsを正本にする

Ollamaとworkerのtokenization、chat template、BOS/EOSの差を消すため、V2では`prompt_token_ids`をcanonical inputとする。

raw promptを使用する場合は、commit前に同じpinned tokenizerでtoken IDsへ変換し、jobにはtoken IDsを格納する。raw textは説明用またはDA用の補助データであり、実行identityの正本にしない。

検査条件は次のとおり。

```text
prompt_token_ids.len() > 0
all token_id < n_vocab
prefill_tokens = prompt_token_ids.len()
prefill_tokens + exact_decode_tokens <= max_context_tokens
max_context_tokens == activated profile value
exact_decode_tokens > 0
```

### 5.4 `--n-predict`を廃止する

`--n-predict`はruntimeごとにtotal budgetとdecode budgetの意味が異なるため、production interfaceから削除する。

置換後は次を明示する。

```text
--prompt-token-ids-stdin
--exact-decode-tokens D
--max-context-tokens C
```

またはBorsh job envelopeだけを受け取り、CLI引数からtoken budgetを排除する。

### 5.5 EOG規則

固定量のPALW workでは、early EOGによって計算量を減らせてはならない。

初期profileでは次を採用する。

```text
stop_on_eog = false
exact_decode_tokensを必ず最後まで実行
EOG tokenは通常のtoken IDとしてoutput commitmentへ含める
eog_first_seen_atはtelemetryのみ
```

これにより、短い出力へ到達するseedを探すgrindingを防ぐ。

---

## 6. Canonical execution policy

runtime manifestは少なくとも次を固定する。

```text
architecture             = x86_64-linux
llama.cpp commit         = exact commit
worker binary SHA-256    = exact digest
patch set SHA-256        = exact digest
compiler/linker identity = exact versions
GGML_NATIVE              = OFF
GGML_METAL               = OFF
GGML_CUDA                = OFF
GGML_BLAS                = OFF
GGML_ACCELERATE          = OFF
GGML_OPENMP              = OFF
CPU feature mask         = exact profile
thread count             = exact value
thread affinity policy   = exact value
n_ctx                    = exact value
n_batch / n_ubatch       = exact values
prefill chunk schedule   = exact schedule ID
sampling                 = greedy argmax, first-index tie
flash attention          = disabled
context shift            = disabled
KV/prefix cache reuse    = disabled across jobs
GGUF SHA-256             = exact digest
tokenizer/vocab identity = exact digest
FP environment           = exact profile
```

### 6.1 CPU feature drift

VPSは再起動、host migration、provider変更によってCPUID feature exposureが変わる可能性がある。

起動時とcapability renewal前に次を検査する。

- CPU feature mask
- worker binary hash
- linked artifact hash
- model hash
- floating-point environment
- golden vector root

一つでも不一致なら`QUARANTINED`へ遷移し、compute capabilityを宣言しない。

### 6.2 floating-point environment

full f32 bytesをcommitするため、次を固定する。

```text
rounding mode = round-to-nearest, ties-to-even
fast-math     = disabled
FP contraction/FMA policy = profile固定
FTZ           = profile固定
DAZ           = profile固定
NaN/Inf       = execution invalid
signed zero   = raw IEEE-754 bytesを維持
serialization = f32.to_le_bytes()
```

任意のlogitがnon-finiteならreceiptを発行しない。

---

## 7. Logits trace設計

### 7.1 名称

現schemeが各GEMM intermediateではなくdecode call後のfull logitsをcommitする場合、正規名称は次とする。

```text
full_logits_trace_root
```

既存wire compatibilityのため`gemm_trace_root` fieldを残す場合でも、schema v4ではaliasであることを明記する。

### 7.2 context binding

> [!NOTE]
> 2026-08-13整合: 正規のpreimageは `consensus/core/src/palw_v2.rs`
> （golden vectorで凍結済み）。初版に対し `version` / `tokenizer_id` /
> `declared_prefill_tokens` の3 fieldを追加した — tokenizer identityとprefill長を
> contextへ明示束縛しないと、同一token列に別tokenizer由来の解釈を主張する余地と、
> prefill/decode境界の再解釈余地が残るためである。

```text
job_context_hash = H_k(
  "misaka-palw/job-context/v2",
  version
  || network_id
  || job_id
  || job_nullifier
  || assignment_id
  || execution_seed
  || model_profile_id
  || runtime_manifest_hash
  || runtime_class_id
  || shape_profile_id
  || trace_scheme_id
  || cu_ruleset_id
  || tokenizer_id
  || H(prompt_token_ids)
  || declared_prefill_tokens
  || exact_decode_tokens
  || max_context_tokens
)
```

### 7.3 event hash

```text
logits_event_i = H_k(
  "misaka-palw/full-logits-event/v2",
  job_context_hash
  || phase
  || phase_step
  || n_vocab
  || logits_dtype
  || logits_count
  || canonical_logits_bytes
)
```

### 7.4 root

初期Dが小さいため、event hashのMerkle rootを採用する。

```text
full_logits_trace_root = MerkleRoot(logits_event_0 ... logits_event_n)
```

rootは次も束縛する。

```text
trace_event_count
first_event_kind
last_event_kind
prefill_tokens
exact_decode_tokens
```

`D` tokenを生成する場合、profileはevent countの式を明記する。例えば「prefill logitsで1 token目を選び、最後のtokenは再feedしない」なら、`trace_event_count = D`を不変条件とする。

### 7.5 output commitment

```text
output_commitment = H_k(
  "misaka-palw/output/v2",
  job_context_hash
  || token_count
  || generated_token_ids
  || rendered_output_hash
)
```

consensus equalityはtoken IDsを正本とし、rendered bytesは補助bindingとする。

---

## 8. Worker lifecycle

### 8.1 状態機械

```text
BOOTING
  -> HASHING_ARTIFACTS
  -> GOLDEN_SELFTEST
  -> READY
  -> BUSY
  -> READY

任意状態
  -> DEGRADED
  -> QUARANTINED
```

### 8.2 startup gate

READYになる前に次を全て実行する。

1. binary/model/manifest hash確認
2. CPUIDとFP environment確認
3. full 64-byte golden vectorを全件再実行
4. input-sensitive negative vector確認
5. wrong profile rejection確認
6. work directory permission確認
7. UDS permission確認

### 8.3 job admission

agentは次の場合にjobを拒否またはabstainする。

- queue full
- deadlineまでの残時間がworst-case estimate未満
- runtimeがREADYでない
- profile ID不一致
- duplicate job ID
- token/context上限違反
- unknown mode/version

開始後にdeadlineを超えたworkerはkillし、partial resultを使用しない。

### 8.4 model常駐方式

段階的に実装する。

**Phase A: per-job process**

- 現行のsubprocess方式
- jobごとに完全なprocess isolation
- correctness baseline
- model load costは大きい

**Phase B: persistent worker pool**

- 各worker processがmodel weightsだけを常駐
- jobごとに新しいllama contextを生成・破棄
- concurrencyは初期1
- KV cache、sampler state、prompt stateをjob間で共有しない
- Phase Aと全golden vectorが一致した後だけ有効化

agentが計算結果をcacheして返す方式は禁止する。

---

## 9. セキュリティ境界

### 9.1 VPSは信頼しない

malicious VPS operatorは、manifestを偽装し、別binaryを実行し、任意rootを返せる。したがって、local manifestはnetwork proofではない。

network側の防御は次で構成する。

- post-commitment assignment
- independent verifier replay
- same activated determinism profile
- provider/validator credential分離
- bond
- no-showとobjective faultの処理
- reward maturity
- auditとDA

### 9.2 authenticated IPC

model runnerとreceipt issuerは認証済みIPCで接続する。

同一VPSでは次を用いる。

- UDS filesystem permission
- Linux peer credentials
- canonical request hash
- responseのrequest hash echo

別VPSでは次を追加する。

- WireGuard/private VPC
- mTLS
- validator session signature
- worker session signature
- replay nonce
- strict deadline

worker session signatureはtransport accountabilityであり、正しい計算の証明ではない。

### 9.3 signer separation

```text
palw-worker: validator key accessなし
palw-agent:  validator key accessなし
kaspad:      signing requestを構築
remote signer/HSM: ML-DSA-87署名のみ
```

worker compromiseからvalidator bond keyを保護する。

### 9.4 shared backend禁止

A/B/verifierが同じ中央worker service、同じcache、同じprocess、同じoperator control planeを共有してはならない。

「VPSを5台立てたが全て一つのAPIへ問い合わせる」構成は、独立replicaではなく一つの計算を5回転送しているだけなので、committee memberとして数えない。

---

## 10. systemd配置例

### 10.1 ディレクトリ

```text
/opt/misaka-palw/bin/palw-agent
/opt/misaka-palw/bin/palw-worker
/opt/misaka-palw/manifests/runtime-v1.borsh
/var/lib/misaka-palw/models/<sha256>.gguf
/var/lib/misaka-palw/golden/
/var/lib/misaka-palw/evidence/
/run/misaka-palw/agent.sock
```

### 10.2 agent service例

```ini
[Unit]
Description=MISAKA PALW Canonical CPU Agent
After=network.target
Before=kaspad.service

[Service]
Type=simple
User=palw
Group=misaka-validator
ExecStart=/opt/misaka-palw/bin/palw-agent \
  --listen unix:///run/misaka-palw/agent.sock \
  --worker /opt/misaka-palw/bin/palw-worker \
  --manifest /opt/misaka-palw/manifests/runtime-v1.borsh \
  --model /var/lib/misaka-palw/models/<sha256>.gguf \
  --max-concurrency 1
RuntimeDirectory=misaka-palw
StateDirectory=misaka-palw
NoNewPrivileges=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectSystem=strict
ProtectHome=yes
ReadOnlyPaths=/opt/misaka-palw /var/lib/misaka-palw/models /var/lib/misaka-palw/golden
RestrictAddressFamilies=AF_UNIX
TasksMax=128
LimitNOFILE=4096
Restart=on-failure
RestartSec=3

[Install]
WantedBy=multi-user.target
```

CPU pinningとmemory limitはVPS profileごとのdrop-inで設定する。

```ini
[Service]
CPUAffinity=2 3 4 5
MemoryMax=12G
```

### 10.3 kaspad側

新しい設定を追加する。

```text
--enable-compute
--compute-endpoint=unix:///run/misaka-palw/agent.sock
--compute-timeout-secs=<profile value>
--compute-max-inflight=1
```

既存`--compute-worker=/path/to/palw-worker`はPhase A互換として残し、public fleetでは`--compute-endpoint`を標準とする。

---

## 11. build・配布

### 11.1 build原則

- VPS上でcompileしない
- 一つのcontrolled builderでbuild
- exact source commit
- pinned Rust/C/C++ toolchain
- `Cargo.lock`と`--locked`
- exact CMake cache
- static library優先
- release artifactとSBOMを生成
- manifestをoffline release keyで署名
- independent second buildでhashを比較

### 11.2 CPU build profile

初期profileは測定済みの制約をそのまま固定する。

```text
-DGGML_NATIVE=OFF
-DGGML_METAL=OFF
-DGGML_BLAS=OFF
-DGGML_ACCELERATE=OFF
-DGGML_OPENMP=OFF
-DGGML_CUDA=OFF
```

ISA別option、FMA、F16C等は暗黙defaultにせず、最終CMake cacheのhashと実行時CPU feature maskをmanifestへ含める。

### 11.3 Linux `build.rs`

Linux CPU artifactではApple frameworkと`-lc++`を無条件linkしてはならない。

概念上は次の分岐とする。

```rust
match target_os {
    "macos" => link_metal_profile(),
    "linux" => link_cpu_profile_without_openmp_or_blas(),
    _ => fail_unsupported_target(),
}
```

Linux profileはMetal、Accelerate、ggml-metal、ggml-blasをlinkしない。C++ runtime、pthread、dl、m等の必要dependencyだけをexact profileとして固定する。

---

## 12. code変更一覧

### 12.1 `misaka-palw-worker`

P0変更:

1. Linux CPU buildを正式化
2. CPU runtime manifest constants追加
3. `--n-predict`削除
4. token ID input追加
5. exact decode policy追加
6. early EOG停止を廃止
7. non-finite logitsをfatal化
8. trace rootへjob context binding追加
9. `full_logits_trace_root`名称追加
10. stdoutをcanonical response 1件だけに制限

P1変更:

1. persistent serve mode
2. model/session APIとjob context APIを分離
3. per-job context reset assertion
4. UDS agent protocol
5. bounded worker pool

### 12.2 `misaka-palw`

```rust
pub enum PalwRuntimeBackend {
    LocalProcess(PalwWorkerRuntime),
    LocalAgent(PalwAgentRuntime),
    RemoteDedicated(PalwRemoteRuntime), // testnet opt-in only
}
```

`ComputeRuntime::execute`と`replay`のpredicateは維持する。verifier methodへpeerのclaimed projectionを渡さない。

### 12.3 `kaspad`

実装状態（2026-08-13、`kaspad/src/palw_agent.rs` / `misaka-palw/src/agent_client.rs`）:

- `--compute-endpoint` — **済**（bare path / `unix://` URI両対応。Land段階=観測のみ:
  reward・work・fork-choice weightを一切与えず、`--compute-worker`(v1 VLT role)も置換しない）
- agent health probe — **済**（30秒間隔、framed Borsh Health round-trip、状態遷移のみログ）
- capability withdraw/quarantine — **済（handle層）**: `PalwAgentCapability` が
  Available / Quarantined / Unreachable を保持し、agent不達・隔離で即時撤回。
  **consensus可視のcapability宣言はまだ何もこのhandleを消費しない**（将来のVLT v2
  capability宣言が参照する唯一のフック）。agent死亡・隔離でもvalidator本体は継続
  （E2E実証: agent kill→1 probe内でwithdraw WARN→kaspad生存）
- queue/deadline telemetry — 部分（Health frameのcounters。Prometheusは未）
- runtime manifest RPC表示 — 未（現状はlog表示: manifest/golden root prefix +
  selftest_passed。RPC surfaceへの追加は別変更）
- old Ollama profileのactivation fence — 済（`9736aec` の `never()` + test pin）

client側の要点: `misaka-palw::agent_client` はagentの応答を**自前の入力から再検証**する
（request hash・job id・counts・CU再導出・job_context_hash再計算）。supervisorの言葉だけで
計算結果を受け取らない — ただしこの検証は「モデルが実際に走った」ことを証明しない
（それは独立replay・committee・bondの仕事、基礎設計§4）。

### 12.4 consensus / registry

- old Ollama profileをRetired
- new CPU profileとtrace schemeを新規登録
- old receiptを新profileで再利用不可
- exact profile/classをcommittee selectionへbinding
- positive creditは別activation flag

---

## 13. failure policy

| 事象 | 処理 |
|---|---|
| worker timeout | kill、abstain、partial result破棄 |
| worker crash | job失敗、agentはfresh processで復旧 |
| model hash mismatch | QUARANTINED、compute capability停止 |
| binary hash mismatch | QUARANTINED |
| golden vector mismatch | QUARANTINED、verdictを出さない |
| peer receiptとlocal replay mismatch | refuting verdict候補。ただしlocal classを再self-test |
| 単一mismatch | 自動slash challengeを即時提出しない |
| queue overload | deadline前にadmission拒否 |
| agent停止 | kaspadはvalidator-onlyで継続 |
| UDS permission異常 | compute disabled |
| remote compute切断 | abstain。共有fallback禁止 |
| CPU feature mask変化 | capability withdraw、再認証待ち |

refutation-dominant設計では、壊れたverifierがhonest executorを害し得る。したがって、local conformance failure時は「結果を返す」より「棄権する」を優先する。

---

## 14. observability

公開してよいmetrics:

```text
palw_agent_ready
palw_runtime_manifest_info
palw_jobs_total{mode,status}
palw_job_duration_seconds
palw_queue_depth
palw_worker_restarts_total
palw_timeouts_total
palw_local_selftest_failures_total
palw_replay_mismatch_total
palw_prefill_tokens_total
palw_decode_tokens_total
```

ログへ含める値:

- job ID prefix
- mode
- profile IDs
- prefill/decode count
- duration
- trace root prefix
- status/error code

ログへ含めない値:

- validator private key
- raw prompt本文
- full logits
- complete rendered output
- environment secret
- remote topology secret

raw promptやevidenceが必要なtestnetでは、明示的なdebug flagとretention期限を設ける。

---

## 15. 検証計画

### 15.1 closed fleet gate

最低構成:

```text
VPS数: 5以上
独立operator: 3以上
CPU microarchitecture: 3種類以上
VPS provider: 2社以上
job数: 10,000以上
各job rerun: 3回以上
root比較: full 64-byte
concurrency: 1 / 2 / 5 process test
restart: 全nodeで実施
soak: 7日以上
```

合格条件:

- same canonical jobは全same-class nodeで100%一致
- distinct inputに対するrootが定数化していない
- binary/model/profile driftは100% fail closed
- timeout後にpartial receiptが一件も出ない
- worker crashがvalidator livenessを止めない
- central shared backendを使わずcommitteeを形成できる

### 15.2 negative controls

次を意図的に変更し、class ID変更または実行拒否を確認する。

- OpenMP ON
- native build ON
- thread count変更
- CPU feature mask変更
- FMA/F16C policy変更
- model 1 byte変更
- worker binary変更
- tokenizer/vocab変更
- prefill schedule変更
- exact decode count変更
- event削除
- event順序変更
- EOG早期発生
- context overflow
- NaN/Inf injection
- duplicate job
- wrong network ID
- expired assignment

### 15.3 rollout stages

| Stage | 内容 | credit |
|---|---|---:|
| V0 | local process、2 VPS、manual vectors | 0 |
| V1 | UDS agent、5 VPS、closed replay | 0 |
| V2 | consensus-visible shadow、public roots | 0 |
| V3 | public no-value testnet、permissionless verifier | 0 |
| V4 | bounded economic rehearsal | very low / capped |
| V5 | external review後のpositive testnet credit | limited |
| Mainnet candidate | independent build、audit、long soak、release ceremony後 | separate approval |

Ollama pathを失効させた事実だけでV4以降へ進んではならない。

---

## 16. 実装優先順位

### P0: 直ちに実装

実装状態（2026-08-13、`palw-worker --mode v2-job` / `consensus/core/src/palw_v2.rs`）:

1. Ollama runtime/profileのRetired fence — **済**（`9736aec` の `never()` 化 + 再有効化禁止のtest pin）
2. Linux CPU `build.rs` — **済**（`4be9cad`）
3. exact CPU build manifest — **済**（`RuntimeManifestV2`: CMakeCache/静的library実測hash、
   GGML_*実値、自バイナリlive hash。`"unpinned"` field解消は§P2の署名付きrelease bundleで）
4. token-ID job schema — **済**（`PalwJobEnvelopeV2`、framed Borsh、fail closed）
5. `--n-predict`廃止 — **済**（v2経路はenvelope予算のみ、`--n-predict`指定は拒否。
   v1 algo4凍結経路は互換のため不変）
6. exact decode / no early EOG — **済**（EOGはtelemetry `eog_first_seen_at` のみ）
7. context-bound full logits root — **済**（event単位でjob_context_hash束縛 + Merkle +外側root）
8. boot golden self-test — **未**（正式golden vectorの登録が前提。現状は
   `scripts/misaka-palw-v2-worker-smoke.py` の再実行一致+negative probeで代替）
9. artifact live hash verification — **済**（モデル毎回全読SHA-256 + 自バイナリSHA-256 +
   FP環境の起動時/ロード後検証）
10. compute failure時validator-only継続 — **既存挙動**（v2はconsensus未接続のLand段階であり、
    worker失敗はjob失敗のみ。接続時のabstain規則は§13に従う）

### P1: fleet運用前

実装状態（2026-08-13、`misaka-palw-agent` / `scripts/misaka-palw-v2-agent-smoke.py`）:

1. `palw-agent` — **済（Phase A）**: boot golden gate（selftest失敗でQUARANTINED、
   golden未登録は`--allow-ungated`なしで起動拒否）、admission（envelope形状・
   6 identity・deadline実現性・duplicate window・単一slot）、per-job supervision
   （pipe drain、min(timeout, deadline)でkill、部分結果破棄）、応答の再束縛検証
   （request hash echo・job id・token counts・CU再導出・job_context_hash再計算）
2. UDS Borsh protocol — **済**（`misaka-palw-agent-borsh/v1`: Job/Health request、
   JobOk/JobRejected/JobFailed/Health response。§5.1のhalf-close契約に注意）
3. systemd sandbox — 未（§10.2のunit例のまま。実配備時に適用）
4. bounded queue/deadline — **済（Phase A形）**: 隠れqueueを持たずbusy=即時拒否、
   deadline admissionはworst-case見積で実行前拒否
5. persistent model worker pool — 未（Phase B。Phase Aとのgolden全一致が有効化条件）
6. Prometheus metrics — 未（Health frameのcounters — total/ok/rejected/failed/timeouts —
   で代替中）
7. capability quarantine/withdraw — 部分（agentのQUARANTINE状態は実装済。kaspad側
   capability宣言との連動は未）
8. five-node deployment harness — 未

### P2: public no-value testnet前

1. independent operator onboarding
2. signed release bundle
3. source-to-binary independent rebuild
4. full determinism matrix
5. restart/migration/concurrency campaign
6. mismatch incident runbook
7. DA/audit retention policy

---

## 17. 最終判断

VPS上の正規経路は次で固定する。

```text
kaspad
  -> local authenticated IPC
  -> palw-agent
  -> pinned palw-worker
  -> direct pinned llama.cpp CPU execution
  -> context-bound full logits trace root
  -> independent same-profile verifier replay
```

Ollama、`llama-server`、外部OpenAI互換APIはconsensus execution pathから外す。

初期は同一VPS内のlocal process/UDSを採用し、各validatorが専用workerを持つ。モデル常駐化はagent内部の性能最適化として行い、jobごとに新しいcontextを生成する。共有中央worker、hidden cache、API fallbackはcommittee independenceとruntime identityを壊すため禁止する。

この設計であれば、今回確認したportable x86 determinismを、VPS fleetの実行経路へ比較的少ない変更で接続できる。残る安全性は「rootを作れること」ではなく、「独立operatorがdeadline内に再現し、不一致・停止・共謀を報酬成熟前に処理できること」で評価する。

---

## 18. 参照した既存資料

- `misaka-tokenbftllm-code-2026-08-11.zip`
- `misaka-palw-main-shared-testnet-audit-20260727.md`
- `misaka-palw-release-readiness-audit-e205335.md`
- `PALW_Current_Code_High_Bond_Slash_Audit_JA.md`
- `MISAKA-whitepaper-v4-ja-ACM.pdf`のstage gate表

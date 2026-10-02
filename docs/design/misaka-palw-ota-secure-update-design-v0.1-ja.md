# MISAKA PALW セキュアOTA設計書

**文書ID:** MISAKA-PALW-OTA-0001  
**版:** v0.1  
**作成日:** 2026-08-12  
**状態:** Draft / Testnet-First / Mainnet未承認  
**対象:** PALW validator、`palw-agent`、`palw-worker`、runtime plugin、llama.cpp、GGUF、tokenizer、golden vector、設定、kaspad連携部  
**関連文書:**
[`MISAKA PALW VPS Canonical Worker 経路設計書 v0.1`](misaka-palw-vps-canonical-worker-design-v0.1-ja.md)、
[`PALW full-logits trace scheme v2`](palw-full-logits-trace-v2-design.md)

> [!IMPORTANT]
> 本書はartifact配布・ローカル導入・fleet rolloutの設計であり、PALWのconsensus activationを
> 承認する文書ではない。OTAによるinstall/health判定は、Compute Set、Ruleset、reward、work、
> fork-choice、slashing ruleを有効化しない。現在の許可範囲は基礎設計書に従い、devnet、
> shadow mode、consensus-visible zero-creditまでとする。positive creditまたはmainnetへの昇格は、
> 別ADR、future activation、全activation gate通過、外部監査を必要とする。

---

## 0. 本書の結論

PALWのOTAは、一般的な「最新版を見つけて上書き再起動する仕組み」にしてはならない。

正規設計は、次の三つを独立させる。

```text
1. Artifact Distribution
   署名済みartifactをVPSへ安全に届ける

2. Local Installation
   inactive slotへ導入し、自己検証後にatomic switchする

3. Consensus Activation
   Compute Set、policy、allocation、VM、wire ruleをchain上の将来DAAで有効化する
```

この三つを一つの「更新成功」フラグへまとめない。

```text
downloaded  != installed
installed   != locally healthy
healthy     != fleet ready
fleet ready != consensus active
```

OTA server、CDN、CI、単一release signer、単一VPSのいずれかが侵害されても、それだけでPALWのpositive compute credit、fork-choice、reward、slashing ruleを変更できないことを設計目標とする。

---

## 1. 用語と規範

本書でOTAは **Over-The-Air Update**、すなわちネットワーク経由のartifact配布・導入を指す。

規範語は次の意味で使用する。

- **MUST:** 実装必須。満たさなければ安全なOTAとして扱わない。
- **MUST NOT:** 禁止。
- **SHOULD:** 原則必須。例外には文書化された理由と代替防御が必要。
- **MAY:** 任意。

### 1.1 安全性の主張範囲

本OTAが担保するのは主に以下である。

- 配布artifactの真正性と完全性
- rollback、freeze、mix-and-match、wrong-targetの検出
- 部分導入・破損導入の防止
- atomic切替と安全なローカルrollback
- rollout中の相関障害の抑制
- key compromise時の復旧経路
- artifact identityとPALW runtime identityのbinding

本OTAだけでは以下を証明しない。

- VPS operatorが申告binaryを本当に実行していること
- workerが正しいLLM計算を行ったこと
- Provider A/B/Auditorが独立主体であること
- PALWの経済安全性
- governance quorumそのものの健全性

これらは、independent replay、receipt binding、bond、audit、future assignment、operator separation、reward maturity、on-chain governanceで別途担保する。

---

## 2. 更新対象の分類

更新対象を影響度により分類し、同じOTA手順で扱わない。

| Class | 対象例 | Consensus identityへの影響 | OTA導入 | 有効化 | 自動rollback |
|---|---|---:|---:|---|---:|
| O0 | dashboard、metrics、log format | なし | 可 | 即時 | 可 |
| O1 | updater、systemd hardening、非意味論agent修正 | 原則なし | 可 | local health後 | 可 |
| O2 | worker実装、llama.cpp patch、compiler/build profile | implementation identity変更 | 可 | Shadow conformance後 | 条件付き |
| O3 | weights、GGUF、tokenizer、template、decode rule、trace scheme | 新Compute Set ID | 可 | proposal→certificate→Shadow→future DAA | 旧set再選択のみ |
| O4 | Compute VM opcode、arithmetic、wire schema、header field | consensus identity変更 | 配布のみ可 | hard forkまたはre-genesis | 原則不可 |
| O5 | emergency halt、revocation、denylist | 緊急制御 | metadata配布可 | chain governance別経路 | 履歴保存 |

### 2.1 O2の扱い

worker binaryだけを変更して同じCompute Setを実行する場合でも、次を新しいimplementation profileとして扱う。

```text
runtime_implementation_id = H(
  worker_binary
  || llama_cpp_artifact
  || compiler
  || linker
  || build_flags
  || patchset
  || OS/arch baseline
  || FP environment
)
```

新profileは、同一Compute Setに対するgolden vector、cross-machine replay、negative controlを通過するまでmint-gradeにしない。

### 2.2 O3の扱い

次の一つでも変われば既存setを上書きせず、新しい`compute_set_id`を作る。

- model weights / GGUF
- tokenizer
- chat template / prompt framing
- BOS/EOS
- decode count rule
- sampling / argmax tie rule
- trace scheme
- arithmetic / quantization rule
- LUT
- semantic program root
- shape / resource rule

人間は「軽微な修正」と呼びたがるが、hashは情緒に付き合わない。1 bitでも違えば別setである。

---

## 3. 脅威モデル

### 3.1 想定する攻撃者

攻撃者は次を行えるものとする。

- CDN、mirror、DNS、proxyの一部を制御する
- 古いmetadataやartifactを返す
- metadataの一部だけを差し替える
- endless streamや巨大fileでdiskを枯渇させる
- architectureの異なる正規binaryを返す
- release repositoryへ不正artifactを混入する
- online timestamp/snapshot keyを奪う
- targets signerの一部を奪う
- CI runnerまたはbuild dependencyを侵害する
- VPS上でupdate中にpower loss、OOM、disk-fullを起こす
- symlink、hardlink、path traversalで任意pathへ書かせる
- update controllerを侵害し、全fleetを同時更新させる
- NTP/DNSを操作してmetadata expiry判断を乱す
- old vulnerable releaseへrollbackさせる
- fleetの一部だけ更新し、determinism classを分断する
- active jobの途中でworkerを差し替え、receipt境界を曖昧にする

### 3.2 信頼しないもの

以下は信頼の根にしない。

- HTTPS/TLSだけ
- CDNやobject storage
- updater serverのHTTP response
- filenameやsemantic version文字列
- VPSが返す自己申告manifest
- `latest` symlink
- CIの成功表示だけ
- package内の`SHA256SUMS`だけ
- operatorの「更新しました」という報告

### 3.3 信頼の根

初期trust rootは、operatorへout-of-bandで配布されたOTA root metadataとする。

必須要素:

- role別public key
- threshold
- root version
- metadata format/version
- network ID
- accepted signature suite
- maximum metadata size
- trusted expiry/freshness policy

---

## 4. 全体アーキテクチャ

```text
                        RELEASE SIDE

  protected source
        │
        ▼
  hermetic build ── independent rebuild
        │                    │
        ├──── tests / fuzz / conformance
        │
        ├──── SBOM / provenance / in-toto evidence
        │
        ▼
  threshold targets signing
        │
        ▼
  metadata repository + untrusted mirrors/CDN

================================================================

                         VPS SIDE

        Internet / mirror
              │
              ▼
  palw-ota-fetcher (unprivileged, network access)
              │ verified metadata + staged bytes
              ▼
  palw-ota-installer (root, NO network access)
              │
              ├─ inactive slot
              ├─ model CAS
              ├─ preflight / golden / policy checks
              │
              ▼
       atomic current switch
              │
              ▼
      palw-agent / palw-worker
              │ authenticated UDS
              ▼
             kaspad
              │
              ▼
  on-chain set/policy/plan/activation at future DAA
```

### 4.1 Pull方式

各VPSがmetadataを取得するpull方式を標準とする。

中央controllerがroot SSHで全VPSへbinaryをpushする方式を正規OTAにしない。pushは侵害時のblast radiusが大きく、operator independenceを見せかけのものにする。

中央controllerは次だけを行える。

- rollout planの提案
- metrics集約
- pause推奨
- signed metadata公開

各operatorはlocal policyに従い、artifactを独立検証して導入する。

### 4.2 DistributionとActivationの分離

OTA metadataに`effective_daa`があっても、それ自体をchain ruleにしない。

- OTA metadata: いつ導入・準備してよいか
- on-chain policy: いつPALW上で有効か

両方が一致した場合だけmint-grade capabilityをadvertiseする。

---

## 5. Roleと鍵設計

TUFのrole分離、threshold、version、expiry、consistent snapshotの考え方を採用する。ただしPALWはML-DSA-87を必須trust pathとするため、標準TUF実装との完全互換を主張する場合は別途POUFと相互運用仕様が必要である。

### 5.1 Role一覧

| Role | 用途 | 推奨threshold | 鍵保管 | expiry例 |
|---|---|---:|---|---:|
| `ota-root` | role/key/threshold更新 | 3-of-5 | offline、分散保管 | 365日 |
| `targets-node-stable` | kaspad/consensus binary | 3-of-5 | offline/HSM ceremony | 30日 |
| `targets-runtime-stable` | agent/worker/llama.cpp | 2-of-3 | HSM、human approval | 14日 |
| `targets-model` | GGUF/tokenizer/set artifact | 3-of-5 | offline/HSM ceremony | 30日 |
| `targets-canary` | share 0候補 | 2-of-3 | HSM | 7日 |
| `snapshot` | targets metadata集合の一貫性 | 1-of-2 | online HSM | 7日 |
| `timestamp` | 最新snapshotのfreshness | 1-of-2 | online HSM | 24時間 |
| `provenance` | build/SBOM/test attestation | build identity | CI attestor | release単位 |
| `ota-emergency` | target撤回・local stop指示 | 2-of-3 | HSM + incident quorum | 24時間 |

数値は初期推奨値であり、mainnetでは鍵ceremonyと脅威分析により確定する。

### 5.2 鍵用途分離

次の鍵を共有してはならない。

- OTA root signer
- OTA targets signer
- chain governance signer
- emergency halt signer
- network manifest signer
- release provenance signer
- validator signer
- provider owner key
- auditor key
- timestamp/snapshot online key

万能鍵は運用が楽である。侵害者にも同じくらい楽である。

### 5.3 Root rotation

root `N+1`は次の両方を満たす。

```text
valid_signatures(old_root_roles) >= old_threshold
AND
valid_signatures(new_root_roles) >= new_threshold
AND
new_root.version == old_root.version + 1
```

clientは中間rootを順番に取得し、versionを飛ばしてはならない。

### 5.4 Root threshold compromise

root thresholdが侵害された場合、通常OTAによる復旧を信頼しない。

必要な処置:

1. OTA停止
2. PALW compute capability停止またはchain emergency halt
3. out-of-band新root配布
4. operatorによるfingerprint照合
5. fresh client stateでtrust reset
6. 侵害期間artifactの全再監査

---

## 6. Metadata設計

### 6.1 Canonical encoding

PALW固有metadataはstrict Borsh LEを正本とする。

- trailing byte拒否
- decode後のre-encode equality必須
- unknown version/kind拒否
- map禁止またはkey canonical sort
- length/count/resource上限
- signature preimageにnetwork IDとdomainを含める

### 6.2 Root metadata

```rust
pub struct PalwOtaRootV1 {
    pub version: u64,
    pub spec_version: u16,
    pub network_id: Vec<u8>,
    pub expires_unix: u64,
    pub consistent_snapshots: bool,
    pub roles: Vec<PalwOtaRoleV1>,
    pub max_metadata_bytes: u64,
    pub accepted_hash_suites: Vec<u16>,
}

pub struct PalwOtaRoleV1 {
    pub role_name: String,
    pub key_ids: Vec<Hash64>,
    pub threshold: u16,
}
```

### 6.3 Release bundle

```rust
pub struct PalwOtaReleaseBundleV1 {
    pub version: u16,
    pub release_sequence: u64,
    pub release_id: Hash64,
    pub channel: OtaChannel,
    pub network_id: Vec<u8>,

    pub created_unix: u64,
    pub expires_unix: u64,
    pub not_before_daa: u64,
    pub withdraw_after_daa: Option<u64>,

    pub impact_class: OtaImpactClass,
    pub consensus_impact: ConsensusImpact,
    pub restart_scope: RestartScope,

    pub artifact_set_hash: Hash64,
    pub artifacts: Vec<PalwOtaArtifactV1>,
    pub compatibility: PalwOtaCompatibilityV1,
    pub rollout_policy_hash: Hash64,
    pub test_evidence_root: Hash64,
    pub provenance_root: Hash64,
    pub sbom_root: Hash64,
    pub known_issues_root: Hash64,

    pub rollback: PalwOtaRollbackPolicyV1,
}
```

`release_id`は`release_id`自身とsignaturesを除くcanonical bodyから導出する。

```text
release_id = Hash64_k(
  "misaka-palw-ota-release-v1",
  canonical_borsh(release_body)
)
```

### 6.4 Artifact record

```rust
pub struct PalwOtaArtifactV1 {
    pub logical_name: String,
    pub artifact_kind: ArtifactKind,
    pub target_triple: String,
    pub sha256: [u8; 32],
    pub blake2b512: Hash64,
    pub size_bytes: u64,
    pub chunk_manifest_root: Option<Hash64>,

    pub install_class: InstallClass,
    pub relative_install_path: String,
    pub unix_mode: u32,
    pub required: bool,
    pub executable: bool,

    pub semantic_identity: Option<Hash64>,
    pub implementation_identity: Option<Hash64>,
}
```

### 6.5 Exact artifact set

clientはmanifestにあるartifactだけを受理する。

```text
missing != ∅  => reject
extra   != ∅  => reject
mismatch!= ∅  => reject
```

`SHA256SUMS`にあるものだけ比較し、entryがないbinaryを無視する実装は禁止する。

### 6.6 Compatibility

```rust
pub struct PalwOtaCompatibilityV1 {
    pub min_ota_client: u32,
    pub min_kaspad_protocol: u32,
    pub max_kaspad_protocol: u32,
    pub supported_networks: Vec<Vec<u8>>,
    pub supported_arches: Vec<String>,
    pub required_cpu_feature_profile: Hash64,
    pub required_compute_vm_ids: Vec<Hash64>,
    pub supported_compute_set_ids: Vec<Hash64>,
    pub min_db_schema: u32,
    pub max_db_schema: u32,
    pub rollback_floor_release_sequence: u64,
}
```

### 6.7 Rollout policy

```rust
pub struct PalwOtaRolloutPolicyV1 {
    pub policy_id: Hash64,
    pub ring_sequence: Vec<RolloutRingV1>,
    pub min_independent_operators: u16,
    pub min_failure_domains: u16,
    pub max_parallel_percent: u16,
    pub probation_seconds: u64,
    pub min_probation_jobs: u64,
    pub pause_thresholds: HealthThresholdsV1,
    pub auto_rollback_thresholds: HealthThresholdsV1,
}
```

rollout policyもrelease signerの署名対象とする。controllerが配布後に割合や順序を勝手に変えてはならない。

---

## 7. Repositoryとmirror

### 7.1 Repository layout

```text
metadata/
├─ 1.root.borsh
├─ 2.root.borsh
├─ timestamp.borsh
├─ <n>.snapshot.borsh
├─ <n>.targets-runtime-stable.borsh
├─ <n>.targets-node-stable.borsh
├─ <n>.targets-model.borsh
└─ <n>.targets-canary.borsh

targets/
├─ sha256/<digest>/palw-agent
├─ sha256/<digest>/palw-worker
├─ sha256/<digest>/kaspad
├─ sha256/<digest>/runtime-manifest.borsh
├─ sha256/<digest>/golden-vectors.borsh
├─ sha256/<digest>/model.gguf
└─ sha256/<digest>/bundle-metadata/
```

### 7.2 Consistent snapshot

metadataとtarget filenameはversionまたはdigestを含める。

同一URLの内容を上書きする運用は禁止する。

```text
禁止: /downloads/palw-worker-latest
許可: /targets/sha256/<digest>/palw-worker
```

### 7.3 Mirror

mirrorは完全にuntrustedとして扱える設計にする。

- mirrorは署名鍵を持たない
- TLSはdefense in depth
- target hash/lengthはsigned metadataから取得
- mirror間で内容が違えばhashで拒否
- 1 mirror障害時に別mirrorへ切替
- malicious mirrorによるDoSは検出・通報するが、安全に「無視して導入」はしない

---

## 8. Release supply chain

### 8.1 Release前提

- protected branch
- two-person review
- signed source tag
- pinned toolchain
- `--locked`
- hermeticまたはnetwork-denied build
- dependency digest固定
- clean workspace
- reproducible build比較
- SBOM
- SLSA provenance
- in-toto layout/link evidence
- secret scan
- license policy
- vulnerability scan
- signed test result

### 8.2 Build step

```text
source tag
  → dependency resolution freeze
  → builder A
  → builder B (別管理者または別環境)
  → output digest comparison
  → test/conformance
  → bundle assembly
  → exact artifact-set check
  → targets threshold signing
```

### 8.3 Independent rebuild

少なくともstable/mainnet対象は二つの独立build pathで再構築する。

合格条件:

```text
source commit equal
build definition equal
dependency digests equal
artifact digests equal
provenance subjects equal
```

bit-identical buildが実現できない場合、差分理由を分類し、未解明の差が残るartifactをstableへ昇格しない。

### 8.4 Supply-chain evidence

releaseには次を添付する。

- SLSA provenance
- in-toto layout
- build link metadata
- unit/integration/fuzz result attestation
- determinism campaign root
- SBOM（SPDXまたはCycloneDX）
- dependency/license report
- vulnerability scan timestamp
- source tag signature
- artifact signatures
- independent rebuild attestations

### 8.5 Bundle denylist

release bundleへ次を含めない。

```text
**/env.local
**/*.seed
**/keys/**
**/state.env
**/*.log
**/known_hosts
**/*secret*
**/.git/**
**/id_rsa*
**/authorized_keys
**/ssh_config
```

加えて、IP、SSH user、private key path、internal hostname、home directory absolute pathをscannerで検査する。

### 8.6 VPS上build禁止

stable artifactを各VPS上で`git pull && cargo build`してはならない。

理由:

- source/dependencyがfleetで揃わない
- compiler差がruntime classを分断する
- compromised VPSが別binaryを作れる
- provenanceと再現性が失われる

VPSは署名済みimmutable artifactを導入するだけとする。

---

## 9. OTA clientの権限分離

### 9.1 Process分離

```text
palw-ota-fetcher
  user: palw-ota
  network: allowed
  root: no
  write: staging only

palw-ota-installer
  user: root
  network: denied
  write: approved slots/CAS/state only
  input: local UDS only
```

network-facing parserをroot processへ入れない。

### 9.2 Fetcher sandbox

推奨systemd制約:

```ini
User=palw-ota
NoNewPrivileges=yes
PrivateTmp=yes
PrivateDevices=yes
ProtectSystem=strict
ProtectHome=yes
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
ReadWritePaths=/var/lib/palw-ota/staging
MemoryMax=<bounded>
TasksMax=<bounded>
```

### 9.3 Installer sandbox

```ini
User=root
NoNewPrivileges=yes
PrivateNetwork=yes
ProtectHome=yes
ProtectKernelTunables=yes
ProtectKernelModules=yes
ProtectControlGroups=yes
RestrictAddressFamilies=AF_UNIX
ReadWritePaths=/opt/misaka-palw /var/lib/misaka-palw /var/lib/palw-ota
```

root installerはartifactを再検証する。fetcherが「検証済み」と言っても信じない。

### 9.4 Filesystem safety

installerは次をMUSTとする。

- `openat2`相当の`RESOLVE_BENEATH` / `NO_SYMLINKS`
- absolute path拒否
- `..`拒否
- symlink/hardlink/device/FIFO/socket拒否
- setuid/setgid bit除去
- owner/group/modeをmanifestから再設定
- file count、expanded size、path length上限
- temp fileへwrite後にfsync
- directory fsync
- atomic rename
- active slotへの直接write禁止

archive展開より、content-addressed個別file配布を優先する。

---

## 10. Local storageとA/B slot

### 10.1 Directory layout

```text
/opt/misaka-palw/
├─ releases/
│  ├─ <release-id-A>/
│  └─ <release-id-B>/
├─ current -> releases/<release-id>
├─ previous -> releases/<previous-id>
└─ recovery/

/var/lib/misaka-palw/
├─ models/sha256-<digest>.gguf
├─ golden/<root>/
├─ manifests/<hash>.borsh
├─ evidence/
└─ runtime-state/

/var/lib/palw-ota/
├─ trusted/
├─ staging/
├─ state.db
├─ attestations/
└─ quarantine/
```

### 10.2 Immutable release

導入済みrelease directoryはread-onlyにする。

更新は必ず新directoryを作成し、`current` symlinkをatomicに切り替える。

### 10.3 Model CAS

GGUFはrelease directoryへ複製せず、hash-addressed CASに置く。

- whole-file SHA-256
- MISAKA BLAKE2b-512 identity
- optional chunk Merkle root
- immutable mode
- reference count
- active/previous releaseが参照するmodelはGCしない

### 10.4 State DB

`state.db`に最低限以下を保存する。

- trusted root version
- highest timestamp/snapshot/targets version
- highest release sequence per channel
- active release
- previous committed release
- staged release
- probation state
- rollback floor
- last trusted time
- last observed chain DAA
- failure reason

DBはWAL、FULL synchronous、schema fingerprint、transactionを使用し、symlink追従を拒否する。

---

## 11. Metadata取得・検証順序

clientは次の順序を固定する。

```text
trusted root
  → sequential root updates
  → timestamp
  → snapshot
  → delegated targets
  → release bundle
  → target artifacts
```

### 11.1 Root

- `N+1`だけ取得
- old/new threshold両方を確認
- version skip拒否
- size cap
- persistence後に次へ進む

### 11.2 Timestamp

- signature threshold
- version monotonic increase
- snapshot hash/size/version
- expiry
- maximum metadata size

### 11.3 Snapshot

- timestamp記載hash/size/version一致
- targets metadataのversion rollback拒否
-以前存在したroleの不自然な消失を拒否
- mix-and-match防止

### 11.4 Targets

- delegated role/path一致
- signature threshold
- version monotonic
- target hash/size
- custom field schema
- channel/network/arch一致

### 11.5 Persistent anti-rollback

local cacheを削除すれば古いreleaseへ戻れる設計は禁止する。

security-critical counterは別のtamper-evident stateへ保存する。

最低限:

```text
highest_root_version
highest_targets_version_by_role
highest_release_sequence_by_channel
rollback_floor_release_sequence
last_trusted_time
```

### 11.6 Freeze attackと時間

wall clockだけに依存しない。

- metadata expiryはwall clockで確認
- `last_trusted_time`より過去へ戻さない
- NTP sourceを複数化
- chainのfinalized DAA/MTPを補助freshness anchorにする
- `not_before_daa`未到達ならinstall/activationを待機
- local時計が大幅逆行した場合はupdateを停止しalert

時間異常時に「expiryを無視して更新継続」は禁止する。

---

## 12. 大容量artifactの取得

### 12.1 Size cap

download開始前にsigned sizeを確認し、次を計算する。

```text
required_space = target_size
               + extraction_overhead
               + old_release_retention
               + safety_margin
```

不足時はdownloadしない。

### 12.2 Chunk download

GGUF等はchunk manifestを使用できる。

```rust
pub struct PalwOtaChunkManifestV1 {
    pub artifact_sha256: [u8; 32],
    pub artifact_size: u64,
    pub chunk_size: u32,
    pub chunks: Vec<ChunkDigestV1>,
    pub merkle_root: Hash64,
}
```

- chunkごとにhash検証
- resume時も既存chunkを再hash
- whole-file hashを最後に検証
- sparse/truncated fileを拒否
- chunk count上限

### 12.3 Network hardening

- redirectは原則禁止、または同一allowlist内で回数制限
- timeout、rate、total byte cap
- egress proxyの利用を推奨
- private/link-local/loopback宛を拒否
- HTTP response headerを信頼してsize capを緩めない
- partial fileは`.partial`で隔離

---

## 13. OTA state machine

```text
Idle
  ↓
MetadataVerified
  ↓
Downloading
  ↓
Downloaded
  ↓
Staged
  ↓
PreflightPassed
  ↓
WaitingForRolloutWindow
  ↓
Draining
  ↓
Switching
  ↓
Starting
  ↓
Probation
  ↓
Committed
```

失敗経路:

```text
any pre-switch failure
  → Rejected / StagingCleaned

Switching/Starting/Probation failure
  → RollbackPending
  → RolledBack
  または
  → Quarantined
```

### 13.1 Crash recovery

各state transitionをDB transactionで記録し、再起動後に冪等に再開する。

例:

- `Staged`中のcrash: hashを再検証してpreflightから再開
- symlink切替後、service start前のcrash: active linkとDBを照合し`Starting`再開
- probation中のcrash: releaseを未commit扱いにし、boot healthに応じrollback
- rollback中のcrash: previous linkへ収束するまで繰り返す

---

## 14. Preflight

inactive slotを切り替える前に次を全て確認する。

### 14.1 Cryptographic

- root/targets threshold
- metadata version/expiry
- artifact SHA-256
- artifact BLAKE2b-512
- exact artifact set
- release ID再導出
- provenance subject一致
- SBOM root一致

### 14.2 Platform

- network ID
- target triple
- OS/kernel/glibc条件
- CPU feature profile
- free disk/inode
- file ownership/mode
- systemd unit validation
- port/UDS collision
- config schema

### 14.3 PALW runtime

- worker binary hash
- llama.cpp artifact hash
- runtime implementation ID
- model/GGUF hash
- tokenizer/template/decode identity
- golden vectors full root一致
- input-sensitive negative vectors
- NaN/Inf rejection
- exact decode rule
- OpenMP/ISA/FP profile
- UDS request/response binding

### 14.4 Consensus compatibility

- current chain DAA
- currently governing Compute Set/policy/plan
- active VM IDs
- candidate releaseがactive ruleを理解すること
- rollback targetもactive ruleを理解すること
- unknown VM/setをgeneric fallbackしないこと
- DB schema compatibility

### 14.5 Self-testはadmissionではない

local golden PASSは、そのVPSがremote networkに対してhonestである証明ではない。

PASSはlocal install gateとして使い、network admissionはindependent replayとcapability policyで別に行う。

---

## 15. 導入フロー

### 15.1 通常runtime update

1. metadata poll
2. root/timestamp/snapshot/targets検証
3. release bundle検証
4. artifact download
5. inactive release directory構築
6. preflight
7. rollout ring待機
8. `palw-agent`をDrainingへ
9. 新規job assignmentを停止
10. in-flight jobが0になるまで待機
11. timeout時はworkerをkillし、partial receiptを破棄
12. `current`をatomic switch
13. systemd daemon reload
14. `palw-agent`起動
15. local health/golden/UDS確認
16. Shadow replay
17. probation
18. commit
19. old releaseを`previous`として保持

`kaspad`はworker-only updateでは可能な限り継続稼働する。

### 15.2 Active job境界

更新中のjobについて次を禁止する。

- old workerでprefillしnew workerでdecode
- partial traceの引継ぎ
- update timeoutを成功receiptへ正規化
-同じassignmentへの二重署名
- old/new runtime rootの混在

jobは一つの`runtime_implementation_id`へ完全にbindする。

### 15.3 Node binary update

kaspad/consensus binaryは別trackとし、自動restartを原則無効にする。

必要事項:

- operator明示承認
- chain sync状態確認
- quorum/finalityへの影響確認
- peer/version readiness
- DB snapshotまたはshadow migration
- rollback floor確認
- future activation DAAより十分前に配布

---

## 16. Rollout ring

### 16.1 Ring構成

| Ring | 対象 | Compute share | 目的 |
|---|---|---:|---|
| R0 Lab | CI/isolated host | 0 | build/test |
| R1 Shadow Canary | 1〜2独立operator | 0 | live chain replay |
| R2 Limited | fleet 5〜10% | 0または極小 | failure-domain試験 |
| R3 Expanded | 25% | policy上限内 | capacity/latency |
| R4 Majority Ready | 50〜80% | activation準備 | readiness確認 |
| R5 Stable | 全対象 | governed | 通常運用 |

### 16.2 Failure-domain aware rollout

次を同時に更新しない。

- 同じjobのprimary/replica候補の大半
- auditor quorumのthreshold以上
- DNS finality quorumのthreshold以上
- 同一cloud/region/operatorの全instance
- release signerと監視系の同時変更

rollout batchは単なるpercentageではなく、以下で分割する。

```text
operator
cloud provider
region
CPU generation
runtime class
role: provider / auditor / validator
payout identity
```

### 16.3 Promotion gate

次ringへ進むには、少なくとも以下を満たす。

- crash loop 0
- artifact mismatch 0
- same-job deterministic mismatch 0
- unauthorized fallback 0
- receipt schema error 0
- audit failure増加なし
- chain head lag許容内
- p95/p99 latency許容内
- memory/disk増加許容内
- update rollback率閾値未満
- operator/failure-domain独立性維持

### 16.4 Pause

以下の一つでも発生したら自動pauseする。

- full logits root mismatch
- same release IDでartifact digest差
- active job二重署名疑い
- unknown VM/set execution
- verifier quorum低下
- auditor replay capacity不足
- chain reorg/finality異常との相関
- canary自動rollback
- security alert Critical

pauseは次ringへの昇格を止める。既にhealthyなnodeをむやみに一斉rollbackしない。

---

## 17. Consensus activationとの統合

### 17.1 原則

OTA clientは次を行ってはならない。

- Compute Setを自動Active化
- work scaleを変更
- target shareを変更
- bond/slash/timeoutsを変更
- emergency haltを解除
- header/VM activationをlocal configだけで変更

### 17.2 O2 runtime implementation update

同じCompute Setに対する新implementationは次を経る。

```text
artifact OTA
  → local conformance
  → capability candidate
  → Shadow replay
  → cross-machine exact match
  → auditor capacity確認
  → certified implementation profile
  → provider supported profile update
```

### 17.3 O3 new Compute Set

```text
Artifact freeze
  → independent reproduction
  → OTA distribution
  → Compute Set proposal
  → activation certificate
  → Shadow policy (share=0, work=0)
  → soak/audit/failure-rate
  → future-DAA Active policy
  → atomic allocation plan
```

### 17.4 O4 VM/wire update

新opcode、arithmetic、wire field、header preimage変更は、OTAでbinaryを届けるだけでは有効にならない。

必要事項:

1. software rollout
2. readiness telemetry
3. independent audit
4. activation quorum
5. future DAAまたはfresh genesis
6. rollback不能点の明示

unknown VMは近似・Generic fallback・旧意味論で実行しない。

### 17.5 Fleet readiness

activation前にchain上または署名済み公開evidenceとして、次を確認する。

- minimum independent operators
- minimum ready provider capacity
- minimum ready auditor replay capacity
- multiple failure domains
- artifact availability
- golden/conformance campaign
- zero-credit soak duration
- mismatch/failure率
- rollback rehearsal

単なる「nodeの80%がdownload済み」は安全性指標として不足する。

---

## 18. Rollback設計

### 18.1 Rollback可能条件

local auto-rollbackは次を全て満たす場合だけ許可する。

- previous releaseの署名・hashが保存されている
- previous releaseがcurrent chain ruleを理解する
- previous releaseがcurrent DB schemaを読める
- rollback floor以上
- active Compute Set/VMをfail-closedで扱える
- rollbackで二重receiptを作らない

### 18.2 Rollback禁止条件

- hard fork activation後にpre-fork nodeへ戻す
- destructive DB migration後に旧schemaへ戻す
- active VMを理解しないbinaryへ戻す
- security denylist済みreleaseへ戻す
- root/targets metadataのversionを戻す
- model/set identityを同じIDのまま古い内容へ戻す

### 18.3 Rollback不能時

安全な旧版へ戻れない場合は次とする。

```text
palw compute capability = OFF
validator/base node     = 継続可能なら継続
new job admission       = STOP
receipt signing         = STOP
operator alert          = Critical
```

「動かないより古い版でも動かした方がよい」は、コンセンサスシステムではfork生成器になり得る。

### 18.4 Model rollback

model artifactをin-placeで戻さない。

旧Compute Setを再利用する場合:

- 旧setがRetiredでないこと
- 新しいfuture policy sequence
-必要ならallocation planでshareを戻す
- current governing viewへ正しく反映

Retiredはterminalとし、同じIDを復活させない。

### 18.5 Auto-rollback trigger

初期例:

- service start失敗
- probation中のcrash loop
- local golden mismatch
- UDS protocol mismatch
- model/runtime hash mismatch
- memory limit超過の反復
- canary same-job mismatch
- active chain compatibility failure

compute correctness疑いがある場合、単純rollbackだけでなくfleet pauseとevidence保存を行う。

---

## 19. Emergency response

### 19.1 Active artifactの脆弱性

1. rollout停止
2. affected releaseをtargetsから撤回
3. `ota-emergency` metadata公開
4. affected nodeをcompute quarantine
5. positive creditへ影響する場合はon-chain Emergency Halt
6. evidence/receipt保全
7. patch build、独立再現、threshold signing
8. Shadow canary
9. future policyで再開

### 19.2 Timestamp/Snapshot key compromise

- online key revoke
- root roleで新keyを指定
- metadata versionを前進
- short expiryで再発行
- clientは新root後にstale cached timestamp/snapshotを破棄
- targets artifactは再署名不要でも、repository consistencyを再構築

### 19.3 Targets key compromise

- channel即時freeze
- on-chain impact評価
- root metadataでtargets key revoke/rotate
- compromise期間に署名された全releaseをdenylist
- trusted releaseへlocal rollback、またはcompute OFF
- root of evidenceから再監査

### 19.4 Build pipeline compromise

- builder identity revoke
- affected provenance range特定
- independent source-to-binary rebuild
- SBOM/dependency再評価
- artifact hashが一致しても、意図したsource/buildか再確認
- positive-value影響時はEmergency Haltを検討

### 19.5 Root threshold compromise

通常OTAで復旧しない。out-of-band root resetを行う。

### 19.6 Incident evidence

保全対象:

- trusted metadata versions
- downloaded metadata bytes
- target hashes
- installed release IDs
- systemd journal該当範囲
- update state DB snapshot
- runtime self-test
- receipt/audit IDs
- chain DAA/block hash
- operator action log

promptやprivate job dataを無制限にincident bundleへ含めない。

---

## 20. DB migration

### 20.1 原則

PALW workerはできるだけstatelessに保つ。

state migrationが必要なcomponentは、migrationをartifactと同じreleaseへbindingする。

```rust
pub struct PalwOtaMigrationV1 {
    pub migration_id: Hash64,
    pub from_schema: u32,
    pub to_schema: u32,
    pub reversible: bool,
    pub migration_binary_hash: Hash64,
    pub precheck_root: Hash64,
    pub postcheck_root: Hash64,
}
```

### 20.2 Destructive migration

破壊的migrationは自動実行しない。

必要事項:

- operator承認
- backup/snapshot
- restore試験
- rollback不能点表示
- maintenance window
- chain compatibility
- migration後validation

### 20.3 Shadow migration

mainnet node DBは可能な限り次を使う。

- new DBへcopy/rebuild
- shadow read comparison
- dual-write期間
- atomic authority switch
- old DB retention

---

## 21. Observability

### 21.1 Metrics

```text
palw_ota_metadata_verify_total{role,result,reason}
palw_ota_target_download_bytes_total{artifact}
palw_ota_target_hash_mismatch_total{artifact}
palw_ota_state{state,release_id}
palw_ota_active_release_info{release_id,channel}
palw_ota_rollback_total{reason}
palw_ota_quarantine{reason}
palw_ota_probation_jobs_total{result}
palw_ota_determinism_mismatch_total{runtime_profile}
palw_ota_fleet_ready_operators
palw_ota_fleet_ready_capacity
palw_ota_auditor_ready_capacity
palw_ota_update_duration_seconds{phase}
palw_ota_disk_required_bytes
palw_ota_disk_available_bytes
```

### 21.2 Logs

記録可:

- release ID
- artifact hash
- metadata version
- state transition
- DAA/block hash
- error code
- runtime implementation ID

記録禁止:

- signing private key
- session token
- raw secret
- private prompt/output
- internal SSH key path
- signed URL credential

### 21.3 Fleet dashboard

最低表示:

- releaseごとのnode数
- operator/failure-domain分布
- current/staged/probation/rollback/quarantine
- root/targets version
- last poll freshness
- deterministic mismatch
- active Compute Set readiness
- auditor capacity
- activation DAA countdown

---

## 22. 検証計画

## 22.1 Unit / property

- canonical Borsh roundtrip
- trailing bytes拒否
- unknown version/kind拒否
- duplicate key/signature count拒否
- threshold計算
- version overflow
- count/length cap
- release ID再導出
- artifact-set exactness
- path validation
- rollback floor
- state machine invalid transition
- crash recovery idempotency

## 22.2 Metadata attack tests

| ID | 攻撃 | 期待結果 |
|---|---|---|
| MT-01 | root version rollback | reject、alert |
| MT-02 | root version skip | reject |
| MT-03 | old thresholdだけでnew root署名 | reject |
| MT-04 | new thresholdだけでnew root署名 | reject |
| MT-05 | expired timestamp | freeze疑い、停止 |
| MT-06 | timestamp version同値で内容差 | reject |
| MT-07 | snapshot mix-and-match | hash/versionでreject |
| MT-08 | delegated path外target | reject |
| MT-09 | target sizeよりendless response | byte capでabort |
| MT-10 | wrong network/arch | reject |
| MT-11 | old valid targets再提示 | persistent versionでreject |
| MT-12 | compromised mirror | signature/hashでreject |

## 22.3 Artifact tests

| ID | ケース | 期待結果 |
|---|---|---|
| AT-01 | binary 1 byte改変 | reject |
| AT-02 | GGUF 1 byte改変 | reject |
| AT-03 | missing worker | reject |
| AT-04 | extra executable | reject |
| AT-05 | symlink archive | reject |
| AT-06 | `../` traversal | reject |
| AT-07 | setuid binary | bit除去またはreject |
| AT-08 | file count bomb | reject |
| AT-09 | chunk corruption | chunk再取得、whole hash必須 |
| AT-10 | resume file改変 | 再hash後reject |
| AT-11 | env.local混入 | release build fail |
| AT-12 | wrong glibc/CPU profile | preflight reject |

## 22.4 Power-loss / filesystem tests

powerを次の各点で強制断する。

- metadata persist前後
- chunk write中
- artifact fsync前後
- release directory完成前
- symlink switch直前/直後
- service stop後
- service start前
- probation中
- commit marker書込中
- rollback中

全ケースで次を満たす。

- active releaseが破損しない
-二つのreleaseが同時activeにならない
- state DBとfilesystemが収束する
- partial receiptを発行しない

## 22.5 Runtime determinism

- same release、same job、multiple VPSでfull 64-byte root一致
- restart前後一致
- cold/warm cache一致
- concurrency条件一致
- input変更でroot変化
- OpenMP/ISA/build drift検出
- wrong model/tokenizer拒否
- early EOG rule一致
- exact decode数一致

## 22.6 Rollout tests

- ring pause
- canary rollback
- controller loss
- mirror loss
- one operator offline
- one region outage
- auditor threshold維持
- provider capacity低下
- simultaneous update制限
- stale rollout plan拒否
- unauthorized rollout policy変更拒否

## 22.7 Consensus tests

- install済みだがactivation前は旧rule維持
- activation DAA跨ぎ
- reorgでactivation前へ戻る場合のgoverning view
- stale policy/planをheaderが指定してもreject
- unknown VM fail closed
- zero-share setからticket生成不可
- Emergency Haltで新ticket停止
- Retired set再有効化不可
- rollback targetがactive VM非対応ならcompute quarantine
- old/new implementation receipts混在拒否または明示class分離

## 22.8 Key compromise rehearsal

- timestamp key revoke/rotate
- snapshot key revoke/rotate
- targets 1 key compromise、threshold未満
- targets threshold compromise
- root 1 key compromise
- root threshold compromiseのout-of-band recovery
- expired metadata下でのoperator runbook
- key revocation propagation
- denylisted releaseの再提示

## 22.9 Performance

測定対象:

- metadata verification CPU/latency
- chunk download/resume
- whole GGUF hash時間
- staging disk peak
- drain時間
- worker restart時間
- probation throughput
- fleet rollout時間
- mirror failover
- state DB growth

性能不足を理由にsignature、whole-file hash、golden vector、fsyncを黙って省略しない。

---

## 23. Release gate

### G0 Structural

- schemas
- canonical encoding
- state machine
- path/resource caps
- unit/property tests

### G1 Supply chain

- signed tag
- pinned toolchain
- locked dependencies
- SBOM
- provenance
- in-toto evidence
- secret scan
- independent rebuild

### G2 Closed A/B

- two-slot install
- power-loss matrix
- rollback
- root/targets rotation
- exact artifact-set verification

### G3 PALW Shadow

- independent operator canary
- zero-credit
- deterministic replay
- capacity and DA
- 72h以上soak

### G4 Public no-value

- permissionless mirror
- malicious metadata tests
- operator runbook only join
- failure-domain rollout
- key compromise rehearsal

### G5 Limited economics

- positive credit cap
- on-chain future DAA activation
- Emergency Halt
- rollback limitations公開
- independent audit

### G6 Mainnet candidate

- critical/high finding 0
- audit remediation retest
- final root/key ceremony
- reproducible release by二者以上
- public transparency records
- 30日以上staging soak
- incident/rollback/halt rehearsal

---

## 24. Runbook

### 24.1 Normal release

1. release scope freeze
2. source review/tag
3. build A/B
4. test/evidence
5. bundle exactness
6. threshold sign
7. candidate metadata publish
8. R0/R1
9. pause window
10. R2/R3
11. fleet readiness
12. chain Shadow/Active governance
13. R4/R5
14. release report publish

### 24.2 Pause

1. timestamp/snapshot更新は継続
2. targets promotionを停止
3. current ringを固定
4. evidenceを保全
5. assignment/capacityを必要に応じ縮小
6.原因分類
7. resumeまたはwithdraw

### 24.3 Local rollback

1. compute admission停止
2. in-flight drain/abort
3. previous compatibility確認
4. current→previous atomic switch
5. service start
6. golden/UDS/chain compatibility
7. probation
8. incident record

### 24.4 Emergency halt

1. severity Critical宣言
2. OTA rollout停止
3. affected target withdraw
4. compute capability停止
5. chain impactなら0x44相当Emergency Halt
6. auditor/providerへ通知
7. evidence snapshot
8. patch/recovery release
9. Shadow再開
10. future policyで復帰

### 24.5 Root rotation

1. new key generation ceremony
2. old/new role metadata作成
3. old threshold署名
4. new threshold署名
5. sequential version publish
6. independent client verification
7. operator rollout
8. old key revoke/保管
9. ceremony transcript保存

---

## 25. 注意事項

### 25.1 OTA成功をPALW安全性と混同しない

署名済みbinaryが正しく導入されたことと、LLM workが正しく計算されたことは別問題である。

### 25.2 全fleet自動更新は中央集権化を招く

mainnet operatorへ強制auto-updateを要求すると、release authorityが実質的なnetwork operatorになる。stable consensus-impacting updateはoperatorの明示承認とfuture activationを必要とする。

### 25.3 相関障害

同じbinaryを一斉導入すると、同じbugで全provider/auditorが停止する。rollout ringとfailure-domain分割は速度のためではなく、network survivalのためにある。

### 25.4 Rollbackできない更新がある

VM、wire、DB schema、genesis、activation semanticsを変更した後は、旧版へ戻す方が危険なことがある。UIに「rollback」ボタンがあるから安全、という人類らしい誤解を禁止する。

### 25.5 Old version fallback禁止

新runtimeが失敗したときOllama、共有API、旧workerへsilent fallbackしてはならない。capabilityを停止する。

### 25.6 Set identityの上書き禁止

weights/tokenizer/decode/trace ruleを変更して既存set IDを維持してはならない。

### 25.7 Time source

expiryを使う以上、時刻異常はsecurity eventである。NTP failureを理由にexpiry checkを無効化しない。

### 25.8 Diskと帯域

GGUFは大容量である。old/current/stagedを同時保持するdisk設計が必要で、容量不足時にactive modelを先に削除してはならない。

### 25.9 License/provenance

model、tokenizer、llama.cpp、依存libraryの配布権とlicenseをrelease gateへ含める。

### 25.10 Telemetryはauthorityではない

`ready=true`、GPU名、driver名、binary hash自己申告は観測値であり、remote attestationまたはcompute proofではない。

### 25.11 Emergency signerを万能鍵にしない

OTA撤回とchain Emergency Haltは別権限にする。単一鍵が更新配布も報酬停止も行える設計を避ける。

### 25.12 Documentation drift

source params、CLI default、systemd unit、operator guide、ports、model manifestを同じrelease pipelineで更新し、文書だけ古い状態を防ぐ。

---

## 26. 実装優先順位

### Phase 0: Metadata core

- canonical schemas
- ML-DSA-87 role verifier
- threshold/root rotation
- timestamp/snapshot/targets
- persistent anti-rollback
- attack test vectors

### Phase 1: Local A/B

- fetcher/installer分離
- versioned release directory
- model CAS
- atomic switch
- state DB
- power-loss recovery
- local rollback

### Phase 2: PALW runtime integration

- drain protocol
- runtime implementation ID
- golden/conformance preflight
- capability quarantine
- no-fallback enforcement
- receipt runtime binding

### Phase 3: Fleet rollout

- signed rollout policy
- failure-domain rings
- metrics/pause/rollback
- operator dashboard
- signed installed-release telemetry

### Phase 4: Consensus integration

- Compute Set proposal/cert/Shadow
- future DAA policy
- allocation plan
- readiness evidence
- Emergency Halt
- governing-view equality

### Phase 5: Supply-chain/mainnet

- hermetic build
- two-party reproduction
- SBOM/SLSA/in-toto
- transparency service
- offline key ceremony
- compromise rehearsal
- external audit

---

## 27. 初期testnet推奨値

以下は検証開始用であり、mainnet定数ではない。

```text
root threshold:                 3-of-5
runtime stable targets:        2-of-3
node/model stable targets:     3-of-5
timestamp expiry:              24h
snapshot expiry:               7d
stable targets expiry:         14-30d
retained committed releases:   2
worker update max parallel:    10%
validator update max parallel: 5%
probation:                      30min AND 100 jobs
Shadow soak:                    >=72h closed, >=7d public
mainnet staging soak:           >=30d
minimum independent operators: >=3
minimum failure domains:       >=3
same-job mismatch tolerance:   0
artifact hash mismatch:        0
silent fallback tolerance:     0
```

---

## 28. 最終受入チェックリスト

### Cryptographic

- [ ] rootをout-of-bandでpinした
- [ ] role/key/thresholdが分離されている
- [ ] root rotationはold/new thresholdを要求する
- [ ] version/expiry/rollback/freezeを検査する
- [ ] target hashとsizeをsigned metadataへbindした
- [ ] missing/extra artifactをfail closedにした

### Supply chain

- [ ] signed source tag
- [ ] pinned toolchain / locked dependencies
- [ ] hermetic build
- [ ] independent rebuild
- [ ] SBOM
- [ ] SLSA provenance
- [ ] in-toto evidence
- [ ] secret/topology scan
- [ ] release signerとoperator keyを分離

### Client

- [ ] fetcherはunprivileged
- [ ] installerはnetwork denied
- [ ] safe path resolution
- [ ] A/B immutable slots
- [ ] atomic switch / fsync
- [ ] crash recovery
- [ ] persistent anti-rollback state
- [ ] whole-file hash

### PALW

- [ ] job drainとpartial receipt破棄
- [ ] runtime implementation ID binding
- [ ] model/tokenizer/decode/trace identity検証
- [ ] golden vectors full match
- [ ] unknown VM/set fail closed
- [ ] no Ollama/shared API/silent old-version fallback
- [ ] compute quarantineがvalidator livenessから分離

### Rollout

- [ ] signed rollout plan
- [ ] operator/failure-domain aware rings
- [ ] threshold roleを同時更新しない
- [ ] pause/rollback metrics
- [ ] canary zero-credit
- [ ] public soak

### Consensus

- [ ] OTA installとchain activationを分離
- [ ] new semantic artifactはnew Compute Set ID
- [ ] future DAA policy
- [ ] atomic allocation plan
- [ ] governing-view equality
- [ ] Emergency Halt
- [ ] rollback floor

### Incident

- [ ] timestamp/snapshot key rotation rehearsal
- [ ] targets compromise rehearsal
- [ ] root compromise out-of-band runbook
- [ ] build pipeline compromise runbook
- [ ] local rollback rehearsal
- [ ] chain halt/recovery rehearsal
- [ ] evidence preservation

---

## 29. 未決事項

実装前に次をADRで固定する。

1. OTA metadataを標準TUF互換にするか、TUF semanticsを持つPALW Borsh/ML-DSA POUFにするか
2. root/targetsの最終thresholdと鍵管理主体
3. on-chain governanceとOTA targets signerの組織分離
4. installed-release readinessをchainへどう表現するか
5. model chunk DAをOTA repositoryとPALW DAで共用するか
6. runtime implementation profileをCompute Set内でどうcertifyするか
7. emergency target withdrawalと0x44 haltの連携
8. trusted timeのchain MTP/DAA利用範囲
9. node DB migrationのshadow/dual-write方式
10. transparency logの運営者と監査方法

---

## 30. 参考規格・資料

- The Update Framework Specification 1.0.x
- TUF Roles and Metadata
- Uptane Standard for Design and Implementation 2.1.0
- SLSA Build Provenance v1.2
- in-toto software supply-chain layout and verification
- systemd-sysupdate / A-B resource update model
- MISAKA PALW Version 4 Governance、Versioning、Migration
- MISAKA PALW release readiness audit
- MISAKA PALW VPS Canonical Worker Design v0.1

---

## 31. 最終判断

PALW OTAの初期実装は、次の範囲ならGOとする。

```text
signed metadata
+ exact artifact set
+ unprivileged fetcher / offline root installer
+ immutable A/B release
+ full local preflight
+ zero-credit canary
+ failure-domain rollout
+ compute quarantine
+ chain activation separation
```

次の状態ではpositive-value mainnetへ使用しない。

- 単一online keyでstable artifactを承認できる
- VPSでbuildする
- `latest` URLを直接上書きする
- missing artifactを無視する
- active directoryへin-place updateする
- partial jobをupdate後に継続する
- runtime failure時にOllama/共有API/旧workerへfallbackする
- model/tokenizer変更を同じCompute Set IDで配布する
- OTA導入だけでwork/rewardを有効化する
- rollback不能点が未定義
- key compromise rehearsalを行っていない

PALWでは、更新速度よりも、異なるoperatorが同じartifact identityと実行意味論へ安全に収束し、失敗時にはcomputeだけを止めてbase networkを生存させることを優先する。

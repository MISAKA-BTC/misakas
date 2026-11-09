# RFC-v2-0015 — 設計凍結・延期・不採用・未採択・変更管理

## 2026-11-09公開資料・一人前提の追加境界

[採択RFC-0050](../rfc/0050-public-materials-and-one-honest-panel.md)と[ADR-0218](../adr/0218-public-materials-and-one-honest-panel.md)により、独立選出されたPanelの中の一人が、モデルを事前保有せずに公開資料を取得して全体検証できる新profileを採択する。今回のユーザー採択を0050／0218に記録する。k_min=1は新profileの意味変更、mや資源値・有効化は別記録とし、旧75／41、署名、禁止方式、延期機能、過去レビュー・測定を保持する。 成功の規範所有者は[20](0020-verification-gated-final-and-adversarial-models.md)、公開方針は[v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)。以下の先行採択・基礎契約と衝突する箇所は、この新profileに限って本境界と16〜20を適用する。旧claim・歴史付録は保持し、状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

## 2026-11-09新profileの追加境界

[採択RFC-0049](../rfc/0049-palw-execution-integrity-and-unilateral-refutation.md)と[ADR-0217](../adr/0217-palw-execution-integrity-and-unilateral-refutation.md)により、[20](0020-verification-gated-final-and-adversarial-models.md)の実行整合性・一人の反証・検証未成立時の成功停止を適用する。今回の明示指示を0049/0217へ記録し、旧署名、既存禁止方式、延期機能、過去の採択・検査証拠を保持する。 詳細の規範所有者は16〜20であり、本追記で別の成功条件を定義しない。旧claim・歴史付録の意味とbytesは保持する。状態は **ADOPTED_DESIGN_NOT_ACTIVATED**。

原統合: 2026-10-29 / 採択改訂: 2026-11-08 / 原入力: `misakas-pre(5).zip`

[20本の索引](README.md) · [旧→新対応表](MAPPING.md) · [編集・移行規則](MIGRATION.md)

> **2026-10-29原統合の編集範囲:** 文書の統合・重複する責務の整理。技術仕様の採択、パラメーター変更、runtime有効化、試験成功の宣言ではない。
> **状態:** 採用済み・Draft・隔離実装・履歴を含む統合編集版。各条項の採択・置換関係を継承する。過去の「実装済み」「休眠」「未試験」等は、その記録時点の状態である。
> **番号:** 本書は`RFC-v2`。原条文中の`RFC-xxxx`と`§`は旧版を指す。旧番号・wire・署名・fence名は再採番しない。

## 2026-11-08採択による適用範囲

今回の設計変更は明示的なユーザー採択と新規RFC16〜20／ADR0215に基づく限定改定である。採用したのはNode非保有、非共有・寡占脅威、正常非対話positive、negative対話、検証先行Finalであり、計算validity proof・廃止済みモデル審査・media／モデル改善の自動再開は採用しない。

**優先関係:** 2026-11-08の先行採択はADR-0215と公開裁定v0.2を根拠とする。今回の公開資料・一人前提の新profileには冒頭の0050／0218・RFC16〜20・[公開裁定v0.4](../design/palw/consensus/public-adjudication-policy-v0.4.md)を優先する。以下の基礎契約・旧profile記録と原付録は保持し、旧署名や旧Finalの意味を遡及変更しない。設計採択は実装・測定・有効化の完了を意味しない。

**原文収録元:** [旧0003](../rfc/0003-palw-generative-model-classes.md)、[旧0004](../rfc/0004-palw-model-improvement.md)、[旧0005](../rfc/0005-palw-ml-vm.md)、[旧0015](../rfc/0015-panel-free-permissionless-verification.md)、[旧0016](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)、[旧0017](../rfc/0017-model-eligibility-examination-and-renewal.md)、[旧0021](../rfc/0021-design-freeze-and-documentation-governance.md)、[旧0022](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md)、[旧0027](../rfc/0027-miner-account-bond-and-panel-incentives.md)、[旧0039](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md)。

**関連RFC:** [01 推論・生成Job・入出力](0001-inference-and-generative-jobs.md) / [02 TIR・Kernel・共有Queryデータ](0002-tensor-ir-and-shared-query-data.md) / [03 モデル登録・変換・Artifact](0003-model-onboarding-and-artifacts.md) / [04 モデル二階層・年次カタログ](0004-model-tiers-and-annual-catalog.md) / [05 資源上限・長文脈メモリ・計測](0005-bounded-resources-and-long-context.md) / [06 Panel参加・抽選・確率的検証・認証Receipt](0006-panels-and-probabilistic-verification.md) / [07 Claim・EXEC・仕事量・回答会計](0007-claims-exec-and-answer-accounting.md) / [08 Claim依存・Prefix生成元・未解決責任](0008-claim-dependencies-and-liability.md) / [09 Remote Client・SDK・非保管型操作](0009-remote-client-and-sdk.md) / [10 DAG合意・Gate C・Native EVM決済](0010-dag-consensus-and-native-settlement.md) / [11 Bond予算・配分・Fee同意・決済会計](0011-bond-budgets-fees-and-payments.md) / [12 認証State・保存・Pruning・IBD](0012-authenticated-state-storage-and-ibd.md) / [13 公開Bootstrap・Genesis・有効化とRelease接続](0013-permissionless-bootstrap-and-release.md) / [14 Court統合契約](0014-court-master-contract.md)。

## 1. 一つの管理RFC、異なる状態

旧0021／0022を中核に、凍結・休眠・初期mainnetからの除外・不採用・撤回・未採択を本RFCに集約する。これらを単一の「実装しない」に潰さない。

| 状態 | 意味 | 実装・運用での扱い |
| --- | --- | --- |
| DESIGN_FROZEN | 採用済み構造・意味・責任の通常変更を禁止 | 既存設計の実装、意味不変の修正、試験は続行可 |
| DEFERRED_POST_MAINNET | 初期mainnetの肯定的完成条件から除外 | 無効化確認と共通部の回帰は必要。開始後も自動解禁しない |
| DEFERRED_INACTIVE | 条件未達のため有効化しない | 永久不採用やmainnet後への延期とは別 |
| REJECTED / WITHDRAWN | 不採用、廃止、または後続方針で撤回 | 現行要件として再実装しない。必要な旧履歴互換は維持 |
| PROPOSED / NOT_ADOPTED | 比較案・未採択、または隔離実装までの承認 | 恒久廃止と断定せず、本番採択・有効化に読み替えない |

## 2. 集約台帳

| ID | 対象 | 旧RFC等の出典 | 現在の方針上の扱い | 残すもの |
| --- | --- | --- | --- | --- |
| P01 | 採用済み設計全般 | 0021、PRINCIPLES §11、ADR-0181 | DESIGN_FROZEN | 原則に沿う実装・修正・検証 |
| P02 | 画像生成 | 0003 Part II.1、0022、ADR-0182 | DEFERRED_POST_MAINNET / DISABLED | text、共通job、必要なTIR |
| P03 | 動画生成 | 0003 Part II.6、0022、ADR-0182 | DEFERRED_POST_MAINNET / DISABLED | 共通の入力・出力形式と履歴 |
| P04 | モデル改善protocol | 0004、0022、ADR-0182 | DEFERRED_POST_MAINNET / DISABLED | 独立した不変モデルの登録、通常の変換・較正 |
| P05 | Panel=0 mode | 0015 §1・§13・N5改定 | DEFERRED_INACTIVE、独自gateと明示承認が必要 | 06のPanel経路、11の採用済み共通会計 |
| P06 | 汎用ML VM | 0005、0011 §16、ADR-0172 | REJECTED | 02のTIR・versioned kernel |
| P07 | モデル単位の資格審査・更新・失効・昇格 | 0017の旧案、PRINCIPLES §9、ADR-0179 | WITHDRAWN / 不採用 | 04の二階層、全モデルのclaim検証 |
| P08 | LLM計算のZK／SNARK／STARK／zkVM／PCS等のvalidity proof | PRINCIPLES §8、0007等の旧案、0015 §10 | 使用しない方針。将来研究候補としても復活させない | hash・署名・Merkleによるデータ認証、直接spot check |
| P09 | MISAKA Torrent・専用Seeder・Seeder報酬、モデル取得可否の合意gate | 0008・0014・0015等、ADR-0177 | 廃止・撤回 | 固定identity、claim固有の開示義務、bond配分 |
| P10 | 同じモデルID／head／rootを改善で上書き | 0004の旧方式、ADR-0175 | 撤回した方式を復活させない | 独立した不変モデルとしての登録 |
| P11 | DNS validator overlay／DNS最終承認・reorg veto | 0012 | 退役方針。実移行状態は別確認 | 旧履歴、返還すべきbond、互換移行 |
| P12 | 新Miner Account Bond・新Panel追及報酬案 | 0027 | PROPOSED / NOT_ADOPTED | 比較・計測資料。既存bond制度は維持 |
| P13 | Provider役務の有限化・未払権利取消し／custody移管A/B | 0039 §2・§3 | NOT_ADOPTED | 11へ移した採択済みverifier fee／provider fee-only |

P05を今回新たに「mainnet後」と決定したわけではない。P11は旧ネットワークで既に全移行が完了したとの宣言ではない。P12／P13は永久不採用の決定ではない。

## 3. 凍結するのは設計で、未完の実装ではない

合意・状態遷移、責任分界、モデル制度、認証・裁定、予算、wire意味、不変条件を通常変更しない。明示された調整変数は、構造と不変条件を維持し、台帳・測定・影響・試験・承認を伴って調整する。

重大な問題による構造変更は、新規RFCと新規ADRのレビュー・明示採択が必要である。「文書を短くする」「実装しやすい」だけでこの例外を使わない。今回の15本化は文書の配置・編集であり、技術方式の再設計ではない。

## 4. 初期Mainnetから外す範囲

画像・動画jobの提供、新規受理、実行、claim、固有の経済／合意credit、および改善protocolのepoch・学習／蒸留連携material・候補・評価・選択・固有報酬等を対象とする。肯定的E2E完成は初期mainnetの必須要件にしない。

一方、CLI、SDK、RPC、gateway、登録、fold、claim、報酬、再起動・再同期等から迂回して開かないことを確認する。共通fenceを一律停止してtextを壊したり、共通fenceを開いて延期機能を同時解禁したりしない。

音声、embedding、vision入力等の扱いをこの延期から勝手に拡大解釈しない。年次カタログ追加、固定モデル登録、既存モデルの較正・変換・メモリ最適化もモデル改善protocolとは別である。

## 5. Draft・隔離実装・本番承認

旧0030〜0035のcourt補完、0036〜0038のstate／IBD／回答会計等は、原文のDraft・Proposed・詳細仕様化・隔離実装という範囲を保持する。これらを「実装しない方針」に追加しない。所属先は14、12、07であり、本RFCは承認状態を管理する。

旧0039は採択済みfee範囲と未採択A/Bを分割する。旧0040／0041の採択記録も維持するが、実装・試験・有効化の完了とはしない。古い冒頭Statusと後続記録が食い違う場合、具体的な採択・置換条項と対象版を照合し、番号や日付だけで勝手に優先しない。

## 6. 文書の権威と番号

PRINCIPLES、明示的な採択・置換関係、対応するSpec／ADRの役割を維持する。15本の統合RFCに収録されたこと自体は、新しい技術的採択ではない。監査・evidenceは実行条件付きの観測、historyは当時の記録である。

新番号は`RFC-v2-0001`〜`RFC-v2-0015`とする。旧`RFC-0001`〜`RFC-0041`とは別namespaceで、旧番号を再利用しない。旧path、wire ID、codec tag、署名domain、fence名、source中のRFCラベルを一括置換しない。

通常の更新は該当する15本へ集約する。凍結例外の新規変更提案は別のchange-proposalとして発行・採択し、内容を15本へ反映する。15本は現行の分野別本文数であり、変更提案や歴史資料の総数の上限ではない。新規番号の予約は統合先branchと照合する。

## 7. 受入と再開

41本の移行先が漏れていないこと、sourceの原文hash、重複しない所有先、参照・境界の保全を確認する。廃止方式を通常経路から再有効化しないことは、実装側で別途確認する。

延期機能を再開する場合は、機能別の範囲、責任者、release、実装、実測、肯定／否定試験、復旧、互換移行、独立reviewと明示的承認を要する。mainnet開始や文書mergeだけで再開しない。

## 8. 元の正式な判断資料

[PRINCIPLES](../PRINCIPLES.md)、[設計凍結](../governance/design-freeze.md)、[mainnet延期方針](../governance/post-mainnet-scope.md)、[文書の役割](../governance/document-lifecycle.md)、[パラメーター台帳](../governance/parameter-register.md)を保持する。これらの決定資料を本整理で上書きしない。

---

## 2026-11-08のEXEC・1 BPS追加採択

ユーザーの今回の指示に基づく[RFC-0048](../rfc/0048-exec-credit-schedule-and-one-bps.md)と
[ADR-0216](../adr/0216-exec-credit-schedule-one-bps-and-court-capacity.md)を、
新EXEC／claim profileの設計として採択する。通常編集先は引き続き20冊である。

credit・future schedule・明示ScheduleId・現在slot生成・1 BPS目標・court全体容量の条件を追加する。
Node非保有、global承認の限界、計算validity proofの禁止、非BFT、一つの鎖、
モデル二階層、media／モデル改善の延期を変更しない。
採択は性能達成や公開activationではなく、値未確定のlead／horizon／grace等を
未承認の実装既定値で補う許可にもならない。

---

## 原条文の統合収録

以下は内容を落とさないための原条文である。見出しの深さとリンク先だけを編集し、数式・コード・値・記録を再解釈していない。旧い方針・候補・試験記録も含むため、本文の責務・適用範囲と15の台帳を先に読む。原文内の相互参照は保存した旧ファイルへ戻る。新しい編集先は本書先頭の20本の索引・旧新対応表で確認する。

### 収録箇所

| 原文 | 行 | 入口 |
| --- | --- | --- |
| 旧0003 | 1071–1793 | [II.1 Image generation](#source-v1-0003-L01071) |
| 旧0003 | 2253–2289 | [II.6 Video generation](#source-v1-0003-L02253) |
| 旧0004 | 1–1155 | [RFC-0004: PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR](#source-v1-0004-L00001) |
| 旧0005 | 1–15 | [RFC-0005: PALW VM（不採用）](#source-v1-0005-L00001) |
| 旧0015 | 1–379 | [RFC-0015 — Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](#source-v1-0015-L00001) |
| 旧0015 | 525–543 | [8.3.7 予約額引下げと公開検証・責任担保・共通DAA拘束の一体設計](#source-v1-0015-L00525) |
| 旧0015 | 680–953 | [9. work weight、PALW blockとsettlement](#source-v1-0015-L00680) |
| 旧0016 | 112–156 | [2. Panel=0の前提（決定済み）](#source-v1-0016-L00112) |
| 旧0017 | 44–46 | [廃止した審査の履歴](#source-v1-0017-L00044) |
| 旧0021 | 1–23 | [RFC-0021 — 設計凍結・例外変更手続きと文書の役割分離](#source-v1-0021-L00001) |
| 旧0022 | 1–28 | [RFC-0022 — 初期 mainnet から画像・動画・モデル改善を外す適用範囲](#source-v1-0022-L00001) |
| 旧0027 | 1–437 | [RFC-0027 — Miner Account Bond と Panel 追及報酬: 10万の基礎担保、責任連動の追加拘束、REAL継続性](#source-v1-0027-L00001) |
| 旧0039 | 24–50 | [2. Provider V2 の署名済み上限](#source-v1-0039-L00024) |

---

<a id="source-v1-0003-L01071"></a>

### 旧RFC-0003 / 原文1071–1793行

出典: [0003-palw-generative-model-classes.md](../rfc/0003-palw-generative-model-classes.md)。配置理由: 画像・動画の延期profileを管理RFCへ。

#### II.1 Image generation

##### 1. ImageJob

```
ImageBodyV1 {
  prompt_token_ids_hash: Hash64, prompt_tokens: u32,       // the user's prompt ids under the class tokenizer; carriage as FP
  negative_token_ids_hash: Hash64, negative_tokens: u32,   // empty unless the class offers true CFG
  guidance_q: u16,                                         // guidance in units of 1/16, on the class's grid
  image_index: u16,                                        // R's `position` (§I.1.3); 0 for a single image
  sampler_id: Hash64,                                      // MUST equal the class's sampler descriptor id
  steps: u16,                                              // MUST be in the class's offered set
  width: u16, height: u16,                                 // MUST equal the class's resolution
  output: u8,                                              // ImageRgb8 = 1; nothing else in v1
}
```

**Model and class.** The job names a pipeline class (`envelope.class_id`) of profile `Image`, which
fixes everything the job does not carry: programs, weights, tokenizer, templates, sampler, tables,
resolution and output spec. **One resolution per class** in v1. Shapes are static, so another size is
another class. A listing may register one class per size. Their artifacts share the weight tensors,
but each has its own inventory root (the per-size tables differ), so ADR-0143's one owner per root
holds.

**Canonicalisation (by the gateway) and acceptance (verify, never rewrite).**

| Field | Canonical form | Refused by name |
| --- | --- | --- |
| prompt | ids under the class tokenizer, **without** the class's template tokens (the class adds them, §3.4); the gateway pre-truncates and says so | `PromptTokenOutOfRange` (id ≥ `token_bound`), `PromptTooLong` (> the class maximum) |
| negative prompt | ids, or empty; empty means "the class's unconditional prompt" | `NegativePromptNotOffered` (non-empty on a class without true CFG), `PromptTokenOutOfRange`, `PromptTooLong` |
| guidance | one grid index `guidance_q = round(16 · g)`; a class declares `[g_min, g_max]`; a class with a fixed or distilled guidance declares a single value | `GuidanceOutOfRange`, `GuidanceNotOffered` |
| seed | any non-zero 32 bytes (`PalwGenJobV1.seed`; the all-zero seed is the deterministic classes', §I.0); gateways take 64 hex characters and refuse an integer seed (W1, 2026-10-12) | `SeedRequired` (all zeros on this class), `SeedNotUsed` (non-zero on a class that draws none) |
| sampler/scheduler | the class's `sampler_id = H64(key "misaka-palw/image/sampler/v1", descriptor text)`; the math is in the program, and the descriptor is the registrant's label | `SamplerNotOffered` |
| steps | an element of the class's offered set (at most 8 values, each ≤ 64) | `StepsNotOffered` |
| resolution | exactly the class's `width × height` | `ResolutionNotOffered` |
| `image_index` | any `u16`; image `k` of `n` requested images is the job with index `k` (one inference, one claim) | — |
| output spec | `ImageRgb8` | `OutputSpecNotOffered` |
| conditioning images (edit, img2img, inpaint mask, references), strength, adapters | not in v1: they need §II.4's canonical image input and a later body version; adapters follow RFC-0001 §2.10's derived-class route | `ConditioningImageNotOffered`, `AdapterNotOffered` |

A job is one image. Nothing is silently clamped, rounded, truncated or defaulted by consensus.

##### 2. Deterministic noise

- **The initial latent.** The denoise stage declares `noise: Random { IMAGE_INIT_NOISE_V1, Normal }`
  of shape `[C, H_lat, W_lat]`, the canonical channel-major lane order (§I.1.4). The program:
  1. rescales to the latent's fixed point, `Div_HAFZ(noise, 2^(24 − q_lat))` then `Cast`;
  2. packs into its token layout with `Reshape` and `Transpose` (for example a 2×2 patchify
     `[C, H, W] → [H/2 · W/2, 4C]`);
  3. selects it at the first step: `x = Select(pos == 0, packed_noise, State(latent))`.

  In demand evaluation only the chosen operand is read, so later steps never touch the noise. A
  backend MAY skip computing it there, because no committed value depends on it. (An alternative, "a
  `Fixed` state initialised from an input", would change 04b §3.4 and §9.1; `Select` needs no change.)
- **Bit-exactness.** The noise tensor is the same bytes on every node: R per element, the pinned
  table, and an integer rescale that is part of the program. No backend may substitute a framework RNG.
- **Per-step noise** for stochastic samplers (ancestral, DDPM, SDE solvers) is a second input,
  `Random { IMAGE_STEP_NOISE_V1, Normal }`, whose `step` coordinate is the stage position `p`. A
  flow-matching Euler sampler is an ODE solver and draws only the initial noise.
- **The reproducibility promise.** The tuple (class, prompt ids, negative ids, guidance, sampler,
  steps, seed, `image_index`) gives the same image bytes on any node, at any parallelism, forever. The
  class is immutable data.
- **What the promise is not.** The same seed does **not** reproduce a framework's float image: the
  RNG differs, and the arithmetic is integer. Fidelity comparisons feed the float reference the integer
  run's noise (the table's values divided by `2^24`), never `torch.randn`.

##### 3. The denoise scan

###### 3.1 The loop and its mapping

| Reference loop | PALW-TIR construct |
| --- | --- |
| `latents = randn(...)` | the `Random` input + `Select` at `pos == 0` (§2) |
| `for i, t in enumerate(timesteps)` | the denoise stage's scan: **one step = one position** `p = i`, `T = steps` (a job fact) |
| `σ_i`, `σ_{i+1}`, timestep embedding of `t_i` | `Gather` over pinned tables indexed by `(steps_index, pos)` (§3.2) |
| `cat([latents] * 2)`, `uncond + g · (cond − uncond)` | a batch axis `B = 2`; the combination in `post` with `g` an `External` scalar (§3.3) |
| encoder hidden states | an `External` input `[B, L_max, d_txt]` bound to the encoder stage(s)' `Rows` (§3.4) |
| `transformer(x, t, text)` | layer blocks: one block kind per transformer block kind (for example two-stream and single-stream); the carry is `(image tokens, text tokens, conditioning vector)` |
| `latents = scheduler.step(v, t, latents)` | `post`: the update (Euler: `x + Δσ_i · v`, rounded by `Div_HAFZ`) → **`StateWrite(latent)`, committed** |
| `vae.decode(latents / scale + shift)` | decoder stages, one position each (§3.5); the output is the last stage's `Final` |

**The latent is a `Fixed` state** of the denoise program. It is read at the start of each step and
written by `post`, which the `Final` output kind allows (§I.2.3). A multistep solver's memory (a
previous prediction) is another `post`-written `Fixed` state. The denoiser has no `Hist` state and no
window: `H = 1` everywhere.

###### 3.2 Scheduler and timestep coefficients are pinned integer tables

For each offered step count `S`, the lowerer evaluates in high precision at registration and rounds
half away from zero:

- the sigma schedule `σ_S[0..S]`;
- the step coefficients (`Δσ` for Euler; the solver's weights for multistep solvers);
- the timestep-embedding rows (sinusoidal features of `t`);
- any resolution-dependent shift (resolution is fixed per class, so the shift is a constant).

The tables are params, or consts if small, indexed by `(steps_index, pos)`. A transcendental
evaluated at registration is data (PALW-EX-5). **Per-step activation quantisation scales** are also
tables gathered by `pos`: the scales that timestep-aware calibration needs are native to the IR. A
guidance *embedding* (a distilled guidance input) is a table indexed by the guidance grid index.

###### 3.3 CFG uses a batch axis

With true CFG every activation carries `B = 2` (conditional, unconditional):

- weights broadcast over the batch (`MatMul`'s broadcast batch dimensions);
- attention is per batch element and head (a rank-4 batched `MatMul`);
- each branch has its own text input and its own attention mask;
- `post` computes `v = v_u + Div_HAFZ(g_q · (v_c − v_u), 16)`, with any renormalisation the family
  applies (a per-token norm ratio is the library's `IntRsqrt` pattern).

CFG doubles MACs, lanes and step leaves. The cost is static: a CFG class computes both branches even
at `g = 1`, because shapes cannot change with a job value. A class without CFG is another class.

###### 3.4 Text conditioning: a separate encoder program (a stage), not a pre-scan block

A block runs at every position, and the IR has no control flow. A pre-scan block would therefore
recompute the encoder at every denoise step (S times the cost), or would need a second scan axis in
one program, where every position pays for both. As a **stage**, the encoder has four advantages:

- it is an LM program of the kind RFC-0002's corpus already lowers, with output `Rows` instead of
  logits;
- it has its own identity, so identical ids give identical rows across jobs and classes, and a cache
  of them is an execution fact (ADR-0145 §6);
- the court adjudicates it with the LM machinery: KV history and H dissection;
- CFG's unconditional branch is a second stage running the same program over the negative (or empty)
  prompt.

A **causal** encoder runs as a scan over `prefix ‖ prompt ‖ suffix` (the stage's `TokenRule`). Its
`Rows` output is the final-norm hidden row, or a concatenation of intermediate layers taken by tap
variants of the layer block (distinct block kinds at those layers). A **bidirectional** encoder runs
as a single-position program over a padded token axis (a `JobTokens` input) with a `Final` output
`[L_max, d]`.

The edge into the denoiser does four things, all class data:

- it drops template rows the model drops (`StageRows.drop`);
- it pads with zero rows to `L_max`;
- it passes the row count (`StageRowCount`) for the attention mask (`Compare(Iota < len)` → `Select`
  to a score that `IntExp` maps to exactly 0);
- if the family attends to pad tokens instead of masking them, the encoder runs over pad ids to
  `L_max` and no mask is used.

A class that offers no negative prompt MAY carry the empty-prompt rows as a param.

###### 3.5 VAE decode as the final program(s)

A layer schedule has one carry type (NF-5), and a decoder changes resolution. The decoder is
therefore **a chain of single-position stages, one per resolution level**, for example
`H/8 → H/4 → H/2 → H` for an 8× VAE. The last stage ends in the in-IR quantisation to `u8` (§4).

- **Convolutions.** Nine shifted `Slice`s (padding by `Concat` with an `Iota(step 0)` fill), a
  `MatMul` per tap, and exact sums. That is one order-free region, so a fused convolution kernel is a
  legal fused kernel (F-2), provided it computes the exact integer sum.
- **Other operations.** GroupNorm and SiLU are library templates. Nearest upsampling is `Reshape` +
  `Broadcast` in two rank-4 steps.
- **Size cap.** The `2^28`-element cap binds at 2,097,152 pixels for a 128-channel full-resolution
  activation. A lowerer splits channel groups to stay under it.

A U-Net *denoiser*, with skips across resolution levels inside one step, cannot be split this way.
It needs a carry pass-through (a carry-out that names a carry-in, re-committing nothing), which is
deferred with U-Net families.

###### 3.6 Attention over image tokens is inside one step

A diffusion transformer has no history. Each step attends over all `N` image tokens (and `L` text
tokens) as a **tensor axis**, not as `H`. Its reductions over the token axis are over a `Fixed` axis,
and PALW-TIR-32's H dissection does not cover them (§5).

The full score tensor `[B, heads, N_tot, N_tot]` exceeds `2^28` elements at a few thousand tokens.
The lowerer writes attention per group of heads; the node count stays well under 512 per block.
Backends never materialise scores. They MUST keep the exact **two-pass** softmax (maximum first, then
exact sums), because the single-pass online rescaling of flash attention rounds a running sum and is
not byte-identical.

###### 3.7 What `TirProgramV1` lacks — exactly

| Need | PALW-TIR v1 today (04b) | Minimal change | Kind |
| --- | --- | --- | --- |
| conditioning, noise, guidance and step count enter the step | inputs are `{token: idx, pos: idx}` (`Input(0)`, `Input(1)`) only | `inputs: [InputDecl]` — `External` and `Random` (§I.2.3) | program format (V2) |
| the step's result is a latent, hidden rows or an image | one `logits` node + `logits_scheme_id`; `post` runs only where logits are consumed (Phase F §2.5) | `output: Logits \| Rows \| Final` | program format (V2) |
| the latent update is the step's last act | NF-19: `post` writes no state; a global state is written only by `pre`; `pre` cannot read per-layer instances | for `Rows`/`Final`, `post` MAY write a global `Fixed` state that `pre` does not write; §9.4's writer is occurrence 0 or `L + 1` | normal form (V2) |
| encoder → denoiser → decoder, with different trip counts and shapes | one program per class (Phase F §2.3) | the pipeline class (§I.2.3) | class format (new object) |
| a decoder that changes resolution | NF-5: one carry signature | a chain of single-position stages | none |
| a reduction on a `Fixed` axis larger than a tile's ceiling | PALW-TIR-32 dissects only reductions over `H` | committed partial reductions (§5.5), then generalised dissection (§5.6) | none now; admission + court later |
| per-step MACs and leaves far above an LM position | per-position ceilings (Phase F `max_macs_per_position`; `PALW_STEP_MAX_LEAVES = 2^22`) | per-profile, per-job ceilings | fence values |

**The primitive set is unchanged**, and no hard reason to change it was found. Every operation of a
diffusion transformer and its VAE maps to the 25 primitives:

- linear layers;
- LayerNorm and RMSNorm (including QK-norm);
- GELU and SiLU (tables, or the library);
- adaptive-norm modulation, gates and residuals;
- multi-axis RoPE with static positions (tables);
- masked softmax attention;
- token `Concat`/`Slice`, patchify and unpatchify;
- 3×3 convolution, GroupNorm and nearest upsampling;
- timestep and guidance embeddings (tables);
- the scheduler update, the CFG combination, the u8 quantisation and the noise rescale.

The near misses are avoided without one:

- bitwise operations for R (R sits at the job layer);
- `Pad` (`Concat` with a fill);
- rank-5 upsampling (two rank-4 steps);
- oversized score tensors (split by head group).

##### 4. ImageOutput

- **In-IR quantisation.** The last decoder stage maps its fixed-point output `y` (nominally
  `[−1, 1]`) to pixels by the family's formula, typically `(y / 2 + 1/2) · 255`. It rounds half away
  from zero and clamps:

  ```
  p = Clamp(Div_HAFZ(Mul(Add(y, ONE_q), 255), 2 · ONE_q), 0, 255)
  ```

  It then transposes `[3, H, W] → [H, W, 3]`. The float reference clamps and rounds half to even; the
  integer program fixes its own rule, a named lossy site and not a tolerance. The output node is
  committed (`i16` or `idx`), and the `Clamp` proves its interval `[0, 255]`.
- **Consensus.** `output_root` (§I.3.2) over the canonical bytes: `u8` HWC row-major RGB, no alpha, no
  padding, no gamma or ICC conversion, with header `(ImageRgb8, H, W, 3)`. The claim carries it, and
  `TirOutputDigestMismatch` enforces it. For 1024² that is 3 MiB of canonical bytes (12 MiB of lanes
  in the step tree).
- **Presentation.** The gateway handles encoding (PNG/WebP/JPEG), metadata (prompt, seed and class in
  text chunks), colour profiles and thumbnails. It MAY serve a reproducible PNG (§I.3.4). None of it
  is consensus. Why not a canonical PNG: §I.3.4.

##### 5. Verification and court

###### 5.1 Commit points

Per diffusion-transformer block and step, the lowerer commits:

- the modulation vectors;
- `q`, `k` and `v` after QK-norm and RoPE, **head-major** (`[heads, tokens, d_head]`), so that one
  head's keys and values are whole leaves;
- the attention output (also head-major);
- the post-attention residual stream;
- the MLP hidden layer;
- the carry-out.

Per step it also commits the combined velocity and the updated latent (the committed `StateWrite`).
Encoder stages commit as LM programs do (RFC-0002 Phase F). Decoder stages commit convolution outputs
as needed, plus partial statistics (§5.5). Head-major layout matters: close carriage opens whole
leaves, so a token-major `k` would make one head's keys cost the whole tensor.

###### 5.2 The step space

The step space runs stage-major. Within a stage it follows Phase F §2.5: positions in order, commit
points by slot within a position, then `Fixed` checkpoint leaves every `C`. The denoiser has no
`Hist`. Because the latent's writer is committed, **the latent at the start of step `p` is a leaf**:
the `StateWrite` at `p − 1`. A checkpoint every `C` steps adds nothing, and `C = S` minimises leaves.

###### 5.3 Bisection over steps × tiles

The existing ladder (binary or k-ary, at most 48 rounds) runs over the concatenated leaves and narrows
to the first divergent leaf, which is a (stage, step, occurrence, node, tile). Its cone is evaluated
by the generic `TirCone` arm (Phase F §2.7), with four additions:

- random inputs are recomputed from the claim's seed and `image_index` (§I.1.7);
- `External` inputs are opened as upstream leaves, or read from the job;
- job scalars come from the job;
- the latent comes from the previous step's committed `StateWrite`.

Example A (§*Compute*) has about 6.8·10^7 leaves (`≈ 2^26`), within a ladder capped at `2^32`
(`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`), which means 26 binary rounds or 9 at k = 8.

###### 5.4 Tile cones against the court ceilings

The terminal tile must stay within ≤ 16 Mi MACs, ≤ 8 committed operands and the close-byte ceiling
(≈ 16.8 MB on testnet-12 — **but see §5.4a: what a close can actually carry is 3.2 MB**, and every
"fits" in the table below that rests on the larger number is re-judged there). Opened committed values cost 4 bytes a lane, and weights 1 byte (int8,
stored output-major, Phase F §2.10). Parameterised, with **example** numbers for `d = 3,072`,
`d_head = 128`, MLP ratio 4 and `N_tot = N + L = 4,096 + 512`:

| Cone (tile) | MACs | Opened bytes | Example | Fits? |
| --- | --- | --- | --- | --- |
| q/k/v projection (1,024 lanes of one token) | `tile · d` | row + modulation + `tile · d` weights | 3.1 M; ≈ 3.2 MB | yes |
| attention output (8 queries × 1 head) | `8 · 2 · N_tot · d_head` | `8 · N_tot · d_head` (K and V of the head) + queries | 9.4 M; 4.7 MB | yes |
| MLP down / carry-out (1,024 lanes) | `tile · 4d` | hidden row + `tile · 4d` weights | 12.6 M; 12.7 MB | yes, tight (tile 512 → 6.4 MB) |
| latent update | elementwise | tiles of `v`, `x` | — | yes |
| decoder 3×3 conv at full resolution (8 px × 128 ch) | `8 · 128 · 9 · C_in` | 3-row input window + weights | 1.2 M; ≈ 0.2 MB | yes |
| decoder mid-block self-attention, **if the family has one** (`N_px` latent pixels × `C` channels, one head; 2 queries) | `2 · 2 · N_px · C` | `8 · N_px · C` | `N_px = 16,384`, `C = 512`: 33.6 M; 67 MB | **no** |
| GroupNorm statistic at full resolution (`C/G` channels × `H · W` output pixels per group) | — | `4 · (C/G) · H · W` | `4 · 4 · 1,048,576` = 16.8 MB | **no** (at the ceiling) |

Every cone above reads at most four committed tensors, within the limit of 8. For attention over
image tokens, **opened bytes bind, not MACs**. One head's K and V cost `8 · N_tot · d_head` bytes,
which stays under the ceiling up to about **15,600 tokens at `d_head = 128`** (about 31,000 at 64),
leaving margin for queries and paths. With 16× total compression (8× VAE, 2×2 packing) that is about
4 MP: 1024² and 1536² fit, while 2048² (16,896 tokens) does not.

###### 5.4a Which demand model prices an image cone, and what a close carries (FR-22, resolved 2026-10-01)

Lane H's corpus (FR-22) found that §5.4 and the code disagree: this section sizes cones with element-exact
demand against a 16.8 MB close, while the code prices a tile by **box demand** (spec 04b §10.3) and bounds a
carried close at **3.2 MB** (PALW-TIR-38). They are three different quantities with three different rules,
and §5.4 conflated them:

1. **The work a tile costs the court — box demand, kept.** The court's tile ceilings (16 Mi MACs, the
   operand count, the evaluation-work limit) are checked against the box rule of spec 04b §10.3, and image
   cones are admitted under it (the release's rules, or ref2's H7 row past `palw_tir_fence2`, which changes
   only the `TopK` row; a diffusion transformer has no `TopK`, so the two coincide for it). The court itself
   evaluates element-exactly, so box pricing is conservative: an attention-output tile of `T` lanes is
   charged `T · N · (dh + 1)` MACs where the exact figure is `(⌈T / dh⌉ + 1) · N · dh + T · N`. An
   element-exact (range-aware) admission price is a consensus-visible change — a registration it admits and
   the box rule refuses — so it is a fence value in H7's style (lane H's **C2**), not a reinterpretation.
   **The reduced profile and Example-C do not need it; it is needed from about 4,000 tokens** (table below).
2. **The bytes a close carries — element-exact reads, never box.** PALW-TIR-38 prices a close from the
   court's read set (`TirCloseDemandV1`: an abstract twin of the demand evaluator over element sets, each read
   priced as the close carries it). The pipeline admission's check today (`generative close bytes as carried`)
   is the cheaper **necessary** condition — the stage view's opened bytes plus a 16 KiB frame — and the
   sufficient measure, the twin generalised to a pipeline's stages, is **PALW-GEN-20** (built with step 6;
   a class registers on a network that arms it only when its worst close is measured by the twin).
3. **The ceiling on that number is 3.2 MB, not 16.8 MB.** 16.8 MB is the testnet-12 ruleset's
   `max_close_bytes` (202 chunks); what the fold assembles is `min(max_close_chunks, 32)` chunks — the state
   row's bitmap — of at most 100,000 bytes (`palw_tir_carriable_close_bytes_v1`): **3,200,000 bytes**
   (PALW-TIR-38, the decision of 2026-09-28). §5.4's table was computed against the first.

**What a close opens, per cone** (a committed lane is four bytes in a leaf preimage, a weight one byte; the
frame is 16 KiB; paths add about 3 KB a contiguous run):

| cone (tile `T`) | opened bytes | binds on |
| --- | --- | --- |
| q/k/v, MLP-up, attention-output and any other projection (contraction `K`) | `4K + T · K` | the weights: `T · K` |
| MLP-down (contraction `4d`) | `16d + T · 4d` | the weights |
| attention-output (one head, `N` tokens, head dimension `dh`) | `8 · N · dh` (K and V of the head) + queries and statistics | **the head's K and V, whatever `T`** |
| latent update, modulation, norms | a few tiles of the row | nothing |

The largest tile under 3.2 MB (16 KiB frame, four runs of paths) and, beside it, the largest attention-output
tile under 16 Mi MACs by each price:

| class | tokens `N` | `dh` | K,V of a head | projection `T` (`K = d`) | MLP-down `T` | attn-out `T`, box | attn-out `T`, exact |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| reduced profile (§II.1.6): `d = 64`, 4 heads | 24 | 16 | 3.1 KB | 49,548 | 12,384 | 41,120 | 349,514 |
| Example-C-sized: `d = 1,536`, 24 heads | 1,024 | 64 | 0.52 MB | 2,060 | 512 | 252 | 8,128 |
| SD3-medium at 1024² | 4,250 | 64 | 2.18 MB | 2,060 | 512 | **60** | 1,920 |
| Flux at 1024² (`d = 3,072`, 24 heads) | 4,608 | 128 | **4.72 MB** | 1,028 | 254 | **28** | 1,720 |

The numbers say, in the order of the coordinator's questions:

- **Which model for image cones: box for work, element-exact for bytes.** The box rule's cost is leaf count,
  not safety: at SD3-medium's 1024² a 60-lane attention-output tile is 109,000 leaves per block-step
  (4.6 M per step against the 2^22 per-position cap); exact pricing allows 1,920 lanes (6,375 leaves). That
  is C2's case, and it opens above ~4,000 tokens. Below it (Example-C: 252-lane tiles, 6,241 leaves per
  block-step, ~0.59 M leaves a step, ~4.7 M a job of eight steps) box pricing admits the class.
- **Close sizes at the sizes that matter.** The reduced profile's closes are a few kilobytes at most (a projection
  tile of `T` lanes at `d = 64` opens `4K + T · K`: 4.4 KB at `T = 64`). At Example-C size the weights bind, not the attention: a
  projection tile of 1,024 lanes opens 1.6 MB, an MLP-down tile of 512 lanes 3.1 MB, and a head's K and V
  0.52 MB; at SD3-medium 2.18 MB; **Flux at 1024² does not fit** (4.72 MB of K and V alone).
- **Tiling that fits the existing cap (decision).** The tile is a per-commit-point layout choice
  (`commit_tiles`), so no cap change is needed for the first profile: **(a)** a projection or MLP tile is
  chosen by its weights, `T · K ≤ 3.1 MB` — at `d = 1,536`: projections `T ≤ 2,060` (use 1,024), MLP-down
  `T ≤ 512`; **(b)** an attention head's K and V are opened whole whatever the output tile (every output
  element of a head reads all of them), so **tiling cannot shrink them**: they fit while `N · dh ≲ 390,000`
  (`N ≤ 6,182` at `dh = 64`, `N ≤ 3,091` at `dh = 128`); past that the committed-partial lowering of §5.5 (key
  chunks, committed row maxima and denominators) is the no-protocol-change route, and a dissection over a
  declared axis (§5.6, C3) the protocol one; **(c)** the VAE's mid-block attention and GroupNorm statistics
  follow §5.5 unchanged.
- **A cap change only with numbers.** Widening the bitmap (32 → at most 64 chunks, 6.4 MB) is a consensus
  change under a court version; it would admit Flux-class heads at `dh = 128` up to ~6,000 tokens (K,V
  `8 · N · dh` ≤ 6.1 MB), at the price of every close carrier. **Not proposed**: nothing in scope needs it —
  Example-C is at 0.52 MB, SD3-medium at 2.18 MB — and each chunk is a carrier-paid transaction. Revisit with
  C2 and C3, when a class that needs it exists to be measured.
- **The one-move limit is stricter than any of this** (§I.4.7, G24): on a held-regime chain a pipeline class is
  convictable in one move only where a close fits one carrier (~100 KB), which a 1,024-lane weight tile
  (`T · K = 1.6 MB`) does not. Until the held leaf challenge (decision 22, fence `palw_held_close_chunks_v1`) is
  armed, PALW-GEN-21 bounds a class's closes by one carrier on such a chain; the reduced profile is inside it (a
  few KB a cone), Example-C is not.

**UNet families** need a per-occurrence carry signature (lane H's **C1**: NF-5's single carry signature,
NF-4's eight carries and NF-21's re-committed skips block them); RFC §3.5 already defers it as "carry
pass-through". The first profile is a transformer (MMDiT), so it is unaffected; C1 is not scheduled here.

###### 5.5 What does not fit, and the fix that needs no protocol change: committed partial reductions

An exact reduction can be written as two exact reductions with a committed intermediate: partial sums
over chunks and then their sum, or partial maxima and then their maximum. The integers are the same
(sums are exact, PALW-TIR-24), and every cone fits. It is a lowering choice; consensus does not change.

- **Mid-block attention**, keys in chunks of 1,024. Commit, in order:
  1. the per-chunk maxima `m_c`;
  2. the row maximum `m`;
  3. the per-chunk exponent sums `S_c = Σ IntExp(s − m)` and value sums `V_c`, both against the
     global `m`;
  4. `S = Σ S_c` and `V = Σ V_c`.

  Every cone then opens at most `1,024 · C · 8` bytes (4.2 MB at `C = 512`). The cost is
  `N_px · chunks · (C + 2)` lanes, about 1.35·10^8 once per image in the example: the size of one
  full-resolution activation.
- **GroupNorm.** Commit per-row partial `Σx` and `Σx²`, `[H, G, 2]` lanes, a few tens of thousands.
  Each partial's cone reads one row. Variance is exact in integers:
  `(n·Σx² − (Σx)²) / n²`, with the library's exact-centring idea.

For images up to about 4 MP this is enough, and **the first image profile needs no dissection
change**.

###### 5.6 Proposed: dissection over a declared reduction axis (PALW-TIR-32′)

Committed partials over the transformer's attention cost commitment at every execution. At 2048² they
add about +50 % lanes; for video, more. Dissection costs only in disputes. **Proposal:** Phase F §2.8's
generic H dissection, applied to any **declared** exact reduction.

- **The declaration.** A program-format flag `dissect` on a `ReduceSum`/`ReduceMax` along a `Fixed`
  axis, or on a `MatMul` over its contraction axis. It enters the encoding only with the court version
  that adjudicates it, so no program carries an inert field.
- **Soundness transfers.**
  - Every such reduction is exact, so partials over index ranges are well defined and fold without
    error.
  - The root claim carries every dissected reduction's totals over the demanded elements, and they are
    checked by the finalize step: the cone evaluated with the totals supplied must reproduce the
    committed tile.
  - Rounds narrow **one** index range shared by the cone's dissected reductions, which MUST have equal
    extents. Each child claims every reduction's partial, and the fold is checked per reduction. A lie
    in any total therefore forces a lie in some child's partial of *that* reduction, which the
    challenger follows.
  - The bottom recomputes each reduction's terms over one chunk by demand evaluation. The other
    reductions' totals are supplied from the root claim: this is how the softmax's maximum enters
    `IntExp(s − m)`.
- **What `H` gave for free and a `Fixed` axis does not.** PALW-TIR-32 guarantees structurally that a
  term at history index `t` reads only history row `t`, so a bottom costs `O(h_chunk)`. Over a `Fixed`
  axis a term may read anything. Admission MUST therefore compute the bottom's cost by box demand at
  the chunk extent (04b §10.3, with the reduced extent replaced by the chunk), and MUST refuse a
  declaration whose bottom does not fit. There is no new primitive, one court version bump, and the
  objects are Phase F's `CourtTir*` with "H range" read as "declared-axis range".
- **When.** Needed beyond about 15,600 tokens (at `d_head = 128`): images above about 4 MP, and video
  (§II.6).

###### 5.7 Replaying the latent from checkpoints

The latent's writer is committed, so replay is a leaf read (§5.2). Solver memories are committed the
same way. A seat resumes from the last committed latent (the resume rule of ADR-0133 and Phase F
§2.6). A state checkpoint of the latent is legal but redundant. A stochastic sampler's per-step noise
is recomputed, never replayed.

###### 5.8 Who loses on malformed commitments — PALW-TIR-33 carries over

- **Every committed value is the executor's statement.** That covers step leaves of every stage
  (encoder rows and the latent included), state leaves, and output-digest tiles. A committed value
  outside its node's proven interval convicts the executor, whichever leaf the challenger disputed.
- **Edge inputs** are upstream commit points, so the same rule covers them. Admission proves each
  edge's upstream interval inside the downstream `[lo, hi]`, so no honest value is ever out of
  interval downstream.
- **Random inputs are never committed.** The court recomputes them, and other noise is convicted at
  the first cone that reads it.
- **Job values** (guidance, steps, prompt ids) are checked at acceptance: they are refused by name and
  never convicted.
- **The output digest.** `TirOutputDigestMismatch` convicts the executor.
- **A lane outside its node's dtype** (finding G21, decided 2026-10-01). The step tree's leaf hash is over
  the four-byte lanes as committed — a lane is the value's low 32 bits whatever the dtype — so an
  executor that commits one (it hashes its own tree with its own code) has a leaf that opens and proves
  under its stage's root, and PALW-TIR-33's interval check convicts it. A hash that refused such a lane
  would make the leaf unprovable (`LeafNotProven` convicts nobody) and let the executor escape. The court
  reads disclosed lanes without judging them and hashes them as committed (04b §15.14).
- **A challenger cannot manufacture any of these.** The values are opened against the executor's
  roots.

##### Fidelity — usefulness, never validity

**The reference.** The family's pinned float pipeline: library versions fixed in the fixtures, eager
attention, fp32. It is fed **the integer run's noise** (`PALW_GAUSS_Q24_V1` values divided by `2^24`),
its sigma table, guidance and prompt ids. Measured on a fixed prompt set (for example 200 prompts × 2
seeds), with per-family thresholds set in Phase A of this profile:

1. **Latent trajectory error.** `‖x_int − x_float‖ / ‖x_float‖` per step: the maximum over steps, and
   at the end.
2. **Image error** against the float image with the same noise: PSNR, SSIM, LPIPS.
3. **Text–image alignment.** A CLIP-type score from a pinned scorer, integer image against float image
   on the same prompt: the mean difference and a paired test.
4. **Distribution.** FID/KID of the integer set against the float set (KID for small sets).
5. **Encoder fidelity.** The per-token cosine of the encoder rows against float.

These decide whether a lowering is useful and how a listing describes it. They never decide whether a
claim is valid (ADR-0053; RFC-0002 criterion 5). Activation ranges of diffusion transformers drift
across steps. The IR's `pos`-indexed scale tables (§3.2) are the tool for that. A family that cannot
meet its thresholds has a quantisation problem, not an IR problem (RFC-0002 open question 3).

##### Compute — a parameterised estimate

With `P_tok` the parameters applied per token (≈ `P` for a single-stream transformer, less for a
two-stream one), `N_tot = N + L_txt`, width `d`, `L_blk` attention layers, `S` steps and CFG factor
`c` (2 with true CFG, else 1):

```
denoise MACs  ≈ S · c · ( P_tok · N_tot  +  2 · N_tot² · d · L_blk )      (FLOPs = 2 × MACs; the brief's
                                                                            "2·params·tokens" counts FLOPs)
total         ≈ denoise + encoder (≈ P_enc · L_txt · (1 + [true CFG])) + decoder D_vae(H, W)
lanes         ≈ S · c · λ · d · N_tot · L_blk,  λ ≈ 10  (q, k, v, attention out, residual, MLP hidden ≈ 4d, carry)
leaves        ≈ lanes / mean tile_len
```

**Worked examples.** These are *illustrative geometries, not facts about a named model*.

| Example | Geometry assumed | MACs per image |
| --- | --- | --- |
| **A**: the brief's case ("4B DiT, ~4,096 image tokens, ~28 steps"; `P = 4·10^9` read from a model name, not verified) | `d = 3,072`, `L_blk = 35`, `N = 4,096` (1024², 8× VAE, 2×2 packing), `L_txt = 512` | per step and branch 1.84·10^13 + 0.46·10^13 = **2.3·10^13**; `S = 28, c = 2`: **1.3·10^15**; `S = 4, c = 1`: **9.2·10^13** (+ decoder ≈ 5·10^12, encoder ≈ 1–2·10^12) |
| **B**: a 20B-class two-stream transformer | `P_tok ≈ 10^10`, `d = 3,072`, `L_blk = 60`, `N_tot ≈ 7,400` (≈ 1.7 MP), `S = 50, c = 2` | ≈ 9.4·10^13 per step and branch → **≈ 10^16** |
| **C**: a small transformer | `P = 6·10^8`, `d = 1,152`, `L_blk = 28`, `N = 1,024` (512², 8× VAE, 2×2 packing), `N_tot ≈ 1,324`, `S = 20, c = 2` (or `S = 4, c = 1`) | **3.9·10^13** (**5.5·10^12**), with a 512² decoder (≈ 1.3·10^12) and a 2B-class encoder over ~300 tokens |

The decoder figures assume an 8× VAE with widths 128/256/512/512 and three ResNet blocks per level.

**Backends.** The sustained effective rates are assumptions, to be measured in the drills:

- the generic typed CPU backend (Phase F F9): ≈ 0.1 TMAC/s;
- a tuned CPU int8 backend (Phase G: VNNI/AMX, SDOT/I8MM): ≈ 2 TMAC/s;
- a GPU integer backend (Phase G: int8 tensor cores with `i32` accumulation, exact under F-1 because
  order-free sums are proved not to overflow): ≈ 120 TMAC/s on a consumer card, lower for the IR's
  non-GEMM work and its two-pass attention.

| Workload | MACs | Generic CPU | Tuned CPU | GPU integer | Lanes committed |
| --- | --- | --- | --- | --- | --- |
| C, 4 steps, no CFG | 5.5·10^12 | ≈ 55 s | ≈ 3 s | ≈ 0.05 s | ≈ 6.8 GB |
| C, 20 steps, CFG | 3.9·10^13 | ≈ 6.5 min | ≈ 20 s | ≈ 0.3 s | ≈ 68 GB |
| A, 4 steps, no CFG | 1.0·10^14 | ≈ 17 min | ≈ 50 s | ≈ 1 s | ≈ 80 GB |
| A, 28 steps, CFG | 1.3·10^15 | ≈ 3.6 h | ≈ 11 min | ≈ 11 s | ≈ 1.1 TB |
| B, 50 steps, CFG | ≈ 10^16 | ≈ 28 h | ≈ 1.4 h | ≈ 1.4 min | ≈ 5.5 TB |

**What the table means.**

- **Commitment dominates on GPUs.** At about 1,200 MACs per committed byte (Example A), keeping up with
  a 120 TMAC/s GPU needs about 100 GB/s of leaf hashing, far beyond BLAKE2b on CPU cores (≈ 1 GB/s
  each). The ratio is the same as LLM prefill, so this is a property of the step-leaf format (4-byte
  lanes, BLAKE2b), not of images. The mitigations are listed under *Open questions*.
- **Leaves.** Example A with CFG makes about 2.4·10^6 leaves per step (at a mean tile of 4,096 lanes),
  within the per-position cap `2^22` but not by much. A model with about twice Example A's `d · L_blk`
  would exceed it. Denoise stages need per-profile, per-job leaf ceilings.
- **Every seat that re-executes a job pays the same MACs.** An image claim's network cost is its
  execution times the replicas, which the pricing must reflect.
- **Practical on testnet first:** Example-C-sized classes (≤ 1B, ≤ 1024 tokens, ≤ 8 steps, preferably
  without CFG). They take minutes on the generic backend and seconds tuned. Drills use reduced
  random-weight fixtures, as RFC-0002 does.
- **4B-class distilled (`S ≈ 4`, no CFG)** after Phase G's GPU integer backend and GPU leaf hashing.
- **28–50-step CFG models of 4B and above**, and the 20B class, are GPU-only. Their per-job cost also
  stresses panel economics.

##### Verification checklist before the profile is frozen

Nothing about the target checkpoints' architecture was verified for this draft, and no network access
was used. Before the image profile is frozen, confirm each item from the model cards, the configs and
the reference pipeline, and record the source:

| # | Confirm | Enters |
| --- | --- | --- |
| 1 | repository ids, licences and redistribution terms of transformer, text encoder(s) and VAE (an integer artifact is a derived work) | registration, artifact |
| 2 | transformer: stream layout (single, two-stream, mixed), block count, width `d`, heads and head dimension, MLP ratio and activation, norms (LayerNorm/RMSNorm, QK-norm), modulation form, positional encoding (RoPE axes, dimensions, text vs image position ids), total and per-token parameters | lowerer, §3.1, *Compute* |
| 3 | text conditioning: encoder model(s); which hidden states (last, final-norm, intermediate layers); template or system prefix and dropped rows; maximum tokens; padding and masking; pooled vectors | §3.4, stage edges |
| 4 | guidance: true CFG, a guidance embedding, or none (distilled); default and range; any renormalisation | §3.3, `offers` |
| 5 | sampler: solver (flow-matching Euler or other), shift (static or resolution-dependent), sigma schedule, default and supported step counts, stochasticity | §3.2, noise domains |
| 6 | latent space: channels, spatial compression, packing, scaling and shift factors, latent normalisation statistics | §2, §3.5 |
| 7 | VAE decoder: blocks, channel widths, GroupNorm groups, mid-block attention (present? heads, width), upsampling, output range | §3.5, §5.4–5.5 |
| 8 | resolutions: default and supported sizes or aspect buckets, token count `N` per size | class per size, §5.4 |
| 9 | reference pipeline: class and library version, default dtype, fp32 reproducibility, attention backend | *Fidelity* |
| 10 | variants: distilled vs base checkpoints; editing and reference-image inputs (for a later body) | classes, §II.4 |

##### 6. The first image profile (activation step 6): design

**Decided 2026-10-01 (coordinator, step 6 after the carriage).** The first image profile is the **SD3 / MMDiT
family on a reduced fixture**, lowered by new reusable feature lowerers, held to a same-noise float
reference, and registered as a pipeline class. It is a *lowering and fidelity* step: it changes no consensus
rule, and it is judged by the freeze criteria of RFC-0002's manner (three-way bit identity, measured fidelity,
every cone within the ceilings).

**Scope and ownership.** Everything is new files in `misaka-palw-tir-lower` (`wt-rfc3-lower`, branch
`rfc3/lower`, merged into `rfc3/fp-v5`); lane G owns the generic frontend (`spec.rs`, `hf_config/*`, branch
`tir/generic`), which this step **does not refactor**. The lowerers take a typed `GenSpec` (FR-22's skeleton:
a DiT or UNet denoiser, the sampler, the latent, the text taps, the VAE) built from the diffusers configs by the
fixture harness; its reader for `model_index.json` plus each component's `config.json` (`_class_name`, not
`architectures`) is the generic frontend's to adopt later, so the lowerer interface is **configuration in,
`TirProgramV2` stages and a weight binder out**, and a real checkpoint changes numbers, not shape.

**The fixture** (diffusers 0.40, `HF_HUB_OFFLINE=1`, never `from_pretrained` on a hub id; tiny seeded random
configs; **parameter counts computed on the meta device first, one model per subprocess**, the host's
watchdog kills a Python process over 12 GB):

| component | diffusers class | reduced configuration |
| --- | --- | --- |
| denoiser | `SD3Transformer2DModel` | latent 8×8×4, `patch_size = 2` (16 image tokens), 2 joint blocks, 4 heads of 16 (inner dimension 64), `joint_attention_dim = 32`, `caption_projection_dim = 64`, `pooled_projection_dim = 32`, `qk_norm = None` |
| text | `CLIPTextModelWithProjection` | hidden 32, 2 layers, vocabulary 64; **prompt ids are the job's** (no tokenizer: the harness feeds ids, and the pipeline's `prompt_embeds` path is the reference's) |
| VAE | `AutoencoderKL` | `block_out_channels = (8, 16)`, `layers_per_block = 1`, 4 latent channels, 4 groups; the decoder is the program |
| sampler | `FlowMatchEulerDiscreteScheduler` | `shift = 3.0`, offered step counts `[2, 4]` |

**Stages** (`TirPipelineV1`): (0) the text encoder — an existing causal-rows lowering, its final rows and
pooled vector committed; (1) the **denoise scan** — `TripRule::JobSteps`, the carry the concatenated
text + image stream `[L + N, d]` (NF-5 holds), the latent a global `Fixed` state written in `post`
(`StateWrite`, NF-29), the initial latent a `Random { Normal }` input, `σ` and the timestep coefficients
pinned integer tables per `(steps index, position)`; (2) the **VAE decoder** — a chain of single-position
stages, the last clamping and transposing to `ImageRgb8`. **CFG is the second iteration** (a batch axis of
two, or two positions: lane H's C4): the first fixture is guidance-free, so the lowerers are validated one
at a time.

**The new feature lowerers** (each a library function over the existing builders, one file each; the
vocabulary is FR-22's):

| lowerer | recipe over the 25 primitives | lossy sites |
| --- | --- | --- |
| `CONV_DENSE_V1` | `Concat` zero row, `Gather` im2col by a pinned index table (a param, clamped for the interval proof), `Reshape`, `MatMul`, the narrowing | 1 |
| `NORM_GROUP_SPATIAL_V1` | i64-wide `ReduceSum`s; **row partials `[H, G, 2]` committed** (§5.5) so no cone reads a whole tensor; `Log2Floor` + `IntRsqrt`; `γ`, `β` in the narrowing | 2 |
| `ACT_TABLE_V1`, `EMBED_TIMESTEP_TABLE_V1` | SiLU/GELU as `Gather` of a pinned table; the timestep sinusoid evaluated at registration and gathered by the step index | 1–2 |
| `MOD_ADALN_V1` | `MatMul(SiLU(c), W)` committed; `Slice` chunks; `layer_norm_exact`; `Mul(ONE + scale)`, `Add(shift)`; `x + gate · f(x)` | ~3 |
| `ATTN_JOINT_STREAMS_V1` | per-stream q, k, v; `Concat`; one attention over the joint stream with committed row maxima and denominators; `Slice` back | ~4 |
| `PATCH_EMBED_V1` | patchify as `Reshape`/`Transpose`, the embed as `MatMul`, the learned position table cropped to the grid | 1 |
| `GEN_SAMPLER_AFFINE_V1` | `x' = StateWrite(x + HAFZ(dσ_i · v / 2^24))` | 1 |
| `GEN_STAGE_VAE_V1` | resnet blocks with `NORM_GROUP_SPATIAL_V1`, nearest upsample, conv; the output stage `Clamp(Div_HAFZ((y + ONE) · 255, 2 · ONE), 0, 255)` | ~8 a block |

**Fidelity** (RFC §Fidelity): the harness runs the integer pipeline and the diffusers float pipeline **fed the
integer run's noise** (`PALW_GAUSS_Q24_V1` words divided by `2^24`) with the same prompt ids, the same sigma
table and the same steps, and reports the latent trajectory error per step, the final image's maximum and
RMS error and its PSNR, over a fixed set of seeds. **Thresholds are measured first and frozen after** (the
RFC sets them in Phase A of the profile): nothing is written down as a target before the first measured run.
A block-by-block float check (each lowerer against its diffusers module on the same weights) comes before
the pipeline's, so a lossy site is attributed to its lowerer.

**The court battery on the tiny pipeline** (the step-5 battery, over this class): a planted fault at every
commit-point kind — projection, attention output and its statistics, MLP, modulation, norm partial, latent
`StateWrite`, timestep embedding, convolution, VAE output — forged noise, a forged edge, a forged digest, an
out-of-interval lane (G21: convicted by PALW-TIR-33), each convicted at its leaf by the cone or output close
**and** through the carriage (a one-move accusation over the claim, §I.4.7). The close sizes are measured by
the pipeline twin (PALW-GEN-20) and must sit under both ceilings: 3.2 MB carried, and the one-move carrier
(PALW-GEN-21) — measured on the built fixture (§6.1): 3.6 KB to 73.7 KB, every cone-closable kind under one carrier.

**The §II.1 checklist** is completed *for the fixture* (every item from the diffusers 0.40 classes and
configs, sourced to the file) and for the real SD3-medium **only as far as the model cards and configs can be
read offline** — the items that need a download (licences, redistribution, default resolutions) stay open
and are named as such.

**Deferred, and why.** UNet families (lane H's C1, per-occurrence carry signatures; RFC §3.5); the
range-aware admission price (C2) and dissection over a declared axis (C3), needed from ~4,000 tokens
(§5.4a); the profile ceilings for real sizes (C5); CFG as a batch axis (the second fixture); real-checkpoint
conversion (the lowerer interface only); **audio** — five corpus entries are blocked on `JobAudio` and the
audio ceiling set (RFC §II.5), which is scheduled **after step 8**.

**Order**: the fixture generator and the float reference; each lowerer with its own float check; the pipeline
assembly and class declaration (`gen_declare_layout_v1`: the layouts the gate admits); the integer run
against the float run (the measured fidelity); the court battery; the record.

##### 6.1 As built — the first image profile, recorded 2026-10-01 (lane D)

Everything below was built and measured on the reduced fixture; nothing in it changes a consensus rule, and the whole
profile stays dormant behind `palw_gen_v1`. Sources: `misaka/palw/tir/lower/src/diffusion/*` (branch `rfc3/lower`, merged
into `rfc3/gen-claim`), `tests/diffusers_sd3.rs`, `misaka/palw/sdk/tests/gen_sd3_class.rs`,
`tools/gen_diffusers_sd3_fixture.py`. The fixture is diffusers 0.40.0 (`SD3Transformer2DModel` 284,304 parameters,
`CLIPTextModelWithProjection` 20,736, `AutoencoderKL` 43,619; BF16; seeded; generated offline, one model per subprocess).

**The pipeline** (`TirPipelineV1`, 13 stages; every edge's shape, dtype and interval proved by `validate_pipeline`):
`text_rows` and `text_pool` (the HF frontend's causal CLIP lowering, `Rows` and `Final`), `denoise` (`JobSteps`; the
`Random { IMAGE_INIT_NOISE_V1, Normal }` input, the latent a global `Fixed` state written in `post`), then ten `Fixed {1}`
VAE stages (`vae.in`, `vae.mid.r0`, `vae.mid.at`, `vae.mid.r1`, `vae.up0.r0`, `vae.up0.r1`, `vae.up0.us`, `vae.up1.r0`,
`vae.up1.r1`, `vae.out`), the last producing `i16 [H, W, 3]` in `[0, 255]`.

**The lowerers as they ended up** (one file each, library functions over the builders; names are the checkpoint's
module paths; `Calib` names every calibration site and the float reference notes the same names):

| lowerer | as built | lossy sites | notes |
| --- | --- | --- | --- |
| `CONV_DENSE_V1` / linear | im2col `Gather` + one `MatMul` + one narrowing per channel; weights `i8` output-major `[out, in]`, transposed in the program | 1 | output-major so a cone of a tile of outputs reads whole rows (below) |
| `NORM_GROUP_SPATIAL_V1` | `[H, G, 3]` row partials committed (`Σx`, `Σx²` split at bit 31), totals clamped to their lanes' true ranges | 2 | a slice carries one interval for three lanes: without the clamps the analysis refuses `thi · 2^31` |
| activations | composed from the library's Q24 forms (`silu`, `gelu_tanh_q24`) between two narrowings; **not** the 65,536-entry table | 3 | a table costs ~11.7 KB per distinct lookup in a cone (a 64-lane leaf's close was 751 KB) |
| `MOD_ADALN_V1` | `LayerNorm · (1 + scale) + shift` from `layer_norm_exact`, the gated residual `h + gate · f` into the `i32` stream | 3 | modulation codes at `2^-e` (power-of-two exponents from the calibration) so Q24 conversion is a multiply |
| `ATTN_JOINT_STREAMS_V1` | per-stream q/k/v, one attention over `[image ‖ text]`, committed head-major q/k/v/ctx and committed row maxima and reciprocals | 4 | the last block's text stream is `context_pre_only` (no output projection, text carry passed through) |
| `PATCH_EMBED_V1`, `EMBED_TIMESTEP_TABLE_V1` | strided conv as rows + cropped position table; sinusoid rows for every offered step count, gathered by `base[steps_index] + pos` | 1 | the job's steps scalar is the step count's POSITION among the offered counts (the class gate says so) |
| `GEN_SAMPLER_AFFINE_V1` | `x' = StateWrite(x + HAFZ(Δσ_i · v · s_v / 2^24 / s_x))`, the latent at `2^-20`, `Select(pos == 0, noise, State)` | 1 | `Δσ` in Q24 per `(steps index, position)`; the sigma table equals the scheduler's own to 2e-8 |
| `GEN_STAGE_VAE_V1` | resnet / mid attention / nearest ×2 (a pinned `Gather`) / head with `Clamp(HAFZ((y + 1) · 255 / 2), 0, 255)` | ~8 a resnet | one op group per stage; every lowerer's output a commit point |

**What the first build taught** (each is a rule a later profile should start from):

1. **The IR's rank limit is 4**, so diffusers' `nhwpqc → nchpwq` unpatchify is a pinned `Gather`, not `Reshape`/`Transpose`.
2. **A table is not free in a cone.** An axis-0 row of a param is its own artifact leaf with its own Merkle path
   (~1.1 KB); an activation table opens one leaf per distinct lookup. The first denoise `StateWrite` close was 159,714 B
   because `post` had no commit points; the first `cond_a` close was 751,455 B because SiLU was a table.
3. **Weights are opened by rows, so `tile_len` is a cone-size parameter.** A cone of a `T`-lane output tile opens `T` weight
   rows; at `tile_len = 64` every linear tile cost ~70 KB whatever its width. The class is declared at `tile_len = 16`.
4. **A program block holds 512 nodes.** A joint block as one block is ~530 nodes once the activations are composed; it is
   two layer blocks (attention half, MLP half) with the modulation split between them (widest block 273 nodes).
5. **Commit points make cones small, and cost leaves.** Committed: the modulations, the modulated streams, the embedders'
   outputs, `norm_out`'s modulation and rows and `proj_out`'s rows, and every VAE lowerer's output (15,738 canonical step
   leaves at `tile_len` 16).
6. **A worker opening a cone rehashed the whole inventory per leaf** (`open_artifact_leaf_v1`: 60 s a close on a 1.5 MB class);
   `open_artifact_leaves_v1` builds the tree once (byte-identical openings, tested across inventory sizes).

**Fidelity** (`tests/diffusers_sd3.rs`; four evaluation cases the calibration did not see, steps 2 and 4, diffusers' own
reference fed the integer run's noise words; thresholds are the measured values below, frozen by this record):

| check | measured |
| --- | --- |
| Rust `f64` denoiser vs diffusers, velocity, on diffusers' own text tensors | 1.1e-6 … 7.0e-6 of the velocity's maximum |
| each transformer half vs its float half (one carry in, one carry out) | 0.02 % … 0.06 % of the stream maximum |
| each VAE stage vs its float tensor | 0.3 % … 10.1 % of the site maximum (`vae.up1.r1` the worst) |
| integer pipeline latent vs the float trajectory, per step | 0.30 % … 1.4 % of the latent's maximum |
| integer image vs diffusers' image | PSNR 43.8 … 45.7 dB, maximum pixel error 7 … 11 of 255 |

**The close sizes (PALW-GEN-20, measured by the worker on the class's own run).** All commit-point kinds of the class, the
cone close at the first leaf of each, against the one-move carrier (95,037 B). The table is `docs/evidence/rfc3-sd3-tiny-cone-closes-2026-10-01.md`; the summary: 61 kinds are cone-closable and every one fits one carrier, the largest 73,676 B (`vae.up0.us`,
margin 21,361 B), the denoiser's largest 29,140 B, the CLIP text stages' largest 70,376 B (a `Gather`, frontend-lowered);
two kinds are the CLIP stages' fused-attention outputs, DISSECTED commit points, which a lie convicts through the held
dissection (ADR-0103), not a cone close.

**The §II.1 checklist, completed for the fixture** (each item sourced to the file and line of diffusers 0.40.0 as installed
in the harness venv; the real SD3-medium column says what could be read offline and what cannot):

| # | Item | Fixture (diffusers 0.40.0) | SD3-medium, from configs/code only |
| --- | --- | --- | --- |
| 1 | repositories, licences, redistribution | seeded random weights generated locally; no hub, no licence | **open** — the model cards and licence text need a download; an integer artifact is a derived work |
| 2 | transformer | `SD3Transformer2DModel` (`models/transformers/transformer_sd3.py` 79–170): two streams, `JointTransformerBlock` (`models/attention.py` 580–749) per block, `context_pre_only` on the last (`transformer_sd3.py` 160–167), `AdaLayerNormZero` (`models/normalization.py` 130) with chunk order shift/scale/gate/shift_mlp/scale_mlp/gate_mlp, `AdaLayerNormContinuous` (`normalization.py` 307, scale FIRST), LayerNorm `eps = 1e-6`, `FeedForward` GELU-tanh ×4, joint attention image tokens first (`attention_processor.py` 1422), no q/k norm, learned/sincos position table cropped to the grid (`embeddings.py` 459, 532); 2 blocks, 4 × 16, `pos_embed_max_size = 8` | the same classes; `num_layers`, widths and `pos_embed_max_size = 96` (`transformer_sd3.py` 132) are config values; SD3.5's `qk_norm` and `dual_attention_layers` are refused by name (`Sd3Config::from_json`) |
| 3 | text conditioning | `CLIPTextModelWithProjection` through the HF frontend: `prompt_embeds` = the final-norm rows (`last_hidden_state`), pooled = `text_embeds` at the first `eos`; the template `bos ‖ prompt ‖ eos ‖ pad…` to 8 | SD3's pipeline takes the PENULTIMATE hidden states of two CLIPs and a T5 (`pipelines/stable_diffusion_3/pipeline_stable_diffusion_3.py` 289–330, 713); a truncated-depth `Rows` program and the T5 are later work — the fixture's difference is deliberate and documented (`tools/gen_diffusers_sd3_fixture.py`) |
| 4 | guidance | none (guidance-free first fixture); the batch-axis CFG is the second iteration | true CFG, `guidance_scale` default from the pipeline (`pipeline_stable_diffusion_3.py` 382, 665–666): **open** as to the model card's recommended range |
| 5 | sampler | `FlowMatchEulerDiscreteScheduler` (`schedulers/scheduling_flow_match_euler_discrete.py` 92–152, 283–420): static `shift = 3.0` applied to the training sigmas AND again in `set_timesteps`, terminal sigma 0, Euler `x + (σ_next − σ) · v`; offered counts 2 and 4 | the same code; the default step count and shift are scheduler config values |
| 6 | latent space | 4 channels, ×2 compression (one upsample), patch 2, `scaling_factor 1.5305`, `shift_factor 0.0609` applied as `z / scaling + shift` (`pipeline_stable_diffusion_3.py` ~1136) | 16 channels, ×8 compression; the factors are VAE config values |
| 7 | VAE decoder | `AutoencoderKL.decode` (`autoencoders/autoencoder_kl.py` 36, 199, 214), `Decoder` (`autoencoders/vae.py` 180–), `UNetMidBlock2D` (`unets/unet_2d_blocks.py` 589) with ONE-head attention over the pixels, `UpDecoderBlock2D` (2575), `ResnetBlock2D` (`resnet.py` 188), nearest ×2 + 3×3 (`upsampling.py` 74), GroupNorm `eps = 1e-6`, no quant convs | the same classes at 4 resolutions; GroupNorm groups and widths are config values |
| 8 | resolutions | 8×8 latent → 16×16 image | 1024² default (4,096 tokens at patch 2): needs lane H's C2/C3 (§5.4a), **open** for the card's aspect buckets |
| 9 | reference pipeline | diffusers 0.40.0, torch 2.14, transformers 5.17, fp32; the reference is the pipeline's own denoise loop fed the integer run's noise | the same; SDPA attention (`JointAttnProcessor2_0`) |
| 10 | variants | base only | distilled/turbo variants and editing inputs: **open** |

**Deferred, with the reasons recorded** (not built here, not forgotten):

* **UNet families (lane H's C1).** A UNet denoiser carries skip tensors across resolution levels inside one step, and its
  decoder re-reads encoder tensors: that needs a per-occurrence carry signature (the IR has ONE carry signature for every
  layer block, ≤ 8 tensors, and re-committed pass-through carries). The first profile is a transformer and is unaffected.
* **Audio.** Five corpus entries are blocked on `JobAudio` and the audio ceiling set (FR-23, class *protocol*):
  `whisper` (audio input binding, log-mel front end, Conv1d stem, cross-attention), `wav2vec2` (raw-waveform input,
  conv feature extractor, grouped positional conv), `speecht5` (speech pre/post-nets and a vocoder, an audio output
  kind), `musicgen` (multi-codebook autoregressive decoder with a delay pattern, audio output), `encodec` (causal conv
  codec with residual vector quantisation). Scheduled after step 8.

---

<a id="source-v1-0003-L02253"></a>

### 旧RFC-0003 / 原文2253–2289行

出典: [0003-palw-generative-model-classes.md](../rfc/0003-palw-generative-model-classes.md)。配置理由: 画像・動画の延期profileを管理RFCへ。

#### II.6 Video generation

**Job fields.** The image body plus `frames`, which MUST equal the class's. fps is class metadata,
carried in the output header. The item index is `clip_index`. One class per (resolution, frame count).

**Canonical output.** `u8` THWC with header (`T`, `H`, `W`, fps).

**Mapping onto the TIR scan.** It generalises §II.1:

- the latent gains a time axis, `[C, F, H, W]` (`VIDEO_INIT_NOISE_V1`);
- the denoise stage is the same scan (one step = one position), with multi-axis RoPE over
  `(frame, y, x)` from static tables;
- attention runs over all spatio-temporal tokens inside a step.

A frame-causal 3-D VAE decoder maps naturally onto a **scan over latent frame chunks**: its causal
convolution caches become `Fixed` states, and its `Rows` output is the pixel frames per position.

**What the IR lacks.**

- Dissection over the token axis (§II.1.5.6) is **required**, because one head's K and V exceed the
  close ceiling at a few tens of thousands of tokens.
- Per-position leaf ceilings (`2^22`) are exceeded by a single step at production sizes.
- The commitment volume decides feasibility.

**Court feasibility, with an example geometry** (`P_tok = 1.4·10^10`, `d = 5,120`, `L_blk = 40`,
`N_tot ≈ 33,000`, `S = 50`, `c = 2`):

- about 9·10^16 MACs per clip, about 13 minutes on a 120 TMAC/s GPU;
- about 2.7·10^13 bytes of lanes to hash;
- about 34 MB of K and V per head.

Production-scale video is therefore not practical under per-step full commitment. It needs dissection
**and** either packed lanes or lazy commitment (*Alternatives*). Small clips (≤ 1B, ≤ 16 frames at
256²) are feasible for testnet once dissection exists.

---

---

<a id="source-v1-0004-L00001"></a>

### 旧RFC-0004 / 原文1–1155行

出典: [0004-palw-model-improvement.md](../rfc/0004-palw-model-improvement.md)。配置理由: 主題別統合。

### RFC-0004: PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **適用時期の現行方針（2026-10-17 文書化）:** 本 RFC のモデル改善 protocol は **mainnet 後へ延期・当面無効**。初期 mainnet の必須実装・提供・有効化に含めず、mainnet 開始による自動有効化もしない。共有のテキスト・固定登録・検証・会計は維持する。過去の Status・実装・試験・全面有効化計画はこの範囲の有効化承認ではない。[正式な範囲・無効化要求・再開条件](../governance/post-mainnet-scope.md)。今回 node コードは変更しておらず、runtime の無効化は未確認。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


#### 2026-10-09 改定 — 改善結果は独立モデルとして登録する

**新規登録は自由。登録内容は永久不変。モデル改善は新規登録。既存モデルの置換は禁止。** [ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md)を適用する。学習、蒸留、LoRA、dataset処理と評価は続けられるが、勝者を既存lineの次のheadへ反映する方式は新規則で廃止する。評価勝者は独立登録済みのcandidateとして`CandidateSelected`を記録し、そのモデル自身のroot・検証責任・報酬資格・Positionを使う。親のmodel ID、version、root、head、AMMは動かない。親子参照は来歴だけである。

##### 実装状態と残作業

| 項目 | 現在の範囲 | 残作業・有効化条件 |
| --- | --- | --- |
| RFC0004 モデル改善 | epoch・評価・typed/composite artifact・reward foldの既存実装を保持。新しい不変登録fenceと独立candidate選択を実装。旧Part I/Part IIの「統合済み」を同一line更新の有効化根拠にしない | `palw_improvement_v1`と`palw_model_immutable_v1`はshipped ParamsではNone。明示的な高さ・独立試験・関連gateが必要 |
| memory line / typed roots | 学習・評価用の独立material。登録済みモデルのweights/rootを差し替える経路ではない | 担保条件は別決定・検証が必要。旧一覧の「利用者判断4d」はmemory materialの独立した運用条件として扱い、担保額は未設定 |
| kind別の検査費用 | MEASは未取得の実測入力。typed rootやhash登録だけで検査が完了した扱いにしない | kind、サイズ、CPU/GPU、cold/warm取得、公開証拠・exact court、帯域・メモリとdeadlineを実測し、担保・資源上限へ反映する |

以下のpromotion/rollbackと旧予定表は、fence前の設計・実装履歴として保持する。新規則下の実装はspec17の改定節を優先し、自己改善やtyped-rootの休眠経路をこの改定で有効化しない。

> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


| Field | Value |
| --- | --- |
| Status | Revised Draft, 2026-10-06 — improvement remains independent of a VM. ADR-0172 withdraws the later BVM/GVM programme; model extension uses versioned kernels, with RFC07/RFC11 probabilistic verification (§0) |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-29 |
| Normative dependencies | **RFC-0002** (PALW-TIR) and **RFC-0003** (R, pipelines, canonical outputs); new kernel-bound registration/verification additionally follows [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md), RFC07, RFC11 §§15–16 and ADR-0171/0172. Legacy profiles keep their rules. **No VM or EVM-contract dependency** |
| Affects | spec/palw 03 (registry: candidates, composite artifacts), 05 (canonical work of evaluation jobs), 07/08/09 (evaluation claims and their court), 10 (fees, bounties, vesting), 11 (the data-use opt-in), 15 (lines: the improvement policy and protocol promotion), 16 (fences), and a new chapter `spec/palw/17-model-improvement.md` · all networks (dormant until armed) · `consensus/core` (the fold: epochs, candidates, evaluation jobs, promotion, the pool), `misaka-palw-tir` (the scoring library), `misaka-palw-tir-lower` (adapter lowering), `misaka-palw-sdk`, the node |
| Branch | `rfc/0004-0005-vm` (text only) |
| Related | RFC-0005 (VM withdrawal reason), ADR-0172 (kernel-only extension), ADR-0088 and spec 15 (lines, versions, proposals, the market), ADR-0160 (claim capacity), ADR-0144 P1–P7, ADR-0145 (canonical work), ADR-0069 (weight), ADR-0044 F5/F15 (beacons), ADR-0072 (one inference, one ticket) |

#### 概要(日本語)

- **目的。** 外部から登録されたモデル(例:Qwen v12)を、実利用が生む hard case と収益を使って蒸留と RL で強くし、勝った候補を
  次の head(v13)にする。これを **PALW-TIR / 有効なKernelで、VMなしで** 回す。2026-10-06の方針変更により、
  RFC-0005のBVM/GVMは後からも実装せず、モデル拡張はversioned Kernelで行う(§0)。
- **層。** Layer 0 = PALW-TIR(候補を評価する場所)。Layer 1 = この Improvement Protocol(epoch・dataset・候補の registry・
  報酬・promotion)。自動化や学習はconsensusの外。旧Phase B/CのVM実装計画は撤回し、[versioned Kernel 設計](../design/palw/models/versioned-kernels.md)へ置き換える。
- **分担。** 学習は consensus の外、評価は PALW-TIR、promotion はプロトコル。**作り方は自由で、成果物だけを厳密に検証する**
  (Bitcoin はブロックが正しいかを検証し、ASIC の作り方は問わない)。規範:**候補モデルが VM で作られたことを要求しては
  ならない(MUST NOT)**。プロトコルが検証するのは 4 つだけ:モデル成果物(IR class の admission)、provenance policy
  (宣言の形と参照の有効性。真偽ではない)、評価結果、promotion の資格。
- **既存の土台。** line は spec 15(ADR-0088)の line。改良に opt-in した line の head は改良行の `head`(IR class id)と
  その履歴で、spec 15 の version 行と `current` にはプロトコルは書かない(Phase F の class id は重みを含むので新しい重みは
  新しい class。version に class id を書く当初の推奨形は spec 15 の稼働中の読み手 3 つを壊すと分かった — §3)。opt-in した
  line では developer の `ModelVersionPromoted` を拒否し、head はプロトコルの promotion と rollback でだけ動く。利用量は
  head class の Final claim を改良行で数える。
- **consensus-native な epoch 状態機械。** 利用量が閾値を超える → OPEN_IMPROVEMENT_EPOCH → hard case の dataset_root を
  固定 → 候補の提出窓 → CLOSE_SUBMISSION → 凍結後に届いた hard case と setter セットから R で評価項目を引く → 各候補を
  評価 → 合格した最良の候補を promotion し、lineage head を更新。状態・オブジェクト・遷移を規定する(tag は提案だけで、
  番号は取らない)。EVM コントラクトの bounty 市場は任意の追加で、何もそれに依存しない。
- **評価は RFC-0003 の pipeline と TIR の採点 stage。** 親と各候補を同じ評価入力で TIR 上で走らせる。採点は
  (1) R で seed した生成の RFC-0003 正準出力の完全一致、(2) 参照の teacher-forced 対数尤度(forward 1 回で安く正確)、
  (3) 登録済みの judge / reward-model class(評価時に登録集合から R で引く。重みは低く、完全一致の anchor で校正)、
  (4) 親と候補の対比較。**任意コードをテスト実行して評価する機能は対象外**なので、コードは完全一致
  出力か尤度の課題で扱う。評価は隠されていて、しかも未来のもの:凍結後の時間的 hold-out と、答えを採点後に開示する bonded
  setter セット。
- **promotion。** 項目ごとの対比較、δ の差、下側信頼限界 > 0(ピン留めした二項表による整数の符号検定、候補数で Bonferroni)、
  回帰スイートと安全スイートで ε を超える後退なし。満たす候補がなければ現 head のまま。rollback の道がある。
- **候補の成果物。** 完全な重み、または親 + adapter(IR グラフの中の unmerged LoRA 経路。lowering は `tir/lower` a4bd7141f)。
  (1) 候補の最初の P 個の param は親と byte 一致で、artifact_root は親の root と adapter 部の root を合成するので、親の tensor は
  二度と commit しない。(2) class id は Phase F の式のまま合成 root にかかり、親 class・候補 program の root・adapter root・P を
  束ねる。(3) 親の scale を再利用する:adapter が親の約 2 倍の余裕を超えて活性を動かせば clip し、それは候補の評価点に表れる
  候補自身の危険。再較正は byte 一致の再利用を壊すので、完全な重みの候補として出す。(4) 1 射影あたり約 20 node 増えるので、
  468 node の MoE・gated-delta block では `all-linear` が 512 node の上限に入らない。そういう line は狭い adapter か完全な重み。
  DoRA・学習した bias・`modules_to_save`・`layers_to_transform`・融合射影・`fan_in_fan_out`・非正方 rsLoRA は v1 の外。
  prefetch は親の約 3 %(1.5B で約 53 MB)。
- **学習を IR で書くこと。** 理論上は可能(forward・backward・optimizer を compiler で 25 primitive に落とす。autodiff op は
  要らない)。v1 は学習を consensus に入れない(研究手法を凍結してしまうから)。将来の任意 profile として記録し、primitive で
  足りるか(optimizer state、勾配の累積、reduction、R による sampling、batching)を未決事項に残す。
- **報酬は段階的で、すべてに信頼ラベル(V・B・J・T)。** S1 = 完全一致で検証できる課題だけの bounty。S2 = 勝者の manifest が
  宣言した登録済み dataset への gain share(「実際に使ったかは検証できない」と明記)。S3 = ablation(研究)。trainer の報酬は
  vest し、後で証明が出れば没収。原資は line の手数料収入の一定割合 φ、候補の手数料、sponsor の預託で、PALW の worker 報酬には
  触れない。
- **ライセンス・プライバシー・RL。** closed API の出力は `TeacherLicence` 付きの `LICENSED_DISTILL` だけ。base model は派生物を
  許すライセンスだけ。実利用からの hard case は明示の opt-in(既定 off)。RL は IR で表せる報酬による RLVR で、rollout は
  PALW job。
- **容量と費用。** 評価 claim は ADR-0160 の容量を消費する。候補は登録料と評価料を払い、1 epoch の候補数に上限がある。seat の
  prefetch 負荷も数える。例:1.5B、n = 400、候補 4、1 項目 512 位置で約 10^6 位置 — 1 ノード 17 tok/s なら約 17 時間分で、
  多数の executor に分かれ、検証の複製は別。
- **脅威と比較。** poisoning と backdoor(評価は trigger を見られない。軽減はするが閉じない)、汚染、答え鍵の攻略、judge と
  尤度の攻略、sybil とコピー、ライセンス洗浄、結託、出題者の漏洩、beacon の grinding、費用 DoS、容量の圧迫。Bittensor・
  計算の貸し出し(ブリーフの "FLOP")・Virtuals との違いは、検証を先に置いた報酬と、チェーン自身の実行で測る改良。
- **工数(Phase A、VM なし、rung A も待たない)。** 既存コードに対する実装項目 A1〜A15(line の改良 policy と promotion、
  epoch 状態機械、hard case・setter・opt-in、合成 artifact、評価 job 種別、採点 pipeline、promotion 演算、報酬と pool、node 側、
  adapter の lowering、第 2 実装、drill、監査、soak)で合計 **60〜84 engineer-weeks(約 14〜20 EM)**。agent による実装は
  約 3〜5 週、mainnet-safe約 8〜11 か月という**旧Phase Aの見積もり**。新Kernelと確率的検査の実装・監査工数は含まず、
  現在の納期予測ではない。新経路は§0の依存関係を加えて再見積もりする。

#### Summary

A registered model should not stay as registered. This RFC defines the **Model Improvement Protocol**:
a consensus-native loop in which a line's real usage produces hard cases and revenue, anyone trains
candidates off chain by any method, the network evaluates the parent and every candidate on hidden
items drawn from the future, and the protocol promotes the best candidate that passes a conservative
statistical rule. Everything runs on PALW-TIR (RFC-0002) and RFC-0003's pipelines. There is no VM and
no EVM dependency.

- **Layers.** Layer 0 is PALW-TIR, where candidates are evaluated. Layer 1 is this protocol: epochs,
  datasets, the candidate registry, rewards and promotion. Automation stays off chain; new model
  semantics use the [versioned kernel design](../design/palw/models/versioned-kernels.md).
- **The division of work.** Training is outside consensus; evaluation is PALW-TIR; promotion is the
  protocol. The generation method is free and only the artifact is strictly verified — as Bitcoin checks
  that a block is valid, not how the ASIC that mined it was built. A candidate MUST NOT be required to
  come from the VM.
- **What is verified.** Exactly four things: the model artifact (IR class admission), the provenance
  policy (form and references, not truth), the evaluation result (claims accepted under their pinned verification policy), and
  promotion eligibility (fold arithmetic). Training, provenance truth and freedom from backdoors are not
  verified, and every reward says which of its inputs are.
- **Evaluation** runs the parent and every candidate through RFC-0003 pipelines on the same inputs:
  exact match on canonical outputs of R-seeded generations, teacher-forced reference log-likelihood,
  and registered judge classes drawn by R at evaluation time (low weight, anchored). Promotion needs a
  paired win by `δ` with a lower confidence bound above zero and no regression beyond `ε`; otherwise the
  head stays, and every promotion can be rolled back.
- **Candidates are cheap to carry**: full weights, or the parent plus an adapter — an unmerged LoRA
  path in the IR graph — whose composite artifact lets seats reuse the parent's bytes.
- **Implementation.** The RFC ends with a plan against the existing code (spec 15's lines, the fold,
  Phase F's IR classes and court, ADR-0160's capacity) of 60–84 engineer-weeks, about 3–5 weeks of
  agent implementation and 8–11 months to mainnet-safe use. These are historical Phase A estimates,
  excluding the new kernel/proof programme; they are not a current delivery commitment.

#### 0. Kernel-only registration and probabilistic evaluation (2026-10-06)

This section takes precedence over legacy Phase F wire examples and cost assumptions below.
[ADR0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) withdraws
the BVM/GVM programme; [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md) defines the replacement. Preserve
[RFC11 §§15–16](../rfc/0011-permissionless-model-and-long-context-onboarding.md) and
[RFC07](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md): small probabilistic checks
of committed encoded constraints are the ordinary large-model verification path, not full replay.

* **Admission is not a prior proof of every future output.** Bind the active kernel descriptor,
  plan, complete model/task/context and artifact. Check finite typed composition, constraint coverage,
  fees/state and worst DA/court bounds deterministically. An external worker may prepare the artifact;
  the registering user or node need not reproduce its entire inference. Fidelity and operator readiness
  remain separate requirements. Missing semantics require a reviewed kernel extension, never a VM.
* **An evaluation still executes the real task.** Its producer executes and creates evidence; the
  Panel checks complete algebraic/encoded relations under post-commit challenges. Positive receipts,
  DA, the closed challenge window and no unresolved dispute precede Final. ExactMatch is an exact
  comparison of committed answers, not a requirement to rerun inference exactly at every seat.
* **Promotion consumes correctly labelled evidence.** Keep V/B/J/T labels but add an assurance mode,
  claim id, kernel/suite id and applicable error bound for each V result. Exact fold arithmetic,
  bounded exact court, legacy checks and probabilistic claims are distinguishable. A judge's opinion
  remains J even when its execution is valid. A checksum or quorum alone is not a computation proof.
* **Two statistical questions stay separate.** The sign test in §7.5 measures evidence of improvement
  on the selected tasks; §15 of RFC11 bounds computational false acceptance. For at most `M` evaluation
  claims with established check bounds `ε_i`, a union bound is `Σ ε_i` (or `M·ε` if uniform), conditional
  on those assumptions. Add separately the common Panel/randomness/DA failure events and account for
  retries; do not equate the sign-test significance with cryptographic soundness or assume independent
  seats. The proposed per-claim `2^-128` target is not a proved property of this draft.
* **Identity and compatibility are explicit.** Legacy class-id formulas stay unchanged. New-format
  candidates bind kernel and plan as the [Kernel design](../design/palw/models/versioned-kernels.md) specifies. Kernel versions may differ only under an active
  family/evaluation-composition profile preserving task, tokenizer/output and policy requirements;
  changing kernels cannot change the epoch's score definition. Pin the accepted profiles when the
  epoch opens; no mid-epoch change of interpretation or retroactive regrading.
* **Normal-path checks are small, not incomplete.** All output/state/reward-relevant constraints must
  enter the sound aggregation. A failing check localizes to bounded kernel court; silence, missing
  evidence or unsupported constraints cannot finalize. Old profiles/claims retain their existing
  rules, and full replay remains a development/conformance baseline where needed.

For the new route, extend the improvement fence/epoch commitment with the permitted kernel,
verification and evaluation-composition policies; the legacy `palw_improvement_v1` example below is
not sufficient by itself. No activation is implied. Tests must cover forged scores, omitted adapter
constraints, wrong routing/state, invalid suite changes, reorg, old/new claims and correct promotion
after valid small-check Final. No VM implementation is a dependency of these tests.

#### Motivation

1. **A registered model is frozen.** A class is its program, its weights and its tokenizer. Spec 15
   (ADR-0088) lets a line publish new versions and record proposals, evaluations and datasets, but all
   of it is declared: a developer promotes a version by signing it, and "evaluations are declarations
   from anyone" (PALW-MK-3). Nothing connects a line's usage to a better head, and nothing checks that
   a new head is better.
2. **The user's goal** is that models registered from outside grow stronger through RL and
   distillation. Real usage produces hard cases and revenue; teachers and trainers turn them into
   candidates; the network finds the one that is really better and makes it the head; and the loop
   repeats.
3. **Everything evaluation needs is already TIR.** Generation, likelihood and judge models are IR
   classes. Scoring is a small TIR program. Promotion is integer arithmetic in the fold. So the loop
   needs no VM; the [Kernel design](../design/palw/models/versioned-kernels.md) supplies extension rules, while arbitrary automation stays off chain.
4. **The Bitcoin analogy.** Bitcoin checks that a block is valid. It does not care how the ASIC that
   found it was designed. This protocol checks a candidate — its artifact, its declarations, its
   measured result — and does not care how it was trained. That keeps research methods free, and keeps
   consensus out of everything it cannot check.

#### Goals and non-goals

**Goals.**

- G1. Three layers: TIR evaluates (Layer 0), this protocol decides (Layer 1), and any automation is
  optional (Layer 2, outside consensus).
- G2. The generation method is free and only the artifact is verified. No candidate is required to have
  been produced by the VM, or by any particular method.
- G3. The protocol verifies four things and nothing else: the artifact, the provenance policy, the
  evaluation result and promotion eligibility.
- G4. Epochs are consensus-native: states, objects and transitions in the fold, advanced at DAA
  boundaries.
- G5. Evaluation is hidden and in the future; promotion is conservative and can be rolled back.
- G6. Candidates are cheap to carry: adapters reuse the parent's artifact.
- G7. Rewards are staged, and each names what it rests on (V, B, J or T).
- G8. Licences and privacy by default.
- G9. Evaluation is accounted for in the network's claim capacity (ADR-0160).
- G10. Implementable against the current code with no VM and no EVM contract.

**Non-goals.** Training inside consensus in v1 (§12). Verifying which data a trainer used. Code
evaluation by running arbitrary tests (no VM implementation is planned). Subjective validator weights. Renting agents,
GPUs or accounts. Paying from PALW worker rewards. Legal advice.

#### 1. Layers and phases

```
Off chain                    automation, training and research tools; submit ordinary jobs/candidates
───────────────────────────────────────────────────────────────────────────────────────────────────────
Layer 1 (this RFC, Phase A)    the Improvement Protocol: the line's policy · epochs · hard cases · datasets ·
                               the candidate registry · evaluation jobs · rewards · promotion · the lineage head
───────────────────────────────────────────────────────────────────────────────────────────────────────
Layer 0 (RFC02/03/05/07/11)   versioned tensor/state kernels; encoded-constraint checks for normal verification,
                               exact bounded court for disputes; legacy profiles unchanged
```

- **Phase A** remains this improvement protocol, with no VM dependency.
- The former **Phase B/C** BVM/GVM roadmap is withdrawn by ADR0172, not merely delayed.
- New model families follow the [Kernel design](../design/palw/models/versioned-kernels.md)'s extension gates; existing-family plans need no new node
  semantics. Off-chain loops submit ordinary candidates and cannot change promotion or rewards.

#### 2. The division of work

| Activity | Where it runs | What consensus knows about it |
| --- | --- | --- |
| data collection, teaching, training | anywhere, by any method | nothing, beyond declarations |
| evaluation | Active tensor/state kernel: normal constraint checks, exact court on dispute | committed scores and their claim/suite/assurance identities |
| promotion, rewards, rollback | the fold | everything: it computes them |

**The generation method is free; only the artifact is strictly verified.** Normatively (PALW-MIP-1),
**a model candidate MUST NOT be required to have been produced by the VM**, or by any particular method,
tool or party.

##### 2.1 The four things the protocol verifies

| # | What | How | Label |
| --- | --- | --- | --- |
| 1 | **the model artifact** | admitted class under its bound active kernel (legacy Phase F v10 retained), same family interface; complete artifact and plan coverage (§0, §6) | V |
| 2 | **the provenance policy** | the candidate's declarations satisfy the line's policy: every dataset it names is registered, every licence class and teacher class is allowed, every `TeacherLicence` it cites exists and covers the use | V for the form and references; B for their truth |
| 3 | **the evaluation result** | evaluation claims Final under the bound suite/window/DA policy; exact court only for disputes | V with assurance metadata (§0) |
| 4 | **promotion eligibility** | the fold's paired rule over committed scores (§7.5) | V |

##### 2.2 Trust labels

Every reward and every quantity an interface shows carries one of four labels.

| Label | Meaning |
| --- | --- |
| **V**, verified | accepted under the declared assurance mode: exact fold/court result or probabilistic constraint-verified claim; carry suite/error/claim metadata (§0), never imply zero-error whole-execution replay |
| **B**, bonded | a statement someone staked on, challengeable by a stated procedure; a false one forfeits the bond |
| **J**, judged | a registered judge class's output, verified as *the judge's* answer: deterministic, and gameable |
| **T**, trusted | not checkable at all, and stated as an assumption wherever a reward rests on it |

| Object | Label |
| --- | --- |
| a candidate's outputs and scores on evaluation items | V |
| a hard case's hardness for an exact-match or likelihood task | V (given the key: B) |
| an answer key or reference from a setter | B |
| a judge's score | J |
| **training** — which data a trainer used, and how | **T** |
| an artifact's provenance and licence | B |
| a winner's freedom from backdoors | **T**, mitigated (§15) |

No interface may present a B, J or T quantity as verified.

#### 3. Lines, heads and the improvement policy

The protocol builds on spec 15's lines (ADR-0088) rather than beside them.

- **The line.** `PalwModelLineV1` (`consensus/core/src/palw_model_lines_v1.rs`): `(class, owner, name)`,
  with roles, a `current` version, previews, a contributor's permille of the owner's leg, and per-version
  usage (`PalwVersionUsageV1`: attempt claims, free-prompt claims, work leaves).
- **The head of a governed line is a class id in the line's improvement row** (`improvement_lines`),
  with a bounded head history, and it names an IR class. A legacy version is a root in force of one class
  (PALW-MK-2); Phase F's IR class id, however, commits to the artifact root, so new weights are a new
  class.
- **Amended (2026-09-29, implementation).** This RFC first recommended writing the promoted class id
  into spec 15's `PalwModelVersionV1.root` and making it the line's `current` version, so that no new
  pointer was needed (open question 1). Implementation found that form breaks three live readers of spec
  15's rows:
  1. `class_roots_in_force` reads every in-force version root of a class's lines, and attempt admission
     accepts any of them — so a class id would become an "artifact root" an attempt of the parent class
     could name;
  2. a promotion supersedes the parent's version, whose root then leaves force after the grace, so the
     parent class's attempts are refused and its cadence share goes dormant, while the promoted class —
     a separate IR class with its own share — inherits nothing;
  3. free-prompt usage is attributed through the current version's root, so it would land on the wrong
     version.
  The protocol therefore writes neither spec 15's version rows nor `current`: the governed line's head
  lives in its improvement row, and the market, the owner's leg and the parent class's share and roots
  are untouched. The trigger counts the head class's claims at `Final` in the same row (§4), not spec
  15's per-version usage, which attributes by artifact root.
- **Opting in.** The owner signs `ModelLineImprovementPolicySet { line, policy }` (a proposal, like
  every object here). From then on the line is **governed**: its head moves only by this protocol's
  promotion or rollback, and the developer's `ModelVersionPromoted` is refused on it.
  The policy changes only between epochs. Opting out takes effect after the current epoch and a delay.
- **The policy** (`PalwImprovementPolicyV1`):

  | Field | Meaning |
  | --- | --- |
  | `usage_threshold` | usage of the head since the last epoch (claims or work leaves) that opens an epoch |
  | `grid`, `w_collect`, `w_submit`, `w_holdout`, `w_eval` | the epoch's boundaries, in DAA (§4) |
  | `eval_spec` | the scoring pipelines and their weights, the regression and safety suites (roots), the judge set, `n`, `n_min`, `δ`, `ε`, `ε_s`, `α` (§7) |
  | `k_max` | the most candidates an epoch evaluates |
  | `fees` | the candidate registration fee, the evaluation fee per job, the hard-case fee |
  | `phi_permille`, `bounty_share` | the pool's funding and the share of it that S1 may spend (§8) |
  | `provenance` | the teacher classes and licence classes allowed, and whether full-weight candidates are allowed (§6) |
  | `rollback_epochs`, `vest_epochs` | the owner's rollback window `R`, and the vesting length `v` |

#### 4. The epoch state machine

One epoch at a time per governed line. Every transition happens at a DAA boundary that the policy and
the chain's own facts determine, so every node advances every line identically, with no object needed
for most transitions.

```
            usage ≥ threshold at a grid boundary
  Idle ─────────────────────────────────────────────► Open ── t_fix ──► Submission ── t_close ──► HoldOut
   ▲                                                   (hard cases,      (candidates,                (post-freeze
   │                                                    datasets)         fees, bonds)               cases only)
   │                                                                                                   │ t_draw (+ beacon)
   │                                                                                                   ▼
   └── Vesting ◄── Promoted(class) / NoChange ◄── t_score ── Scoring ◄── t_eval ── Evaluating ◄── Drawn
```

| State | Enters when | What happens | Leaves when |
| --- | --- | --- | --- |
| `Idle` | the line opts in; an epoch ends | the head's usage since the last epoch is counted in the line's improvement row (the head class's claims at `Final`, §3) | a grid boundary finds usage ≥ `usage_threshold` → **OPEN_IMPROVEMENT_EPOCH** |
| `Open` | `t_open` | hard cases, registered datasets and teaching artifacts are admitted into the epoch's training material | `t_fix = t_open + w_collect` |
| `Submission` | `t_fix`: **`dataset_root` is fixed** — the Merkle root over the material admitted in `Open`, published for trainers | candidates are submitted, with fees and bonds, up to `k_max` in acceptance order | `t_close = t_fix + w_submit` → **CLOSE_SUBMISSION**: the candidate set freezes; nothing is withdrawn after it |
| `HoldOut` | `t_close` | only hard cases admitted from now on are eligible as evaluation items; setter sets must already be committed | `t_draw = t_close + w_holdout` |
| `Drawn` | the first block `d` DAA past `t_draw` | the epoch seed is that block's beacon (ADR-0044 F5/F15); R draws `n` items from the eligible pool, one judge per judged item from the judge set, and a generation seed per item; setter prompts are disclosed | at once |
| `Evaluating` | `Drawn` | evaluation jobs — one per item and subject (the parent and each candidate), plus pairwise judge jobs — are claimable (§7.2) | `t_eval = t_draw + w_eval`, after which no evaluation claim is accepted |
| `Scoring` | every evaluation claim is final, or the court window after `t_eval` has passed | setter keys are revealed; the fold scores and applies the promotion rule (§7.5) | at once |
| `Promoted(class)` / `NoChange` | `t_score` | the head moves, or not; pool payouts are scheduled; unexecuted evaluation fees are refunded | → `Vesting` |
| `Vesting` | promotion | rewards vest over `vest_epochs`; the owner may roll back within `rollback_epochs`; a proof may roll back and forfeit (§7.6) | the next epoch opens (vesting continues in parallel) |

An epoch with fewer than `n_min` eligible items at `t_draw`, or with no candidate, ends in `NoChange`.

##### 4.1 Objects

Every name here is a proposal. No tag or object number is claimed; the fence and the ADR assign them.

| Object | Signed by | Effect |
| --- | --- | --- |
| `ModelLineImprovementPolicySet` | the owner | opts a line in, or changes its policy between epochs |
| `HardCaseSubmitted` | a bonded submitter | a hard case (§5.1) enters the current state's pool |
| `DataUseOptIn` | a job's committer | makes that job's prompt usable as a hard case (§10) |
| `SetterSetCommitted` / `SetterSetRevealed` / `SetterKeysRevealed` | a bonded steward | a private evaluation set: committed before `t_close`, prompts disclosed at `Drawn`, keys after outputs are final |
| `DatasetRegistered` | a bonded contributor | a dataset with its content root, licence classes, teacher classes and provenance commitment (§5.3) |
| `TeachingArtifactCommitted` / `TeachingArtifactRevealed` | a bonded teacher | an artifact for an S1 bounty (§8.1) |
| `TeacherLicenceRegistered` | a rights holder | a licence for `LICENSED_DISTILL` (§9) |
| `CandidateSubmitted` | the submitter | a candidate (§6), its declarations, its fees and bond |
| evaluation claims | executors | the existing claim objects, with the evaluation job family (§7.2) |
| `LineageHeadRolledBack` | the owner, or anyone with proof | restores the previous version (§7.6) |

The fold keeps one row per governed line (`improvement_lines`) and one per epoch (`improvement_epochs`),
entering `state_root` only once written, as every late table does (ADR-0087 M7).

##### 4.2 Optional contracts

EVM-lane contracts MAY add bounty markets or sponsorship schemes that read the finalized facts of this
protocol — through a read path of the ADR-0089 D2 kind, which is itself optional. **Nothing in this
protocol depends on them**, and none of them can move a head.

#### 5. Hard cases, datasets and teaching artifacts

##### 5.1 Hard cases

```
HardCaseV1 { case_id, domain, prompt (token ids under the line's tokenizer),
             reference: ExactKey { commitment } | Continuation { commitment } | None,
             source: UsageOptIn { job_pin } | Setter { bond } | Artifact { artifact_id },
             head_evidence: Option<{ claim }> }
```

- **Sources**: real usage, with its user's opt-in (§10); bonded problem setters; and `SyntheticProblem`
  and `HardCaseVariant` artifacts.
- **References.** An exact-match task carries a committed key (the answer span); a likelihood task a
  committed reference continuation. A usage case may attach a reference under a bond; without one it is
  judged only.
- **Hardness.** For exact-match and likelihood tasks it is verified (V, given the key): `head_evidence`
  names a final evaluation-kind claim of the head that fails the key, or scores below the policy's
  likelihood floor. A judge-only case's hardness is J. A case the head passes is not hard and pays no
  setter reward.
- **Which pool a case joins** depends only on when it was admitted: before `t_fix`, the epoch's training
  material; in `HoldOut`, the evaluation pool; otherwise the next epoch's.

##### 5.2 Verification types in Phase A

| Type | What decides it | Label |
| --- | --- | --- |
| **EXACT** | the canonical output's answer span equals the committed key (§7.3) | V (the key: B) |
| **LIKELIHOOD** | the teacher-forced log-likelihood of the committed reference (§7.3) | V (the reference: B) |
| **JUDGED** | a registered judge class | J |
| **HUMAN** | a bonded person, challengeable | B |

Arbitrary EXEC (running tests/checkers), CRITIC (executed counterexample) and PROOF checker programs
are not supported by this profile. The former VM Phase C route is withdrawn. A future bounded,
non-VM relation/checker family needs its own kernel specification and activation; these names alone
do not authorize arbitrary program execution.

##### 5.3 Teaching artifacts and registered datasets

```
TeachingArtifactV1 {
  kind:                  Answer | PreferencePair | SyntheticProblem | HardCaseVariant | RewardSignal | Critique,
  task_id, teacher_type, teacher_id, license_class, provenance_commitment, output_hash,
  verification_type:     EXACT | LIKELIHOOD | JUDGED | HUMAN,
  bond, commit: H(artifact ‖ salt), reveal,
}
DatasetRegisteredV1 { dataset_id, content_root, items, license_classes, teacher_classes,
                      provenance_commitment, bond }
```

| Kind | Verified by in Phase A | Label | Paid in |
| --- | --- | --- | --- |
| Answer — a solution trace ending in an answer span | EXACT against the case's key, compared by the fold after the key's reveal | V | S1 bounty; S2 |
| PreferencePair | two outcomes of which one matches the key and one does not; otherwise JUDGED or HUMAN | V (J, B) | S2 |
| SyntheticProblem | the setter's committed key | B | S1, to the setter, if the head fails it |
| HardCaseVariant | the parent's key, and the head verifiably failing the variant | V | S1, to the setter |
| RewardSignal | a SELF_PLAY rollout: the head's final claim and its EXACT or LIKELIHOOD score (§11) | V | S2 |
| Critique | JUDGED only; an executed counterexample needs a separately specified active checker relation | J | S2 |

Kinds that need arbitrary execution — `TestCase`, `Counterexample`, `VerifiedCode`, `ToolTrace`,
`FormalProof` — are not promised by a future VM. They remain unsupported as machine-verified types
unless a separately reviewed non-VM kernel profile defines their exact bounded relation.

**Teacher classes** are declared in every artifact and dataset:

| Class | What it is | Admitted when | Provenance label |
| --- | --- | --- | --- |
| OPEN_DISTILL | outputs of open-weight models whose licence permits training derivatives | the declared licence class is allowed by the policy; bonded attestation | B |
| LICENSED_DISTILL | outputs of a proprietary model | a registered `TeacherLicence` from the rights holder covers the use (§9) | V that the licence exists; B that the use stayed in it |
| SELF_PLAY | the head, or other registered classes, run as PALW jobs | the claims themselves | V |
| HUMAN | human-written work, possibly tool-assisted | bonded attestation of authorship and rights | B |
| TOOL_VERIFIED | outputs of deterministic tools and solvers | the outcome verified by EXACT | V for the outcome |
| PUBLIC_DATA | public datasets under permissive licences | bonded licence attestation | B |

**Admission** is commit–reveal: the commit's block orders submissions; the reveal must match
`output_hash`; content lives off chain, content-addressed; duplicates by `output_hash` (and by the
normal form of an answer span) count once, for the earliest commit, which defeats copying from the
mempool. Spam — malformed, unverifiable under its declared type, duplicate, or inadmissible by licence —
forfeits its bond. An honest failure costs only its fee.

#### 6. Candidates and their artifacts

##### 6.1 What a candidate is

A candidate is an **admitted class under its pinned active kernel** (legacy IR uses Phase F v10) of the parent's family: the same
tokenizer, the same output interface (a `Logits` program whose logits scheme equals the parent's), and a
program the family rule accepts. It is registered as any IR class is (`ClassRegisteredTirV1`, the 1 BILI
burn, the per-block cap), and then entered in the epoch by `CandidateSubmitted` with its fees, bond and
declarations. That wire path describes the legacy profile; new kernel-bound classes use §0's
versioned admission/identity without rewriting legacy ids. Its artifact is one of two kinds.

- **Full weights.** A single artifact root, with scales of its own. Allowed only if the policy allows it,
  and charged for the seats' prefetch (§6.7, §13).
- **Parent + adapter.** The candidate's program is the parent's program with an **unmerged LoRA path** at
  each adapted projection, and its artifact is **composite**: the parent's inventory, reused byte for
  byte, plus a small adapter section (§6.3–§6.6).

##### 6.2 The adapter in the IR graph

The lowering is `misaka-palw-tir-lower`'s (`crate::lora`, `lower::lower_lora`; `tir/lower` a4bd7141f,
hf-coverage §12). It reads a PEFT adapter (`adapter_config.json`, `adapter_model.safetensors`) directly.
Each targeted projection keeps its parent weight and adds an unmerged path:

```
y = W·x (+ b) + (num/den) · B·(A·x)        A: [r, d_in], B: [d_out, r]
num/den = lora_alpha / r   (rsLoRA: lora_alpha / √r, at a square rank) — an exact rational
```

In integers, with every node a v1 primitive:

1. `A·x` is one exact `i32 × i16` product over the parent projection's own input codes. `A` is stored as
   `A' = A·diag(s_x / s_x0)` at per-row `i32` codes, so a split input needs no extra columns. The result
   is narrowed to `i16` at the adapter's own calibrated site `{site}.lora_a`.
2. `B·a` uses per-row `i16` codes, exact in `i64`.
3. `num/den` is applied as an integer `Mul` by `num` and a rounded `Div` by `den`.
4. One narrowing takes the result into the projection's **output scale**. Then comes an `Add` to the
   parent's narrowed output, and a `Clamp`.

To the lowering the node stays a `Linear`, so no pattern changes. Each adapted projection adds about 20
nodes. Merged and unmerged adapters do not round alike, so the candidate's semantics is the unmerged
program's. On four PEFT fixtures (ranks 4–64; q/k/v/o and MLPs; `all-linear`; rsLoRA):

- the unmerged float path matches `transformers`' merged weights to 3·10^−7;
- the integer candidate reaches top-1 1.000 and KL 3–5·10^−5 against the merged logits;
- the reference evaluator, the second implementation and the typed backend agree (hf-coverage §12).

Fidelity is measured, never required (RFC-0002 criterion 5).

##### 6.3 Composite artifacts and the class id

The following is the **legacy Phase F identity**, preserved verbatim. New kernel-bound classes add
the [Kernel design](../design/palw/models/versioned-kernels.md)'s immutable descriptor/plan binding in a new versioned domain (§0); never reinterpret or
rehash an already registered legacy class to opt it into new verification semantics.

The lowering orders params so that **the candidate's first `P` params are the parent program's,
byte-identical in declaration and tensor** (`lower::adapter_params_last`). Params `P..` are the adapter
section.

```
PalwTirArtifactRefV1 = Single { root }
                     | Composite { parent_class: Hash64, parent_root: Hash64, adapter_root: Hash64, p: u32 }
artifact_root(Composite) = H64(key "misaka-palw/improve/composite-artifact/v1",
                               parent_class ‖ parent_root ‖ adapter_root ‖ le32(p))
tir_class_id_v1 = H64(key "misaka-palw/tir/class-id/v1",
                      graph_ir_root ‖ H64(layout) ‖ artifact_root ‖ tokenizer_id)      // Phase F's formula, unchanged
```

- **The parent is never re-committed.** The composite root composes the parent's artifact root with the
  adapter section's root, which is an inventory over params `P..` only. Params `0..P` are served by the
  parent's inventory leaves, unchanged.
- **The class id binds four things** through Phase F's formula over the composite:
  - the parent class (`parent_class`, and through it the parent's program, layout and tokenizer);
  - the candidate's own program root (`graph_ir_root`: the parent's nodes plus the adapter paths);
  - the adapter root;
  - `P`.
- **Admission checks** that:
  - `parent_class` is the line's head, or another registered class of the line;
  - `parent_root` is its artifact root;
  - params `0..P` of the candidate's program carry exactly the parent program's declarations, in order;
  - no param at or after `P` names a parent tensor.
- **Court openings** of param `j` are made under `parent_root` if `j < P`, and under `adapter_root`
  otherwise. This is an appended variant of Phase F's artifact opening.

##### 6.4 The calibration rule

- **The parent's scales are reused.** Every narrowing param in `0..P` is the parent's, byte for byte.
  That is what makes reuse possible.
- **The adapter's own narrowing params** — the `{site}.lora_a` sites, and each path's output-scale
  narrowing — are data in the adapter section, committed by `adapter_root`. The submitter calibrates them
  however it likes. No calibration set is pinned, because admission's range analysis already takes
  params at their dtype's full range.
- **The rule for clipping.** An adapter that moves activations past the parent's calibrated headroom
  (about 2×) **clips** at the parent's narrowing sites. That clip is the candidate's risk. It shows in the
  candidate's evaluation score, and the protocol does nothing else about it.
- **Recalibration makes a full-weight candidate.** Changing any parent-side narrowing param breaks
  byte-identical reuse. A candidate that needs recalibrated scales is therefore submitted as full weights
  (a `Single` artifact), if and only if the policy allows full-weight candidates, and it pays their
  prefetch (§6.7).

##### 6.5 The node budget, and what an adapter may target

- An adapted projection adds about 20 nodes, and a block holds at most 512 (04b NF-12).
- The largest blocks of the corpus, MoE and gated-delta hybrids, already have 468 nodes. `all-linear`
  does not fit there; about two adapted projections do.
- **The rule.** An adapter may target any set of projections that keeps every block within 512 nodes.
  Admission refuses anything else by name (`NormalForm`), and `palw-class check-architecture` reports
  per block how many projections fit.
- Lines whose blocks are near the limit take full-weight candidates for broad changes, or narrow
  adapters (for example `q` and `v` only), until a leaner adapter form exists (open question 15).

##### 6.6 Adapter forms out of v1

The lowering refuses these PEFT forms by name, so v1 tools cannot produce them as composite candidates:

- DoRA;
- trained biases (`bias ≠ none`, `lora_bias`);
- `modules_to_save`;
- `layers_to_transform` and `layers_pattern`;
- targets on fused projections (`qkv_proj`, `gate_up_proj`, `c_attn`);
- `fan_in_fan_out`;
- rsLoRA at a non-square rank;
- module-specific rank or alpha patterns.

Consensus does not parse adapter configurations. It checks the composite rule (§6.3) and admission. A
model trained with one of these forms therefore enters v1 only merged, as a full-weight candidate. Any
of them may later become a specified adapter form.

##### 6.7 Prefetch and work

- **Prefetch.** A seat holding the parent fetches only the adapter section. The lowering stores `A'` at
  `i32` and `B` at `i16`, per row: `r · (4 · d_in + 2 · d_out)` bytes per adapted projection. At `r = 16`
  on the seven projections of a Qwen2.5-1.5B-shaped decoder (28 layers), that is about 53 MB, roughly 3 %
  of the parent's 1.6 GB IR artifact. A full-weight candidate costs the full 1.6 GB per seat.
- **Work.** The adapter adds `r · (d_in + d_out)` MACs per adapted projection and position: about 2 % of a
  1,536-wide model at `r = 16` (hf-coverage §12).

#### 7. Evaluation

##### 7.1 Items: hidden, and in the future

- **A temporal hold-out.** Items come from hard cases admitted in `HoldOut`, after the candidate set froze,
  so no candidate can have trained on them.
- **The draw.** At `Drawn`, R (RFC-0003, domain `CLASS_UNIFORM_V1`), keyed by the epoch seed — the
  beacon at the first block `d` DAA past `t_draw` — draws `n` items from the eligible pool.
- **Bonded setter sets.** Stewards commit private items (prompt and key commitments) before `t_close`.
  Prompts are disclosed at `Drawn`, when every candidate is already fixed. The references of
  teacher-forced items are disclosed at `Drawn` too, because a teacher-forced job reads its reference as
  prefill. Keys of generated items are disclosed only after every subject's outputs are final. A steward
  who does not reveal forfeits its bond, and its items drop out for every subject alike.
- **Caps.** No submitter or steward supplies more than a fraction `κ` of an epoch's items (§15).

##### 7.2 The evaluation job

```
PalwEvalJobV1 { line, epoch, item, subject: Parent | Candidate(class_id), mode, pipeline }
  mode     = Generate { seed, max_new, stop_ids }           // RFC-0001 §A's rule, R domain 0 under `seed`
           | TeacherForced { reference }                     // prompt ‖ reference as prefill, logits over the reference
  seed     = H64(key "misaka-palw/improve/eval-seed/v1", epoch_seed ‖ item)    // the same for every subject
  pipeline = the policy's scoring pipeline for the item's domain
```

- **Everything is derived.** The chain derives each job's context from the epoch, the item and the
  subject, as Phase F derives an IR attempt's context (`palw_tir_attempt_v1`): the yardstick layout of
  the subject's class, `job_id = H(epoch ‖ item ‖ subject)`, and the item's prompt commitment.
  Executors choose nothing.
- **The pipeline** is an RFC-0003 pipeline: a subject stage (the subject class, in `Generate` or
  `TeacherForced` mode) followed by TIR scoring stages. It commits one step tree (PALW-GEN-3) and a
  `score_root` over the committed score tensor (RFC-0003's output digest).
- **Three additions to RFC-0003's pipeline format**, and nothing else (landed as A7, `rfc4/eval`):
  - a `Decode` stage kind — the text profile inside a pipeline, with FP V4's commitments and Phase F's
    `TirDecodeToken*` arms;
  - a `FinalizedOutput { claim, stage }` binding (token source 5) — an input bound to a final claim's
    committed output in the same epoch, opened under its output root, so a pairwise stage can read the
    parent's and the candidate's generations without running them again;
  - a `Key` token source (token source 4) — an exact-match key read as an input of the scoring stage,
    bound to the item's committed key.
- **Adjudication** is Phase F's arms and RFC-0003's pipeline rules, unchanged: `TirCone`, H dissection,
  `TirLogits`, `TirDecodeToken*`, `TirOutputDigestMismatch`.
- **Claiming.** Evaluation jobs are claimed openly: the first valid claim per job in the accepting
  chain's order is the one, it is paid the job's evaluation fee at `Final`, and it counts against the
  executor's claim capacity (§13).
- **Missing evaluations count for the incumbent** (amended 2026-09-29). An item missing a candidate's
  evaluation is a loss for that candidate; an item missing the parent's evaluation is a win for the
  parent against every candidate. Missing data can block a promotion and can never make one.
- **Not the reward path.** Evaluation claims earn their fee and no quantum, ticket or eligibility.

##### 7.3 Scoring stages

Each is a TIR program in the scoring library (`misaka-palw-tir`, with golden vectors), run as a stage:

| Kind | What it computes | Cost | Label | Weight in promotion |
| --- | --- | --- | --- | --- |
| **ExactMatch** | the answer span between the item's delimiter ids in the generated canonical output (RFC-0003: generated token ids) equals the key's ids: `Compare`, masks from `Iota`, `Gather`, `ReduceSum` | one R-seeded generation | V (key: B) | primary |
| **RefLogLik** | `Σ` over the reference of the log-probability of each reference token under the subject: the library's wide softmax, `IntLn`, `Gather`, an exact `ReduceSum` (Q24) | one teacher-forced forward pass: prefill only, cheap, exact | V (reference: B) | primary for likelihood tasks |
| **Judge** | a registered judge (reward-model) class scores the subject's output; the judge is drawn by R at `Drawn` from the policy's judge set, and anchored (below) | one judge pass | J | low: a guard, never sufficient |
| **Pairwise** | a judge compares the parent's and the candidate's generations (read by `FinalizedOutput`), in an order drawn by R | one judge pass per pair | J | low: a guard |

- **Anchors.** A judge's jobs include anchor items whose exact answers are known. A judge whose anchor
  accuracy falls below the policy's floor is excluded for the epoch, and its scores are void.
- **Same seed, same item, every subject.** Pairing is exact: the parent and every candidate see the same
  prompt and the same randomness.

##### 7.4 Code in Phase A

Arbitrary code-by-tests execution is **outside this profile**. No future VM implementation is
promised. Code tasks currently use:

- **exact output** — the value a program prints, the result of an expression, a canonical single-line
  fix; or
- **likelihood** — the teacher-forced log-likelihood of a known-good reference solution or patch.

Both are weaker than tests. The policy says so, and the label (V given a B key) says what they rest on.

##### 7.5 Promotion: a paired rule in integers

Each item gives, for candidate `C` and the parent `H`, a win, a loss or a tie. ExactMatch items compare
pass and fail; RefLogLik items compare the two log-likelihoods, by sign. Let `b` be `C`'s wins and `c`
its losses over `n` counted items. `C` is **eligible** iff all of the following hold:

1. **Enough evidence**: `n ≥ n_min`.
2. **It beats the head by δ**: `b − c ≥ δ · n`.
3. **Its lower confidence bound is above zero**: an exact one-sided sign test on the `b + c` decisive
   items rejects "no better" at level `α / K`, with `K` the number of candidates (a Bonferroni split).
   That is `b ≥ k(b + c, α / K)`, where `k` comes from a **pinned integer table** of binomial critical
   values. No float is involved.
4. **No regression beyond ε** on the policy's regression suite, by the same paired counts.
5. **No regression beyond ε_s** on the policy's safety suite.
6. **The judge guards hold**: the Pairwise and Judge outcomes show no significant loss (the same sign test,
   in the other direction). A judge can block a promotion, but can never make one.

The winner is the eligible candidate with the largest `b − c`, ties going to the earliest submission. It
becomes the line's current version, `LineageHeadSet`, applied by the fold at `t_score`. **If none is
eligible, the head stays.**

##### 7.6 Rollback

- **By the owner**, within `rollback_epochs` of a promotion, without proof — the owner answers for the
  product.
- **By anyone, with proof**: a final claim of the promoted class failing a committed canary or safety
  item it was bound to pass, or a regression shown on items drawn in a later epoch, or a licence violation
  upheld on a bonded challenge.
- **Its effect**: `LineageHeadRolledBack` makes the previous version current again (the version history
  of spec 15 keeps every row), forfeits the winner's unvested rewards and bond (§8.4), and bars its
  submitter's candidates for the policy's period.

#### 8. Rewards, staged

Every reward names what it rests on (§2.2). Every payout comes from the line's pool (§8.5). None of them
creates a quantum, a ticket or eligibility (ADR-0144 P1).

##### 8.1 S1 — bounties, only for EXACT-verifiable tasks

- **A bounty per exact-match hard case** (V): paid to the first `Answer` whose answer span matches the
  key, in commit order, once the key is revealed.
- **Setter rewards** (V for hardness, B for the key): for a `SyntheticProblem` or `HardCaseVariant`
  that the head verifiably fails.
- **Forfeits** (V): spam and duplicates forfeit their bonds to the pool.
- **Nothing else pays in S1.** Likelihood, judged and human outcomes do not earn bounties.

##### 8.2 S2 — the gain share, through declared, registered datasets

- At a promotion, a policy share of the winner's epoch reward goes to its submitter (the trainer), and
  the rest to the data.
- The data share goes to the **registered datasets** that the winner's `CandidateSubmitted` declared, in
  proportion to their declared weights, with a cap per dataset and per contributor.
- **The trust assumption is stated** in the policy, in the object and in every interface: the chain
  cannot see what a trainer used (T). A trainer can name datasets it never used — the caps bound that —
  or omit ones it did, which costs only the omitted.
- ADR-0088 D7–D8 already pays an adopted proposal's contributor a permille of the owner's leg while its
  version is current. S2 is the protocol's form of the same idea for governed lines.

##### 8.3 S3 — marginal attribution ("Proof of Useful Data"), as research

- **Ablation contests.** For a dataset slice `X`, independent trainers train with and without `X`; each arm
  is replicated by **at least two independent trainers**; the arms are evaluated as candidates are (§7).
  The marginal gain of `X` is the paired difference on the same hidden, future items — for example slice
  A +4.1 points, B +0.2, C −1.0.
- **Payout** follows the gain's lower confidence bound where it is above zero: A is paid, B is inside the
  noise, C is not paid.
- **Teacher reputation** — Alpha, Beta, Gamma — comes from observable records: adoption (the registered
  datasets that winners declared) and downstream gain (S3). It sets bond sizes and caps. It never
  multiplies a payout.
- **This is research.** Replication and bonds are the only guards against a trainer who shapes the arms,
  and each contest costs several training runs per slice. S3 stays on testnets within this RFC.

##### 8.4 Vesting and forfeit

A winner's S2 reward and its candidate bond **vest over `vest_epochs`**. They are forfeited if a backdoor,
a canary failure or a licence violation is proven before they vest (§7.6). A model cannot be un-trained;
money not yet paid out is the only lever left.

##### 8.5 The pool

- **Funding:**
  - a parameter share `φ` of the line's fee income — the owner's market leg (spec 15 PALW-MK-7), split
    the way `split_owner_leg_v1` already splits it with an adopted contributor;
  - candidate registration fees;
  - sponsor deposits, bound to the line's pool as a model-sink output is bound to a market object
    (PALW-MK-11).
- **Not PALW worker rewards.** They are the reward path (ADR-0144 P4–P7), and the market's buyback slice
  (PALW-MK-8) already takes its share of them.
- **Nothing is minted.** Evaluation fees are escrowed per job and paid to the executor, or refunded if the
  job is never executed.
- **Payouts** leave the fold as outputs, by the path market settlements already use.

#### 9. Licence and legal

- **Closed APIs.** API terms commonly forbid using outputs to train competing models. Outputs of
  proprietary models — Claude-, GPT- and Gemini-type agents — are admissible as training data **only
  under `LICENSED_DISTILL`**, backed by a registered `TeacherLicence` from the rights holder:

  ```
  TeacherLicenceV1 { rights_holder_key, model_family, scope: { domains, uses }, per_use_fee, expiry, revocation }
  ```

  This is also the long-term market: model companies register their teacher and earn a teacher fee per
  admitted use.
- **Tools and agent operators.** Such a model may help produce an artifact that is verified another way
  (an answer that matches a key), and that artifact may enter training data, **only if its licence allows
  it**. The default is no.
- **The base model.** A line may be governed only if its base model's licence permits derivatives. The
  owner declares it in the policy, under a bond. Research-only and no-derivatives licences cannot enter.
- **Attestations** of licence and provenance are bonded and challengeable. A proven false attestation
  forfeits the bond and excludes the contributor's datasets from future declarations.
- **Limits.** The chain cannot adjudicate law. A bond prices dishonesty; it does not make an unlicensed use
  lawful. This RFC is not legal advice.

#### 10. Privacy

- A hard case taken from real usage needs the job's **explicit data-use opt-in**, which is **off by
  default**.
- The opt-in is a separate object, `DataUseOptIn { job_pin }`, signed by the job's committer. The FP job
  format does not change (RFC-0001 §A is frozen).
- The chain checks that the case's prompt matches the job's committed `prompt_token_ids_hash`. Opting in
  publishes the prompt ids and the head's output with the case; redacting first is the user's
  responsibility.
- `PanelDa` jobs stay private unless their user opts in. Nobody can opt in another person's job.

#### 11. Reinforcement learning

- **RL with verifiable rewards fits.** ExactMatch and RefLogLik are public reward functions expressible
  in TIR. Trainers run RL against them off chain; judged rewards are available too, at J.
- **Rollouts are PALW jobs.** The head's rollouts on hard cases run as PALW jobs, and their scores as
  scoring stages. Submitted as `SELF_PLAY` `RewardSignal` artifacts, they are verified reward data (V) that
  any trainer can buy or use without trusting whoever generated them.
- **The chain judges the outcome, not the update.** Only the next candidate's measured result is paid.

#### 12. Training as IR: possible, and not in v1

- **In theory, training is expressible.** A compiler can lower a forward pass, its backward pass and an
  optimizer step to the 25 primitives: every derivative is itself a composition of `MatMul`, elementwise
  operations, tables and reductions. No autodiff primitive is needed, because differentiation happens in
  the compiler, not in the IR.
- **v1 does not put training into consensus.** A consensus training method would freeze today's research
  methods into protocol, and it would make every candidate pay for a replayable training run that nobody
  needs in order to check the candidate.
- **A future optional profile.** A line MAY, in a later version, accept candidates that carry a
  replayable IR training run. Whether the primitives suffice is open question 9: optimizer state (moments
  as `Fixed` states, or weights as states), gradient accumulation over a batch (exact sums, but
  over-sized reductions), sampling and shuffling (R), batching (tensor axes), and the fidelity of integer
  training itself.

#### 13. Capacity and cost

- **Evaluation claims are claims.** Each reserves capacity by ADR-0160's stage-1 rule (`⌈w_c / ρ⌉`)
  like any other claim. Producers pay execution/proof cost; seats pay the bound kernel's check/DA cost,
  not mandatory full segment replay on the probabilistic route. An epoch's evaluation budget —
  `n × (k_max + 1)` subject jobs plus judge jobs, times their positions — is capped per epoch by the
  policy and by a network ceiling expressed as a share of the span's claim capacity, so evaluation cannot
  crowd out attempts.
- **Candidates pay.** A registration fee (plus the IR class's own 1 BILI burn), and evaluation fees for
  their own `n` subject jobs and pairwise jobs.
- **Candidates per epoch** are capped by `k_max` and by the network's ceiling.
- **Prefetch (legacy replay baseline).** Under the legacy full-replay profile every seat must hold the subject's artifact. Adapters
  cost about 3 % of the parent (§6.7), so four adapter candidates add about 210 MB to a 1.5B line; a
  full-weight candidate adds 1.6 GB per seat, and is priced for it.
- **A worked size.** A Qwen2.5-1.5B-shaped line, `n = 400`, four candidates plus the parent, 256 prompt
  and 256 generated tokens per item: 400 × 5 × 512 ≈ 1.0 × 10^6 positions. At the 17 tokens per second
  that Phase F's typed backend measured for a 1.5B decoder (2026-09-28), that is about 17 hours of one
  node, spread across many executors, before replicas. Teacher-forced items cost only a prefill. The
  pairwise jobs add one judge pass per item and candidate. This is execution sizing, not a verifier
  cost estimate for encoded-constraint checks. Measure proof generation, cold preprocessing, query
  openings, material availability and worst disputes separately; a new kernel must specify which
  weights/openings a seat needs rather than assume all weights fit in its RAM.

#### 14. Kernel extensions and off-chain automation

The [Kernel design §§K.0–K.8](../design/palw/models/versioned-kernels.md) replaces the withdrawn VM roadmap. A new kernel may add a
reusable computational or checker family after soundness/resource/court review and coordinated
activation. A frontend plus a declarative plan suffices only within active semantics. New kernel
support cannot bypass this RFC's family, provenance, evaluation, fees or promotion rules.

Research/training loops may run in any off-chain software and submit ordinary artifacts. The protocol
never requires evidence that a candidate was produced by a particular tool. No new VM, EVM fallback
or arbitrary EXEC/CRITIC/PROOF checker is implicitly authorized. A separately specified bounded
checker relation is possible, but unimplemented types remain unsupported and cannot earn as verified.

#### 15. Threats

| Threat | Mechanism | Mitigation | What remains |
| --- | --- | --- | --- |
| Poisoning and backdoors | a trainer or contributor plants a trigger in a winner | evaluation cannot see a trigger it does not contain; canary and safety items, vesting and forfeit, rollback | **not closed**: a well-hidden trigger passes evaluation, and only a later proof and rollback reach it |
| Contamination | evaluation items leak into training | the temporal hold-out, setter prompts disclosed only after the freeze, the R draw | a leak through a setter (below) |
| Answer-key gaming | a setter's keys favour one candidate's quirks | per-setter caps `κ`, independent stewards, per-setter anomaly checks, bonded key challenges | within one cap |
| Likelihood gaming | a candidate memorises reference-like text | references disclosed only after the freeze; the hold-out; likelihood paired with exact-match tasks | on-distribution memorisation is also learning |
| Judge gaming | outputs tuned to a known judge | the judge drawn by R at evaluation time, anchors, a guard-only weight | inherent to judges |
| Sybils and mempool copying | another's artifact or case resubmitted | commit–reveal, dedupe by hash and normal form, bonds | near-duplicates |
| Licence laundering | unlicensed teacher output declared OPEN or HUMAN | bonded attestations, challenges, exclusion, `TeacherLicence` | laundering nobody detects |
| Trainer–contributor collusion | a declaration pays friends | registered datasets only, per-dataset and per-contributor caps, S2 labelled T | whatever fits under the caps |
| Setter leakage | a setter hands future items to a trainer | many setters, caps `κ`, private sets from independent stewards | within one cap |
| Beacon grinding | a producer grinds the draw | a beacon that postdates the hold-out, a large `n`, the private sets | small |
| Cost DoS | spam candidates, cases or artifacts | fees, bonds, `k_max`, the per-epoch evaluation budget | — |
| Capacity crowd-out | evaluation starves attempts | the evaluation budget capped as a share of claim capacity | — |
| Evaluation starvation | nobody claims a candidate's jobs | fees make claiming pay; an unclaimed parent job voids the item for everyone | a candidate nobody will run fails |
| Head capture | a bad policy or promotion | the owner's policy, the owner's rollback window, proof-based rollback | trust in the owner's choice of policy |

#### 16. How this differs from nearby designs

| Design, as publicly described | What is paid for | Who decides the reward | This protocol's difference |
| --- | --- | --- | --- |
| Bittensor | a subnet miner's output, as validators weight it | validators' subjective weights, aggregated by consensus | promotion and S1 rest on outcomes the chain computes; judges are guards only |
| Compute-rental designs (the brief's "FLOP") | compute or time delivered | the buyer | nothing is paid for compute: only verified artifacts and measured gains ("agent mining, not agent rental") |
| Virtuals | tokenised agents valued by market demand | the market | a head changes only by a statistical win on hidden, future items; the line's market (spec 15) is kept apart from the evidence |

The claim is narrow: verification first, and wherever the protocol cannot verify (training, provenance,
backdoors) it says so and prices the risk with bonds, vesting and rollback.

#### 17. Implementation plan and effort

The table preserves the original Phase F / `tir/phase-f-f7` planning baseline, in engineer-weeks for
two to four experienced people. Its legacy Phase A dependencies were RFC02 Phase F and RFC03 steps
3–5 (`TirProgramV2`, pipelines, their court). The new kernel-bound route additionally requires §0's
Kernel / RFC07 / RFC11 suites, assurance/identity changes and independent review; re-estimate those costs before
scheduling. There is no EVM-rung or VM implementation dependency.

| # | Work item | Where | Depends on | Engineer-weeks | With agents |
| --- | --- | --- | --- | --- | --- |
| A1 | Spec chapter 17; the vector plan (scoring programs, the sign-test table, epoch transitions) | `docs/spec/palw/17-model-improvement.md` | — | 3–4 | 1–2 days |
| A2 | The line's improvement policy; the head as a class id in the improvement row (§3, amended); protocol promotion; head history and rollback; the developer's promotion refused on a governed line | `palw_state_v2.rs` (`improvement_lines`), spec 15's `ModelVersionPromoted` arm | Phase F | 4–5 | 2–3 days |
| A3 | The epoch state machine: `improvement_lines` and `improvement_epochs`, DAA-driven transitions, the usage trigger (the head class's claims at `Final`), `dataset_root`, the freeze, the draw (R and the beacon) | `palw_state_v2.rs` (fold, `state_root`), a new `palw_improve_epoch_v1.rs` | A2 | 6–8 | 3–4 days |
| A4 | Hard cases, setter sets, registered datasets, teaching artifacts, the data-use opt-in, `TeacherLicence` | appended `PalwConsensusObjectV2` variants; `palw_prompt_ids_v1.rs` for the prompt-hash check | A3 | 4–6 | 2–3 days |
| A5 | Candidates and composite artifacts: `CandidateSubmitted`, the family rule, `PalwTirArtifactRefV1`, sub-root openings | `palw_tir_class_v1.rs`, `palw_tir_artifact_v1.rs`, `palw_tir_admission_v1.rs`, `palw_artifact.rs` | Phase F | 4–6 | 2–3 days |
| A6 | The evaluation job family: derived contexts (as `palw_tir_attempt_v1`), job identity, open claiming, fees, capacity reservation, panels | `palw_tir_attempt_v1.rs`, `palw_job_identity.rs`, `palw_job_state.rs`, `palw_capacity_formulas_v1.rs` | A3; RFC-0003 steps 3–5 | 6–8 | 3–4 days |
| A7 | The scoring library (ExactMatch, RefLogLik, Judge, Pairwise) with golden vectors; the `Decode` stage and the `FinalizedOutput` binding; court coverage | `misaka-palw-tir` library, the RFC-0003 pipeline code, `palw_tir_court_v1.rs` | RFC-0003 steps 3–5 | 5–7 | 3–4 days |
| A8 | Promotion: the pinned binomial table, paired counts, `δ`, `ε`, the suites, the judge guards, the winner, `NoChange` | a new `palw_improve_promotion_v1.rs` | A3, A6 | 2–3 | 1–2 days |
| A9 | Rewards and the pool: `φ` (the `split_owner_leg_v1` precedent), fees, deposits, S1, S2 caps, vesting, forfeit, payouts | `palw_model_market_v1.rs`, `palw_model_lines_v1.rs` | A3, A8 | 5–7 | 2–4 days |
| A10 | Node side: the epoch watcher, the evaluation executor (generation, teacher-forced, scoring), adapter prefetch, `palw-class improve` for cases, datasets and candidates | `misaka-palw-sdk`, `misaka-palw-tir-exec`, `kaspad/src/palw_panel.rs` | A5–A7 | 5–7 | 3–4 days |
| A11 | Adapter lowering: **landed** on `tir/lower` (a4bd7141f, hf-coverage §12). What remains is emitting the composite layout (`P`, the adapter-section inventory) and per-block node-budget reports in `check-architecture` | `misaka-palw-tir-lower`, `misaka-palw-sdk` | Phase F | 1–2 | about 1 day |
| A12 | An independent second implementation of the scoring library, the promotion rule and the transitions; differential tests; a model-checked state machine | a ref2-style lane | A3, A7, A8 | 4–6 | 2–3 days |
| A13 | Drills: D-M1 a whole epoch on a salted t12 chain with a small real model and an adapter that wins; D-M2 one that must not; D-M3 a court battery on evaluation claims; D-M4 rollback by the owner and by proof; D-M5 the fence crossing on the shipping binary; D-M6 copying, setter leakage, grinding, fee DoS | scripts, the drill harness | all | 5–7 | 3–5 days |
| A14 | Two external audits (the state machine and the court; the economics) and their fixes | — | A1–A13 | 3–4 | — |
| A15 | Testnet soak (at least 3 epochs), bounty, activation | — | A14 | 3–4 | — |
| | **Total** | | | **60–84** (≈ 14–20 engineer-months) | implementation ≈ 3–5 weeks |

- **Implementation with AI agents**: about **3–5 calendar weeks** for A1–A13, to a whole epoch passing
  D-M1…D-M6 on a salted testnet-12 chain, with two or three agents and a lead who reviews and
  integrates.
- **Mainnet-safe**: about **8–11 months** from the start. That covers the second implementation's triage
  and the differential tests (1–2 months), two audits with fixes (3–4 months, partly in parallel), at
  least 3 epochs armed on a testnet (2–3 months at the epoch lengths a testnet uses), a bounty window
  overlapping them, and the activation. It cannot come before PALW-TIR and RFC-0003's pipelines are
  themselves mainnet-safe. These are Phase A estimates; the withdrawn VM effort estimate is not a scheduling basis.

#### Proposed Spec text (new chapter `spec/palw/17-model-improvement.md`)

Applies past `palw_improvement_v1`.

- **PALW-MIP-1 (the method is free).** A model candidate MUST NOT be required to have been produced by the
  VM, or by any particular method, tool or party.
- **PALW-MIP-2 (four checks).** The protocol MUST verify a candidate's IR class admission and its family
  relation to the parent, its declarations against the line's provenance policy (form and references),
  its evaluation results, and its promotion eligibility. It MUST NOT condition anything else on how a
  candidate was produced.
- **PALW-MIP-3 (governed lines).** A line is governed only after its owner signs an improvement policy.
  A policy MUST change only between epochs, and opting out MUST take effect only after the current
  epoch and a delay.
- **PALW-MIP-4 (the head).** A governed line's head MUST be an IR class id, kept with its history in the
  line's improvement row, and it MUST move only by this chapter's promotion or rollback. The protocol
  MUST NOT write spec 15's version rows or `current`.
- **PALW-MIP-5 (epochs).** A governed line MUST run at most one epoch at a time, advanced by the
  transitions of §4 at the DAA boundaries its policy fixes.
- **PALW-MIP-6 (opening).** An epoch MUST open at the first grid boundary at which the head's usage since
  the last epoch reaches the policy's threshold.
- **PALW-MIP-7 (the dataset root).** At `t_fix` the epoch's `dataset_root` MUST be fixed over the material
  admitted while `Open`.
- **PALW-MIP-8 (candidates).** Candidates MUST be submitted in `[t_fix, t_close)`, at most `k_max`, each an
  admitted IR class of the parent's family, with its fees and bond. No candidate MAY be withdrawn after
  `t_close`.
- **PALW-MIP-9 (the hold-out).** Evaluation items MUST be drawn by R under the epoch seed from the cases
  admitted in `HoldOut` and from committed setter sets. Setter prompts and teacher-forced references MUST
  be disclosed only after `t_close`, and the keys of generated items only after every subject's outputs
  are final. An item missing a subject's evaluation MUST count for the parent.
- **PALW-MIP-10 (evaluation jobs).** Every item MUST be evaluated for the parent and every candidate by
  evaluation jobs whose contexts the chain derives, with one generation seed per item shared by every
  subject.
- **PALW-MIP-11 (scores).** A score MUST be the committed output of the policy's scoring pipeline, and it
  is adjudicable as any committed output.
- **PALW-MIP-12 (judges).** A judge MUST be drawn by R at `Drawn` from the policy's registered judge set.
  A judge failing its anchors MUST be excluded for the epoch. A judge's score MAY block a promotion and
  MUST NOT make one.
- **PALW-MIP-13 (promotion).** The fold MUST promote the eligible candidate with the largest `b − c` under
  §7.5's rule, with the pinned table, and otherwise leave the head unchanged.
- **PALW-MIP-14 (rollback).** The owner MAY roll a promotion back within `rollback_epochs`. Anyone MAY
  roll it back with a proof of §7.6's kinds.
- **PALW-MIP-15 (composite artifacts).** A composite candidate's first `P` params MUST be the parent
  program's params, in order, with the parent's declarations, served under the parent's artifact root.
  Its artifact root MUST bind the parent class, the parent's artifact root, the adapter section's root
  and `P`, as §6.3 states. A candidate that changes any parent-side param, its scales included, MUST be a
  full-weight candidate (§6.4). Every block MUST stay within the 512-node limit (§6.5).
- **PALW-MIP-16 (rewards).** Payouts MUST come only from the line's pool. S1 MUST pay only EXACT-verified
  artifacts. S2 MUST pay only registered datasets that the winner declared, within their caps. Trainer
  rewards MUST vest over `vest_epochs` and MUST be forfeited on a proof of §7.6's kinds.
- **PALW-MIP-17 (not a reward path).** Nothing in this chapter MAY create a quantum, a ticket or
  eligibility, or draw on PALW worker rewards.
- **PALW-MIP-18 (licences).** A declaration of `LICENSED_DISTILL` MUST cite a registered, unexpired
  `TeacherLicence` that covers the use. A governed line's base model licence MUST permit derivatives, as
  its owner declares under a bond.
- **PALW-MIP-19 (privacy).** A hard case MAY use a job's prompt only under that job's `DataUseOptIn`, and
  its prompt MUST match the job's committed prompt hash.
- **PALW-MIP-20 (capacity).** Evaluation claims MUST reserve capacity as claims do (ADR-0160), and an
  epoch's evaluation budget MUST NOT exceed the policy's cap or the network's ceiling.

#### Activation plan

One fence, with Phase F D1's shape:

```
palw_improvement_v1: Option<PalwImprovementFenceV1 { activation, scoring_set_id, sign_table_id,
                                                     court_version, ceilings }>
```

It is Some-only in both fingerprints, collapses to `never()` as a whole option, is visited
activation-only by `for_each_fence`, and sits at an unused height. `scoring_set_id` hashes the scoring
library, and `sign_table_id` the pinned binomial table. `ceilings` bounds `k_max`, `n`, the positions
per epoch and the evaluation budget's share of claim capacity. `validate_palw_v2` refuses to arm it
without `palw_tir_v1`, RFC-0003's `palw_gen_v1` and `palw_kary_court` at or below it, or on a ruleset
without the A-2 tolerance.

| Step | Work | Exit gate |
| --- | --- | --- |
| 1 | A1–A12 (§17) | vectors pass on two implementations; the state machine is model-checked |
| 2 | Drills D-M1…D-M6 on a salted t12 chain, on the shipping binary | all pass |
| 3 | testnet-12 (or its successor), at an unused height after RFC-0003's fence; a first governed line, adapters only | at least 3 epochs with no unresolved defect; audits closed |
| 4 | Full-weight candidates and S2 | a further epoch |
| 5 | Mainnet | after the audits, the soak and the bounty; S3 stays on testnets |

#### Alternatives

| Alternative | Why not |
| --- | --- |
| **The market in EVM contracts, with consensus reduced to a read path and a head field** (an earlier draft) | The user chose consensus-native epochs. Contracts remain an optional add-on (§4.2) |
| **Developer promotion only** (spec 15 today) | Declarations, not verification. Kept for lines that do not opt in |
| **Declared evaluations** (`ModelEvaluationPosted`) | Declarations again. Kept as records |
| **Training inside consensus** | It would freeze research methods and charge every candidate for a replay nobody needs (§12). A later optional profile at most |
| **Proof of training** | Float, nondeterministic and huge; nothing practical proves a training run today |
| **Judges as the primary reward** | Gameable, so judges only guard |
| **Paying for compute or agent time** | It pays for effort, not results ("agent mining, not agent rental") |
| **Validators' subjective weights** | Opinions can be bought; scores computed by the chain cannot |
| **Requiring candidates to come from a VM program** | Forbidden by PALW-MIP-1: it would make research methods a consensus matter |

#### Security, principles and compatibility

- **Threats** are in §15. The one that remains open is a well-hidden backdoor, which evaluation cannot see.
- **PALW-PR-1 and P2.** No validity rule of a claim, a reward or a prompt reads what an output means here.
  Scores are outputs of registered scoring programs that the line's owner chose, and they decide only
  that line's head. Whether the user reads PALW-PR-1 as covering this too is open question 2; if so,
  promotion would have to move to the optional contracts.
- **P1, P3 and P4–P7.** Evaluation jobs and every payout are outside the reward path. Registration stays
  permissionless: any bond may submit a candidate. A line's improvement never changes another line's
  economics.
- **Compatibility.** Lines that do not opt in are untouched. The FP job format does not change. Every
  object is appended, dropped by name below the fence and skipped by older builds under A-2.

#### Open questions

1. **IR versions** — *resolved 2026-09-29*: the head is a class id in the governed line's improvement row,
   and spec 15's version rows are not written (§3). The first recommendation — a version naming a class
   id — broke three live readers of spec 15 (the roots in force that attempt admission accepts, the parent
   class's roots and share after a supersede, and free-prompt usage attribution).
2. **PALW-PR-1's scope**: is protocol promotion by registered scoring programs acceptable (recommended),
   or must promotion move to the optional contracts?
3. **The usage trigger**: claims or work leaves, and the threshold.
4. **Parameters per domain**: `δ`, `ε`, `ε_s`, `α`, `n`, `n_min`, `k_max`, `κ`, the windows, `φ`,
   `vest_epochs`, `rollback_epochs`.
5. **Continuous scores**: the sign of the paired difference (recommended), or a fixed-point paired LCB.
6. **Claiming evaluation jobs**: open claiming (recommended), or assignment by R as a seat duty.
7. **The evaluation budget**: its share of ADR-0160 capacity.
8. **The `Decode` stage, the `FinalizedOutput` binding and the `Key` source** — *resolved*: added to
   RFC-0003's pipeline format (A7, `rfc4/eval`).
9. **Training as IR**: do the 25 primitives suffice for a future training profile (optimizer state,
   gradient accumulation, reductions, sampling by R, batching)?
10. **Challenging a setter's key**: bonded human/judged procedures keep their trust labels; a new
    machine-checked relation needs a separately reviewed non-VM kernel extension, not a promised Phase C.
11. **Artifact content**: who must keep teaching artifacts and datasets available for audits and
    challenges, and whether that is bonded.
12. **The safety suite**: who writes it for a line, and whether it may veto a winner that passes
    everything else.
13. **Full-weight candidates**: allowed from the first epoch, or adapters only at first (recommended).
14. **Cross-line teachers**: may another line's head teach a governed line as `SELF_PLAY`, and under
    which licences?
15. **A leaner adapter form** for blocks near the 512-node limit (MoE and gated-delta hybrids at 468):
    fewer nodes per adapted projection, or adapters shared across projections.

#### Decision

<Open.> The drafter's recommendation:

- Accept Phase A as the first deliverable of the improvement roadmap, as the user ordered.
- Implement A1–A13 as soon as RFC-0003's steps 3–5 land. Arm on a testnet with adapter-only candidates
  and S1 first; add full-weight candidates and S2 after an epoch; keep S3 on testnets.
- Follow the [versioned kernel design](../design/palw/models/versioned-kernels.md) for new model semantics. Existing Phase A engineering estimates do not include the new
  probabilistic kernel's prover/checker work and must be re-estimated before scheduling that route.

#### Part II — Verifiable computation beyond fixed weight files (2026-10-08)

> **日本語要約:** MISAKA は「LLM の重みファイルを登録するチェーン」ではなく、「独立検証可能な有用 AI 計算を登録・実行・決済するチェーン」である。
> モデルを「認証された計算仕様 + 型付き state root の集合」に一般化する。
> 対象は、重みを使うモデル、推論中に記憶を更新するモデル、検索型、モデルとツールの複合である。
> 任意の外部 API や Python は認めない。報酬の条件は今後も G14(Panel 外の普通の検証者が公開材料だけで不正を立証できる)である。
> 本 Part は**「全 RFC の実装完了」の範囲に含む**(ユーザー決定 2026-10-08)。

##### II.1 Why

Model families will keep changing. The likely shapes are:

- weights plus external memory, retrieval and tools;
- models that update memory, or some parameters, during inference (Titans/MIRAS-style test-time memory);
- many small models plus external data, instead of one giant checkpoint.

Weight-free methods are not expected to replace general LLMs within 5–10 years. Even so, a protocol that equates "model" with "one fixed weight file" would freeze today's architecture into consensus.

This RFC already treats improvement as computation over PALW-TIR: lines, heads, candidates, evaluation. Part II extends the same idea from *improving a model between jobs* to *models whose computation reads and updates authenticated state other than fixed weights*. One invariant is unchanged: **an ordinary bonded verifier outside any Panel, from public authenticated material only, reaches an objective conviction or a correctly classified DA/default for every rewarded computation.**

##### II.2 The computation specification

A class binds a versioned **computation specification**: the kernel id and plan id (RFC-0005 / ADR-0172), plus a list of **typed roots**. Each root kind is versioned. A class whose kinds the active kernel does not implement is refused by name, never treated as another kind.

| Root kind | Bound at registration | Per job / per claim | What G14 requires |
| --- | --- | --- | --- |
| `Weights` | the artifact (weight) root, as today | — | as today: commitments, localisation, exact court |
| `Memory` | the initial memory root, an **update rule** expressible in the active kernel (TIR `StateWrite` / `HistAppend` / `Fixed` states, or a fenced kernel extension), and retention terms | the pre-state and post-state roots of every update step, committed; memory carried across jobs is a chain-tracked state root | a fault is localised to ONE update step and judged exactly from the committed pre-state, the inputs and the rule. The pre-state must be publicly available (DA) for the claim's liability horizon; withholding it is a default, never fraud |
| `Retrieval` | a reference-data **snapshot root** (a public, retained corpus), a deterministic index commitment, and a deterministic retrieval rule (integer scores with a pinned tie rule, as RFC-0002 FR-09's counting threshold) | the retrieved item ids plus their openings against the snapshot | the snapshot's availability (DA); a retrieval result judged per item: a wrong item, a missed better item, or a wrong opening |
| `Composite` | a pipeline of computation specifications. Tool stages are allowed only as verified kernels with their own specifications | per-stage commitments | per-stage localisation (RFC-0003 edge courts, extended); a tool stage is judged like any other stage |

Forbidden in every kind:
- arbitrary external APIs;
- unverified code (Python, remote services);
- non-deterministic retrieval;
- any state an outsider cannot obtain and re-derive.

A semantic the active kernel cannot express is a versioned kernel extension: a coordinated, fenced protocol change (RFC-0005 §K), never a per-class escape hatch.

##### II.3 Relation to the improvement protocol

- A `Memory` class's persistent state is the in-job analogue of a line head. A promoted memory state is a candidate in this RFC's sense: evaluated and promoted under §§4–8, with its root recorded on the line.
- Test-time parameter updates (a slice of weights updated per job) are a `Memory` root whose state is a parameter subset. The update rule must be a kernel-expressible optimiser step. Training-as-IR stays §12's optional profile; nothing here makes consensus train models.
- Retrieval corpora are teaching artifacts in §5's sense when a line uses them; their rights follow §9.

##### II.4 Required evidence (part of "every RFC implemented")

1. **The wire form.** Every class registration carries a computation specification with typed roots, behind a dormant fence. `Weights` alone reproduces today's classes byte for byte: a regression test shows existing class ids and roots are unchanged.
2. **A `Memory` class end to end on the real node.**
   - registration → jobs whose claims commit per-step pre/post roots → a lie in one update step, convicted by an outsider from public material;
   - a withheld pre-state classified as a default;
   - memory carried across two jobs.
3. **A `Retrieval` class end to end.** A snapshot root and a deterministic rule; a wrong item and a missed better item each convicted; a withheld snapshot slice classified as a default.
4. **A `Composite` class with one verified tool stage**, convicted at the stage that lied.
5. **Bounds.** Every kind passes RFC-0011's bounded-verification gates (validator work, public bytes per prosecution, carrier fit) and RFC-0015's window and economics rules before it is OPV-eligible.
6. **Census.** RFC-0011 §18's `D_complete` counts a repository that is a complete computation of a supported kind, even when it is not a single weight checkpoint.

Activation follows the single full-activation release. Nothing in Part II is armed on its own.

#### Shared verification-challenge boundary — 2026-10-08

For new Kernel/model conformance or probabilistic claim checks in this improvement workflow, bind and use only
[RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol).
The [Kernel design](../design/palw/models/versioned-kernels.md)'s approved checker/challenge/soundness tuple and RFC11's Static Admission → Beacon Conformance → Active
Eligibility apply to newly onboarded candidate profiles. Conformance sampling authorizes neither an unknown op nor
a Kernel activation, and does not replace this RFC's evaluation/promotion criteria or per-claim checks.

The earlier epoch hold-out/item/judge/generation-R draws are separate protocols and preserve their existing scope;
their beacon formula is not the new algebraic/conformance seed, nor evidence that its source is unbiased. Share
source facts only under independently reviewed bindings/timing and completely distinct domains. No new seed formula,
committee/DNS/BFT authority, runtime activation or successful candidate benchmark is supplied by this amendment.

#### Mission alignment amendment — 2026-10-07

candidate/evaluation/promotionの各claimにも、外部public bondによる証拠取得とobjective courtを適用する。admission/evaluationのjury多数決は計算の真偽を決めない。draw前のholdout secrecyは保てるが、評価claimの検査と裁定に必要な入力が公開に認証・取得できないまま新しい報酬を有効にしない。品質比較と計算の一致は別の主張である。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### 改定 — 2026-10-12（公開検証不能な経路の報酬除外：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 詳細は[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md) N4。

`DataUseOptIn`（既定は無効）の範囲で扱うユーザーデータを、公開検証のために強制公開しない。公開訴追に必要な証拠を確保できない経路は新規の報酬・weight対象から外し、既存の実装記録は削除せず将来の有効化対象を限定する。

---

<a id="source-v1-0005-L00001"></a>

### 旧RFC-0005 / 原文1–15行

出典: [0005-palw-ml-vm.md](../rfc/0005-palw-ml-vm.md)。配置理由: 主題別統合。

### RFC-0005: PALW VM（不採用）

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。

PALW-BVM・PALW-GVM は実装しない。ISA、syscall、guest メモリ、gas、VM court、toolchain を合意規則として維持すると、モデル対応に必要な実装・監査・履歴再生の負担が増えるため。
モデル拡張には [versioned Kernel と宣言的 plan](../design/palw/models/versioned-kernels.md) を使い、未対応の意味論は審査済みの Kernel 更新で追加する。

---

<a id="source-v1-0015-L00001"></a>

### 旧RFC-0015 / 原文1–379行

出典: [0015-panel-free-permissionless-verification.md](../rfc/0015-panel-free-permissionless-verification.md)。配置理由: 主題別統合。

### RFC-0015 — Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**トークンの表示名は Misaka、ticker は BILI。** `misaka` 系アドレスプレフィックスと既存のprotocol/CLI/API識別子は維持する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

> **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。


* **Status:** Revised Draft / deferred implementation design, 2026-10-10後続改定。2026-10-14: モデル配分を`α = 1.5`・Miner＋Verifier元本・30日平均で確定し、ブロック生成機会にも適用、マイナー報酬は登録者へ直接分配しない（§8.5.5）。モデル入手への合意不介入、固定identity、条件付きG14、拘束miner元本によるcoinbase配分と個別bond上限を採用する。旧Seeder/TRDC/FPR gateは撤回。配分式・経済評価・実装・activationは未完了。
* **Implementation status (2026-10-15):** **implemented, dormant, unverified** — kernel routeのOptimisticPublicVerification lifecycle・court・Final・producer reservation、ADR-0176 budgetとの結合、公開verifier、RPC表示、§13.3のactivation gate record（`consensus/core/src/palw_panel_free_gate_v1.rs`、pin一覧は空）が統合branch `integ/rfc02-rfc06-12-g14`にある。build/test/実node試験は未実施でPASSではない。Panel=0は2026-10-12改定（N5）どおり有効化しない。対応表: [rfc-0015-implementation-status-2026-10-15.md](../design/palw/integration/rfc-0015-implementation-status-2026-10-15.md)。上のStatus行は履歴として保持する。
* **Source baseline:** MISAKA-BTC/misakas public main `43f0bcb362d37cba79414940f3fdd368d40d3377`（2026-10-07確認）。
* **Hard prerequisite:** [RFC-0014][R14]のDispute Completenessとpublic non-seat prosecutionが、対象ネットワークで本RFCの全reward-bearing class/profileについて完全に成立していること。RFC14の文書作成・mergeだけでは満たさない。
* **Activation:** 前提達成後の別の協調upgrade。提案fence名 `palw_panel_free_v1` は設計用の未割当名で、既存Paramsのfieldではない。activation DAA、wire tag、署名domainはここでは割り当てない。
* **Decision:** 固定Panelの選出・必須receipt quorum・seat依存のlicensingを新経路の合意要件から外し、public authenticated material、permissionless verifier、objective court、producer担保、challenge windowで検証・処罰を行う。§8.3のclass非依存Block Cap / Reward Cap / Final Weight Capと共通DAA拘束を新modeの経済規則とする。
* **Not activated by:** RFC14/RFC15のmerge、seat/quorum定数の変更、テスト用小modelの成功、operatorのみのdrill、代役captureを使うcourtテスト、将来のcompact proofの提案。
* **Normative language:** MUST / MUST NOTは新mode有効化後の規則。以下のphase/mode/descriptor名は設計用で、現行Rust型の存在や有効化を意味しない。

#### 0. 日本語での決定と有効化禁止条件

**Panel=0は可能な設計である。ただし「検証をなくす」のではなく、
「固定Panelによる検証をなくして、permissionless verifier + objective fraud proofへ完全移行する」。**

Panel=0は、claimごとに事前選出したPanel seatを合意上要求しないという意味である。
full nodeの決定的validation、LLM計算の検査、DA、court、producer/public challengerのbond、
不正へのslashは残る。watchdogや監査サービスは任意のpublic参加者として継続できる。

**次の条件が完全に成立するまで、本RFCのPanel=0 modeを有効にしてはならない。**

> **Panel外の普通のpublic bondが、producerの秘密状態を使わず、
> public authenticated materialだけから不正をlocalizeして客観的convictionまで進める。**

「秘密状態」はproducerのheap、鍵、内部cache、debug endpoint、攻撃注入instanceや
genesis operator専用の非公開materialを含む。
producerが開示を拒否する場合は、正直なverifierが適格なon-chain要求とdeadlineから
客観的DA/default処理へ進める必要がある。materialがないのに計算不正のexact proofが
作れるとは主張しない。

前提の達成は§1のrelease gateで確認する。**現在のコードには未解決のgapがあるため、
この文書は現行MISAKAがPanel=0で安全に稼働できるという宣言ではない。**

#### 1. 必須のactivation gate: Public Fraud Prosecution Complete

##### 1.1 G14の内容

以下のG14は正しい登録modelを保有/取得できたverifierの条件付き検証能力である。
全保有者の配布拒否に対する取得保証は撤回する。claim固有の有限証拠開示とmodel取得は別scopeであり、
非公開modelの監視・経済安全性は別途未立証として評価する（ADR177 D2/D7）。

仮称 `G14_PUBLIC_FRAUD_PROSECUTION_COMPLETE` はrelease criterionであり、
単一の合意boolean、operatorの承認署名、Panel投票ではない。
次の全条件を満たす検証記録がMUST必要である。

1. **Ordinary public entry:** genesis operatorでも既存seatでもないpublic bondが、
   公開された同じ資格・maturity・担保・fee規則で参加できる。
2. **Fresh verifier:** claim公開後に起動した独立node/clientが、開始cutoffまでに
   class/plan/job/input/model materialを取得して検査できる。
3. **Authenticated material only:** 取得するbytesはclaim/program/artifact/rootへ束縛される。
   producerやoperatorの秘密状態・非公開API・liar instanceを使わない。
4. **Complete localization:** 対象planの故障を宣言されたbytes/work/rounds内で
   bounded exact terminalへ落とせる。whole dense captureや巨大モデル全体の緊急replayへ逃げない。
5. **Objective outcome:** 開示された不正にはexact conviction、未開示には適格なDA/default、
   honest claim/誤ったchallengeには棄却という、決定的な結果を得られる。
6. **Permissionless filing:** ExecutorRefuted、必要DA unit、interactive court、
   RFC14のnon-seat予約・共有progressを使える。別bondのcourt先取りで排除されない。
7. **Actual chain path:** RPC取得から署名・fee・mempool/carrier inclusion・state fold・
   conviction/slash・Final阻止まで、実node E2Eを完結する。
8. **Recovery and resources:** restart/IBD/reorg/重複proof/退出/担保保持/資源上限と
   worst-case deadlineを検証する。
9. **Post-commit challenge completeness:** [RFC07 Part VI](../rfc/0007-palw-verification-certificates-and-algebraic-checks.md#post-commit-challenge-protocol)のimmutable policyをbindし、statement/prover messageを各challenge前に固定する。sourceは独立に有効化・検証された将来のcanonical PALW workであり、fresh outsiderがqualification/Final/settlement、同じsource順序・seed・queries・各GKR roundを公開materialから再構成できる。grinding、abort/retry、withholding、reorg、資源・保持期限のboundsとexact escalationが全許可profileで成立する。
10. **Immutable identity / conditional prosecution / non-interference（2026-10-10後続改定）:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)とRFC14 §16を適用する。モデル入手・Seeder・Torrent/PoR/全量取得監査/lease/TRDC/FPRを合意条件にしない。claimは固定Model ID/root/specをbindし、正しいmodelを得た外部verifierがclaim固有証拠からlocalize/convictできることを試験する。モデル未取得だけでqualification/Final/weightを止めず、閉鎖時に実効検出pが0となる場合も経済評価へ含める。モデル全量をcourt要求で取得する経路を作らない。

この追加条件はG14の一部であり、別のPanel=0 activation shortcutではない。RegisteredDormant / ConformancePassedと
ActiveRewardableをRFC11 §17に従って分け、semantic/constraint/court/public-material/resource completenessをsamplingで代替しない。
source不足やtimeoutをheartbeat/BASE-0/EXEC hash、committee署名、local RNGで補わず、pending/既定deadlineとして扱う。
chain livenessは現行mainのまま進む。既存のsame-seed watcher非独立性、positive checks、DA/defaultとconvictionの区別を維持する。

このgateは本RFCで許可する**全reward-bearing class、kernel、layout、task/context profile**
に適用する。canonical 8k held/fusedの三gap [C12]、整合したgarbage trace、borrowed trace、
state/routing/checkpoint/output不正を含む。
小geometryやoperator-onlyの成功を、別の最大形状の成功として数えない。

future kernelを全て実装する義務ではないが、未対応profileをreward-bearingにしてはならない。
既存の広告・class envelopeをこっそり縮小してgateを通ったことにしない。
Candidate/研究用profileと、合意上報酬を得るprofileを明確に表示する。

##### 1.2 gate未達の取り扱い

G14が未達、記録欠落、前提profileで失敗、または資源/期限未確定なら:

* 本RFCのfenceはdormantのままにする。activation heightを設定しない。
* 現行のPanel/receipt/担保規則を維持する。
* Panel=0の「暫定有効化」「検証省略での試験運用」「一部operatorを信頼する例外」を設けない。
* failing classを含むmodeを、別のclassでの成功やcompact proof構想で有効化しない。

G14がPASSになっても、自動的にPanel=0にはしない。
Panelを外す新lifecycle、担保・reward/weight、DAとwatcherの実運用、codec/fingerprint、
移行・client表示という本RFC固有のgateも別に満たす必要がある。

#### 2. 検証する役割と安全性の前提

| 役割 | 新modeでの責任 |
| --- | --- |
| Producer | 有効classで計算し、固定Model ID/root/specをclaim/input/output/state/witnessへbindする。必要なclaim materialの開示・保持、担保/feeを負担する。モデルの常時Seeder義務はなく共有は任意 |
| Permissionless verifier / watchdog | 任意のpublic参加者として独立検査する。不一致をlocalizeし、必要materialを要求してproof/courtへ進む |
| Full node | object・binding・合意規則と受理された客観証拠を決定的に検証し、state・weight・報酬・slashを更新する |
| Optional off-chain model peer / claim-evidence provider | modelの保存・Torrent/mirror共有は任意で、chainのSeeder登録・lease・監査・default対象にしない。claim固有証拠を別に提供する場合は、その有限なscopeと規範的責任だけを扱う |
| Optional verification service | 監視・検査・material保持のサービスを提供する。契約や報告に固定Panelの合意権限を与えない |

「誰でも検査できる」と「実際に誰かが検査する」は異なる。
本RFCのoptimistic modeは、**各claimについて少なくとも1人のcapable honest verifierが
期限内に実際に検査・追及すること**を安全性の運用前提にする。

chainは形式上有効なrootや署名だけから、計算不正を自動的に察知するわけではない。
watcherが後から参加できること、bondが存在すること、bountyが高いことだけでは、
honestな監視の存在を数学的に保証しない。
モデル取得・検査の開始に間に合わない参加者は、そのclaimの安全性の前提に数えない。

Panel多数派の正直さを要求しない一方で、chain進行、bounded inclusion、
認証DA、commitment/署名の安全性、checkerのsoundness、資源・監視能力という仮定は残る。
full node全体が各巨大LLMをreplayする設計にはしない。

#### 3. なぜ現行のseats/quorumを0にするだけでは成立しないか

baselineの既存codeには以下の依存がある。

| 現行依存 | 新modeで必要な変更 |
| --- | --- |
| PALW_V2_PANEL_SEATS=5 / QUORUM=3 [C01] | 旧定数をゼロにせず、別のsettlement modeとversionを定義 |
| Provisional → PanelBound → ReceiptLicensed → Final [C02] | 固定Panelを経ないChallengeable経路を追加 |
| seat membership・receipt quorum検証 [C03] | 新modeでは必須licensing条件から外す。旧objectでは維持 |
| Final/settlementにbasis_k下限がある [C04] | signer数を根拠にしない新しいeligibilityと担保計算 |
| Valid signer lock、served mask、Panel recordによるescrow release [C05]–[C06] | producer中心のreservation/escrowと独立したDA/監査経済 |
| court開始がReceiptLicensedを要求する [C07] | 新Challengeable状態のchallenge surfaceとdeadlineを認める |
| conviction funnelが既存phaseを処理する [C08] | 新modeを明示的に扱い、旧claimの意味は変更しない |
| operator filerの計算proof不足 [C11] | RFC14の共通filerで完全に閉じてからmodeを有効化 |
| operator-anchorによるPanel binding [C13] | 新modeにはPanel bindingを要求しない。producer/public bondの公開資格は別に満たす |

空receiptを「全員Valid」、quorum=0を「検査済み」、空Panelのserved maskを
「DA義務充足」と扱う実装は禁止する。
`basis_k=1`を偽装したり、既存の`max(basis_k,2)`で署名者不在の経済を正当化したりしない。

#### 4. 選択する新modeとclaim lifecycle

##### 4.1 OptimisticPublicVerification

本RFCの基本modeは仮称 `OptimisticPublicVerification` とする。
class/claim identityはmodeとversionをbindし、network policyが対象classをadmitする。
producerがmodel metadataだけで自由に「検査を減らすmode」を選ぶことはできない。

~~~text
Producer: claim + public bond + authenticated commitments/material
    ↓ public admission + producer reservation + fee
Challengeable
    ↓
permissionless verification / fixed challenge window
    ├─ accepted exact fraud proof → Convicted → void + producer slash
    ├─ accepted interactive pursuit → Disputed
    │      ├─ exact guilty → Convicted
    │      ├─ required material withheld → DA/default
    │      └─ upheld / dismissed → resume original bounded clock
    └─ window closed + no unresolved accepted dispute + DA/accounting satisfied
           ↓
         Final (assurance = optimistic public verification)
~~~

Challengeableやassurance fieldは提案概念であり、既存enumを変更済みという意味ではない。
固定PanelのPanelBound/ReceiptLicensedを偽造してこの経路へ入れない。

##### 4.2 Challengeableのadmission

少なくとも以下を合意上検査する。

* RFC14のDisputeAdmissible class/plan/kernel、対象mode、固定artifact/job/layout/task/context identity。
* RFC14 §16.4の新`ModelRewardEligible`。semantic/constraint/court planと有効rulesetを検査し、modelの取得可否を入力にしない。
* producer authorization、public bond資格、最大同時claims/aggregate gain、必要な責任担保。
* model資本snapshot、network/model/個別bond予算、claim受理時の最大権利予約と共通DAA時計。
* claim固有input/state/trace/output/witnessの認証manifest、許可unit/累積scope、有限な開示・retention義務。
* court/localizer資源、challenge window・cutoff・hard deadline、feeと固有work/slice identity。

モデルのpeer数・download成功票・URL到達性・PoR/Full Fetch/leaseはadmission/Finalに要求しない。
claim固有証拠をhash告知だけで充足とせず、適格要求/defaultと算術的convictionを区別する。
model取得目的のweight-file/range・全量列挙/反復要求は受理しない。
許可witnessから実際にterminalへ到達する方法とmodel operandの認証を確定する（ADR177 D2）。
model提供拒否だけではcourt/void/Slash/weight停止を起動しない。旧FPR/CHALLENGED gateは撤回する。

##### 4.3 正の検査receiptの位置づけ

新modeでは固定PanelのValid receiptをChallengeable/Finalの必須条件にしない。
任意verifierの検査報告やサービスreceiptを受け取れても、
その人数・多数決をclaimの真偽の最終判定に使わない。

これはRFC14 §7.3 / RFC11・ADR0171 [C14]–[C15] の
required positive evidence/receipt路線を全ネットワークで削除する変更ではない。
**新modeについてだけ、そのFinal条件をoptimistic admission・DA・window・court・担保へ
置き換える、明示的な別upgrade**である。

旧mode、旧claim、新しいnode-verifiable proofを必須にする将来modeには、
それぞれのpositive evidence条件を維持する。
RFC15が未有効の間は、RFC14の既存・提案条件を緩めない。

#### 5. public materialと後から参加するverifier

G14を満たした共通fraud filer [R14] を、roleによらず利用する。
claim公開後、普通のpublic bondが同じpublic RPC/DAから:

~~~text
claim/plan解決
    → canonical input・model/artifact・root情報を取得
    → 独立した検査
    → mismatch
    → 階層的localization
    → committed witness取得 / on-chain開示要求
    → bounded exact court / ExecutorRefuted
    → canonical conviction
~~~

既存Panelのprivate cacheやoperatorの特別なreadがないと証拠を作れないclassは許可しない。
少なくとも、完全なgarbage、整合した偽trace、別jobのtrace流用、
単一stepとstate/checkpoint/boundary/出力の嘘を閉じる。

public discoveryはoperator producerも含む全対象claimを提供し、
Challengeable時刻、開始cutoff、Final floor、hard deadline、material/root、
未処理sessionと提出済みproofを返す。
active disputeの回答を他のverifierが再利用できるようにし、
sessionの先取りで別人の有効proofを拒否しない。

cold model/artifact取得、最大profileの検査、DA開示とcarrier inclusionを時間budgetに含める。
claim公開後に起動したfresh verifierが実際に間に合うことを実測する。
現行capを超えるwhole captureやliar instanceへの依存が残れば、gateはFAILである [C12]。
モデル自体は任意のoff-chain配布で取得し、chainが取得結果を資格判定へ使うことはない。
全peer停止や公開拒否だけでは新claim/reward/Final weightを止めない。モデル取得はG14試験の条件であり、
公開参加による資本増加と閉鎖時の検出率を別の経済評価へ含める。claim固有の有限witness/court責任は維持する。

#### 6. challenge window、Final、censorshipとspam

##### 6.1 時計

Challengeableの受理時点でbase windowとabsolute hard deadlineを固定する。
必須claim固有materialの依存で必要期間を満たせない場合はChallengeableへadmitしない、
または定義されたDA/defaultで終了する。足りない時間を「watcherが速ければよい」で補わない。

RFC14のnon-seat追及予約を利用し、適格なinteractive disputeだけが有界の延長を得る。
別bond、別session、再送、別site、重複DA要求でhard deadlineを再開始しない。
適格direct objective proofは、他bondのopen courtだけを理由に排除しない。

~~~text
start + B_cold_material + B_check + B_localize + B_disclose
      + B_court + B_carrier + B_reorg_slack
    <= dispute_hard_deadline
~~~

実装で一部作業が並行できる場合も、使用した計測条件とworst-case boundを記録する。
秒とDAAを無条件に同一視しない。

##### 6.2 Final predicate

新modeのFinalは最低でも:

~~~text
class/plan/mode admitted under completed G14
AND producer commitment/authorization/reservation valid
AND permitted claim-specific evidence / court / retention obligations satisfied
AND network + model coinbase allocation + producer bond budgets satisfied (§8.5)
AND base challenge window + class verification horizon expired
AND no unresolved accepted pursuit / required court
AND unique work, accounting and bounded-liability conditions satisfied
AND class-independent bond claim / block / reward / Final-weight accounting valid (§8.3)
~~~

有罪または適格DA/defaultでvoidされたclaimはFinalへ進めない。
honest claimへの誤ったchallengeは、deadline・deposit規則により有界で棄却する。
「challengeがない」ことを「正しさが証明された」と表示しない。

##### 6.3 inclusion仮定と資源独占

proofを必要期限までcanonical chainへ載せられることが安全性の前提である。
検閲耐性を新しいPanel/委員会の署名で代替しない。
carrier lane、fee、proof処理budget、複数producerへの配送と
inclusion/reorg slackを具体化し、honest proofが遅延する敵対的node試験を行う。

unlimited censorshipや無制限の敵対的資金でも必ず安全とは主張しない。
depositだけで全Sybilを防いだとはしない。
shared progress、direct-proofの扱い、session/claim aggregate capにより、
正当な追及を排除せず総負荷を有界にする条件を審査する。
chainがそのinclusion仮定を満たせないなら、Panel=0のrelease gateを通さない。

#### 7. 通常時の検査、soundnessと監視市場

##### 7.1 実際に監視する主体

public watcherはpublic bond/feeと資源条件を満たして参加し、
固定の選出・seat assignment・Valid quorumを必要としない。
professional watchdog、利用者、自分の利益を守るサービス、独立node等が監視できる。

ただし監視市場を開いただけでは各claimのhonest検査を保証しない。
全reward-bearing profileで検査能力・cold material取得・通常時費用、
attack-induced load、実際の稼働と離脱を測定する。
監視されないclaimがwindowを通過し得ることは、optimistic modeの明示的な限界である。

nodeは形式・root shape・authorizationを検査する。
提出されたfraud proof/court evidenceは全nodeが検証する。
大きなclaimを誰かが検査する前に、chain自身が「怪しい」と判別する機能を仮定しない。

##### 7.2 確率的checker

通常検査にADR0171の承認済みconstraint suiteを使う場合、
honest watcherが仕様どおり実行しても未検出の残余確率がある [C15]。
未検出のclaimに、exact courtが存在するだけで有罪proofが自動発生するわけではない。

全claimの見逃しには、監視不在、suiteの条件付きcheck error、
challenge/binding/DA/inclusion/resource仮定の破綻を別々に考慮する。
`epsilon_check <= 2^-128`という検査器の目標を、
監視不在も含めたネットワーク全体の無条件安全性へ読み替えない。

複数watcherが同じseedやtranscriptを検査しただけで、
誤り確率を人数分のべき乗にしてはならない。
post-commit randomness・全constraint coverage・independent checksの条件を満たす必要がある。

##### 7.3 検査サービスの報酬

bountyは不正検出の誘因だが、攻撃がない通常時の監視費用も扱う。
任意サービス契約、公開検査fee、DA保持fee等を設計できる。
合意上の固定Panelへの必須支払やサービス多数決を新しい名前で復活させない。

任意のaudit completion報告を「honestに全計算を検査した証明」とみなさない。
報酬条件は客観的に検証可能な仕事/証拠か、合意外の明示的なサービス契約にする。
サービス運営者の名前・内部ログ・主張をfraud裁定の信頼根拠にしない。

#### 8. Panel担保を外した後の経済設計

##### 8.1 新しいproducer reservation

現行のValid signer lockやbasis_k [C04]–[C06] は新modeで存在しない。
その損失吸収能力を除いたまま、既存のproducer reservationだけで安全と仮定しない。

新modeはclaimと同時claimsの最大fraud gain、unsettled work/weight、
報酬・escrow・state/取引への影響、court/DAコストを評価し、
**producer中心のlocked collateralと未払escrowでどこまで回収できるか**
を新しい経済規則として固定する。

* 検査前の全escrow解放・完全work creditを禁止する。
* 予約と担保退出のclockはhard deadline・retention/liability期間に整合させる。
* concurrent claimsで同じ担保を重複して損失吸収に使わない。
* nonexistent signer lockをkで割る計算を持ち込まない。
* nominal slashと実際のcollectedを区別する。
* fork-choice操作や外部の経済損失は、単一claimのrewardだけをgainとして隠さない。

bond比率・escrow解放時点・総未決済creditの値は、release前の分析と実測で決める。
これらが未定ならPanel=0を有効化しない。

##### 8.2 convictionとbounty

既存conviction funnelとproducerのapplicable slash/rights失効 [C08]–[C09] を再利用し、
新phaseへの移行を明示する。計算fraudとDA/defaultを別basisで記録する。
watcherのローカル検査failureだけでslashしない。

interactive追及はpublic bondのdeposit/exposureを要する。
有効な客観proofにはdefined bounty、正当なcourtにはdeposit返却、
誤ったchallenge/義務不履行にはdefined費用を適用する。
同じproofの重複、複数session、relayでproducerを二重処罰しない。

bountyの財源は徴収できた担保・明示的fee pool等に限定し、
架空のnominal額や未発行の報酬から支払ったことにしない。
producerとchallengerの自己共謀によるfake fraud/bounty循環、
同じworkの再利用・proof front-runningを経済試験に含める。

Panel=0ではPanelFalseValidの対象seatは新claimに存在しない。
producerの処罰を、存在しないseatへのS4処罰完了に依存させない。
旧claimの署名者責任は旧規則の期間・scopeで維持する。
任意検査サービスへの契約違反は、別の定義がない限りPanelFalseValidとして処理しない。

---

<a id="source-v1-0015-L00525"></a>

### 旧RFC-0015 / 原文525–543行

出典: [0015-panel-free-permissionless-verification.md](../rfc/0015-panel-free-permissionless-verification.md)。配置理由: Panel=0固有の予約引下げ案は休眠管理を維持。

###### 8.3.7 予約額引下げと公開検証・責任担保・共通DAA拘束の一体設計

**Panel=0でAttemptの予約額を引き下げる場合は、倍率`rho`だけでなく、escrowの予約規則、
公開検証能力、実際に徴収可能な担保、§8.3.3の共通DAA拘束を一体で設計する（MUST）。**
現行のモデルclassは[`palw_capacity_class_creditable_v1`](../../consensus/core/src/palw_audit_door_v1.rs)の
escrow割引対象に含まれず、計算量の予約部分だけを`rho`で割っても予約総額は同じ比率では下がらない。
`rho = 1,000`から`10,000`への変更だけで、約3,500 BILIが約350 BILIになると仮定してはならない。

新modeのescrow予約を減らすには、対象となる全報酬profileで、modelを得た検証者の計算検査・claim固有証拠からの客観裁定が
責任期限内に完結する能力と、同時claimの最大損失を回収できる担保・未払報酬をMUST評価する。
Panel signer lockがない状態で、旧modeのaudit/credit条件をそのまま安全性の根拠にしない。
予約引下げ後も共通`d + W`までの早期回復禁止、未解決責任の保持、担保重複使用禁止、
class非依存Block Cap / Reward Cap / Final Weight Capを維持し、§8.3.6の期待利益条件を再評価する。
拘束期間を延ばすだけで、公開検証能力や徴収可能額の不足を満たしたことにしない。

担保予約としてのescrow termと実際の未払報酬額を区別し、予約引下げを報酬削減や発行量増加と
同一視しない。具体的な予約額・割引条件・共通拘束期間は実測と敵対的経済分析で固定する。
約350 BILIや`rho = 10,000`は検討例であり、本RFCでの採用値・有効化を意味しない。

---

<a id="source-v1-0015-L00680"></a>

### 旧RFC-0015 / 原文680–953行

出典: [0015-panel-free-permissionless-verification.md](../rfc/0015-panel-free-permissionless-verification.md)。配置理由: 主題別統合。

#### 9. work weight、PALW blockとsettlement

Panel=0はLLM計算の正しさへの処罰経路を置き換える設計である。
それだけでfake claimを防ぐfork choice、時計、DAA、native EVM決算が完成したとはしない。

RFC08のclaim-backed work/slice [C17] に適用する場合、
root/slice/jobの一意性、最大未決済credit、依存sliceの失効、
reorgと各writer/readerのFinal条件を新modeに揃える。
同じ実行を複数のclaim/blockで重複報酬にしない。

unresolved claimからirreversibleな報酬/state・外部releaseを許可しない。
早期のblock inclusionと経済的なsafe settlementは区別する。
RPC/bridgeは新modeのFinal、pending dispute、assuranceを読めなければsafeと表示しない。

RFC14 §16.10の規則E接続を継承する。適用modeのFinal/court条件を満たさないworkを確定weightへ昇格させず、
provisional weightのcap・用途・失効・undoを全fork-choice/settlement readerで証明する。
escrowだけでは不可逆なchain選択を防げない。EXEC_TX/EXEC_SLICEのweight/DAAは0のままである。
全peer停止でもmodel入手性だけで合意結果を変えないこと、閉鎖時の検出率、
private maturation・全Panel共謀、partition/reorg/restartを区別して検証する。

RFC12のDNS finality撤去・native EVM変更 [C18] は別のupgradeである。
固定PALW Panelの撤去とDNS validatorの撤去を同一変更と呼ばない。
本RFCからDNS、BlockDAG、署名、hash、PoW/clockの既存規則を黙って削除しない。

#### 10. 将来のnode-verifiable compact proofとの関係

> **除外(2026-10-15 ユーザー恒久決定、[PRINCIPLES §8](../PRINCIPLES.md)):** 暗号学的な計算証明(ZK、zk-SNARK/STARK、zkVM、PCS ベースの succinct proof など)は使用しない。本節の「将来の compact proof」は暗号学的証明を含まない範囲(決定論的再計算・確率的な代数チェック)に限り、暗号学的証明は採用候補ではない。

producerが全nodeの承認済みkernel checkerで安価に検証できるcompact proofを
提出する方式も、固定Panelなしで設計できる。
ただし本RFCの基本modeはoptimistic public verificationであり、
compact proofがすでに実装・有効という意味ではない。

将来その別modeを追加するなら、semantic/constraint coverage、post-commit challenge、
whole-claim soundness、worst-case node資源、公開witness、exact courtを別途審査し、
新scheme/version/assuranceをbindする。positive proofが必須のmodeでsilence=passにしない。

**compact proofを導入する場合でも、本RFCのG14を省略・代替してPanel=0を有効化しない。**
普通のpublic bondからの独立した不正追及が完結することを、この提案の共通前提とする。

ADR0172のversioned kernel方針 [C16] に従い、VM fallback、TEE validity trust、
BFT運営委員会、trusted committeeによる真偽判定を追加しない。

#### 11. 移行・wire互換・公開表示

##### 11.1 段階

~~~text
current Panel rules
    → RFC14 public fraud prosecution complete (G14 PASS)
    → panel-free lifecycle/economics/DA/Final implementation
    → adversarial shadow / real-node E2E / independent review
    → separately scheduled network upgrade
    → Panel=0 for admitted new-mode claims
~~~

5/3から1/1を経由するdrillは任意であり、本RFCの必須中間段階ではない。
1/1にも別の担保・lifecycle検証が必要で、既存basis_k=2依存を定数変更だけで迂回しない。
1/1成功は0へのgate達成を意味しない。

##### 11.2 互換性

mode/phase/object/class identity、署名domain、codec、state root/carriage、Params mirror、
mempool gate、SDK/RPC/manifest、fingerprintとactivation scheduleを一緒に固定する。

* 旧claimは旧Panel/receipt/担保・deadline規則で完了させる。
* 旧receiptを新modeのevidenceに読み替えない。
* 新modeではPanelBound/ReceiptLicensedが必須でなくなることを全writer/readerへ反映する。
* 不明versionを成功扱いせず、非対応nodeはupgradeなしに新履歴へ参加しない。
* class identity/plan/rulesetをbind/admission時に固定し、途中upgradeでcourt意味を変えない。
* restart/IBD/reorg/pruningでpublic prosecutionと担保を再現する。

旧networkの規則、まだadmitしていないprofile、mode間の違いを保持する。
G14未達のprofileを旧modeから新modeへ自動移行しない。

##### 11.3 利用者に示す情報

最低限、claimのsettlement mode、window/deadline、pending court/DA、Final assuranceを公開する。
新modeのFinalは「optimistic public verificationの条件を満たした」と表示し、
「全nodeがLLM全計算の正しさを証明済み」「監査なしでも必ず安全」と表示しない。

監査サービスがoptionalであることと、honestな実監視が安全性の前提であることを併記する。
watcher台数やreceipt件数を、independent ownershipやhonestyの証明として表示しない。

#### 12. 実装変更の対応表

| 面 | 変更候補 | 必須確認 |
| --- | --- | --- |
| mode/claim lifecycle | palw_state_v2、palw_mode_v2、versioned claim/object encoding | 空quorumによる旧licenceではなく、新Challengeableとruleset |
| public admission | class/TIR admission、RFC14 dispute plan、public bond reads | 全reward profileでG14、非operator資格とmaterial取得 |
| court surface | palw_court_v2、objective offence gate/fold、RFC14予約 | licenseのない新claimを正当に追及でき、先取りで排除しない |
| shared filer | RFC14 palw_fraud_filer、operator/seat/SDK adapters | fresh ordinary bondがpublic dataだけでproof/courtへ到達 |
| Final/DA/locks | deadline sweep、Final helper、vesting/rights、DA sessions | admitted DA、hard deadline、unresolved dispute、producer担保 |
| economics | producer reservation、aggregate gain、bounty/fees、全経路の発行/報酬会計 | signer lock依存を除いたcost/回収可能額・退出条件、§8.3のclass非依存総量cap・共通DAA拘束 |
| block/settlement | processor/mining/work-slice、RPC/native EVM readers | stage別weight、unique work、safe/Finalの一致 |
| network upgrade | Params/bundle mirror、fingerprint、codec/IBD | gate未達でactivation不可。旧履歴は不変 |
| UI/docs | explorer/options/SDKのmode・assurance表示 | no-challengeと正しさの証明を区別 |

上記は実装箇所の設計であり、このRFCの作成でRustコードを変更したことを意味しない。

**2026-10-15追記:** 各行の実装箇所（file:line）、試験、未了項目は[実装状況表](../design/palw/integration/rfc-0015-implementation-status-2026-10-15.md)に記録した。状態はimplemented / dormant / unverifiedであり、表の存在は試験PASSや有効化を意味しない。

#### 13. 必須試験とactivation checklist

##### 13.1 前提gateの否定試験

次のいずれかを故意に残したbuildで、Panel=0のactivationが拒否されることを確認する。

* operatorだけがproofを作れる、public bondの資格・APIが不足する。
* producer heap/drill instance/private endpointが必要になる。
* canonical held/fusedのcap/prefix/committed tile gapが残る。
* garbage/borrowed traceがDAに答えると計算不正の追及が終わる。
* non-seat pursuit中にFinalできる、open court先取りでdirect proofが拒否される。
* 最大profileやcold material時間、resource/inclusion boundsが未検証。
* class/planの完成記録を欠く、またはstaged upgradeのnetwork identityが一致しない。
* Block Cap / Reward Cap / Final Weight Cap / 共通DAA拘束が未実装・未確定、または報酬経路の一部が集約会計を迂回する。

##### 13.2 Panel=0 node E2E

baselineのfold unit testだけでは新modeの証明にならない。
以下をreal node、普通のpublic bond、public authenticated materialで実行する。

| ケース | 必須結果 |
| --- | --- |
| 正しいclaim、固定Panelなし | 新Challengeable → window/DA完了 → Final。空receiptで旧licenceを偽造しない |
| 偽claim、producer/operator共謀 | 後から起動したfresh public verifierがlocalize → exact conviction → slash/Final阻止 |
| material withholding / rootは開く偽trace / borrowed trace | それぞれDA/default・exact fraud・identity/output faultへ到達 |
| 有効proofがあるが別courtが先行 | objectively valid proofを受理し、session・露出・担保を決定的に清算 |
| malformed proof / honest producer / faulty checker | 不正なslashなし、bounded棄却 |
| watcherの退出・復帰・全watcher不在 | 前二者の時計と追及を検証。不在ケースの安全性をPASSとして報告しない |
| spam、全interactive枠占有、carrier遅延 | cap/hard deadlineとinclusion仮定の範囲内で正当追及が進行 |
| 同時claims、担保退出、fake fraud bounty | gain/予約二重使用・自己共謀利益・架空collected支払なし |
| 同額bond、異なるmodel/class/PWU/token数、正直な計算対省略偽造 | Block Cap / Reward Cap / Final Weight Capの共通上限が不変。巨大計算の主張や高速偽造で拡大しない |
| 早期検証/Final、取消、void/conviction、再送・別session | 経済的発行枠は受理DAA `d`からの`d + W`より早く回復しない。報酬予約の即時再利用なし |
| Attempt / Free Prompt / 一claimから複数receipt・block / slice・rider・batch | 受理予約とblock/報酬付与の両方で集約capを検査。未払・将来請求権を含め二重払い・別経路の迂回なし |
| 期間境界、遅延payout、bond分割・退出/再登録・鍵変更・担保移転 | 同額資本の総上限が増えず、消費履歴・時計・残存責任を引き継ぐ。丸め/burst/繰越の境界を検証 |
| `d + W`到達、未解決court/DA/retention、slash後の新claim | 枠回復と担保解放を分離。残存責任と徴収済み資本を控除し、担保の重複使用なし |
| Attempt/escrow予約引下げ、倍率変更、同時claim増加 | §8.3.7の公開検証能力・徴収可能額・共通DAA拘束を一体評価。総量cap・残存責任・期待利益条件を維持 |
| 容量×1/×100/×1000、Final/retired/reversal、丸め・最小credit | §8.4のブロック権利・報酬・確定weight総量が不変。全writer/readerで同じ配分を使用 |
| restart/IBD/reorg/duplicate proof | 同じcanonical stateを復元。二重reward/slashや時計再開始なし |
| 最大admitted class/context/layout | canonical規模で各budget内。小fixtureで置換しない |
| work-sliceとnative settlementを対象にする場合 | weight/escrow/state/bridgeのFinal条件と不正失効が一致 |
| 旧claimをまたぐupgrade | 旧Panel duty/receipt/lockは旧規則のまま完了 |

検査方式のsoundness review、冷温両条件の資源計測、監視市場の運用条件、
producer-only担保の経済評価を別々に記録する。
有限のE2E件数だけから全入力の完全性やhonest watcherの永久存在を結論しない。

##### 13.3 全て満たすまで有効化しない

~~~text
ACTIVATION_ALLOWED
    = G14_PUBLIC_FRAUD_PROSECUTION_COMPLETE
      AND all rewarded profile dispute/resource gates
      AND new lifecycle/court/Final implementation complete
      AND panel-free collateral/accounting review complete
      AND class-independent bond claim / block / reward / Final-weight caps + common DAA hold complete (§8.3–8.4)
      AND public material/watcher/inclusion operating assumptions supported
      AND ADR177 non-interference / capital / allocation / open-vs-closed ECON-MEAS gates complete
      AND RFC14 claim-specific prosecution / Rule E gates complete
      AND independent adversarial node E2E + recovery/migration PASS
      AND separately coordinated network schedule
~~~

これはreleaseのAND契約であり、operatorの口頭承認だけでtrueになるfieldではない。
前提の一部だけの成功、RFCのmerge、0 seats設定、将来proof方式を根拠に有効化してはならない。
RFC14 §16.8/ADR177 §3のnon-interference、資本重複排除、model/network/bond予算保存、
公開対閉鎖の敵対的経済評価をPanel=0でも実施する。model未取得だけの停止・Slashを起動しない。
正しいmodelを得たfresh verifierによるclaim固有courtと計算反証を実証する。
旧7/5/15%・監査/lease・FPR全量公開/代替Seeder/公開費gateは撤回する。
G14 PASSでも自動有効化しない。モデル非公開時の経済優位は未立証であり、旧claimと発行上限を保持する。

**2026-10-10追加（ユーザー指示）:** 本RFCの有効化には、[PALW確率・経済安全性ゲート](../design/palw/economics/probabilistic-economic-security-gate.md)（四条件、T1–T6、A–F、三つの反例、経済的抑止と独立の被害有界化）のPASSを追加で要求する。PASS / FAIL / UNKNOWNのうちUNKNOWNはPASSにしない。2026-10-14改定後のPESGでは、検出確率の下限と期待利益が負であることは評価指標であり、有効化gateではない。

#### 14. 本RFCで決めない値と現状

activation DAA、object tag・signature domain、bond/exposure比率、bounty/費用、
最大未決済work/gain、challenge/cold-fetch/court/inclusion時間、session cap、
DA保持期間、node資源、check error parameterは別途固定する。
§8.3の`N_max(C, W)` / `R_max(C, W)`の導出式と具体値、共通DAA期間`W`、
`Q_max(C, W, rho)` / `F_max(C, W)`、倍率と一claim配分・block権利端数の関係、
rolling/epoch集計と境界/繰越、block・rewardの帰属時計、担保増減・分割/移転時の履歴、
支払構成、責任期間、旧modeとの移行会計もrelease前に固定する。
§8.3.7のAttempt/escrow予約額と割引条件も、公開検証能力・徴収可能担保・共通DAA拘束の評価後に固定する。
§8.5の`f`/配分時計/実効元本、model内配分、残余/zero denominator、claim固有court scope、
公開優位の数値基準・市場仮定・閉鎖時の実効検出率も固定する。
方針は個別cap/共通拘束とモデル入手への不介入に決定するが、未確定値を任意の件数/日数/倍率で埋めない。
それらが未確定の間はDraftであり、稼働保証ではない。

baselineには固定Panelとsigner担保・receipt/licensingへの依存があり [C01]–[C07]、
外部proof/fresh held証拠にも未完部分がある [C10]–[C12]。
**したがって本RFCの必須前提は、現在のbaselineでは達成済みとしない。**

本RFCの作成でPanel定数、合意コード、bond、ネットワーク設定、稼働serverは変更しない。
RFC14の既存機能に対する45 test PASSを、Panel=0や新gateの実装成功として流用しない。
新modeの実node試験・経済評価・upgradeは未実施である。

#### 15. 参照

**R14** — [RFC-0014: Panel多数派に依存しないfraud prosecution][R14]。
特に§2（条件付き保証）、§3（Dispute Completeness）、§5（fused DA）、
§6（共通filer）、§7（予約/Final/先取り）、§13（fresh outsider E2E）。

以下の現行sourceは全てbaselineへ固定する。

* **C01** — [現行5 seats / quorum 3][C01] — `consensus/core/src/palw_fp_devnet_v3.rs`.
* **C02** — [現行claim phase][C02] — `consensus/core/src/palw_state_v2.rs`.
* **C03** — [receipt quorum validation][C03] — `consensus/core/src/palw_panel_v2.rs`.
* **C04** — [Final basis_kの既存下限][C04] — `consensus/core/src/palw_state_v2.rs`.
* **C05** — [Valid signer lockの経済計算][C05] — `consensus/core/src/palw_state_v2.rs`.
* **C06** — [Panel/served mask/basis_kによるescrow release][C06] — `consensus/core/src/palw_state_v2.rs`.
* **C07** — [ReceiptLicensedを対象とするcourt開始][C07] — `consensus/core/src/palw_court_v2.rs`.
* **C08** — [客観convictionからclaimを処理する現行funnel][C08] — `consensus/core/src/palw_state_v2.rs`.
* **C09** — [producerの没収・slash][C09] — `consensus/core/src/palw_state_v2.rs`.
* **C10** — [seat proof filerとheld gap][C10] — `kaspad/src/palw_filer_replay.rs`.
* **C11** — [外部operatorのDA-only不足][C11] — `kaspad/src/palw_operator_da.rs`.
* **C12** — [fresh verifierと8k held/fusedの三gap][C12] — `kaspad/src/palw_filer_held_e2e.rs`.
* **C13** — [operator-anchorの現行trust境界][C13] — `docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md`.
* **C14** — [required positive evidenceとexact court][C14] — `docs/rfc/0011-permissionless-model-and-long-context-onboarding.md`.
* **C15** — [確率的constraint検査の未有効化][C15] — `docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md`.
* **C16** — [versioned kernelと禁止する代替信頼][C16] — `docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md`.
* **C17** — [claim-backed work、weightとsettlement][C17] — `docs/rfc/0008-palw-claim-backed-consensus-blocks.md`.
* **C18** — [PALW-only合意とnative EVMの別upgrade][C18] — `docs/rfc/0012-palw-only-consensus-and-native-evm-settlement.md`.

[R14]: ../rfc/0014-panel-independent-fraud-prosecution.md
[C01]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_fp_devnet_v3.rs#L780
[C02]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L5830
[C03]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_panel_v2.rs#L2981
[C04]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L1003
[C05]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L4602
[C06]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L4700
[C07]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_court_v2.rs#L333
[C08]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L23107
[C09]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/consensus/core/src/palw_state_v2.rs#L26728
[C10]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_replay.rs#L5
[C11]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_operator_da.rs#L27
[C12]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/kaspad/src/palw_filer_held_e2e.rs#L12
[C13]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0010-permissionless-palw-panel-and-claim-completion.md#L33
[C14]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0011-permissionless-model-and-long-context-onboarding.md#L363
[C15]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0171-probabilistic-constraint-checks-and-court-on-dispute.md#L3
[C16]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md#L9
[C17]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0008-palw-claim-backed-consensus-blocks.md#L1
[C18]: https://github.com/MISAKA-BTC/misakas/blob/43f0bcb362d37cba79414940f3fdd368d40d3377/docs/rfc/0012-palw-only-consensus-and-native-evm-settlement.md#L1

#### Mission alignment amendment — 2026-10-07

本RFCのPanel=0は、固定Panelの担当検証をpermissionless verifier + objective fraud proofへ移す別modeである。前提であるRFC14の全completion gatesが、公開materialだけを使う普通の非Panel bondで完全に成立するまでdeferred/inactiveを維持する。前提成立だけで自動有効化せず、本RFCのpositive verification、pending/maturity、DA、exposure、monitoring economics、migrationと明示的activationも必要とする。空quorum、無receiptのsilent acceptance、単なるpanel_seats=0では成立しない。

本節は、衝突する将来の実装指示・受入条件を改定する。本文中の既存実装、過去の測定、旧claimの規則はその時点の記録である。新しい合意規則はversioned specification・実装・独立試験・明示的activationを経て初めて適用する。[ADR173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)、[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)、[RFC15](../rfc/0015-panel-free-permissionless-verification.md)を参照する。

#### MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。

#### 改定 — 2026-10-12（Panel=0は当面有効化しない：提案）

**状態:** 設計の改定・提案。実装、敵対的試験、経済評価、ネットワーク有効化は未実施。数値は本番定数ではない。 全文は[Panel=1・Bond/time予算・モデル拘束資本倍率の改定案（2026-10-12）](../design/palw/economics/panel1-bond-budget-model-capital-amendments-2026-10-12.md) §4と[RFC/ADRレビューの整理（2026-10-12）](../audit/palw/design-reviews/rfc-adr-review-resolution-2026-10-12.md) N5。

Panel=0は将来設計として保持するが、当面の実装・release対象から外す。§7のとおり監視市場を作るだけではhonest verifierの存在を保証できず、多数の監視者を登録させるだけでは誰かが必ず正直に検査することの証明にならない。モデル保有Panelを必要とするmode（RFC-0010 2026-10-12改定のPanel=1）を維持し、Panel=0は監視者の離脱、Panelの共謀、検証費用、最大claim負荷、cold model取得時間、不正発覚からcourtまでの時間を測る別の安全性実証の後に判断する。ADR-0176の共通bond予算はPanel=1にも適用し、RFC15の有効化を待たない。§8の共通経済部分にはモデル別claim bond台帳・配分係数（ADR-0177 2026-10-12改定）を置く。

#### 実装状況 — 2026-10-15（implemented, dormant, unverified）

**状態:** コードは実装済み・休眠・未検証。build、test、devnet、実node試験は本改定では実施していない（ユーザー規則: 先にコード、検証は後）。いずれもPASSと記載しない。

* Panel=0のclaim lifecycle（`Challengeable`、固定window、hard deadline、未解決disputeによるFinal阻止、DA/default、liability中のconviction）、mode付きclass identity、derived OPV eligibility、producer reservation、公開verifier（`kaspad/src/palw_kernel_verifier.rs`）、RPCのmode/assurance表示はkernel routeに実装済みである。V2 Attempt経路はPanel規則のまま（旧claimは旧規則）。
* §8.3のclass非依存Q/B/R/F capと共通DAA拘束`d + W`はADR-0176 budget engine（`palw_bond_budget_v1`）を再利用する。OPV claimは`Q = 1`、`R` = job escrow、`B = F = 0`を予約する。activation gate recordが存在するnetworkでは、budgetのないOPV claimを入口で拒否する。
* §13.3 `ACTIVATION_ALLOWED`をコード化した（`PalwPanelFreeGateRecordV1`）。12 conjunct（§13.3の10項目、PESG、N5解除のユーザー裁定）が全てPASSで、各々に固有のevidence/independent review digestがあり、genesis・height・terms・budget/allocation policyにbindし、kernel route・bond budget・model allocation・court model scope・rule Eのfenceが同時以前に有効で、かつdigestがbinaryにpinされている場合だけ、`palw_panel_free_v1`の検証が通る。pin一覧は空であり、operator flagや設定だけでは有効化できない。
* 同日第2段（lead default、ユーザーレビュー待ち）: gateはG14 public court（`palw_staged_public_court_v1`）と`palw_verifier_pay_v1`も要求し、TIR/Float32 staged routeはrecordにbindした報酬profile集合が含む場合だけ要求する。OPV policyを持つ全network（isolated calibration・試験networkを含む）でbudgetのないPanel-free claimを拒否する。kernel claimの`Final`を読む全reader（EXEC work-slice、claim read/RPC、web表示）は一つのsettlement述語（pending / final-revocable / settled / convicted / defaulted、fraud-expiryは`liability_until`）を使う。OPV networkではbondのduty gateがOPV予約・`d + W`・`H_L`の間担保を保持する。
* 最終段: RFC-0010 V3のbeacon（`ChainPanelBeaconHistoryV1`）が読むOPV Final factも同じsettlement述語から`claim_final`/`da_satisfied`を導く（GAP-B3、追加weight/rewardなし）。EXEC work sliceをOPV claimで検証する結合worldのnode試験sourceを追加した。RFC15のコード段階は完了（未build・未実行・未検証）。
* 新しいwire tag、object kind、aux table、RPC op、p2p version、funded actionは割り当てていない（`GetPalwKernelClaimResponse`内のproto field 35/36とBorsh wire v2を追加）。testnet-12のparams/schedule idは変わらない。isolated calibrationのparams identityは変わる。
* 未了: §1のG14実証、各budget/資源の実測、経済・監視市場の独立評価、PESG、敵対的実network試験、work-slice/native settlement readerの接続、一部のnode E2E（実装状況表 §4/§6）。値はユーザー決定待ち（[user decisions §RFC15](../history/decisions/2026-10/rfc-0006-0012-user-decisions-2026-10-15.md)）。

---

<a id="source-v1-0016-L00112"></a>

### 旧RFC-0016 / 原文112–156行

出典: [0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md](../rfc/0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md)。配置理由: Panel=0の有効化禁止条件を集約。

#### 2. Panel=0の前提（決定済み）

##### 2.1 Kernel claimとそれを運ぶblock（事実）

* Kernel経路のobjectは「one object, one transaction」で運ばれる（`palw_kernel_route_fold_v1.rs` L6–12）。
  `ClaimCommitted`は、そのtransactionを受理するchain blockのfoldで発生する（L1395–1420）。reorgでは受理blockと予約が作り直される。
* claimは抽選ticketを持たない。利用者が投稿したjobに対してseal → revealで提出され、1つのjobに1つのclaimである（status doc L73）。
* claimのためのblockは生成されない（`palw_bond_budget_fold_v1.rs` L1501）。
* EXEC_SLICEはweightもDAAも0で、sliceごとのsubsidyはない（RFC-0008 L134）。
* 共通`W_work`抽選と物理モデルblockは、V2 Attempt lane（Panel検証）にだけ存在する（status doc L15–18）。

##### 2.2 B/R/Fの予約（D-P1/P2で決定）

| 主体 | 時点 | 内容 |
| --- | --- | --- |
| jobの投稿者のbond | `PostJob` | escrow（`claim_reward`）を予約し、`job_fee`をburnする。検査料escrowも積む（`ledger.rs` L1584–1617） |
| producerのbond（ADR-0176台帳） | `ClaimCommitted`（受理chain block） | **Q = 1、B = 0、R = escrowの全額、F = 0**。責任担保と`liability_until`までのincome hold。共通拘束はここから数える |
| モデル台帳 | なし | D-P4により、Kernel claimはモデル予算から取らず、記録もしない |

Kernel claimは、それ自体では報酬対象ブロック権利もFinal weightも作らない。Q・R・責任担保・carrierの資源上限は残る。

##### 2.3 資金の区別（D-P3で決定）

* Kernel claimの報酬は利用者払いescrowだけで、新規発行しない（`ledger.rs` L147–151、`palw_kernel_route_fold_v1.rs` L22–27）。
* coinbase subsidy（worker carve）はV2 Attempt/rider claimの経路だけに流れる。両者を混ぜない（MUST NOT）。
* `P_m`の配分（RFC-0015 §8.5.5）はcoinbase subsidyの配分なので、Kernel claimには適用しない。

##### 2.4 clipの禁止（D-P4で決定、code `eee1710ce`、休眠・未build）

* 修正前: 配分fenceがあると、Kernel claimのR（escrow）が、coinbase由来の`model_available`でclipされていた。
* 修正: `palw_budget_origin_is_user_escrow_v1`（KernelRoute）と`PalwBondBudgetStateV1::preview_claim_for_origin_v1`を追加した。
  * 利用者払いのoriginはモデル予算を無視する。
  * escrowの全額を予約しなければならない。claimごとの上限を下回る場合は`EscrowNotReservable`、共通R capが足りない場合は`Exhausted { Reward }`。どちらも受理前に拒否する。
  * `reserve_claim_v1`とfoldの`plan_bond_budget_v1`はこの関数を使う。escrow claimはモデルを記録しない。
* test source（未実行）:
  * `rfc0016_p4_user_escrow_is_never_clipped_by_the_model_budget`
  * `rfc0016_p4_insufficient_r_refuses_an_escrow_claim_before_acceptance`
  * `rfc0016_a_mode_switch_never_reacquires_issuance_capacity`

##### 2.5 coinbase採掘経路とPanel（D-P5で決定）

* 今回の変更では、V2 Attempt lane（coinbase）の既存Panel検証経路を残す。
* **Kernel経路のPanel=0は、coinbaseを受け取る採掘経路をPanel-freeにしない**（MUST NOTで、そう記述しない）。
* coinbase経路の検証modeを変える（Panel=1へ移すことを含む）には、検証・rotation・責任・Final条件を別途確認する必要がある。

---

<a id="source-v1-0017-L00044"></a>

### 旧RFC-0017 / 原文44–46行

出典: [0017-model-eligibility-examination-and-renewal.md](../rfc/0017-model-eligibility-examination-and-renewal.md)。配置理由: 廃止審査の履歴を集約。

#### 廃止した審査の履歴

旧資格・抽選審査・更新・失効・試行手数料等は [履歴](../history/rfc/0017-model-eligibility-retired-2026-10-15.md) へ分離した。元本文で既に撤回済みの範囲であり、今回新たに規則を変更したものではない。

---

<a id="source-v1-0021-L00001"></a>

### 旧RFC-0021 / 原文1–23行

出典: [0021-design-freeze-and-documentation-governance.md](../rfc/0021-design-freeze-and-documentation-governance.md)。配置理由: 主題別統合。

### RFC-0021 — 設計凍結・例外変更手続きと文書の役割分離

- **Decision:** Accepted — 今回のユーザー指示を手続き化。文書化日 2026-10-17。
- **Scope:** 文書・設計の変更管理。
- **Implementation:** 文書と検査ツール。
- **Verification:** [検査結果](../maintenance/validation-report.md)。
- **Activation:** 合意有効化なし。
- **Related:** [ADR-0181](../adr/0181-design-freeze-and-controlled-change.md)。

#### 方針

採用済み設計は原則凍結する。変数・パラメーターは設計を保つ範囲で証拠・影響・試験・承認を残して調整できる。重大問題で設計を変える場合は新規 RFC と新規 ADR の両方をレビュー・明示的に採択する。
[正式手続き](../governance/design-freeze.md) / [台帳](../governance/parameter-register.md)。

#### 文書の役割

README は目的別入口、STATUS は適用範囲、PRINCIPLES は原則、Spec は条件付きの規範、ADR は採用理由、RFC は技術検討、design は詳細設計、testing は方法、audit/evidence は観測、history は旧版とする。分類だけで採用・撤回・有効化しない。

#### 移行・検査

既存主文書の番号・パスを保持し、補助資料を分ける。移動パスと前後の hash、原文を保持する。番号重複は完全パスで台帳管理し、新しい衝突を禁止する。既存の欠落資料を推測で作らず、新しい不備を検査する。

この整理版で添付索引の次番号 0021 を使用した。他ブランチ予約を統合時に照合する。

---

<a id="source-v1-0022-L00001"></a>

### 旧RFC-0022 / 原文1–28行

出典: [0022-post-mainnet-media-and-model-improvement-scope.md](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md)。配置理由: 主題別統合。

### RFC-0022 — 初期 mainnet から画像・動画・モデル改善を外す適用範囲

- **Decision:** Accepted — 追加のユーザー指示の具体化。文書化日 2026-10-17。
- **Scope:** RFC-0003 の画像・動画経路と RFC-0004 のモデル改善の延期。
- **Implementation:** 文書方針。runtime の無効化は未確認。
- **Verification:** node の拒否・回帰試験は未実施。
- **Activation:** 当面無効、mainnet 開始で自動有効化しない。
- **Related:** [ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md)。

#### 1. 要求

対象機能は mainnet 後へ延期し、初期 mainnet の必須実装・提供・有効化から除外する。詳細と境界は [正式方針](../governance/post-mainnet-scope.md) に集約する。旧予定表・実装完了記録は今回の有効化承認ではない。

#### 2. 共通経路との分離

RFC-0003 全体を無条件に停止せず、必要なテキスト・TIR・検証の共通部分を残す。共有 fence や別の CLI / RPC / admission / fold から画像・動画が開かないことを確認する。改善を止めても ADR-0175 の不変登録を止めない。

#### 3. 検査

通常入口・別入口の対象 job 拒否、改善 epoch / 評価 / 候補選択 / 固有報酬の不進行、共通経路からの迂回不能、再起動・再同期の一貫性、テキスト・認証・予算の回帰を確認する。延期機能の肯定的 E2E 完成は初期 mainnet の条件から外すが、無効化と共通コードの安全性は免除しない。

#### 4. 再開

mainnet 後に対象 release と実装・測定・試験・レビュー・互換性・移行を明示し、機能ごとの提供・有効化承認を得る。自動有効化する日付は設定しない。技術構造を変える場合は [設計凍結規程](../governance/design-freeze.md) の手続きを要する。

#### 5. 非変更事項

今回 node コード、定数、wire ID、fingerprint、既存 claim の責任を変更しない。音声・embedding 等の採用を新たに承認しない。以前の試験結果を現在の無効化証拠に流用しない。

---

<a id="source-v1-0027-L00001"></a>

### 旧RFC-0027 / 原文1–437行

出典: [0027-miner-account-bond-and-panel-incentives.md](../rfc/0027-miner-account-bond-and-panel-incentives.md)。配置理由: 主題別統合。

### RFC-0027 — Miner Account Bond と Panel 追及報酬: 10万の基礎担保、責任連動の追加拘束、REAL継続性

- **Status:** Proposed / Review Required（未採択）。作成日: 2026-10-20。
- **依頼:** ユーザーの「100,000 MSKを最低額とし、抱える claim の責任が増えれば追加 bond」「Panel に十分なインセンティブ」「担保負担による claim 減少・heartbeat 依存を考慮」を詳細化する。
- **対応 ADR:** [ADR-0193](../adr/0193-miner-account-bond-for-panel-incentives.md)。番号は添付 ZIP と会話で確認した範囲のローカル仮割当。他 branch の重複確認は統合前に必要。
- **Owner / reviewers:** 設計責任者による採択・経済レビュー・合意実装レビュー・独立検証レビューを要する。担当者は未指定。
- **Implementation / Verification / Activation:** 合意実装なし。数式検算に加え、既存node/外部検証の実測ログ再集計、shadow再生、合成資本実験を実行（§18）。新Account Bondでのnode実測・敵対的統合試験・有効化なし。wire ID、Params、fence DAA、fingerprint は未割当。
- **通貨表記:** ユーザーの表記に合わせて本書では MSK と記す。添付正本の ticker は BILI（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。同じ native coin の旧ラベルであり、再改名・別通貨化・単位変更ではない。`1 coin = 100,000,000 sompi`。
- **上位規範:** [PRINCIPLES](../PRINCIPLES.md)、[変更手続き](../governance/design-freeze.md)、[RFC-0026](../rfc/0026-court-policy-master-contract.md)。採択済み条項・旧 claim・既存証拠は変更しない。

#### 0. 一文でいう設計

**既存の claim 予約 bond は「claim を出すことにコストを持たせる」ために維持し、新しい Miner Account Bond は「不正が客観的に確定したとき、Panel / 外部検証者が追及費用と成果報酬を実際に回収できる」ための別勘定とする。Account Bond は最低100,000 MSKで、残存責任の総額がその内側に収まる間は追加不要とする。**

```text
既存の生産・claim 予約勘定 P ── 既存の claim 発行コスト・権利・責任・Q/B/R/F
新しい Account Bond A     ── 客観的な違反時の追及費用償還・固定成果報酬・純罰則
通常の Panel 報酬勘定     ── 既存の適格報酬配分 / 実在する承認済み原資
court の FeeEscrow        ── 開示・対話・裁定補助の作業料を段階払い
```

「100,000 MSKを入れるだけで Panel が毎回それを受け取る」設計ではない。正当に行動した Miner の新担保元本を通常の運営費として削らない。返還可能な担保は、未没収のまま支出できる収入ではない。

#### 1. 問題、確認済みの事実、変更が必要な理由

##### 1.1 解く問題

Panel が異常を見つけた後、開示要求・対話・局所化・最終証拠・回復・保持に費用を負う。通常の receipt 報酬だけでこの追加作業が採算に合うとは限らず、追及しない方が有利になる懸念がある。一方、一律に claim ごとの担保を大幅増額すると、正直な Miner の受理余力が減り、有用な REAL block が途切れ、heartbeat に依存する時間が増える可能性がある。

これは**設計上の懸念と検証課題**である。§18の追記測定で長期拘束と既存P減少による制約を数値化したが、「現行 Panel は必ず赤字」「10万で十分」「新担保で heartbeat が減る」と実証したものではない。重大変更の正式採択には §15 の費用・参加・継続性の測定証拠を要する。

##### 1.2 既存設計との照合

| 観察 | 本提案への制約 | 出典 |
| --- | --- | --- |
| 既存の claim 予約は一律3,000という全種別共通定数ではない。約3,200の記録は特定 Attempt の見積りで、weight / right / penalty / other に分かれる後続案もある | 「約3,000」は説明用概数。現行の式・保持・損失担保を消さない | [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md) の2026-10-15改定 |
| 既存の Panel 報酬配分と、Panel 自身の exposure 拘束がある | 「Panel 報酬ゼロ」からの新設ではなく、追及報酬の不足を補う提案 | [ADR-0124](../adr/0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)、[Panel economy 実装](../../consensus/core/src/palw_panel_economy_v1.rs) |
| Panel の最低 collateral を producer の共通下限の10倍から導く経路がある | 新しい10万を既存 `min_collateral_sompi` に直接代入しない | 同上 `palw_panel_collateral_floor_v1` |
| reporter share は旧10%と後続49%を version/fence で分ける | 新 account 没収の全額へ旧 reporter 割合を追加適用しない | [既存 settlement 定数](../../consensus/core/src/palw_state_v2.rs)、[ADR-0032](../adr/0032-palw-fee-bond-escrow.md) |
| 共通期間・Q/B/R/F・非早期回復と、REAL / heartbeat の区別が採用方針にある | 参加負担を減らすために既存の上限・責任時計を短絡しない | [bond 仕様](../spec/palw/bond-budgets.md)、ADR-0176 |
| Final と SecurityCertified は別 | 正常報酬の支払を口実に未認証の発行由来報酬を払わない | [RFC-0018 §7](../rfc/0018-dag-parallel-multi-model-consensus.md) |

既存 fee / 報酬率だけの調整では、Miner 全体の新しい参加資格、複数 claim を跨ぐ新しい責任予約、退出制限、account 原資からの償還を定義できない。本案は単なる数値変更ではなく、新 RFC と新 ADR を要する責任会計の追加である。

##### 1.3 入力の範囲

添付 ZIP には RFC-0026 はあるが、会話にある ADR-0192 本体と F02/F03 詳細案の対応ファイルは確認できなかった。会話の貼付案は **Proposed の追加要件**として扱い、A/A の採択・実装を仮定しない。確認したファイル、行範囲、digest、未確認事項は [source-review](../rfc/0027-assets/source-review.md) に記録する。

#### 2. 目的と非目的

**目的:** 不正追及に対する徴収可能な報酬原資を事前固定する。正常検証の報酬と不正立証の成果報酬を分離する。基礎担保を複数 claim に無二重計上で割り当て、必要な追加額を予測可能にする。小規模 Miner の実用性、Panel の採算、継続的 REAL 生産を同じ試験で評価する。

**非目的:** 既存予約 bond を廃止・一律値へ変更しない。既存の損失担保を「単なる fee」と再定義しない。全額没収を Panel の自由裁量にしない。モデル単位の公開性・取得可能性を審査しない。計算の暗号学的正当性証明を導入しない。DNS-BFT、Panel=0、独立 frontier、延期中の画像・動画・モデル改善の経路を本案だけで有効化しない。

#### 3. 用語と勘定の境界

| 記号 | 意味 |
| --- | --- |
| `AccountID` | 署名鍵の変更でも残る責任 identity。新規 wire/hash domain は別レビューで割当 |
| `A` | 新 Account Bond に専用ロックした実在元本。既払 slash を控除した残額 |
| `A_min` | 100,000 MSK。本提案に対するユーザー指定の最低額。稼働パラメーターとして未有効 |
| `P` | 既存の生産資本・claim 予約・既存責任を支える勘定。適用 ruleset の制約をそのまま受ける |
| `E_A` | account 元本がまだ負っている、新しい責任予約の総額 |
| `L_c` | claim `c` の全許容訴追・違反 outcome に対して必要な account 責任 envelope |
| `FeeEscrow_j` | job `j` の作業料として前払いされた、消費可能な実在資金 |
| `ServicePool` | 既存報酬配分等から適法に資金化された通常検証費用用の勘定。未発行・未認証の予定額を含めない |
| `Q/B/R/F` | 既存の claim / 報酬 block / 報酬 / Final weight の上限 |

Account Bond は EVM 表示残高そのものではなく、既存 native bond と接続する拘束資本の**別用途 subledger**とする。同じ物理 UTXO / bond 内で表現しても、`lot_id / purpose / amount / encumbrance` による元本分離を要する。

原則 `A` は既存生産資本 `P`、Panel 自身の担保、申立人担保、利用者 escrow、第三者 custody の担保と兼用しない。将来の没収予定額、未払報酬、資金移動待ちを入金と見なさない。既存の元本を用途変更できるのは、それが無拘束であり、既存の期間予算・責任履歴が移動によって消えない場合だけである。

#### 4. 最低額と追加 bond

##### 4.1 推奨式: 基礎担保を先に使い、不足だけを追加する

```text
A_required = max(A_min, E_A)
TopUpNeeded = max(0, A_required - A)
HeadroomForNewLiability = max(0, A - E_A)
```

`A_min + E_A` としない。最低10万は、未使用のまま永久に上積みする手数料ではなく、責任を予約できる実在の基礎プールである。初期の複数 claim が10万の範囲に収まるなら、account の追加入金は不要である。ただし同じ1 MSKを同時に二つの損失へ保証しない。

複数 claim の同時裁定を全て扱うため、基準は `E_A = Σ_c L_c_remaining + L_account_specific` とする。account 固有の違反を設ける場合、その別予約も和に含める。何件も不正を起こせるのに最大1件だけを取る `max_c L_c` は禁止する。

##### 4.2 例: 数式の説明であり採択値ではない

仮に同一 profile の `L_c = 12,000 MSK` とする。これは測定済み料金・本番設定ではない。**§18の追記実験では、この説明値は長期責任下の継続発行を大きく制限したため、実用候補額としては採用しない。**

| 残存責任を持つ claim 数 | account 責任総額 | 必要 Account Bond | 最低10万を超える追加額 |
| ---: | ---: | ---: | ---: |
| 1 | 12,000 | 100,000 | 0 |
| 5 | 60,000 | 100,000 | 0 |
| 8 | 96,000 | 100,000 | 0 |
| 9 | 108,000 | 108,000 | 8,000 |
| 20 | 240,000 | 240,000 | 140,000 |

既存予約を説明上3,000/claimと置くなら、5件の「新 account + 旧予約の額面」は115,000となる。しかし**必要 wallet 残高の保証値ではない**。例えば既存側で `ΣL ≤ uP`、`u=0.5` が適用されるなら、旧予約15,000に必要な `P` は少なくとも30,000となり、`A+P` は130,000以上になる。他の役割の拘束、既存下限、fee、保持・Q/B/R/Fも別に確認する。`A` に既存 `u` を再度掛けて「10万入金したのに5万しか裏付けにならない」と黙って変更しない。

##### 4.3 基礎額の限界

10万は「全 profile の任意件数を保証する額」ではない。最小の実用 profile でも `L_c > 100,000` となるなら、その差額が必要であり、「10万で参加できる」と宣伝しない。この不一致は §15 の参加実用性 gate で未達と報告する。最低額を自動で上げたり、モデルを除外したり、未計上の分岐を消して達成と表示しない。

#### 5. `L_c` の導出と最悪責任

##### 5.1 original acceptance で固定する項目

claim、ruleset、source root、profile、許容要求集合と canonical な導出、責任 signer、義務 identity、各作業の tariff と件数上限、成果報酬上限、不可還流の罰則、通常期限、hard deadline、後続 horizon、資金 lot と帰属先を固定する。

F02/F03 貼付案の義務 identity は、`(claim, ruleset, source root, dispute family, range/coordinate, turn, response kind)` とする。owner、relay、受付競争順を含めない。account の支払い ticket はその義務 identity と `funding ruleset / job kind / entitlement slot` を参照する。支払先を変えることと仕事を増やすことを混同しない。

##### 5.2 費用 envelope

各合法 scenario `s` に対し、同時発生し得る一意な費用・違反の集合を `J(c,s)` とし、次を基準に導出する。

```text
L_c = max over legal s of Σ over unique j in J(c,s)
        (CourtCostReimbursementCap_j + FixedBountyCap_j + NonreturnablePenaltyCap_j)
```

scenario 内で複数の合法 choice、内部 node、署名、回復、post-Final の別責任が同時に必要になり得るなら全部を含める。相互排他的であると合意規則から証明できる outcome だけ `max` で共有し、それ以外は和を取る。tree 全 node に要求が広がる profile を「1人1経路」で見積もらない。

同じ認証済み body を共有できる場合の重複統合は許すが、異なる claim / root / ruleset は流用しない。期待不正率、期待 owner 数、保険的な分散、確率的 overbooking で額を割らない。予約の厳密な最大値を求める計算自身も有界であり、導出できない項は admission 不可とする。

##### 5.3 既存担保との二重徴収防止

責任台帳の費目に `charge_id / obligation_id / funding_source / cap / paid / reimbursed / extinguished` を持たせる。既存予約が保証する費用・penaltyを、新 account の同じ費目へ無条件に足さない。

既存予約による没収・損失は元 ruleset のまま保ち、新 account は**追加の追及費用償還・成果報酬・明示された追加罰則**のみを負う。旧規則により同じ実費がすでに賄われた場合、新 account の償還はその未償還差額までとする。旧 reporter 配分と新 bounty の併給を認める場合は、双方を最初から総報酬 cap と共謀還流分析へ入れる。旧 reporter 割合を新没収額へ機械的に重ねない。

#### 6. Panel の報酬: 通常作業、追及作業、立証成功を分ける

##### 6.1 三つの報酬

| 種類 | 支払対象 | 原資 | 不正がなくても成立するか |
| --- | --- | --- | --- |
| 通常検証報酬 | 適用 verifier 規則で認められた検証 job / receipt | 既存の Panel 報酬配分、または別途承認・資金化済み ServicePool | 成立する。未認証の新規発行は不可 |
| 追及作業料 | 規定の開示・局所化・対話・裁定補助を完了した一意な milestone | 申立て時に担保とは別に確保した FeeEscrow | 成立する。棄却でも完了分は支払う |
| 立証成果報酬 | 客観裁定で有効になった一意な違反の成果 entitlement | 当該 claim に予約された Account Bond の没収 | 計算不正 / 帰責成立の default 等、定義済み違反に限る |

ここで Panel は drawn seat に限定しない。既存の適格 public verifier も同じ仕事に同じ料金で参加できる。ただし、誰でも同じ仕事を無制限に重複請求できるという意味ではない。

##### 6.2 正常時の財源を没収見込みに依存させない

不正ゼロなら没収収入もゼロになる。従って通常検証は既存の適格報酬経路を維持する。既存20%という値をこの文書で変更せず、「20%あれば十分」とも置かない。既存の Gate C と Panel leg の接続には未決記録があるため、実際の支払経路を確認する。

新しい通常作業料や最低報酬保証を既存経路だけで賄えない場合、承認済みの実在 ServicePool を予約してから仕事を依頼する。既存 `panel_reserve_sompi` が表示されているだけでは自由に送金可能とは仮定せず、資金化・支払権限・保全・払い出し規則を実装レビューする。未承認の reserve を予算へ算入しない。これが未完なら新しい通常報酬保証は未達であり、没収予定額で穴埋めしない。

claim に由来する issuance leg は `Final` だけで支払わず、適用 ruleset の `SecurityCertified` 条件を満たす。外部前払いの作業料と発行由来報酬は別勘定であり、前者の支払から claim の計算 credit は生まれない。

##### 6.3 court の段階払いと費用償還

申立て時に、閉じた要求集合の該当 job と最大実行 tariff に対する `FeeEscrow` を確保する。既存の fee / challenger bond に支払用原資が含まれるかは科目ごとに照合し、**返還される予定の bond を同時に作業料として支出しない**。公開 frontier の法的な開示権と、費用を負担する job の支払権を分ける。

作業料は完了 milestone ごとに entitlement を確定し、適用する成熟 / reorg 条件を経て支払う。全 court 完了を待たせない段階支払を推奨するが、合意前の自由送金や未取得成果への前払いはしない。料金は実時間の自己申告ではなく、予約済みの標準作業単位と適格成果物に基づく。

Miner の違反が確定した場合、account から FeeEscrow の実際の費用負担者へ、支払済み・未償還の適格費用を cap 内で戻す。Panel が既に受け取った作業料をもう一度 Panel に払わない。第三者が費用を拠出する場合は、非保管型の資金帰属と受領先を固定し、Signer の資金鍵を委譲しない。

正しい claim に対する棄却は、申立人側が完了済み作業料を負担し、残額を返す。棄却だけで申立人を「悪意あり」と slash しない。偽署名・二重請求など別の客観的違反は、その責任者に帰属させる。

##### 6.4 料金と「十分」の判定

profile / job ごとに以下を計測し、承認済み tariff を original acceptance で固定する。

```text
Cost_ref = 計算 + bytes/転送 + IO + 署名/検証 + 保持 + 運用
           + 費用立替の資本拘束 + 支払待ち + 適格な回復作業
TargetPay = Cost_ref + 明示された利益余裕
```

利益余裕を正に置く方針を推奨するが、その率・参照ハードウェア・token建て費用シナリオは測定・承認待ちである。単一マシンの費用だけで全 Panel の採算を保証しない。チェーンが電気代・人件費・token価格・実測 GPU 時間を oracle なしに知ると仮定しない。費用調査を人手でレビューし、versioned tariff に反映する。

通常検証、成功 court、棄却 court、供給欠損、交代/回復について、総受領・自己負担・資本費・待ち時間・純利益を別々に示す。Panel 報酬だけを上げて Miner が退出する点まで、同じ感度分析に含める。

#### 7. 成果報酬と slash の客観性

##### 7.1 裁定権限

Panel は証拠を提出・検証する。資金の没収は、合意が再計算できる有効な客観裁定と固定された科目・上限に従う。Panel vote、自己申告の労力、罰を重くしたいという要求で額を決めない。データの署名・commitment は帰属・同一性を認証するものであり、モデル計算の正しさそのものの証明ではない。

主観的な「悪意の強さ」ではなく、計算不正、署名済み矛盾、適格な開示/応答義務違反等の機械的な違反種別へ価格を対応させる。前の説明にあった5,000 / 25,000 / 100,000の段階額と40/40/20配分は採用しない。料金表は未確定である。

##### 7.2 新 account charge の配分

```text
S_j = CostReimbursement_j + FixedBounty_j + NonreturnablePenalty_j
0 <= S_j <= 予約された当該違反の残存責任
```

費用償還は実際の適格費用と cap の小さい方。bounty は profile / 違反種別ごとの事前固定額を上限とし、Miner の account 総額、Panel が要求した slash 額、owner 数には比例させない。不可還流部分は v1 提案では burn とし、同じ攻撃から claim 可能な ServicePool への再循環を行わない。burn額・比率は未承認であり、没収対象ごとに全配分の合計が実没収額と一致しなければならない。

**1件の違反で account 全額を機械的に取らない。** 当該 claim に予約された上限を超えず、他 claim の保証元本を侵食しない。「最大100,000没収」という商品を提供するなら、その最大額を実際に当該責任へ予約する必要がある。複数件それぞれに10万を保証しながら総元本10万だけとすることは禁止する。

##### 7.3 無応答は四条件を維持する

RFC-0026 の (i) accepted 義務と鍵・資源・予約、(ii) 十分な実供給、(iii) 明示的 inclusion 前提の保証範囲内、(iv) 適格応答不在と期限超過、を全て要する。未収録の送信や個別検閲はチェーンから観測できないという限界を変えない。F02 の供給・配送境界・包含前提が未完成の profile では、新しい default slash を有効化しない。

#### 8. 自作自演、Sybil、重複報酬への防御

##### 8.1 誰が仕事を取っても額と開示権を増減させない

同じ義務 identity は一つの job 群に統合する。作業料の受領 entitlement は既存の割当を尊重し、未割当の job には versioned な機械的割当を定義する。外部の人が開示を取得・対話を継続する権利は、その受領 entitlement の所有・先着とは独立させる。

drawn Panel が放棄した場合、未実施 milestone だけを引き継ぐ。完了済み milestone の再請求はしない。同じ違反の bounty は `offence identity` ごとに一回。既存の reporter commit/reveal 等で正当に確定した受領者を第三者 relay に置換しない。共有開示から複数者が同じ反証を作った場合も、報酬資格の決定を開示経路の owner 独占条件へ逆流させない。

##### 8.2 共謀者の合計収支で評価する

Miner、requester、Panel、reporter、relayer、費用拠出者を同一の経済主体が運営する scenario を必須とする。異なる鍵であることを独立した主体の証明としない。

v1 の任意 court は requester の実資金で作業料を前払いし、無制限の一般財源補助を与えない。同一主体がその作業料を自分へ払っても新収入は生まれない。自己告発で実費償還と bounty を自分へ戻しても、`S - 全還流 > 0` の不可還流損失が残るように設定する。旧予約の reporter 報酬、既発生の補助、別leg・複数jobの支払も全て集計する。

この会計上の自己還流防止と、外部市場利得・計算省略を含む不正抑止は別である。実効検出率 `p_effective` は未測定なら UNKNOWN。`p_effective × (失効報酬 + 還流控除後損失)` は評価指標であり、PRINCIPLES §0 に反して p の下限証明や全攻撃の期待利益負を最終 gate にしない。

##### 8.3 「作業した」の偽装

認証済みで形式に適合した成果物の存在だけでは、全計算を正しく検査した証明にならない。receipt の資格は既存の承認前提・検証規則・責任へ接続し、裁定 milestone は必要な数値検査を含む。署名だけの架空完了、no-op、同じ body の再包装を支払の根拠にしない。見えない労働を完全に観測できると主張しない。

#### 9. 原子的 admission、状態、退出

##### 9.1 新規 Miner の参加範囲

新 fence 対象の**報酬対象 producer / Miner claim の新規受理**には `A >= max(A_min,E_A_after)` を要求する。Attempt、Free Prompt、receipt-backed な報酬権、rider / batch 等の元 claim に漏れなく結びつける。同じ元 claim の運搬・再送で二重に10万を要求しない。

通常 tx、wallet の受取、単なる relay、外部告発者、既存の Panel-only 役割、heartbeat には、新しい Miner 参加条件を課さない。fee-only / Kernel 等の非採掘経路は既存責任のまま保ち、採掘 credit へ変換する経路があるならそこで本条件を検査する。非採掘 job からの不正補償の拡張は別 scope とする。

共通 `min_collateral_sompi` は変更しない。新しい account 最低額を Panel の10倍係数、告発予約倍率、モデル seed、既存生産枠へ自動伝播させない。既存の各役割の下限・露出検査は、分離後に実際に使える対応資本で維持する。

##### 9.2 original acceptance の原子的処理

1. 旧新 ruleset と claim identity、AccountID、資金 lot、責任 signer を解決し、閉じた要求集合と全支払 envelope を導出する。
2. RFC-0026 の全資源予約・既存 `P` 側の claim 予約・Q/B/R/F・保持・依存 horizon を検査する。
3. 新 `E_A_after`、最低額、既存責任との非重複、通常検証の支払契約と必要な事前原資を検査する。court の実際の requester FeeEscrow は申立て時に資金化するが、その最大 tariff と被告の償還 cap はここで予約する。
4. すべて成功した場合だけ claim、予約、料金表、責任、期限、帰属を一つの状態遷移で記録する。失敗時は一部だけ残さない。

後着の高 fee court、別 root、owner / Sybil、途中の料金表変更で受理済み保証を減らさない。block 開始状態による適格区間、object 順、sweep 順は F02/F03 の新 profile の採択・境界試験に従い、旧 claim に流用しない。

##### 9.3 状態の分離

```text
Unregistered / Funding → Active
Active → AdmissionPaused（不足・新規受付停止）
Active / AdmissionPaused → Exiting（新規受理禁止、既存義務を履行）
Exiting → Withdrawable（全必要責任の消滅を確認した範囲だけ）
```

account の不足は既存 claim の計算不正を意味しない。slash 後に `A < A_min` なら新規の報酬対象 claim を停止し、補充で再開できる。既存の応答・開示・court・回復・証拠保管は不足中でも継続できるようにする。新担保不足を理由に正しい応答の受付や relay を拒否しない。

slashing と `L_remaining` の減少は原子的に適用する。予め当該責任を十分に予約していれば、当該 debit 後も他 claim の予約を残せる。想定外にそれが破れる場合は会計破綻として新規受付・退出を停止し、被害や不足を隠す pro-rata な減額・架空 payout をしない。

##### 9.4 退出と余剰資本

Active 中は `A - max(A_min,E_A)` の範囲のみを余剰候補とする。ただし退出成熟・transfer hold 等の適用条件を満たす必要がある。全退出では新規受理を先に停止し、claim の終了だけでなく、post-Final 告発、Gate C、bond の別 claim、保持、未払作業料、未償還費用、monetary material の horizon を完了してから必要分を解放する。

hard deadline 到達だけで返還しない。署名済み旧責任の horizon を後から縮めない。鍵変更、分割、退出再登録、複数modelで消費履歴・slash・残存責任を消さない。退出要求を撤回しても枠の回復時計を早めない。

#### 10. 終了理由の効果

| 結果 | account 元本 / Panel 支払 | claim・責任・証拠 |
| --- | --- | --- |
| 正常に完了 | 新 account を slash しない。通常報酬は適格条件の後。残存予約は必要 horizon 後に解放 | Final と認証・支払・削除条件を分離 |
| 計算不正が客観的に確定 | 規定償還 + 固定 bounty + 不可還流罰則。予約 cap 内 | 適用規則の void / credit 取消 / 責任継続 |
| 十分な供給と inclusion 前提下で default | 定義された default 用 charge のみ。計算不正と同一視しない | 元 ruleset の義務違反の効果 |
| 正しい claim に対する棄却 | 新 account は無傷。requester が完了分の作業料を負担。未使用 fee は返還 | 既存責任終了条件を満たすまで保持 |
| Panel が放棄 / 最終証拠未提出 | 未実施分に報酬なし。適格完了分はその契約どおり。Minerへ転嫁しない | 未実施jobの引継ぎ。Minerの既履行を無視してslashしない |
| 合意で検出できる供給欠損 | Miner の default slash なし。未提供分は非支払。既実施分は元の FeeEscrow / 正規原資から | Final 保留、元 hard 内で再割当、残余不足は未解決・非 credit |
| hard 到達で未解決 | 到達のみでは没収・報酬成功・返還を発生させない | 担保・証拠・依存責任を保持。共有 frontier 段階1の blocking を残す |
| 偽署名・偽造完了・重複請求 | 適格作業 / supply 消費として数えない。署名者等の別責任を検査 | 無関係な Miner へ責任転嫁しない |

#### 11. Miner の資本負担と heartbeat への配慮

##### 11.1 守ること

10万の固定部分は一回だけ、追加は残存責任の不足分だけとする。claim 数だけの荒い tier ではなく必要 `L_c` で増やす。正当な重複開示を統合し、標準 tariff と費用上限を固定して予測可能にする。追加担保は任意のトップアップとし、wallet からの無断引落しをしない。

既存 `P` から `A` を取り分けると生産 headroom が減る。新資金として足す場合も参入資本が増える。**本案が Miner を必ず軽くするとは主張しない。** 100,000未満しか用意できない新規 Miner は、本提案の採掘経路へ参加できない。集中・退出への影響を受入判定に含める。

新 account 元本を積んだだけで Q/B/R/F、model の配分 score、rule E participation、検証抽選の stake を増やさない。既存の対応勘定 `P` / verifier 元本と役割を区別する。移行時の全 reader との整合を §14 で確認する。

##### 11.2 必要資本と生産量の評価

```text
A_required ≈ max(100,000, λ × L_account_per_claim × H_actual + burst責任)
TotalCapital_required = A_required + P_required + 他の非重複拘束 + fee流動性
```

`λ` と `H_actual` は同じ時間単位で評価する。これは資本計画の近似であり、合意 admission は実際の予約集合の厳密な和で判定する。`H_actual` は Final までだけではなく、新 account が実際に負う post-Final・回復・保持まで含む。旧3,000の拘束と新 `L_c` の拘束を混同しない。

参照 Miner に対して、最低10万で何件同時に保持できるか、何 claim/DAAを維持できるか、いつ・いくら補充が要るか、Panel費を含む純収益が正かを表示する。特定 model を除外して実用性を達成したことにせず、profile 別に達成 / 未達 / UNKNOWN を残す。

##### 11.3 heartbeat は非常用のまま残す

[ADR-0140](../adr/0140-the-heartbeat-is-the-emergency-generator.md) と ADR-0176 に従い、heartbeat の妥当性、fee-only の扱い、時計・回復経路を本案では変えない。bond が足りないノードも heartbeat を扱える。heartbeat の観測比率だけを入力として担保要件・責任予約・検証条件を自動で緩めない。攻撃者の claim withholding による担保値操作を作らない。

「heartbeat = 0」を目標にしない。REAL と数える block の既存定義を保ち、riderやただの claim 受付を採用 REAL block の代用にしない。計算完了、抽選当選、claim admission、採用 REAL、認証/Final、支払、担保再利用を別々に測る。

##### 11.4 未達時の順序

まず重複計上・一律10万/claim・最低額の二重加算・Panel下限への伝播を直す。次に実仕事の再利用、対話・codec・実装の最適化、同じ保証内の料金・純粋な予算変数を評価する。受理済み義務を削ったり既存 reservation を安く見積もったりしない。

それでも追及原資と参入実用性の両方を満たす領域がなければ、**本案の有効化を保留**する。10万の再検討、通常報酬配分の変更、別の資金源は明示的な再承認事項とし、本 RFC を口実に自動適用しない。旧新 claim が混在する状況での safety-preserving な新規受付停止を、既存責任の免除にしない。

#### 12. RFC-0026 F02/F03 との接続

本案の原資保証は、ブロックの bytes、work、IO、byte-DAA、control の実供給を代替しない。original acceptance で両方を AND 検査する。通常申立て `a+3,120` と hard `a+10,000` を維持し、責任 horizon はその先も必要に応じて保持する。各等号は未採択の境界表を推測で埋めない。

同じ DAA の最初の accepting chain block のみを supply とする案、DAA skip の供給ゼロ、専用 quota の再計算は、貼付 F02/F03 案と同じく Proposed。account 入金や支払済み fee を「供給済み」の第三者証明にしない。通常 tx が予約 quota を使った場合の処罰猶予と再割当を維持する。

owner 8枠の外の検証者にも、閉じた要求集合内で開示から裁定まで進める経路を残す。新 Miner Account Bond をその外部者の参加条件にしない。requester feeは実費・濫用コストであり開示権の買占め権ではない。共有 frontier 段階2を前提に段階1の停止リスクを消したと表示しない。

#### 13. 合意状態・RPC・保存要件（設計契約、未実装）

最低限、account ごとの元本lot、用途、受理済み責任envelopeと消滅条件、役割別のfree/encumbered、ruleset、料金表version、出金待ち、将来の新規受理可否を復元可能にする。

支払側は `job_id / milestone_id / offence_id / fee funding lot / recipient / tariff cap / earned / matured / paid / reimbursed / burn / remaining` を持ち、複数経路の累積上限と再送の冪等性を確認する。実装は全額を整数 sompi で計算し、丸め規則、checked overflow、0件/0額、最大値を固定する。誤って不足状態を通す saturating 加算は採用しない。

RPC は次を別々に返す: `accountRequired`, `accountLocked`, `accountLiability`, `accountTopUpNeeded`, `productionHeadroom`, `legacyReservation`, `unresolvedLiability`, `nextLegalRelease`, `panelJobFunding`, `admissionReason`。仮の名称であり wire field / proto 番号は未割当。

少なくとも理由を `AccountMinimumMissing`、`AccountLiabilityInsufficient`、`LegacyProductionHeadroomInsufficient`、`CourtCapacityInsufficient`、`ServiceFundingInsufficient`、`QBRFExceeded`、`DependencyHorizonInsufficient` に分ける。これにより「bond不足」のどの要素がREAL停止を引き起こしたかを測定できる。

snapshot / IBD / reorg / undo は元本、charge、FeeEscrow、entitlement、既払・未払・償還、burn、旧新rulesetを一貫して復元する。会計根拠を pruning するのは monetary material の責任が終わってからとする。

#### 14. 互換性・最小変更・移行

| 既存文書・経路 | 将来採択された場合に追加する範囲 | 変更しない範囲 |
| --- | --- | --- |
| ADR-0176 / bond formula | 新 account 専用責任、原子admission、役割別資本reader | 既存Q/B/R/F、P側予約式、回復時計、損失有界化 |
| ADR-0124 / Panel economy | 追及費用と成果報酬、jobの明示的料金・帰属 | 共通下限の自動引上げなし。旧receipt報酬・旧claim不変 |
| ADR-0032 / reporter | 新 account に限定した償還/bounty/burnの独立科目 | 旧chargeの10%/49% versionと既存offence identity |
| RFC-0018 / Gate C | 新支払が原資・認証条件を守るよう接続 | 未認証creditゼロ、Finalと認証の分離 |
| RFC-0026 F02/F03 | 原資を予約・供給・共有開示へ接続 | 時計・供給・inclusion・owner非独占・未解決保全 |
| heartbeat / recovery | 新account資格の対象外であることを確認 | validity、clock、REAL定義、fallback |
| ADR-0177 / 配分読取 | 新account専用資本を生産/配分資本から区分する接続 | モデル二階層・既存配分規則・検証者の独立要件 |

本作成では既存正文を置換しない。採択時にこの表の具体節・実装責任者を確定する。新しい ruleset / claim fence は後日の別承認とする。既存 `min_collateral_sompi` の数値変更で実装しない。

旧 claim に新 account への遡及的な連帯保証を追加しない。移行時に既存元本を新 account に移すなら、旧請求・未解決責任に充てた lot を使わない。対象となる新規claimを停止してdrainしても、旧新の応答や復旧は継続する。fork境界直前直後のclaim、同一block、reorg、再起動で責任が増減・消失しないことを検査する。

#### 15. 受入試験と有効化 gate

##### 15.1 合意・会計・攻撃試験（すべて node-level は未実施）

| ID | 試験 | 必須結果 |
| --- | --- | --- |
| AB01 | 10万未満/等値/超過、責任が10万未満/等値/超過 | `max`式、追加額、Active条件が一致 |
| AB02 | 全claim同時不正、同時default、account固有責任 | promised charge総額が実在元本以下。他claimの予約を奪わない |
| AB03 | original acceptanceの各途中で失敗 | claim・両担保・fee・予約・責任の部分残りなし |
| AB04 | 同一要求、全許容choice、tree全node、別root、owner全枠共謀 | 重複は一回、非重複は全計上。外部者が開示から裁定に到達 |
| AB05 | 自己告発、複数鍵Panel、relay/front-run、複数bounty | 一意なentitlement、不可還流損失、旧新charge非二重払い |
| AB06 | 正常/棄却/不正ゼロ、FeeEscrow不足、ServicePool空 | 正常元本を削らない。無原資支払なし。正当な完了作業を契約どおり支払 |
| AB07 | Panel放棄・永久沈黙・偽署名・架空完了・偽実費 | 別理由として処理。Minerへの無関係な転嫁なし |
| AB08 | 通常tx、有料court、不正codec flood、個別検閲仮定外 | 実供給と受理前DoSを分離。供給欠損でdefault slashしない |
| AB09 | 同一DAA、DAA skip、同一block、期限±1/等値、最遅申立て | 新旧境界と供給会計一致。遅延による期限逃れなし |
| AB10 | slash後のA_min割れ、追加資金、未解決court、hard到達 | 新規停止/再開を分離。応答継続、必要担保/証拠保持 |
| AB11 | 退出、鍵変更、分割、model変更、元本purpose変更 | 旧責任/消費履歴をresetしない。旧資本二重使用なし |
| AB12 | 10万を導入、Panel下限10倍経路、告発枠、heartbeat | 共通下限へ漏れず、非Miner役割を不当に締め出さない |
| AB13 | Final未認証、認証失敗、post-Final反証、複数報酬leg | 未認証issuanceなし、支払/償還/返還/creditの整合 |
| AB14 | MAX整数、丸め、0件、同時payout、再送 | 元本保存、overflow拒否、二重支払なし |
| AB15 | fence混在、snapshot、IBD、restart、deep reorg、pruning | 全writer/reader・undoが同じ責任/原資/支払へ復元 |
| AB16 | 担保不足でREALが止まりheartbeatのみ、その後補充 | heartbeatは常時既存規則どおり。REALへの復帰を実証 |

F02/F03 の T02/T03/T04/T05/T06/T07/T09/T13/T14/T15 と重複部分を対応付けるが、それらが本ZIPに実装済みとは主張しない。オフライン数式検算が AB01/AB02 の一部を確認しても、この表の node-level 合格とは表示しない。

##### 15.2 経済・継続性の同時 gate

同一の入力job、model/profile集合、計算能力、ネットワーク条件、参加者構成を用い、(A)旧規則、(B)総資本固定で新accountを取り分ける、(C)新資金を追加して既存Pを維持、を比較する。

| Gate | 測定・承認すること | 本作成の状態 |
| --- | --- | --- |
| G-Collectible | 全同時責任、全分岐、post-Finalを含む原資・予約の不足ゼロ | 数式・shadow部分検証済み / 全branch・node未達 |
| G-Panel | 対象profileごとの純利益、支払待ち、資金立替、p50/p95費用、通常/棄却/成功/回復の報酬 | 旧実測CPU/bytesを再集計、価格感度のみ。金銭的十分性は未達（§18） |
| G-Entry | 参照小規模Minerの総必要資本、最低額での有用な同時件数・持続claim率、追加担保の頻度 | 長期責任・旧P切出しの制約を数値化。同profileのnode実用性は未達（§18） |
| G-REAL | 採用REAL率・最長bond起因停止・heartbeat依存時間・補充後回復が承認許容域内 | UNKNOWN / shadow拒否数や合成admissionをREAL実測に換算しない（§18） |
| G-Concentration | 参入/退出率、資本集中、PanelとMinerの両役割拘束、token価値シナリオ | UNKNOWN |
| G-Compatibility | 旧claim、共通下限、全reader、Gate C、reorg/IBD、返還・pruningに不整合なし | 未実施 |
| G-Court | RFC-0026の供給・inclusion・共有訴追・hard horizonをnode-levelで満たす | 未達 / 既存P0を解除しない |

「十分なPanel利益」「許容heartbeat依存」「参照Minerの実用件数」の数値閾値はレビュー前に固定する。測定結果を見て都合よく閾値を動かさない。閾値未定のままPASSとしない。全てが同時に成立するパラメーター領域が確認できないなら、新規則の有効化は不可とする。最低10万を保って両立できるという経済保証はまだない。

#### 16. パラメーター候補と未決事項

| 項目 | 値 / 方針 | 状態 |
| --- | --- | --- |
| `A_min` | 100,000 MSK = 10,000,000,000,000 sompi | ユーザー指定の提案要件。設定変更なし |
| 追加額 | `max(0, max(A_min,E_A) - A)` | 本RFCの推奨構造、未採択 |
| 既存予約 | 適用rulesetの実額。約3,000は説明用 | 変更なし |
| `L_c` / tariff / bounty / burn | scope別に測定・導出。前の段階額・割合は不採用 | §18に局所費用・感度を追加。全branchの責任額と料金承認は未決 |
| 通常Panel報酬 | 現行versionの経路を維持。追加保証のServicePoolは実資金を要する | 配線・採算の検証待ち |
| 初期court fee立替 | requester / 明示的第三者の実資金。返還担保の二重支出なし | 既存feeとの接続待ち |
| 新accountの全stake readerからの区分 | Q/B/R/F・model配分・rule E・抽選への自動加算なし | 実装設計待ち |
| duty/entitlementの重複排除・bounty受領者 | 既存commit/revealを尊重し、新jobへ明示的に接続 | 詳細レビュー待ち |
| 全境界と支払成熟 | acceptance / deadline / payout / release別に定義 | 未決 |
| 経済gate閾値 | 同時測定で採択前に固定 | 未決 |
| 新ruleset / domain / fence / fingerprint | なし | 割当・有効化は別承認 |

#### 17. 証拠・レビュー・採択記録

2026-10-20: ユーザーの文書作成依頼に基づき、この RFC と ADR を作成。既存ZIPの正文・実装を読取り、数式例・リンク・差分を検査するための補助資料を同梱した。**文書作成の承認と、設計採択・経済妥当性・実装完了・統合・有効化は別である。**

必要レビュー: 費用導出と外部Panelの採算、Miner資本とREAL継続、全責任の非二重計上、役割別資本reader、shared frontier権利、客観裁定、資金保存、移行・リプレイ。採択者・採択日・有効化日はいずれも未記入。

補助資料: [出典と未確認事項](../rfc/0027-assets/source-review.md) / [数式・資金フロー検算](../rfc/0027-assets/economic-examples.md) / [検証記録](../rfc/0027-assets/validation.md)。


#### 18. 2026-10-20 追記 — 実測ログ再集計とAccount Bond負荷実験

**証拠正本:** [追記測定レポート](../rfc/0027-assets/measurement/measurement-report.md)。既存実測（M_ARCHIVE）、新shadow再生（R_TRACE）、合成実験（S_MODEL）、仮定付き価格導出（D_CONDITIONAL）を分ける。新Account Bondのnode実測や本番十分性を完了したものではない。

##### 18.1 実施した範囲

添付されたnode時系列17 run・2,563 claim、外部検証者の21 sampleを再集計した。追加で1,320条件の受付列shadow、326条件の合成資本実験、150行の資本逆算、120行の費用感度を実行した。再集計・会計の1,365検査と解析コード32単体試験はPASS。元の実測は2026-09-26と2026-10-09/10、今回の処理は2026-10-20である。

この環境にRust toolchainと元の大型artifactがなく、新Account Bondの合意実装・支払配線も入力にない。したがってfresh node / 実モデルの追加再実行は未実施。既存生ログ再解析と新しい合成実験を「新規node実測」とは表示しない。

##### 18.2 最低額と実用頻度に関する結論

`L_c=12,000`では10万が支えるのは8件。全責任が適法に終了するイベントをH=10,000 DAA後へ置いた楽観的な合成条件でも、新accountだけの継続上限は0.576件/720 DAAだった。目標30件/720 DAAの試験で、旧Pを維持して10万を別途加えても、account不足で提供機会5,000回中4,904回を止めた。これはadmissionモデルの値であって、heartbeat比率ではない。

同条件で目標5 / 30 / 90件を均等発行するなら、必要な同時account責任は70 / 417 / 1,250件。10万だけに収める1件責任上限はそれぞれ1,428.57142857 / 239.80815347 / 80 MSKとなる。1件12,000なら必要Accountは840,000 / 5,004,000 / 15,000,000 MSK。**これらの頻度は感度試験の入力で、実用性の採択済み閾値ではない。**

H=3,120/10,000/20,000は試験入力であり、notice/hard到達だけで解放する新規則ではない。未解決でreleaseなしの試験は最初の8件以降停止した。一般のaccount保有責任が後続horizonまで残る場合、その実際の保持を使って必要額を再計算する。同DAA release先行という楽観境界と、厳しい境界で重複数が1件増える条件も区別した。

##### 18.3 Panel費用と財源

Qwen2.5-0.5B referenceの外部検証は、3 positions正常でprocess CPU p50 141.85秒（n=5）、32 positionsのlate違反のローカル裁定まで628.31秒（n=1）。小標本であり、全profileの最大費用ではない。21 sample中4件のgather証拠は272,271,336 bytesとなり、当該wire上限67,108,864 bytesで拒否された。報酬を増やしてもこの提出不能を解消したことにならない。

実費単価は未取得のため、仮定のCPU/転送/保持/運用単価から感度だけを導出した。一つの局所作業に利益を残す料金を置いた例では、32 positionsの部分責任は約43.21 / 396.63 / 3,966.31 MSKと単価仮定で変動した。全合法branchのL_cではない。運用費を含む中間例396.63でも、30件/720 DAA・H=10,000を最低10万で支える条件239.81を超える。逆に一部が収まっても、全分岐・同profile・実支払・参加者採算の合格とはしない。

未没収のAccountは通常Panel収入ではない。正常時のFeeEscrow/既存報酬資金化、実費、支払待ち、同一主体への還流を別に検証する。価格モデルで利益率を付けたことを、実地の利益・参加動機の実証としない。

##### 18.4 RFCへの反映

100,000という最低要件と`max(A_min,ΣL_c)`を維持するが、**最低額を「実用頻度を保証する推奨資本額」と表示しない。** 前版の12,000/claimは説明に限り、実用料金候補へ昇格させない。

候補の評価順は、同profileで全合法job・費用・固定bounty・burnを導出し、実際の保持期間と目標頻度から必要Account、旧P、fee流動性を計上する。旧PにAを上乗せする場合と、同じ総資本からAを切り出す場合を別に提示する。同一profileでない旧A16予約例とQwen2.5検証費用を組み合わせて両立達成としない。

G-Panel/G-Entryは部分証拠が増えたが十分性は未達、G-REAL/G-Concentrationは未実測、G-Court/G-Compatibilityは未達。有効化保留と旧claim保護を維持する。heartbeat数値のための担保/検証の自動緩和はしない。

---

<a id="source-v1-0039-L00024"></a>

### 旧RFC-0039 / 原文24–50行

出典: [0039-finite-fee-consent-and-provider-payment-bounds.md](../rfc/0039-finite-fee-consent-and-provider-payment-bounds.md)。配置理由: 未採択のprovider有限役務A/Bを採択済みfee-onlyから分離。

#### 2. Provider V2 の署名済み上限

V1 契約は変更せず、V2 は少なくとも現行 enrollment 全項目と `service_until, last_admissible_payment_daa, material_until, late_payment_policy, fee_release_policy_digest, queue_reservation_id` を署名対象とする。provider 全員と sponsor が同じ canonical payload に署名する。原 claim、依存集合、EXEC plan、材料 bytes 上限、全 offer/fee/collateral を維持する。未知 version や未知 policy は拒否する。

候補となる期限導出は次のとおりである。`a` は認証された受理 DAA、`H` は既存 consumer の最大責任期限、`Q` は**新たに承認・実測が必要な**物理支払遅延上限、`W` は既存 policy の材料保持 tail。

- `service_until >= H`。
- `last_admissible_payment_daa = service_until + Q` (checked arithmetic)。
- `material_until >= last_admissible_payment_daa + W`。
- すべての実依存 claim の hard と material 期限が署名済み上限内に収まるものだけを受け入れる。
- queue reservation は仕事を受ける前に確保し、全ての native payment consumer が同じ予約を課金する。平均 throughput の見積りを最悪遅延の保証にしない。

**finality は別条件である。** DAA 上限を設定しても、その時刻までの finality は保証されない。finality 未成立で安全に材料を捨てること、未解決の支払を確定済みにすることは、本案から導かれない。以下のどちらを採るかと、その finality 条件を決める必要がある。

#### 3. 遅れた支払の選択肢

| 案 | 期限時に未払いの権利 | 利点 | 費用・未解決条件 |
| --- | --- | --- | --- |
| A: 権利を維持し、事前資金と有限予約で保管を引き継ぐ | 権利を消さず、支払 escrow と材料 custody を明示された次の契約へ移す。元 provider の責任を本人の署名外で延長しない | 正当な報酬を期限だけで失わせない | 次の provider の明示同意、移管の実 bytes と認証、独立 reserve、移管完了前の旧契約責任、移管不能・finality 停止時の処理が必要。有限資金で無期限保持は保証できない |
| B: 期限で未消費の支払枠を取消し、sponsor に一度だけ返す | **新 V2 で事前同意した**未払い権利のみ期限時に消し、対応 escrow を返す。支払済み fee と既存 V1 権利は取消さない | 有限期間・有限資金の契約にしやすい | 正当に働いても遅延で支払を失う可能性がある。consumer/provider 双方の明示同意、ネットワーク遅延と当事者違反の区別、実払済みと refund の競合排除が必要。finality 前に取消しを不可逆とみなせない |

A は現時点で完成した採択案ではない。引継ぎ失敗時に署名済み上限を超えて旧 provider を拘束することも、自動的に B の権利取消しへ切り替えることも認めない。事前資金・受入枠・最後の backup の署名済み期限内に義務を完了する条件と、完了できない場合の具体的な経済状態が必要である。方向承認だけで本番へ進めない。

B を選ぶ場合は、期限で失われる新 V2 の未払い権利を明示し、取消しと refund を同じ transition で行う。どちらの案も既存の honest/network progress 前提の下で評価する。無条件の wall-clock 進行や finality 保証を新たに要求するものではない。通常の reorg 保護を維持し、finality 前の変更は取り消せる状態として扱う。稼働中の V1 entitlement を B に移さない。

**判断を分離する:** 通常 verifier の有限 fee 契約は独立してレビュー・採択できる。provider は Q の測定と、A の終端失敗時または B の権利取消しの具体的扱いが閉じるまで未採択とする。

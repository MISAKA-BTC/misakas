# RFC-0003: PALW Generative Model Classes — one job, determinism and output layer, and the class profiles on top of it

| Field | Value |
| --- | --- |
| Status | Draft, 2026-09-28 — open questions 1–12 decided by the user on 2026-09-28 (see *Decision*); implementation of Part I and program version 2 started on `rfc3/impl` |
| Author(s) | MISAKA core (drafted with Claude) |
| Created | 2026-09-28 |
| Affects | spec/palw 03 (registry: pipeline classes), 04b (PALW-TIR: program version 2 — input tensors, output kinds, `post` effects), 05 (canonical work: attention over a token axis), 07/08/09 (claims, verification, court: derived inputs, stage edges, output digest), 11 (job lanes: a generative job family), 16 (fences) · all networks (dormant until armed) · `consensus/core` (R, the job family, pipeline admission, court), `misaka-palw-tir`, `misaka-palw-tir-lower`, `misaka-palw-sdk`, gateway |
| Branch | `rfc/0003-image-generation` (text only) |
| Related | RFC-0001 (§A FP Job V4, frozen; ADR-0082 D11 sampler), RFC-0002 (PALW-TIR; spec [04b](../spec/palw/04b-tensor-ir.md) on `tir/core`; Phase F integration design on `tir/phase-f`), ADR-0040 (integers only), ADR-0044 (F5/F6/F15: no randomness from executor-chosen fields), ADR-0053 (no tolerance), ADR-0057 (backends below the semantic boundary), ADR-0069 (weight needs e2e adjudicability), ADR-0072 (one inference, one ticket), ADR-0074 D1 (canonical prompt mode), ADR-0096 (refuse by name), ADR-0133 (resume), ADR-0135 D7 (a new op is a protocol upgrade), ADR-0144 (P1–P7), ADR-0145 §6 (a cache is an execution fact) |

## 概要(日本語)

- **目的。** 画像生成モデル(Qwen-Image の 2.x 系、FLUX.2 [klein] 4B など)を、テキスト LLM と同じ形 ——
  **Job → 決定論的な実行 → commit された出力 → 争い → court による replay** —— で permissionless に
  登録できるようにする。同じ形を埋め込み・音声・動画にも使う。モデルごとの差は小さな「class profile」に
  閉じ込め、共通層を先に作る。RFC は増やさず、この 1 本を **Part I(共通層)** と **Part II(class profile)** に分ける。
- **Part I-1 決定論的乱数 `R(seed, domain, step, position, lane)`。** 置き場所は **(a) job/consensus 層**。
  D11 と同じ keyed BLAKE2b-512 で、乱数 tensor は TIR program に **派生入力** として入り、court が job から
  再計算する(executor は commit しない)。**IR の primitive は増やさない**:R の入力はすべて job の事実と
  静的な座標で、計算された値には依存しないから。(b) 新 primitive は bit 演算・新しい `prim_set_id`・fence・
  全 backend での hash 実装が要るのに得るものがない。(c) library 合成は XOR なしの算術 PRNG を各 class が
  持つことになり、統計的な品質も「protocol が固定する」性質も失う(しかも seed を入れる入力 tensor は結局要る)。
- **R の鍵は job id ではなく job の seed(32 byte)。** D11 と同じで、同じ seed で同じ出力を再現でき、
  executor が選ぶ `job_nonce` に乱数の意味を持たせない(ADR-0044 F6 と同じ規律)。R は lottery には一切使わない。
- **RFC-0001 の D11 サンプラーは R の domain 0 として 1 byte も変えない**(key 文字列、`seed ‖ position ‖ lane`、
  digest の上位 13 bit、Gumbel 表)。新しい domain(画像の初期ノイズ・step ノイズ、音声、動画、class 定義用)は
  同じ文法で、1 回の hash から 16 bit の語を 32 個取る。
- **ビットから値へ。** 一様整数は語そのもの(任意範囲は ⌊w·n/2^b⌋)。Gaussian は **2^16 行の pinned 逆 CDF 表**
  (Q24、|z| ≤ 約 4.33、反対称で平均がちょうど 0)を引く「データ」で、演算ではない。protocol 側の入力種別
  `Normal` にするので区間が正確に決まり、class ごとに表を持たない。Gumbel は D11 の表のまま。
- **Part I-2 Canonical tensor と execution profile。** dtype・TF32・kernel 選択・GPU 数といった
  「execution profile」は MISAKA では不要。consensus の意味は整数 TIR program で、backend はバイト一致、
  fused kernel は identity の外、並列化は順序自由領域の中でしか起きず値を変えない。残る関心事は
  (1) 入力前処理(画像・音声のデコードとリサイズ)、(2) tokenization と固定テンプレート、(3) 出力の表示用
  エンコード、(4) fidelity 用 float 参照の固定、(5) ノードの性能設定 —— どれも profile ではない。
- **全 profile 共通の program 面。** `TirProgramV2` = V1 + 入力 tensor(job の値・上流 stage の出力・R の派生入力)
  + 出力種別(`Logits` / `Rows` / `Final`)+ `Rows`/`Final` の program では `post` が global Fixed state を
  書ける(1 step 1 writer は維持)。class は複数 program の **pipeline**(stage、構造だけの edge、1 本の step tree)。
  **primitive は 25 個のまま**で、足す強い理由は見つからなかった。
- **Part I-3 Canonical Output。** consensus は出力 node の整数 tensor を種類ごとの正準バイト列にしたもの
  (text = token id 列、image = raw u8 HWC、audio = PCM i16 LE、embedding = i32 Q 形式、video = u8 THWC)。
  digest は step tile に揃えた Merkle root で、step tile と食い違えば structural fault(executor の負け)。
  PNG・WAV・MP4・UTF-8 表示は presentation で consensus ではない。
- **Part II-1 画像生成(5 章)。** ImageJob(prompt/negative の token id、guidance は 1/16 格子、seed、
  image_index、sampler、step 数、解像度、出力 = RGB8。正準化は gateway の仕事で、受理時は検査と名指しの拒否だけ)。
  ノイズは R の `Normal` を latent の正準順 `[C, H, W]` で引き、pos = 0 で `Select`。denoise は **1 step = 1 position**、
  latent は `post` が書く Fixed state、scheduler 係数は `(step 数, pos)` で引く表、CFG は batch 軸、text encoder は
  別 program(stage)、VAE decode は解像度ごとの単一 position stage の鎖、画像 token 上の attention は 1 step 内の
  tensor 軸(H ではない)。
- **court。** bisection は既存の ladder で steps × tiles を narrowing する。K/V を head-major に commit すれば、
  attention の cone は約 15,600 token(head 次元 128)まで 16 Mi MAC・約 16.8 MB の上限に収まる(効く制約は MAC ではなく
  開示バイト)。VAE の mid-block attention や GroupNorm の統計のように収まらない exact reduction は、lowerer が
  **commit した部分和による 2 段 reduction** で解く(protocol 変更なし)。それを超える規模(2048² 級・動画)には、
  RFC-0002 の H dissection を **宣言された任意の reduction 軸へ一般化** することを提案する(H と違い、bottom の費用は
  admission が box demand で検査する)。PALW-TIR-33 はそのまま(不正な commit は executor の負け)。
- **fidelity と計算量(正直に)。** fidelity は同じノイズを入れた float 参照との latent 誤差・PSNR/LPIPS・CLIP 型スコア・
  FID/KID で測り、有用性の基準であって正当性の基準ではない。計算量は (P, N, S, CFG 係数) でパラメータ化した。
  **例 A**(P = 4×10^9、N = 4,096、S = 28、CFG 2)で約 1.3×10^15 MAC/枚、S = 4・CFG なしで約 1×10^14。
  汎用 CPU backend で数時間〜十数分、調整済み CPU で分単位、GPU 整数で秒単位。commit 量は例 A で約 1 TB/枚で、
  GPU では MAC より leaf の hash が重い。testnet の最初は ≤ 1B・少 step、4B 級は RFC-0002 Phase G(GPU 整数 backend)の後。
- **対象モデルについて確かなのは「text-to-image・拡散/flow transformer・VAE の latent 空間」だけ。** 構成は未検証なので、
  見積りは例として示し、image profile を凍結する前にモデルカードで確かめる checklist を付けた。
- **他の profile(各 1 段落)。** テキスト = RFC-0001 FP Job V4 をそのまま参照(再定義しない。D11 が domain 0)。
  埋め込み = `Rows`/`Final` + pooling stage、seed は 0 固定。マルチモーダル入力 = 正準 u8 画像を job が commit し、
  正規化・patchify(と固定比のリサイズ)を整数 stage で行う(デコードは consensus 外)。音声 = PCM i16 を正準形に、
  mel 前処理を整数 stage に、生成は codec token の LM か latent flow。動画 = 画像の一般化(latent に時間軸)で、
  token 数が大きいので一般化 dissection が必須、現実性は commit 量が決める。
- **決定(2026-09-28、ユーザー)。** 未決事項 1〜12 はすべて推奨どおり:鍵は seed、protocol 共通の 2^16 Gaussian 表、1 class 1 解像度、VAE は最初から on chain(最初は ≤ 512²)、profile ごとの上限、testnet は commit 量を受け入れる、画像の attention は `attention_prefill`、pipeline 登録は複数 carrier、最初の画像 class は小型(モデルカード確認後に選ぶ)、一般化 dissection は動画と一緒、音声の multi-codebook は後、画像 job の既定は `PanelDa`。
- **オフチェーンの対応物。** MISAKA Studio の diffusers sidecar(別 session で構築中)は consensus 外のローカル生成。
- **実装順序。** R と canonical output → TIR V2 → pipeline class と court 拡張(fence `palw_gen_v1`、休眠)→
  画像 profile(小型 class、埋め込みを smoke test に併用)→ ドリル → testnet → Phase G 後に 4B 級 → マルチモーダル入力
  → 音声 → 一般化 dissection → 動画。テキストは RFC-0001 の列車のまま。

## Summary

Generative classes — images first, then embeddings, audio and video — become registrable the way
text classes are, with one loop for all of them: **a job fixes every input; execution is a
deterministic integer PALW-TIR pipeline; the output is a committed integer tensor with a canonical
byte form; a dispute ends with the court re-evaluating one cone.** Per-model difference is kept to a
class *profile* that reuses one common layer.

**Part I, the common layer, is built first.** (1) **Deterministic randomness:** `R(seed, domain,
step, position, lane)`, a counter-based keyed BLAKE2b-512 fixed by the protocol and independent of
execution order, parallelism, batching or device count. It lives at the job layer; random tensors
enter a program as *derived inputs* that the court recomputes; no primitive is added. RFC-0001's
D11 sampler is R's domain 0, byte for byte. (2) **Canonical tensors and the execution profile:**
the integer program already fixes everything an execution profile would, so no profile object is
needed. What remains is input preprocessing, tokenisation, presentation encoders and the float
reference for fidelity. The program surface every profile needs is small: input tensors, output
kinds, and state writes in `post` (`TirProgramV2`), plus pipeline classes (stages, structural edges,
one step tree). (3) **Canonical outputs:** one canonical byte form per output kind, a tile-Merkle
output digest, and presentation kept outside consensus.

**Part II, the class profiles:** Image generation in five chapters (ImageJob, deterministic noise,
the denoise scan, ImageOutput, verification and court), followed by fidelity, a parameterised
compute estimate and a checklist to verify before the profile is frozen. Then Text generation
(RFC-0001 unchanged), Embedding/encoder, Multimodal input, Audio and Video, each as small as it can
be. **The 25 primitives of PALW-TIR v1 stay; `prim_set_id` does not move.**

## Motivation

1. **The target.** The user wants image models — the Qwen-Image line (2.x) and FLUX.2 [klein] 4B
   are the named examples — to be registrable as data. What is certain about them for this RFC is
   only that they are text-to-image diffusion/flow transformers working in a VAE latent space. Their
   exact geometry is unverified here, so every number below is parameterised, and the
   *Verification checklist* lists what must be confirmed first.
2. **Everything today is text-shaped.** A PALW-TIR v1 program maps `{token, pos}` and its states to
   logits (04b §3). Selection happens outside the program, in the FP decode rule (RFC-0001 §A, D11).
   The only committed output is a token sequence. There is no definition of a non-text job, of
   randomness other than the text sampler's, or of an output that is a tensor.
3. **Without a common layer, each modality repeats the per-family release problem.** RFC-0002
   removed "a release per architecture" by making the class a program. If images, audio and video
   each brought their own job wire, their own noise, their own output format and their own court arm,
   the same problem would come back as "a release per modality". The loop *job → deterministic
   execution → committed output → dispute → court replay* must have one shape. Profiles may differ
   only in their job body, their programs and their output kind.

## Goals and non-goals

**Goals.**

- G1. One job envelope, one randomness function and one output rule, shared by every profile.
- G2. No new primitive. The court stays a PALW-TIR v1 interpreter that knows no model.
- G3. Randomness is a protocol function of job facts, reproducible bit for bit on any hardware, in
  any order, at any parallelism.
- G4. Consensus bytes and presentation are separated for every output kind.
- G5. Every profile is adjudicable by cones within the court ceilings, and where it is not, the RFC
  says so and names the fix.
- G6. Profiles are minimal: a job body, programs and an output kind. They add no semantics beyond Part I.
- G7. RFC-0001 §A (FP Job V4) is untouched.
- G8. Registration stays permissionless (P7) for multi-program classes.
- G9. An honest estimate of compute and commitment volume, and of what runs on testnet first.

**Non-goals.** Bit-identity with a float reference or with a framework's image for "the same seed"
(fidelity is measured, never required). Decoding user files (JPEG, PNG, MP3, MP4) inside consensus.
Content policy (no validity rule reads what a prompt or an output means, PALW-PR-1). Local,
non-consensus generation: MISAKA Studio's diffusers sidecar, being built separately, is the
off-chain counterpart, the image analogue of RFC-0001 §2.0's Chat path. In image v1: editing,
img2img, inpainting, reference images and adapters are refused by name (§II.1.1). A separate RFC per
modality. zk proofs. Production-scale video in v1 (only its path is drawn here).

---

# Part I — The common layer

## I.0 One job shape

Every non-text profile uses one job family. Text keeps FP Job V4 (§II.2).

```
PalwGenJobV1 {
  version:  u16 = 1,
  envelope: PalwJobEnvelopeV1,   // the FP V3 envelope fields with their spec-11 meanings: network_domain,
                                 // class_id, executor_bond, executor_pubkey, operator_id, anchor_block,
                                 // anchor_daa, job_nonce, privacy_mode, prompt_mode
  seed:     [u8; 32],            // R's key (§I.1); all zeros for a class that draws no randomness
  body:     PalwGenBodyV1,       // Image | Embedding | Audio | Video — Part II
}
gen_job_id_v1 = H64(key "misaka-palw/gen-v1/job-id/v1", borsh(job))
```

- **One encoding per behaviour.** Acceptance verifies the canonical form and refuses anything else
  by name, as RFC-0001 §A.2 and ADR-0096 do. It never rewrites a job. Canonicalisation (text to ids,
  a typed guidance value to its grid, an integer seed to 32 bytes) is the gateway's work.
- **The body's kind MUST equal the class's profile** (`BodyKindMismatch`). The seed MUST be all zeros
  exactly when the class declares no random input (`SeedNotUsed`), so a deterministic class never has
  two job ids for one computation.
- **The class binds the tokenizer.** A pipeline class's id commits to its tokenizer (Phase F D4), so
  the job carries none. This closes the gap that FP V3's `tokenizer_id` documents for text.
- **Canonical prompt mode** (ADR-0074 D1) carries over. The network's own job derives its prompt from
  its canonical anchor (computed over the envelope as `fp_canonical_anchor_v1` does), and its seed as
  `H64(key "misaka-palw/gen-v1/canonical-seed/v1", anchor)`, a pure function of the job.

## I.1 Deterministic randomness

### I.1.1 Where R lives — decision: (a), the job layer

**R is a consensus function at the job layer**, a keyed BLAKE2b-512 of job facts and static
coordinates (the discipline D11 already ships). **Random tensors enter a TIR program as derived
inputs.** Their elements are functions of the job alone. The executor never commits them, and the
court recomputes every element a cone reads. **No primitive is added**, and PALW-TIR v1's
`prim_set_id` is unchanged.

The argument:

1. **Every input of R is a job fact or a static coordinate; none is a computed value.** The seed is
   a job field. The domain comes from the class's declaration. The step, position and lane come from
   the scan position and the tensor's static shape. A value that depends only on the job is a job
   constant, like the prompt's token ids: it belongs where job constants are computed, and the IR only
   needs a way to receive it. Randomness whose *use* depends on data (comparing a uniform draw with a
   computed CDF, or rejection) is still a draw of job facts followed by `Compare`/`Select`, so it does
   not need R inside the IR either.
2. **(b), a new primitive, buys nothing and costs a protocol upgrade.** A `Hash`/`Philox` primitive
   needs either bitwise operations (XOR, rotate), which PALW-TIR has none of, or a monolithic hash
   primitive. Either way it brings:
   - a new prim-set descriptor, so a new `prim_set_id` and a fence;
   - a range transfer function that says "any value";
   - new cost formulas and golden vectors, and a second implementation of the hash;
   - bit-exact BLAKE2b or Philox in every backend, GPU included, inside the evaluator.

   It would also sit badly with RFC-0002's granularity rule, whose primitives are exact integer
   operations and named lossy sites of the arithmetic a model performs. A primitive would be needed
   only if randomness had to depend on *computed* values, and no inference family does that.
3. **(c), a library composite, is possible but strictly worse.** Without XOR, a counter-based
   generator must be purely arithmetic. For example, a multiply–add mixer whose 32-bit rotation of a
   64-bit word is a swap of halves, `(x mod 2^32)·2^32 + ⌊x / 2^32⌋`, is expressible with `Mul`,
   `Div(Floor)` and `Sub`, and its 64-bit squares need 32-bit limbs to stay inside `i128`. The costs:
   - 40–60 nodes per draw;
   - statistical quality that is unvetted in this form (Philox, Threefry, ChaCha and BLAKE all mix
     with XOR);
   - every class carrying its own copy, so "the protocol fixes R" becomes a convention: a registrant
     can ship a weak generator, and "seed 42" means different noise in two classes;
   - the seed still has to enter the program as an input tensor, so (c) needs (a)'s input facility
     *and* the mixer.
4. **(a) is what D11 already is.** The text sampler's Gumbel index is a keyed BLAKE2b that the court
   recomputes from the claim's job (`gumbel_index_v1`). This RFC states that once, for every domain.

### I.1.2 The function

```
R(seed, domain d, step, position, lane)  →  an unsigned integer of b_d bits

  W_d   = words per digest: ⌊512 / b_d⌋ for a blocked domain, 1 for domain 0
  block = ⌊lane / W_d⌋,   word = lane mod W_d
  D     = BLAKE2b-512( key = K_d, message = enc_d(seed, step, position, block) )      (64 bytes)
  R     = the bits [word · b_d, (word + 1) · b_d) of D, read as a big-endian bit string
```

"Big-endian bit string" means bit 0 is the most significant bit of `D[0]`. For `b = 16`, word `k` is
`256·D[2k] + D[2k+1]`. For `b = 13` and `W = 1` it is `(256·D[0] + D[1]) >> 3`, which is exactly D11's
`u16::from_be_bytes([d[0], d[1]]) >> 3`. R is a pure function of its five arguments. Blocking packs
`W_d` consecutive lanes into one hash call, and a lane's value does not depend on which other lanes
anybody computes.

### I.1.3 The canonical encoding of every input

| Input | Type | Encoding | Where the value comes from |
| --- | --- | --- | --- |
| `seed` | `[u8; 32]` | the 32 bytes as they are | the job's committed seed (`PalwGenJobV1.seed`; for text, `PalwFreePromptJobV3.sampling_seed`) |
| `domain` | registered id | selects the BLAKE2b **key** `K_d` (its ASCII bytes, no length prefix, no terminator, at most 64 bytes), the message layout `enc_d` and the width `b_d` | the domain id in the class's input declaration, looked up in §I.1.4 |
| `step` | `u32` | 4 bytes LE | for a per-step domain, the consuming stage's scan position `p`; otherwise 0 |
| `position` | `u32` | 4 bytes LE | the job's item index (`image_index`, `clip_index`); for domain 0, the committed logits row |
| `block` | `u64` | 8 bytes LE | `⌊lane / W_d⌋` |
| `lane` | `u64` | — | the element's row-major flat index in the declared input shape |

`enc_d` for every domain except 0 is `seed ‖ le32(step) ‖ le32(position) ‖ le64(block)` (48 bytes).
Domain 0 keeps D11's layout, `seed ‖ le32(position) ‖ le64(lane)` (44 bytes). Keys are distinct, so
the two layouts can never collide.

**Why the seed and not the job id.** The brief's first argument, `job_id`, is realised as the job's
seed:

- **D11 already keys on the seed.** One convention for every domain.
- **Reproducibility.** The same (class, prompt, parameters, seed) gives the same image in any job.
  That is what users mean by "seed". A job id also hashes the fee-bearing envelope, the bond and the
  anchor, so two identical requests would produce different images.
- **No randomness from executor-chosen fields.** The job id contains `job_nonce`, which the executor
  picks for uniqueness and which "carries no lottery meaning" (invariant F6). Keying R on the job id
  would let a free re-roll of the nonce choose among outputs. The seed is inside the job id anyway
  (FP V3 docs), so it cannot change after the fact.
- **Execution facts stay cacheable.** Identical computations have identical randomness (ADR-0145 §6).

Gateways SHOULD map a user-typed integer seed `n` to `le64(n) ‖ 0^24`, so that "seed 42" is the same
image on every gateway. That is a presentation convention, not consensus.

**R is never a lottery input.** Eligibility still consumes a beacon that postdates the claim
(ADR-0044 F5/F6/F15). R decides only *what the execution computes*. A chosen seed buys a different
answer and never a better draw (the D11 argument). Grinding a seed costs a whole execution (ADR-0072).

### I.1.4 Domains

| Id | Name | Key `K_d` | `b` | `W` | `step` | `position` | Lane order | Used by |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 0 | `TEXT_GUMBEL_V1` | `misaka-palw/decode-select-v2/gumbel/v1` | 13 | 1 | — | committed logits row | vocabulary lane | RFC-0001 D11 (unchanged) |
| 1 | `IMAGE_INIT_NOISE_V1` | `misaka-palw/rand/image-init-noise/v1` | 16 | 32 | 0 | `image_index` | latent `[C, H, W]` row-major | §II.1 |
| 2 | `IMAGE_STEP_NOISE_V1` | `misaka-palw/rand/image-step-noise/v1` | 16 | 32 | denoise step `p` | `image_index` | latent `[C, H, W]` | stochastic samplers |
| 3 | `AUDIO_INIT_NOISE_V1` | `misaka-palw/rand/audio-init-noise/v1` | 16 | 32 | 0 | `clip_index` | latent `[C, T]` | §II.5 |
| 4 | `AUDIO_STEP_NOISE_V1` | `misaka-palw/rand/audio-step-noise/v1` | 16 | 32 | step `p` | `clip_index` | latent `[C, T]` | §II.5 |
| 5 | `VIDEO_INIT_NOISE_V1` | `misaka-palw/rand/video-init-noise/v1` | 16 | 32 | 0 | `clip_index` | latent `[C, F, H, W]` | §II.6 |
| 6 | `VIDEO_STEP_NOISE_V1` | `misaka-palw/rand/video-step-noise/v1` | 16 | 32 | step `p` | `clip_index` | latent `[C, F, H, W]` | §II.6 |
| 7 | `CLASS_UNIFORM_V1` | `misaka-palw/rand/class-uniform/v1` | 32 | 16 | 0 or `p` (declared) | item index | declared shape | any class-defined use, through its own table |

- A domain's key is never reused.
- A new domain is a protocol change: a row here and a new `rand_set_id` (§Activation). Domain 7
  exists so that a new *use* of randomness in a class does not need one.
- A pipeline declares each domain **at most once** (PALW-RND-7). A stage that needs two streams sizes
  one input to hold both and splits it with `Slice`, since lanes are independent. This keeps every
  `(domain, step, position, lane)` unique within a job.
- The **lane order of the init-noise domains is canonical**: channel-major over the latent's
  `[C, H, W]` (`[C, T]`, `[C, F, H, W]`). Two classes with the same latent geometry therefore draw the
  same noise for one seed, and a program packs the noise into its own token layout with `Reshape` and
  `Transpose`. Admission enforces the rank. The meaning of the axes is the registrant's statement.

### I.1.5 From bits to values

| Transform | Value `v` from word `w` | dtype, interval | Where it runs |
| --- | --- | --- | --- |
| `Uniform{b}`, `b ∈ {1, 2, 4, 8, 16, 32}` | `v = w` | `idx`, `[0, 2^b − 1]` | job layer (input kind) |
| `Normal` (`b = 16`) | `v = PALW_GAUSS_Q24_V1[w]` | `i32` Q24, `[G[0], G[65535]]` | job layer (input kind) |
| `Gumbel` (domain 0 only) | `PALW_GUMBEL_Q24_V1[w]` (D11's table, `b = 13`) | `i32` Q24 | the FP decode rule, outside any program |
| uniform below `n` (recipe) | `⌊w · n / 2^b⌋`, bias `< n / 2^b` | in-program: `Mul`, `Div(Floor)` | program |
| Bernoulli(`p`) (recipe) | `Compare(w < ⌊p · 2^b⌋)` | in-program | program |
| class-defined distribution | `Gather(table, w)` over the class's own table | in-program | program |

**The Gaussian table.** `PALW_GAUSS_Q24_V1[i] = RHAZ(Φ⁻¹((i + ½) / 2^16) · 2^24)` for
`i ∈ [0, 2^16)`, where RHAZ rounds half away from zero. It is evaluated at 60 or more significant
digits by a pinned generator script, in the manner of `scripts/palw-gumbel-table.py`, and pinned by
`BLAKE2b-512(key = "misaka-palw/rand/gauss-q24/v1", the 65,536 entries as LE i32)`. The digest is fixed
when the generator lands, not in this draft. Properties:

- it is data rather than arithmetic, exactly as PALW-EX-5 treats "a transcendental evaluated at
  registration";
- it is strictly increasing;
- it is antisymmetric (`G[65535 − i] = −G[i]`), so the table's mean is exactly 0;
- every entry has probability `2^−16`;
- it reaches `|z| ≈ 4.33`;
- its centre spacing is `≈ 3.8·10^−5 σ`, far below a Q12 latent's resolution.

For comparison, D11's `2^13` resolution would stop at `≈ 3.84σ`. Resolution `2^16` puts about 4 values
of a 262,144-value latent at each end entry, against about 32 at `2^13`.

**Why `Normal` is an input kind and not an in-program `Gather` over a class-carried table.** The
lookup is the same "data, not arithmetic" either way. Doing it at the job layer has four advantages:

1. The interval is exact. A param takes its dtype's full range (PALW-TIR-9), so a class-carried table
   would need a never-firing clamp and would leave a wide interval.
2. No class carries 256 KiB of table, and no noise cone opens an artifact leaf.
3. "Seed `s`, domain `d`, shape `X`" is the same tensor in every class, as a protocol guarantee
   rather than a convention.
4. It is the shape D11's Gumbel lookup already has.

A class that wants another distribution declares `Uniform` and gathers its own table in its program.

### I.1.6 RFC-0001's sampler is domain 0, unchanged

RFC-0001 §A is Implementation Frozen, and nothing here changes a byte of it.

- **The index.** `gumbel_index_v1(seed, position, lane)` *is* `R(seed, TEXT_GUMBEL_V1, —, position,
  lane)`:
  - `K_0 = "misaka-palw/decode-select-v2/gumbel/v1"`;
  - `enc_0 = seed ‖ le32(position) ‖ le64(lane)`;
  - `b_0 = 13`, `W_0 = 1`.
- **The value and its consumer.** The value is `PALW_GUMBEL_Q24_V1[w]`, pinned by
  `PALW_GUMBEL_Q24_V1_DIGEST_HEX`. Its consumer is the lane key `value·2^24 + ((T_q·G) >> 24)` in the
  §A pipeline's step 6.
- **`position`** is the committed logits row: row 0 is the one the last prefill position produced.
- **Vectors.** D11's pinned table digest and the FP V4 golden vectors that exercise seeded selection
  (`consensus-vectors/fp-v4/`) stand as domain 0's vectors. `rand-v1` adds raw index vectors
  computed by the shipped `gumbel_index_v1`, so the two can never drift apart.

**Where domain 0 differs from the generic layout, and why it stays.**

- There is no `step` coordinate, because text has one scan.
- It uses one hash per lane. The court opens two lanes per dispute (I-2), so blocking would buy
  nothing there.
- Its word is 13 bits.

These are values of the family's parameters, not exceptions to its grammar.

**What this RFC takes from D11.**

- The keyed-BLAKE2b discipline.
- The rule that the court takes sampler inputs **from the claim, never from the challenger**
  (`PalwDecodeSamplingV2`).
- The pinned-table transform.
- The argument that a seed inside the job id cannot be changed after the fact.

**What stays outside programs.** Text selection (the lane key and the §A pipeline) remains a
job-layer rule that consumes an LM program's logits. RFC-0002's non-goal ("sampling stays in the logits
scheme and the FP job") is kept.

### I.1.7 Derived inputs: how a program receives R and how the court recomputes it

A program declares `Random { domain, dist }` inputs (§I.2.3). At position `p`, element `e` of such an
input is `dist(R(seed, domain, step, position, e))`, where `step = p` for a per-step domain and 0
otherwise, and `position` is the job's item index.

- **The executor** computes the input with the consensus function. It never uses a framework RNG
  (torch, cuRAND).
- **The court's demand evaluator** (04b §9.4) gains a sixth question, `input(p, j, i)`. For a random
  input the *court answers it itself*, from the claim's job: it is never opened from a leaf and never
  taken from the challenger. The work is one hash per distinct block, charged as elements under the
  existing limits.
- **PALW-TIR-33 does not apply**: nothing was committed, so nothing can be malformed. An executor that
  used other noise is convicted at the first commit point whose cone reads the noise, because the
  court computes the true noise there.

### I.1.8 Properties

- **Order-, partition- and device-independence.** Each element is a pure function of
  `(seed, domain, step, position, lane)`, and no generator state exists. Any split across threads,
  GPUs, batches or resumptions computes identical values. A framework generator's values depend on
  draw order, device and generator type; that dependence is gone.
- **Domain separation.** Distinct keys give independent PRFs, so one seed yields unrelated streams in
  two domains.
- **Quality.** BLAKE2b-512 as a keyed PRF: its outputs are indistinguishable from uniform, and so are
  the words inside a digest.
- **Cost.** A keyed call is two compressions (the key block, then the message). The key block's state
  is per-domain constant, so an implementation MAY precompute it. That is about one compression per 32
  Gaussian values: the 262,144 initial-noise values of a 1024² image take 8,192 calls, about a
  millisecond on one core.

## I.2 Canonical tensors and the execution profile

### I.2.1 One meaning, so no profile

In float ML stacks an *execution profile* exists because one model gives different bits under
different kernels. It covers dtype and autocast, TF32 on or off, cuBLAS/cuDNN algorithm choice,
attention backend, deterministic flags and device count. In MISAKA the consensus meaning of a class is
its integer program:

- **Types and rounding are part of the program.** Every value is an integer of a declared type. Loss
  happens only at named sites (PALW-TIR-2/3). No float accumulation or TF32 path exists to toggle
  (ADR-0040 A).
- **Kernel choice is outside the identity.** A fused kernel is correct iff it is byte-identical at
  every commit point, and the court runs the reference evaluator (F-1/F-3, PALW-TIR-17).
- **Parallelism cannot change a value.** Reordering is legal only inside order-free regions, whose
  every partial sum is proved not to overflow (PALW-TIR-4/24). R is per element (§I.1.8).

Every axis of a profile therefore either lives inside the program (dtype, quantisation, rounding, the
scheduler's tables) or cannot affect a bit (kernels, threads, devices, batching). Nodes still choose
kernels, but that is node configuration, never identity.

### I.2.2 What remains of the concern

Five concerns survive. None of them is an execution profile.

| Concern | Rule |
| --- | --- |
| **Input preprocessing** — file decoding (JPEG/PNG/HEIC, MP3/AAC/Opus, MP4), orientation, colour management, resampling | Consensus starts at a **canonical integer tensor** (u8 HWC pixels, PCM i16). Decoders stay outside. Resize, normalise and patchify are either done by the gateway to a class-declared size (disclosed, non-consensus) or are an integer stage of the pipeline (§II.4) |
| **Tokenisation and templates** | Text to ids stays at the gateway; the job carries ids (the RFC-0001 stop-string precedent). A class's fixed prompt template (an encoder's system prefix, its pad token, its maximum length) is class data (§II.1.3.4) |
| **Presentation encoders** | PNG/WebP/JPEG, WAV/FLAC/Opus, MP4/WebM and UTF-8 rendering are never consensus (§I.3) |
| **The float reference for fidelity** | Pinned library versions, eager attention, fp32, and the *same noise* as the integer run. It decides usefulness, never validity (RFC-0002 criterion 5) |
| **Node performance configuration** | Backends, fused kernels, threads and batch sizes are node policy, held byte-identical by F-1 |

### I.2.3 Canonical tensors at every boundary: the program surface every profile shares

A tensor crosses a boundary in three places: job to program (inputs), stage to stage (edges), and
program to user (outputs). PALW-TIR v1 has only the text case of each: `Input(0)` is a token and
`Input(1)` the position (04b §3.2); the output is a `logits` node consumed by the decode rule. §II.1.3.7
derives the gap from images. The minimal shared fix is below. The **primitive set is unchanged**:
program version 2 declares `PRIM_SET_ID_V1`.

```
TirProgramV2 = TirProgramV1 with
  version: 2
  inputs:  [InputDecl]                      // NEW — Ref::Input(2 + j) names inputs[j]; Input(0), Input(1) as in V1
  output:  Logits { node, scheme_id }       // replaces V1's `logits` + `logits_scheme_id`; V1's meaning exactly
         | Rows   { node }                  // the node's value at every position (an encoder's hidden rows)
         | Final  { node }                  // the node's value at the last position (a latent, an image)
InputDecl   { name: String, dtype, shape: [u32], source: InputSource }
InputSource = External { lo: i64, hi: i64 }                      // bound by the pipeline: a job value or an upstream output
            | Random   { domain: u16, dist: Uniform{bits: u8} | Normal }      // §I.1.7
```

- **Inputs.** At most 16; rank ≤ 4; at most `2^28` elements; each used; names unique. `External`
  inputs are `i8`/`i16`/`i32`/`idx` with `lo ≤ hi` inside the dtype. `Uniform` is `idx`, `Normal` is
  `i32`, and domain 0 is not allowed (text selection is not a program input).

  Range analysis (04b §7) gains the leaves `External → [lo, hi]`, `Uniform{b} → [0, 2^b − 1]` and
  `Normal → [G[0], G[65535]]`. Inputs cost nothing (§8), like params. They are constant over the scan,
  except per-step random domains, which are keyed by `p`.
- **Output kinds.** `Logits` keeps V1 and Phase F exactly: `post` runs only at positions whose logits
  are consumed, and it writes no state. `Rows` and `Final` need `post` at every position. For those
  two kinds **`post` MAY `StateWrite` a global `Fixed` state that `pre` does not write**. NF-19's
  one-writer-per-instance-per-step is kept, and `post` still appends to no history.

  In §9.4, the writer of a global state is occurrence 0's `StateWrite` or occurrence `L + 1`'s,
  whichever exists; NF-19 makes it unique. The output node is committed, has a committable dtype and
  has no `H` (as NF-6).
- **What V2 is not.** V2 has no control flow and no new primitive. No input is written by the
  program. A V1 program keeps being admitted as V1. A V2 program with no inputs and a `Logits` output
  computes what its V1 twin does, but under a different `graph_ir_root`.

**The pipeline class.** A class may be several programs run in sequence, each over its own scan:

```
TirPipelineV1 {
  version: 1,
  profile: u8,                               // Image | Embedding | Audio | Video (| Text, later)
  stages:  [StageDecl],                      // 1..=16, in execution order
  output:  { stage: u8, spec: OutputSpecV1 }, // §I.3
  offers:  ProfileOffers,                    // the job-parameter domains the class accepts (Part II)
}
StageDecl {
  name: String,
  program: [u8; 64],                         // graph_ir_root; the program bytes ride the registration (Phase F D3)
  layout:  PalwTirLayoutV1,                  // Phase F D5, per stage
  trip:    Fixed(u32) | JobSteps | TokenCount,   // the stage's T, a job fact known at acceptance
  tokens:  Option<TokenRule>,                // Input(0) at each position
  bind:    [Binding],                        // one per External input, in declaration order
}
TokenRule { prefix: [u32], source: Prompt | Negative, suffix: [u32], pad: Option<(u32 id, u32 to_len)> }
Binding = JobScalar(field) | JobTokens(TokenRule, u32 len)
        | StageRows { stage, drop: u32, pad_to: u32 } | StageFinal { stage } | StageRowCount { stage, drop: u32 }
tir_pipeline_class_id_v1 = H64(key "misaka-palw/tir/pipeline-class-id/v1",
                               borsh(pipeline) ‖ artifact_root ‖ tokenizer_id)
```

- **Edges are structural.** Each input element maps to one committed element of an earlier stage's
  output, to a job value, or to a zero pad. There is no arithmetic in an edge, and an edge reads only
  earlier stages. Admission proves every edge's upstream proven interval (∪ {0} when padded) ⊆ the
  input's `[lo, hi]`, and the shapes agree.
- **One step tree** (Phase F D7, generalised). Stage-major, then per stage Phase F §2.5's order:
  commit points by slot, `Fixed` checkpoints every `C`, `Hist` tiles every `h_tile`. A `Rows`/`Final`
  program's `post` commit points appear at every position. Stage outputs are commit points, so
  Phase F's invariant holds: *every leaf is adjudicated from leaves that precede it*. The first
  divergent leaf of the ladder is therefore always adjudicable.
- **Admission.** Admission checks:
  - every stage program under `tir_admit_v1` with the V2 rules;
  - every edge;
  - the job-parameter domains;
  - the output spec (§I.3.2);
  - the per-job cost and leaves under per-profile ceilings (§Security, *ceilings*);
  - the class id.

  A per-job trip count is bounded by `offers` (`JobSteps ≤` the largest offered step count;
  `TokenCount ≤` the class's maximum prompt length).
- **A pipeline is not a VM.** Stages run in declared order, always, each over a trip count fixed at
  acceptance. There is no condition and no loop between them (PALW-TIR-18 holds per stage).

## I.3 Canonical outputs

### I.3.1 The rule

**The consensus output of a job is the value of the class's output node, a committed integer tensor,
serialised in the canonical byte form of its kind. Its digest is part of the claim. Everything a user
sees beyond those bytes is presentation.**

The output node is already committed as step leaves. The digest adds no trust. It adds a *portable
identity* for the result: users, explorers and caches can name "the image with digest X" without
the step tree. It also adds a two-opening consistency check.

### I.3.2 The digest

```
OutputSpecV1 { kind: u8, shape: [u32], meta: [u8] }        // meta per kind: sample rate/channels, fps, q, ...
canonical bytes B = the output node's elements, row-major, each in the kind's element encoding (below)
tile t      = the bytes of the node's lanes [t·tile_len, min((t+1)·tile_len, E)) — the node's step tiles
leaf_t      = H64(key "misaka-palw/output/tile/v1", le32(t) ‖ bytes of tile t)
output_root = H64(key "misaka-palw/output/root/v1", borsh(OutputSpecV1) ‖ merkle_root(leaf_0 … leaf_(n−1)))
```

- **The value domain.** Admission proves that the output node's interval lies inside the kind's value
  domain (for example `[0, 255]` for pixels), so every honest lane has a canonical encoding.
- **A malformed lane.** A lane outside that domain is already a PALW-TIR-33 violation.
- **The fault.** An executor whose step tile and output tile disagree is convicted by the new
  structural fault **`TirOutputDigestMismatch { tile }`**. It needs two openings and one move, in the
  pattern of Phase F's `TirLogitsTraceMismatch`.
- **The execution root.** A tensor-output pipeline's execution root binds `output_root` where an LM
  binds `full_logits_trace_root`.

### I.3.3 Per output kind

| Kind | Consensus bytes (element encoding) | Header (`OutputSpecV1`) | Presentation, never consensus |
| --- | --- | --- | --- |
| **Text** | the committed generated token ids, `u32` LE, cut at the committed stop or end of generation (RFC-0001 §A.3 step 7, ADR-0096 D6) — as the FP commitment already carries them; **no new digest** | tokenizer (class), count | UTF-8 detokenisation, stop-string trimming, chat formatting |
| **Image** `ImageRgb8` | raw `u8`, HWC row-major, RGB, top-left origin, no alpha, no padding; values as the program emits them (no gamma conversion, no ICC) | `H`, `W`, `C = 3` | PNG/WebP/JPEG, metadata, colour profiles, thumbnails |
| **Audio** `PcmI16` | PCM `i16` LE, interleaved by channel, frames in time order | sample rate, channels, frames | WAV/FLAC/Opus/MP3 containers, resampling, loudness normalisation |
| **Embedding** `EmbeddingI32` | `i32` LE `[n, d]` row-major in the class's fixed point (`q` fractional bits); normalisation, if any, done in the program | `n`, `d`, `q`, normalised flag | float conversion (`v / 2^q`), similarity search, vector-DB formats |
| **Video** `VideoRgb8` | raw `u8` THWC (frames, then image rows) | `T`, `H`, `W`, fps as a rational | MP4/WebM/GIF |
| **Tensor** `TensorLe` | the node's dtype (`i8`/`i16`/`i32`) LE row-major | dtype, shape, `q` | — (latent-only outputs, §II.1 alternatives) |

### I.3.4 Why raw tensors, not a canonical PNG, WAV or MP4

A canonical PNG must fix each row's filter, the deflate stream (the compressor's exact choices,
which differ across implementations and versions), the chunk order and the metadata. Pinning a
compressor bit for bit is a consensus dependency on third-party code. It is the same class of problem
as trusting a JPEG decoder on the input side. The only trivially canonical PNG uses stored
(uncompressed) deflate blocks. That is the raw bytes plus a wrapper, and it adds nothing to them.
Lossy formats (JPEG, Opus, H.264) are non-canonical by nature.

So raw bytes are consensus. A gateway MAY serve a *reproducible PNG* (filter 0 on every row, stored
deflate) that anyone can derive from the raw bytes, without making it consensus. Output bytes never
ride the chain: the user's own node holds them (P1), and the court opens tiles.

---

# Part II — Class profiles

Each profile states its job body, its canonical output, how it maps onto the TIR scan and states,
what (if anything) the IR lacks, and court feasibility. Image is the detailed one. The others reuse
it and Part I.

## II.1 Image generation

### 1. ImageJob

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
| seed | any 32 bytes (`PalwGenJobV1.seed`); gateways map an integer `n` to `le64(n) ‖ 0^24` | — |
| sampler/scheduler | the class's `sampler_id = H64(key "misaka-palw/image/sampler/v1", descriptor text)`; the math is in the program, and the descriptor is the registrant's label | `SamplerNotOffered` |
| steps | an element of the class's offered set (at most 8 values, each ≤ 64) | `StepsNotOffered` |
| resolution | exactly the class's `width × height` | `ResolutionNotOffered` |
| `image_index` | any `u16`; image `k` of `n` requested images is the job with index `k` (one inference, one claim) | — |
| output spec | `ImageRgb8` | `OutputSpecNotOffered` |
| conditioning images (edit, img2img, inpaint mask, references), strength, adapters | not in v1: they need §II.4's canonical image input and a later body version; adapters follow RFC-0001 §2.10's derived-class route | `ConditioningImageNotOffered`, `AdapterNotOffered` |

A job is one image. Nothing is silently clamped, rounded, truncated or defaulted by consensus.

### 2. Deterministic noise

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

### 3. The denoise scan

#### 3.1 The loop and its mapping

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

#### 3.2 Scheduler and timestep coefficients are pinned integer tables

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

#### 3.3 CFG uses a batch axis

With true CFG every activation carries `B = 2` (conditional, unconditional):

- weights broadcast over the batch (`MatMul`'s broadcast batch dimensions);
- attention is per batch element and head (a rank-4 batched `MatMul`);
- each branch has its own text input and its own attention mask;
- `post` computes `v = v_u + Div_HAFZ(g_q · (v_c − v_u), 16)`, with any renormalisation the family
  applies (a per-token norm ratio is the library's `IntRsqrt` pattern).

CFG doubles MACs, lanes and step leaves. The cost is static: a CFG class computes both branches even
at `g = 1`, because shapes cannot change with a job value. A class without CFG is another class.

#### 3.4 Text conditioning: a separate encoder program (a stage), not a pre-scan block

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

#### 3.5 VAE decode as the final program(s)

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

#### 3.6 Attention over image tokens is inside one step

A diffusion transformer has no history. Each step attends over all `N` image tokens (and `L` text
tokens) as a **tensor axis**, not as `H`. Its reductions over the token axis are over a `Fixed` axis,
and PALW-TIR-32's H dissection does not cover them (§5).

The full score tensor `[B, heads, N_tot, N_tot]` exceeds `2^28` elements at a few thousand tokens.
The lowerer writes attention per group of heads; the node count stays well under 512 per block.
Backends never materialise scores. They MUST keep the exact **two-pass** softmax (maximum first, then
exact sums), because the single-pass online rescaling of flash attention rounds a running sum and is
not byte-identical.

#### 3.7 What `TirProgramV1` lacks — exactly

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

### 4. ImageOutput

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

### 5. Verification and court

#### 5.1 Commit points

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

#### 5.2 The step space

The step space runs stage-major. Within a stage it follows Phase F §2.5: positions in order, commit
points by slot within a position, then `Fixed` checkpoint leaves every `C`. The denoiser has no
`Hist`. Because the latent's writer is committed, **the latent at the start of step `p` is a leaf**:
the `StateWrite` at `p − 1`. A checkpoint every `C` steps adds nothing, and `C = S` minimises leaves.

#### 5.3 Bisection over steps × tiles

The existing ladder (binary or k-ary, at most 48 rounds) runs over the concatenated leaves and narrows
to the first divergent leaf, which is a (stage, step, occurrence, node, tile). Its cone is evaluated
by the generic `TirCone` arm (Phase F §2.7), with four additions:

- random inputs are recomputed from the claim's seed and `image_index` (§I.1.7);
- `External` inputs are opened as upstream leaves, or read from the job;
- job scalars come from the job;
- the latent comes from the previous step's committed `StateWrite`.

Example A (§*Compute*) has about 6.8·10^7 leaves (`≈ 2^26`), within a ladder capped at `2^32`
(`PALW_CONTEXT_LADDER_MAX_STEP_LEAVES`), which means 26 binary rounds or 9 at k = 8.

#### 5.4 Tile cones against the court ceilings

The terminal tile must stay within ≤ 16 Mi MACs, ≤ 8 committed operands and the close-byte ceiling
(≈ 16.8 MB on testnet-12). Opened committed values cost 4 bytes a lane, and weights 1 byte (int8,
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

#### 5.5 What does not fit, and the fix that needs no protocol change: committed partial reductions

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

#### 5.6 Proposed: dissection over a declared reduction axis (PALW-TIR-32′)

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

#### 5.7 Replaying the latent from checkpoints

The latent's writer is committed, so replay is a leaf read (§5.2). Solver memories are committed the
same way. A seat resumes from the last committed latent (the resume rule of ADR-0133 and Phase F
§2.6). A state checkpoint of the latent is legal but redundant. A stochastic sampler's per-step noise
is recomputed, never replayed.

#### 5.8 Who loses on malformed commitments — PALW-TIR-33 carries over

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
- **A challenger cannot manufacture any of these.** The values are opened against the executor's
  roots.

### Fidelity — usefulness, never validity

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

### Compute — a parameterised estimate

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

### Verification checklist before the profile is frozen

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

## II.2 Text generation

**Job fields.** FP Job V4, exactly as RFC-0001 §A froze it: `PalwFreePromptJobV3` fields at version
7, then `DecodeConfigV4`. That means the envelope, prompt ids, `decode_token_limit`,
`max_context_tokens`, `sampling_seed`, `temperature_q` and the decode controls. Nothing here redefines,
extends or reorders any of it.

**Canonical output.** The committed generated token ids, cut at the committed stop or end of
generation. UTF-8 is presentation (§I.3.3).

**Mapping onto the TIR scan.** An LM program is RFC-0002's `TirProgramV1` with output `Logits`, and
Phase F integrates it: one position per token, `Hist` for attention, `Fixed` for recurrences. RFC-0001
§A's pipeline selects the next token from the committed logits *outside* the program, with its
randomness as R's domain 0 (§I.1.6). A future TIR text class is a one-stage pipeline whose job is FP
Job V4. A vision-language class (§II.4) adds an image stage and needs a later FP job version that
embeds V4 unchanged plus image inputs.

**What the IR lacks.** Nothing.

**Court feasibility.** Phase F's arms (`TirCone`, H dissection, `TirLogits`,
`TirDecodeToken(Tiled)`) plus RFC-0001's per-lane two-tile refutation of the selection (I-2).

This RFC sits *under* RFC-0001 (its randomness is domain 0) and *beside* it (canonical outputs), and
changes nothing in it.

### II.2.1 Text pipelines and the vision-language job (the VLM path)

Scheduled by the coordinator on 2026-09-29, after `JobImage` (§II.4). Everything here is dormant
under `palw_gen_v1`. Items that belong to RFC-0001's live text lane are marked **[RFC-0001]**; none
is built before that lane's owner agrees.

**The text stage.** A TIR text class is a pipeline whose **output stage is its language model**. That
stage is a `Logits` program, with RFC-0002's `TirProgramV1` meaning lifted as version 2
(`Logits { node, scheme_id }`), and its trip rule is **`TextStream`** (04b §15.6, `TripRule` tag 3):

- Its positions are the text job's stream, one position per id: the prompt ids, then the generated
  ids. `T = |prompt| + |generated| − 1`, because the last generated id is never fed back.
- Its `Input(0)` at position `p` is the stream's `p`-th id. It has no token rule; the stream is the
  job's.
- It reads earlier stages through the ordinary bindings (`StageFinal`, `StageRows`); a text-only
  class has no earlier stage.
- Its `max_trip` is the class's `max_context`, Phase F's `prefill + decode − 1` bound.
- Only the output stage may be `TextStream`, and it must be a `Logits` program. A `Logits` program
  may only be that stage (NF-P9′). Every other stage keeps PALW-GEN-1's trip count fixed at
  acceptance. The text stage's trip count is the claim's committed stream length, within the job's
  limits, exactly as a Phase F text class's is.

Nothing else about text changes (PALW-GEN-10 stands):

- the job of a class with no image slot is **FP Job V4** (RFC-0001 §A), unchanged;
- selection, the decode controls and stop are RFC-0001 §A.3's. They are applied outside the program
  to the committed logits, with `R`'s domain 0 (§I.1.6);
- the court over the text stage is Phase F's: `TirCone`, H dissection, `TirLogits` and
  `TirDecodeToken`;
- a job whose `prompt_tokens + decode_token_limit − 1` exceeds `max_context` is refused at acceptance,
  as Phase F refuses it.

**The vision-language job: FP Job V5 (image inputs) [RFC-0001].** A class with image slots takes this
job, which embeds V4 unchanged:

```
PalwFreePromptJobV5 = { v4: PalwFreePromptJobV4, images: Vec<ImageInputRefV1> }   // images: 1..=16, one per slot
wire                = le16(8) ‖ (borsh(v4) without its version word) ‖ borsh(images)
fp_job_id_v5        = H64(key "misaka-palw/fp-v5/job-id/v1", le64(|bytes|) ‖ bytes)   // the whole borsh
```

The embedded job *is* an FP Job V4 (version 7, its `DecodeConfigV4` present), and V4's own rules
admit it verbatim. On the wire only its version word says 8. No V3/V4 rule admits version 8, no V4
byte string decodes as V5, and V4's wire, ids, acceptance and fingerprints do not move. Built
dormant on `rfc3/fp-v5` (2026-09-29), behind the fence `palw_fp_job_v5`, which requires
`palw_gen_v1` and `palw_fp_decode_rules` at or below it.

- **One encoding per behaviour** (open question 14, decided 2026-09-29). A class with image slots takes
  V5 only, and a class without slots takes V4 only (`JobVersionNotOffered`). `images` is never empty,
  so a text job has exactly one encoding.
- `images` satisfies PALW-GEN-11: exactly one image per slot, at the slot's size.
- **Where images travel.** Image bytes never ride a transaction, whatever `privacy_mode` says about the
  prompt ids. They travel with the capture to the panel, and a dispute opens tiles against
  `input_root`. `PublicDa` makes a V5 job's prompt ids public, not its images.
- **Price [RFC-0001] — recommendation, pending user confirmation (open question 13).** RFC-0001's D10
  pays decode leaves only (prefill is 0). A V5 job's image stages are real work, so they are priced as
  **prefill-equivalent tokens**:
  - each image slot of the class declares `token_equivalents`, a count of prompt tokens, in its
    offer. The class id covers it;
  - admission floors it at `⌈admitted per-image work / per-token work⌉`. Per-image work is every
    non-text stage's admitted job work in MAC-equivalents (ADR-0131's table), split evenly over the
    images the stage depends on. Per-token work is the text stage's admitted work for one position.
    A non-text stage that depends on no image is refused, because it would be prefill no rule
    prices;
  - a V5 job is charged the sum of its slots' counts at the job's per-token price.

  Built as pure functions (`palw_fp_v5_image_tokens_v1`, `palw_fp_v5_image_charge_v1`). Nothing in the
  lane reads them until the user confirms.
- **Numbering.** The version number and the name are RFC-0001's to assign at freeze. RFC-0001 §A.8
  keeps "V5" for an emergency fence of V4; if that fence comes first, this job takes the next free
  version. Nothing here binds a name.

**Placement by placeholder ids (no job field).** This replaces §II.4's `start` job scalar.

- The image rows enter the text stage through one `StageFinal` edge from the vision stage: rows
  `[N_img, d_text]`, every slot's rows concatenated in slot order.
- The LM program places them itself. A `Fixed` state `cursor` counts the image rows placed so far.
  At a position whose token is the class's image placeholder id (a constant of the program) **while
  `cursor < N_img`**, the input embedding is `Gather(image_rows, cursor)` and the cursor advances.
  At any other position — including **a placeholder after the `N_img`-th, which is an ordinary token,
  embedded as a token** — the input embedding is the token's embedding.
- Placement is therefore a total, deterministic function of the prompt ids. It needs no job field
  and no acceptance rule, and a court replays it like any other state. A prompt with fewer
  placeholders than image rows uses fewer rows. A placeholder past the last row is embedded as a
  token, as HF embeds a generated placeholder, and as the lowering does (`rfc3/lower` 4c25416a5,
  hf-coverage §16). The two readings agree whenever a prompt has at most `N_img` placeholders, which
  chat templates guarantee; the class's chat template (the gateway's) emits exactly `N_img`.

**The step tree and the court.** One tree, stage-major (PALW-GEN-3): the image stages' leaves, then
the text stage's.

- A dispute over a text-stage leaf is Phase F's. Its cone may read image rows, which are the vision
  stage's committed output, carried under PALW-TIR-33 (a `StageFinal` edge). A dispute over a row is
  a dispute over the vision stage's leaf, adjudicated at the first divergent leaf.
- The vision stage's own cones read image lanes from tiles proven under `input_root`
  (PALW-TIR-48).
- **As built** (`rfc3/fp-v5`, 2026-09-29, dormant; library level):
  - the tree is structural, from the programs, the layouts and the trip counts. Per position: every
    commit point in slot order, cut into its commit tile. The text stage's logits are leaves only
    where the decode consumes them. After every `C`-th position, every `Fixed` state not written in
    `post`. Keyed leaves, a Merkle root per stage bound with its count, and one step root over the
    stage roots;
  - the worker runs the stages and then the text stage through FP Job V4's decoder. The panel
    re-executes and names a changed answer, a changed stage root or an unbound root;
  - the court adjudicates any leaf of any stage from the carried leaves before it (earlier stages
    entirely), the class's params, `R`, the job's facts, edges read under PALW-TIR-33 and image
    lanes from proven tiles. The decode door holds each generated id to the V4 selection over the
    committed logits row;
  - V5 rides the lane's job type as version 8, its images after the V4 tail (the same bytes as the
    wrapper). The commitment, the payload, the claim id and the signed message carry it with no
    layout change. The V4 validators refuse it, and the V5 validator applies V4's commitment rules
    to its V4 view past `palw_fp_job_v5`.

  - **the pipeline admission and the registry** (`palw_gen_admission_v1`, dormant under
    `palw_gen_v1`): the preflight; the one step tree counted exactly in closed form — the widest job
    within the court's ladder and `max_job_step_leaves`, and `pwu_per_inference` equal to the count
    of the class's most expensive offered job; every commit point's cone at its own tile length
    against the court (a cone that reduces over the history must fit whole — the generative court
    dissects no history before court version 3); the class id; and 0‰, since no attempt lane
    exists for pipelines. The registration is `ClassRegisteredGenV1`, object tag **68** (renumbered
    from 67 on 2026-09-29: RFC-0002's second IR fence takes 67 for `DefaultAccusedTirLeaf` and
    merges first). The fold writes the class's `gen_classes` row (rooted without the class's
    bytes, delta entry 90, carriage tail `0xC2`). A V5 claim's class is the row its job names: a
    text class, its tokenizer, its slots and its stream;
  - **the params against the artifact root** (`palw_gen_artifact_v1`): the pipeline inventory is
    Phase F's per program, in program order, in one tree, each leaf named `p<k>/<name>`. A close
    carries the param leaves its cone reads with their paths; the court reads a weight only from a
    leaf proven under the class's `artifact_root`, so an executor that ran other weights is convicted
    at its first divergent leaf. **Nodes load a class from a `PALWTIR2` file**
    (`misaka-palw-tir-artifact::v2`): the pipeline's and every program's canonical bytes, the class
    as declared, the tokenizer id, and one tensor table in exactly this inventory's order (program
    by program, each program's declared params in PALWTIR1's order), 64-byte aligned, so the root
    streams from the file front to back. It is not a consensus object; the node holds a class from it
    only when its pipeline, programs, class and tokenizer are the row's and its tensors hash to the
    row's `artifact_root`. PALWTIR1 readers are unchanged and refuse it by its magic;

  - **the closes as consensus objects** (`palw_gen_close_v1`): a V5 claim's execution root binds
    the job id, the class, the step leaf count, the step root and the generated ids; `GenCone` (tag
    10) opens the leaf the ladder narrowed to (the claim's one stage-major order), `GenDecodeToken`
    (tag 11) holds an id to its committed logits row. The prompt rides only when the disputed stage
    reads it. Below `palw_gen_v1` both are dropped by name;
  - **the history dissection, composed** (RFC-0002 F7, spec 04b §9.5): a text stage's attention
    cone is dissected, not closed whole. Admission asks F7's obligations, value bound, sizing and
    window of the stage's view verbatim; `CourtGenRootClaimed` (tag **69**, renumbered from 68 with
    the registration) opens F7's own phase, whose rounds and choices are F7's objects unchanged;
    `GenDissection` (tag 12) is the bottom. Every stage is sized under the block's box-demand rules:
    below `palw_tir_fence2` the DAA-2,000 release's, under which a dissected cone with a `TopK` is
    refused by name; past it ref2's H7 row (RFC-0002's second IR fence), under which `V` covers a
    TopK tile and such a cone is admitted dissected like any other. Admission targets: tir-lower's lowered tiny LLaVA, Qwen2-VL and Qwen2.5-VL (`rfc3/lower`
    4c25416a5), each admitted with its attention dissected; LLaVA's attention leaf is argued end to
    end (an honest responder acquitted, a lie in the totals convicted wherever it hides).

  Built since: the node's worker, seat, capture and court halves (`misaka-palw-base0::gen_worker`,
  re-exported by kaspad's `palw_gen_seat`), the source input and the forced prefix (§II.2.2), and
  `PALWTIR2`. The kaspad panel wiring and the V5 lane itself (the walk admitting version 8) wait
  for the V5 lane's fence, which the user decides, and open questions 13 and 15's prices.

**Order of implementation for this path.**

1. 04b: `TextStream` and NF-P9′, and the IR's text run (a selector supplies each generated id). IR
   only, dormant.
2. Consensus, dormant under `palw_gen_v1`:
   - the class profile `Text` (a text pipeline, with or without image slots). Its canonical output is
     the committed generated ids, with no output root (§I.3.3);
   - its preflight (the output stage is the text stage, and `max_context`);
   - the placement pattern's conformance vector.
3. **[RFC-0001]** FP Job V5's wire type, id and acceptance; the image-stage price; the worker and the
   panel executing a pipeline class; the court's composition. The V4 job type lives on
   `rcore/fp-sampler`, so V5 is built on top of it, after that lane's owner agrees.

### II.2.2 Encoder–decoder text classes: the source input and the forced prefix

Scheduled by the coordinator on 2026-09-29, after the node's loops; dormant under `palw_gen_v1` and
`palw_fp_job_v5`, marked **[RFC-0001]** where it touches the lane. tir-lower lowers T5, BART, mBART,
Marian and Pegasus as two-stage pipelines (`rfc3/lower` 8addd5224, hf-coverage §17): an encoder stage
over the source text (`Fixed { n: 1 }`, the source template bound through `JobTokens` and
`JobTokenCount`), then the decoder as the text stage over `TextStream`, reading every layer's cross
keys and values through `StageFinal`. Two things the lowering had to borrow are defined here.

**(a) The source input.** A sequence-to-sequence job has two texts: the source the encoder reads and
the decoder's stream. The lowering carried the source in the job's `negative` list, the only second
list a `PipelineJob` has.

- **The binding.** `TokenSource::Source` (tag 2; `Prompt` 0 and `Negative` 1 unchanged) is the job's
  source ids. Any `TokenRule` reads it as it reads the prompt: a stage's token run, `JobTokens`,
  `JobTokenCount`. `PipelineJob` gains `source`. A pipeline that names no source encodes as before.
- **The class offer.** `max_source_tokens`: 0 exactly when no rule reads the source (a class that
  reads one offers one), and only a text class offers one (the V5 job carries it). Every rule that
  reads the source must hold the longest offered source and its template (the prompt's check). The
  class id covers it, and `source_token_floor` (below).
- **The V5 job field [RFC-0001].** `source: Option<{ token_ids_hash, tokens }>`, after `images`:
  `le16(8) ‖ borsh(v4)[2..] ‖ borsh(images) ‖ borsh(source)`. **The ids travel as an image's bytes
  do**, bound by `token_ids_hash` in the network's prompt-id form: to the worker and the panel, and in
  a close whose stage reads them, never on the chain, under either privacy mode (a `PublicDa` job
  publishes its prompt, not its source). A V5 payload's `prompt_token_ids` is therefore the prompt's
  alone, and V4's payload rules and the fold's prompt accounting read it unchanged. **One
  encoding per behaviour extends open question 14's rule:** a class that has image slots **or reads
  a source** takes V5 only, and a class with neither takes V4 only. A V5 job carries one image per
  slot (none for a class without slots) and its source exactly when the class reads one, never
  neither. Acceptance holds `tokens` to `[1, max_source_tokens]`.
- **The court.** A close carries the source ids whole exactly when the disputed stage reads them (the
  prompt's carriage rule, §II.2.1); a dispute elsewhere reveals nothing of the source.
- **Price — recommendation, pending user confirmation (open question 15, with open question 13).**
  Source tokens are priced as prompt tokens, one per id, at the job's per-token price, **and never
  below the class's `source_token_floor`**. The floor is there because the lowered encoders run one
  position over the padded source axis: their work is the whole width whatever the source's length,
  so a short source charged per id would carry most of the encoder free. The registrant declares the
  floor and the preflight holds it at or above `⌈admitted source work / per-token work⌉`, the image
  slot's rule (open question 13): every stage but the text stage is input work, split evenly over
  the inputs it depends on, images and the source alike, and a text-class stage that depends on
  neither is refused (prefill no rule prices). A job is charged `max(source.tokens,
  source_token_floor)` prompt tokens for its source. Built as a pure function; nothing in the lane
  reads it until the user confirms.

**(b) The forced decoder prefix, declared per class.** A sequence-to-sequence decoder starts from
fixed ids: `decoder_start_token_id`, and for mBART-50 the target language (`forced_bos_token_id`). The
lowering carried them as the job's prompt. They are the class's, not the job's choice, so **the class
declares them**: `forced_prompt_prefix` in its offers (the class id covers it). **They ride as the
prompt's head, and the class fixes what that head is.** This is the hf-coverage option of checking
the prompt's head, not a prefix inside `TextStream`, for one reason: the job is FP Job V4 underneath,
and V4 refuses an empty prompt (`EmptyPrompt`) and hashes, prices and discloses the prompt as it
stands. A seq2seq job's decoder prompt is otherwise empty, so a prefix the IR supplied would leave
the V4 envelope a prompt it refuses, and V4 must not move. With the prefix at the prompt's head the
stream is still `prompt ‖ generated`, and every V4 rule, the step tree and the court read it
unchanged. The class fixes the head:

- the preflight: the prefix only on a text class; every id below the text stage's `token_bound`; no
  longer than `max_prompt_tokens`;
- acceptance: where the prompt's ids ride (`PublicDa`), the prompt starts with the prefix
  (`palw_fp_v5_accept_payload_v1`: the stateless V5 rules, the class resolved, then the head);
- where they do not (`PanelDa`): the worker refuses a job whose prompt does not start with it, and a
  seat judges such a job as not the class's (it files nothing, so the claim is never licensed). The
  executor bears the consequence of running it.

**Order of implementation for this path.** The IR's `TokenSource::Source`; the class offer and the
preflight; the V5 field, its acceptance and the lane's version-8 tail; the prefix's checks; the
carriage of source ids in a close; the worker, seat and capture; a conformance test on the lowered
encoder–decoders once their vectors are exported. Built dormant on `rfc3/fp-v5` (2026-09-29) through
the worker, seat and capture, with the golden toy encoder–decoder
(`consensus-vectors/tir-v2/pipelines/toy-encdec.json`) as its vector; the conformance test on the
lowered models waits for their vectors.

### II.2.3 Evaluation pipelines: the `Decode` stage and finalized outputs (RFC-0004 §7.2)

RFC-0004's open question 8 recommends that its two additions to the pipeline format live here. They
are built dormant (`rfc4/eval`, 2026-09-29), appended to the format so that every earlier pipeline
encodes as before:

- **The `Decode` stage kind** (`TripRule` tag 4). It is the text stage's stream and trip count
  (`prompt ‖ generated`, `T = |prompt| + max(|generated|, 1) − 1`) in a stage that is **not** the
  output. An evaluation pipeline's subject stage decodes (Generate), or is given the ids
  (teacher-forced: its `generated` list is the reference), and the scoring stages after it read:
  - **what it decoded**, through the token source `Generated` (tag 3), which only a stage after the
    `Decode` stage may name;
  - **its consumed logits rows**, through `StageRows` / `StageRowCount`: position `|prompt| − 1` on,
    row `r` the one `generated[r]` came from, which are exactly the stage's committed logits leaves.

  NF-P9′ becomes: the `TextStream` stage is the output, the `Decode` stage never is, and a pipeline
  has at most one of them. The court treats a `Decode` stage as the text stage: its consumed logits
  are its leaves, and the decode door holds each id it selected (in Generate mode).
- **`FinalizedOutput { claim, stage }`** (token source tag 5). It is the generated ids of the job's
  `claim`-th finalized claim (`claim < 8`), from that claim's stream stage `stage`. It is a token
  source rather than a new binding, so a template, its padding and its count are the existing
  `JobTokens` / `JobTokenCount` bindings'. A pairwise judge reads two, and RFC-0005's `Tests` kind one.
  The job carries the ids and the chain holds their commitment. Where that commitment lives and how a
  close carries the ids is the evaluation job family's (RFC-0004 A6).
- **The key** (token source `Key`, tag 4): an evaluation item's key ids, which an exact-match stage
  compares an answer span with. RFC-0004 §7.2 names two additions and nothing else, but an exact
  match needs the key as a list, so this is a third. It is flagged there.

The scoring stages themselves (ExactMatch, RefLogLik, Judge, Pairwise) are ordinary programs of the
scoring library (`misaka_palw_tir::scoring`, vectors `consensus-vectors/tir-v2/scoring/` and
`pipelines/eval-*.json`), adjudicated as any committed leaf (04b PALW-TIR-50).

## II.3 Embedding / encoder

**Job fields.** The body is `EmbeddingBodyV1`:

- `input`: text ids (hash and count, carried as FP), or one canonical image (§II.4) or clip (§II.5);
- `pooling`, which MUST equal the class's (`last`, `mean` or `cls`);
- `dims`, one of the class's offered output widths (a truncatable class `Slice`s the vector);
- `output: EmbeddingI32`.

The seed is all zeros (`SeedNotUsed` otherwise).

**Canonical output.** `i32` LE `[n, d]` in the class's fixed point, L2-normalised in the program when
the model does it (the library's `L2Norm`).

**Mapping onto the TIR scan.**

- A causal encoder is an LM program with a `Rows` output.
- Last-token pooling is its `Final` output.
- Mean pooling is a second, single-position stage that reduces the `Rows` edge (masked `ReduceSum`, a
  `Div` by the row count, the optional normalisation).
- A bidirectional encoder is a single-position program over a padded token axis.

**What the IR lacks.** Nothing beyond Part I.

**Court feasibility.** LM cones as in Phase F. A bidirectional encoder's attention fits up to about
15,600 tokens per head. A pooling tile reads `T · tile` lanes, which fits for `T · tile ≤ 4·10^6`;
longer inputs use committed partial sums (§II.1.5.5).

This is the smallest consumer of Part I: output kinds, a pipeline and the output digest, without
randomness. That makes it the smoke-test class for the drills.

## II.4 Multimodal input (vision canonicalisation)

**Job fields.** `images: [ImageInputRefV1 { input_root, h, w }]`, exactly one per image slot of the
class.

- **Image slots.** The class declares its image slots in its offers: slot `i` is `{ h, w, tile_len }`,
  at most 16 slots, each input tile `tile_len ∈ [4, 2^16]` bytes. The class id covers the slots. A
  job carries exactly one image per slot, at the slot's size (`ImageCount`, `ImageSizeNotOffered`).
  Every slot is read (NF-P10), so no image is optional; a class that takes fewer images is another
  class, as a class at another resolution is.
- An image input is `u8` HWC RGB at its slot's size. `input_root` is §I.3.2's construction applied to
  the image's bytes: header `ImageRgb8 [h, w, 3]`, the slot's `tile_len`, index-bound tile leaves,
  keyed nodes. An input root and an output root of the same bytes are the same commitment.
- Decoding, EXIF orientation, colour management, alpha and **arbitrary-size resampling** are outside
  consensus: the gateway resizes or letterboxes to a declared size and says so.
- Image bytes are far larger than a transaction. They travel like `PanelDa` prompt ids (with the
  capture to the panel), the chain carries `input_root`, and a dispute opens tiles.

**Canonical output.** Per the consuming profile: text through FP (§II.2), or an embedding (§II.3).

**Binding** (04b §15.6, NF-P10). A stage reads image `i` through `JobImage { index: i }` (binding tag 6),
into an `External` input `i16 [h, w, 3]` whose interval contains `[0, 255]`. `i16`, because program
inputs have no `u8` and every pixel is exact in it. One image is bound at one size wherever it is
bound, and the bound images are `0 … n − 1`.

**Admission and court** (04b §15.4, §15.9, PALW-TIR-48). Admission opens an image input at **1 byte a
lane**, and counts it as an operand of its own (it is opened by tiles with their paths). The court
never holds an image. It reads an image lane only from an input tile that the parties carry, after
verifying the tile against the job's `input_root`. A tile not proven under the root is refused
evidence and convicts nobody. A lane whose tile nobody carried fails the evaluation (`Missing`).

**Mapping onto the TIR scan.** A single-position **preprocessing stage** in integer TIR:

- per-channel normalisation (`Mul`/`Add`/`Div` by pinned constants);
- an optional fixed-ratio resize as two `MatMul`s with pinned resampling matrices (filter weights
  evaluated at registration, so data);
- crop as `Slice`;
- patchify as `Reshape`/`Transpose`.

A vision encoder over patches (attention over a `Fixed` patch axis) follows, with a `Rows`/`Final`
output. The LM stage (the text stage, §II.2.1) reads that output through a `StageFinal` edge. It
places patch embeddings where the prompt's image placeholder ids are, counted by a `Fixed` cursor
(§II.2.1), so no job scalar is needed. Dynamic-resolution encoders become one class per resolution
bucket, as images are.

**What the IR lacks.** Nothing beyond Part I.

**Court feasibility.** Vision-encoder cones are the image transformer's (§II.1.5.4): a few thousand
patches fit. A resampling tile costs `tile · s` MACs for source extent `s ≤ 4,096`. Input tiles are
opened against `input_root`.

## II.5 Audio — generation and understanding

**Job fields.** For understanding: `AudioInputRefV1 { input_root, sample_rate, channels, frames }`,
PCM `i16` at the class's declared rate. Decoding and resampling stay outside consensus. For
generation: text ids (and optionally a reference clip as an input), `clip_index`, the steps or token
budget from the class's offers, and `output: PcmI16`.

**Canonical output.** PCM `i16` LE interleaved, with header (sample rate, channels, frames). The last
stage clamps to `[−32768, 32767]`.

**Mapping onto the TIR scan.**

- **Understanding.** A feature stage in integer TIR:
  - framing (`Gather` with a pinned index table);
  - the window and DFT as `MatMul`s with pinned tables;
  - power (`Mul`/`Add`) and the mel filterbank (`MatMul`);
  - the logarithm (`IntLn`) and normalisation.

  Then an encoder over frames (a `Fixed` axis) and, for speech-to-text, a text decoder that is an LM
  program reading the encoder rows through an `External` input. Cross-attention is over a `Fixed`
  axis.
- **Generation.** Either an LM over codec tokens, selected by RFC-0001 §A's rule with domain 0,
  followed by a codec-decoder stage; or latent diffusion or flow over a `[C, T]` latent
  (`AUDIO_*_NOISE_V1`, the image profile in one dimension), followed by a decoder or vocoder stage.
  Transposed convolutions are zero insertion (`Concat`) plus convolution. Periodic activations are
  range reduction plus a table.

**What the IR lacks.** Nothing beyond Part I. A codec LM with several codebooks per position emits
several logits rows per position, which needs a rows-per-position field in the FP logits scheme, a
later FP job version. That is an FP-side extension, not an IR one.

**Court feasibility.** One-dimensional convolution cones are small. Long activations (for example
30 s × 44.1 kHz × 64 channels ≈ 8.5·10^7 elements) fit the `2^28` cap. Attention over thousands of
frames fits up to about 15,600 per head, beyond which committed partials apply.

## II.6 Video generation

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

# Rules, alternatives, security and activation

## Proposed Spec text (sketch)

A new chapter `spec/palw/04c-generative-classes.md`, plus additions to 04b. Applies past `palw_gen_v1`.

**Randomness.**

- **PALW-RND-1 (the function).** Every producer, seat and court MUST compute `R` as §I.1.2–I.1.3
  define it. No other randomness MAY enter an execution.
- **PALW-RND-2 (inputs).** `seed` MUST be the job's committed seed. `step`, `position` and `lane` MUST
  be the declared coordinates. A court MUST take them from the claim's job, never from a challenger.
- **PALW-RND-3 (domains).** A domain MUST be registered with its key, layout and width. A key MUST NOT
  be reused. Domain 0 is RFC-0001 D11, unchanged.
- **PALW-RND-4 (transforms).** `Uniform` and `Normal` values MUST be the word and
  `PALW_GAUSS_Q24_V1[word]` respectively.
- **PALW-RND-5 (derived inputs).** A random input MUST NOT be committed. A court MUST recompute every
  element a cone reads.
- **PALW-RND-6 (not a lottery input).** No eligibility, ticket or beacon rule MAY read `R`.
- **PALW-RND-7 (one declaration per domain).** A pipeline MUST declare each domain at most once.

**Canonical outputs.**

- **PALW-OUT-1 (consensus bytes).** A tensor output's consensus value MUST be its output node's
  elements in the kind's canonical byte form (§I.3.3).
- **PALW-OUT-2 (domain).** Admission MUST prove the output node's interval inside the kind's value
  domain.
- **PALW-OUT-3 (digest).** A claim MUST carry `output_root` computed as §I.3.2 defines it, over tiles
  aligned with the output node's step tiles.
- **PALW-OUT-4 (consistency).** A step tile and its output tile that disagree MUST convict the
  executor (`TirOutputDigestMismatch`).
- **PALW-OUT-5 (presentation).** No encoding beyond the canonical bytes MAY be consensus.

**PALW-TIR program version 2.**

- **PALW-TIR-37 (inputs).** An input MUST be `External`, with a declared interval, or `Random`. Range
  analysis MUST use the declared interval, the word range or the table range.
- **PALW-TIR-38 (output kinds).** The output MUST be `Logits`, `Rows` or `Final`, and it MUST be a
  committed node with a committable dtype and no `H`.
- **PALW-TIR-39 (`post` effects).** For `Rows`/`Final` outputs, `post` MAY write a global `Fixed`
  state that `pre` does not write. NF-19's one-writer rule still holds, and `post` MUST NOT append to
  a history.
- **PALW-TIR-40 (replay).** The writer of a global state MUST be read from occurrence 0 or `L + 1`,
  whichever writes it.
- **PALW-TIR-41 (primitives).** Version 2 MUST declare `PRIM_SET_ID_V1`.

**Pipelines and jobs.**

- **PALW-GEN-1 (pipeline).** A pipeline class MUST run its stages in declared order, each over a trip
  count fixed at acceptance.
- **PALW-GEN-2 (edges).** An edge MUST be structural and MUST read only earlier stages. Admission MUST
  prove each edge's intervals and shapes.
- **PALW-GEN-3 (one tree).** All stages' leaves MUST form one step tree, stage-major.
- **PALW-GEN-4 (identity).** The class id MUST be `tir_pipeline_class_id_v1`.
- **PALW-GEN-5 (the job).** A generative job MUST be `PalwGenJobV1` in canonical form. Acceptance
  MUST refuse every other encoding by name and MUST NOT rewrite a job.
- **PALW-GEN-6 (seed).** The seed MUST be all zeros iff the class declares no random input.
- **PALW-GEN-7 (committed operands).** PALW-TIR-33 applies to every stage. A stage output is the
  executor's statement.
- **PALW-GEN-8 (ceilings).** Per-job MACs, leaves, peak memory and pipeline bytes MUST be within the
  network's ceilings for the profile.
- **PALW-GEN-9 (image job).** An image job MUST satisfy §II.1.1's table.
- **PALW-GEN-10 (text).** Text generation is RFC-0001 §A (FP Job V4). This chapter adds no text rule.
- **PALW-GEN-11 (job images).** A job MUST carry exactly one `ImageInputRefV1` per image slot of its
  class, at the slot's size. An image's `input_root` MUST be §I.3.2's construction over its bytes
  (`ImageRgb8 [h, w, 3]`, the slot's `tile_len`). A court MUST read image lanes only from tiles
  proven under `input_root` (PALW-TIR-48).
- **PALW-GEN-12 (the text stage).** A text pipeline's output stage MUST be a `Logits` program whose trip
  rule is `TextStream`, and no other stage may be either. Its positions MUST be the text job's stream.
  Selection, the decode controls and stop MUST be RFC-0001 §A's.
- **PALW-GEN-13 (the vision-language job).** A class with image slots MUST take FP Job V5 only, and a
  class without slots FP Job V4 only. V5 MUST embed every V4 field unchanged.

## Alternatives

| Alternative | Why not |
| --- | --- |
| One RFC per modality or concern (randomness, outputs, image, audio, …) | The point is one common layer. Separate documents invite separate wires, noises and output formats, which is the per-family release problem at the modality level |
| R as a TIR primitive (b), or as a library composite (c) | §I.1.1. (b) is a protocol upgrade for nothing. (c) gives unvetted, per-class generators and still needs input tensors |
| R keyed by the job id | §I.1.3. It gives randomness meaning to the executor-chosen `job_nonce`, breaks reproducibility across jobs, and diverges from D11 |
| Framework-compatible noise (reproduce `torch.randn` for a seed) | Not bit-exact across devices and generator types, and float Box–Muller is transcendental |
| Gaussian by in-IR Box–Muller (`IntLn` and a sine table), or by a sum of uniforms | More nodes and less exact tails; the sum of uniforms is not Gaussian in the tails. A pinned table is data |
| A pre-scan block, or a second scan axis, for the text encoder | No control flow: every position would pay for both (§3.4) |
| The denoise update outside the IR, as a protocol sampler like D11 | Arbitrary solver math would become protocol code, and the IR expresses it exactly |
| One claim per denoise step | Breaks "one inference, one claim" and multiplies claim overhead |
| **Latent-only output** (the decoder off chain; `TensorLe` output) | Viable as an interim if Phase A shows the decoder's commitment cost is prohibitive. But the user's pixels would then be presentation, unverified |
| A canonical PNG, WAV or MP4 as consensus | A dependency on a compressor (§I.3.4) |
| Multi-resolution classes by padding and masking | Always computes the maximum size, and complicates positions. Deferred; one class per size |
| Autoregressive image-token models (VQ tokens from an LM, then a VQ decoder stage) | Expressible in PALW-TIR v1 today, needing only the pipeline and output kinds. A cheaper path to on-chain images, but not the targets. A variant of the image profile |
| Float pipelines with tolerances | ADR-0053 |
| zkML | Proving cost is orders of magnitude above re-execution |
| **Lazy commitment**: commit only carry-outs and required points, and let the responder commit a block's interior on demand during a dispute (multi-level bisection) | Cuts execution-time commitment volume about tenfold, which matters for GPU speed and video. It is a new court protocol, so later work, not v1 |
| An "execution profile" object inside the class id | §I.2.1: the program already is the profile |

## Security and economic analysis

- **Randomness grinding.**
  - The executor controls no input of R.
  - A requester choosing a seed buys a different output, never a better draw.
  - A seed re-roll costs a whole execution (ADR-0072).
  - R feeds no eligibility rule (PALW-RND-6).
  - The court takes the seed from the claim (PALW-RND-2).
- **Forged noise, edges or outputs.** Noise is recomputed. An edge lie is a lie in an upstream commit
  point, caught at the first divergent leaf. Output bytes are tied to step tiles by
  `TirOutputDigestMismatch` and to their domain by PALW-TIR-33.
- **Unadjudicable pipelines (A4).** Every stage program decodes under v1 primitives. Every cone fits
  or is refused at admission, and generalised dissection is refused until its court version is armed.
  The residual risk is an interpreter defect, the RFC-0002 residual.
- **Mis-fusion on GPUs.** Online-softmax attention and float or TF32 paths are not byte-identical. A
  backend that uses them convicts itself, never the network (F-1, F-4). Integer Winograd or FFT
  convolutions are exact only if implemented exactly.
- **Ceilings and denial of service.** Images are 10^4–10^6 times an LM position. Per-profile,
  per-job ceilings (MACs, leaves, peak live bytes, pipeline bytes) belong in the fence value. Job
  parameters are bounded sets. Prompt lengths are capped. Admission stays linear in program bytes.
- **Weight (ADR-0069).** The primitive set adds nothing to certify. The V2 features (inputs, `post`
  effects, pipelines, the digest fault) need their own drilled family certificate before an image
  class can earn weight.
- **Canonical work.** Activation-by-activation `MatMul`s over a `Fixed` axis (image attention,
  cross-attention) are neither `dense_matmul` nor `attention_*` under Phase F §2.9's rules today. The
  proposal classifies them as `attention_prefill` (an open question), so they are not priced as
  elementwise work.
- **Content and privacy.** No validity rule reads a prompt's meaning or an output's content
  (PALW-PR-1). Policy is the gateway's. Because generation is reproducible, **anyone holding a job's
  prompt ids and seed can regenerate its image**. With `PublicDa` prompts the image is effectively
  public; `PanelDa` limits that to the seats (until a dispute). A user wanting unlinkable images
  should use fresh random seeds.
- **Panel economics.** Every seat that re-executes pays the full cost. The quanta and pricing of image
  claims must reflect `(1 + replicas) × MACs`.

## Compatibility and migration

- **RFC-0001 §A and the FP lane are untouched.** D11's bytes are unchanged, and its vectors are
  domain 0's.
- **PALW-TIR v1 programs and Phase F classes are unchanged.** Version 2 is admitted only past
  `palw_gen_v1`.
- **Legacy classes are untouched.**
- **`PalwGenJobV1` and the pipeline registration are appended object variants.** They are dropped by
  name before the fence and skipped by older builds under the A-2 tolerance, as Phase F's
  `ClassRegisteredTirV1` is.

## Open questions

1. **R's key:** the job's seed (recommended) or the job id.
2. **Gaussian:** a protocol-side `Normal` input kind with a `2^16`-entry table (recommended), or an
   in-program `Gather` over a class-carried table; and whether the resolution should be `2^16`.
3. **One resolution per class** (recommended for v1), or multi-resolution classes.
4. **The decoder on chain from the first class** (recommended), or latent-only output as an interim.
5. **Per-profile ceilings.** Per-job MACs and leaves for denoise and decoder stages, the per-position
   leaf cap, and peak memory.
6. **Commitment volume at GPU speed.** Accept it for testnet, and treat packed lanes for `i8`/`i16`
   commit points, a GPU-friendly leaf hash, or lazy commitment as separate work?
7. **Canonical work.** Classify activation-by-activation `MatMul`s as `attention_prefill`
   (recommended), or add a component to `PalwCanonicalWorkVectorV1`.
8. **Registration carriage.** A pipeline's programs can exceed one carrier (Phase F D3's ≈ 88,000
   bytes). Should the multi-carrier lane that Phase F deferred come with this fence?
9. **The first testnet image model:** an Example-C-sized class, and which family.
10. **Generalised dissection (§II.1.5.6):** with the first image fence, or with video.
11. **Multi-codebook audio tokens:** a later FP job version for several logits rows per position.
12. **Privacy of reproducible outputs:** is the `PublicDa` or `PanelDa` choice enough, or should image
    jobs default to `PanelDa`?
13. **The price of a V5 job's image stages** (§II.2.1) [RFC-0001]. Recommendation, **pending user
    confirmation**: prefill-equivalent tokens. Each slot declares a count, floored at
    `⌈admitted per-image work / per-token work⌉` and charged at the job's per-token price.
14. **May a class with image slots also take text-only V4 jobs?** Decided 2026-09-29: no. A class with
    slots takes V5 only, and the text-only use registers the text-only class (the same weights,
    another pipeline), so each behaviour keeps one encoding. Extended 2026-09-29 (§II.2.2): a class
    that reads a source takes V5 only as well.
15. **The price of a V5 job's source tokens** (§II.2.2) [RFC-0001]. Recommendation, **pending user
    confirmation** alongside 13: source tokens are priced as prompt tokens, at the job's per-token
    price, never below the class's `source_token_floor` (at or above `⌈admitted source work /
    per-token work⌉`: a padded encoder does its whole width for any source).

## Activation plan and order of implementation

The prerequisite is RFC-0002 Phase F: `palw_tir_v1` armed, with admission v10, the step space,
`TirCone` and H dissection. The new fence `palw_gen_v1` is Phase F D1's shape:

```
Option<PalwGenFenceV1 { activation, program_version: 2, court_version: 2, rand_set_id, output_set_id, ceilings }>
```

It is Some-only in both fingerprints, collapses to `never()`, and sits at an unused height.
`rand_set_id` hashes the domain table (§I.1.4) and the table digests. `output_set_id` hashes the
output kinds (§I.3.3).

| Order | Work | Consensus change | Exit gate | Estimate |
| --- | --- | --- | --- | --- |
| 1 | **R**: `palw_rand_v1`, the domain table, the Gaussian table generator and pin, vectors `consensus-vectors/rand-v1/` (domain 0 cross-checked against `fp-v4/`) | none by itself | two implementations agree; D11 vectors unchanged | 1–2 weeks |
| 2 | **Canonical outputs**: encoders, `output_root`, vectors `consensus-vectors/output-v1/` | none by itself | vectors; round trip to reproducible PNG/WAV | 1 week |
| 3 | **PALW-TIR program version 2** in tir/core: inputs, output kinds, `post` effects, range leaves, the `input` question in §9.4, replay; golden vectors; the second implementation | none (crate) | reference = second implementation on V2 vectors | 3–4 weeks |
| 4 | **Pipeline class**: `TirPipelineV1`, edges, one step tree, class id, admission (per-stage v10, edges, offers, output spec, per-profile ceilings), carriage | dormant fence | first-divergent-leaf property over pipelines; mutation corpus refused by name | 3–4 weeks |
| 5 | **Job and court**: `PalwGenJobV1`, acceptance refusals, random inputs in the court, `External` edges, `TirOutputDigestMismatch`, the canonical-work rule | dormant fence | court battery on tiny pipelines: planted faults at every commit-point kind, forged noise, forged edges, forged digests, out-of-interval values | 3–4 weeks |
| 6 | **Image profile**: lowerers for one transformer family, its text encoder and its VAE (reduced fixtures first); committed-partial patterns; the fidelity harness with same-noise float reference; the §II.1 checklist completed | none | freeze criteria in the RFC-0002 manner: three-way bit identity, fidelity thresholds, every cone within ceilings | 6–8 weeks (overlaps 3–5) |
| 7 | **Embedding profile**, as the smoke-test class | none | same gates, trivial cost | 1–2 weeks |
| 8 | **Drills**: devnet, then a salted t12 chain; register tiny image and embedding classes; jobs; replay; disputes (a partial-sum cone included); crossing the fence on the shipping binary | — | RFC-0002 Phase F's D-F1…D-F4 pattern | 3 weeks |
| 9 | **testnet-12**: arm `palw_gen_v1` after `palw_tir_v1`; the first Example-C-sized image class | fence | registration → panel → `Final` | — |
| 10 | **RFC-0002 Phase G**: GPU integer backend, exact two-pass attention, GPU leaf hashing; then a 4B-class distilled image class | node releases | F-4 gates per backend; the throughput report | — |
| 11 | **Multimodal input** (§II.4), then image-editing bodies. Built on `rfc3/impl` (2026-09-29), dormant: `JobImage`, the slots, the image leaf, the court's tile reading. Then the **VLM path** (§II.2.1): the text stage in the IR and the `Text` profile, dormant; FP Job V5 in RFC-0001's lane after its owner agrees | fence value; **[RFC-0001]** for V5 | — | — |
| 12 | **Audio** (§II.5) | fence value | — | — |
| 13 | **Generalised dissection** (§II.1.5.6, court version 3), then **video** (§II.6), subject to open question 6 | fence | — | — |

**Which lanes each fence opens.** RFC-0002 Phase F's decision 12 keeps the free-prompt lane closed
to IR classes: testnet-12's DAA 2,000 (`palw_tir_v1`) opens registration, the attempt lane, panels
and the court for IR classes, and nothing of the free-prompt lane. `palw_gen_v1` opens registration
(the pipeline admission), panels and the court for generative classes; a pipeline has no attempt
lane (it registers at 0‰). **Opening the free-prompt lane to IR and pipeline classes is its own
later fence.** For a pipeline class that lane is FP Job V5's (`palw_fp_job_v5`, §II.2.1): its walk
admits a V5 commitment only once the fence is armed and open question 13's price is confirmed.
Until then the walk skips every version-8 payload, whatever the height.

**Order of implementation, in one line:** R and canonical outputs, then TIR V2, then pipelines and
the court, then the image profile (with embedding as the smoke test), then drills and testnet, then
Phase G and the 4B class, then multimodal input, audio, and dissection with video. **Text stays on
RFC-0001's train throughout.** A TIR text class follows Phase F and depends on nothing here beyond
naming R's domain 0. Steps 1–8 take about 4–5 calendar months with two or three people, after Phase F.

## Decision

**2026-09-28 (user): open questions 1–12 are decided as recommended.**

| # | Question | Decision |
| --- | --- | --- |
| 1 | R's key | the job's **seed** (§I.1.3), not the job id |
| 2 | Gaussian noise | the protocol-wide `Normal` input kind over the `2^16`-entry table `PALW_GAUSS_Q24_V1` (§I.1.5) |
| 3 | resolutions | **one resolution per class** in v1 |
| 4 | the decoder | the VAE decoder is **on chain from the start**; the first classes are **≤ 512²** |
| 5 | ceilings | **per-profile** ceilings (per-job MACs and leaves, the per-position leaf cap, peak memory) |
| 6 | commitment volume | **testnet accepts it**; packed lanes, a faster leaf hash and lazy commitment are separate later work |
| 7 | canonical work | activation-by-activation `MatMul`s (image attention) count as **`attention_prefill`** |
| 8 | registration carriage | **multi-carrier** pipeline registration comes with `palw_gen_v1` |
| 9 | first image class | a **small** class, chosen after the model-card checklist (§II.1) |
| 10 | generalised dissection | **with video** (§II.6), not with the first image fence |
| 11 | multi-codebook audio | **later** (a later FP job version) |
| 12 | privacy | image jobs **default to `PanelDa`**; `PublicDa` stays available when the user chooses it |
| 13 | V5 image price | **pending user confirmation** — recommendation: prefill-equivalent tokens per slot, floored at `⌈per-image work / per-token work⌉`, at the job's per-token price |
| 14 | V4 jobs on a class with image slots | **no** (2026-09-29): a class with slots takes V5 only; likewise a class that reads a source (§II.2.2) |
| 15 | V5 source-token price | **pending user confirmation** — recommendation: priced as prompt tokens, at the job's per-token price, never below the class's floor `⌈source work / per-token work⌉` |

The rest of the RFC (Parts I and II, the program surface, the activation order) stands as written.

# RFC14 / RFC1–20 監査ソース抜粋

監査対象: `misakas-pre.zip`。以下の行番号はZIPを展開した原本の行番号です。

ZIP SHA-256: `c13be519c006ac707d2c3f2ae1dcb468b50695844121f1e4c6e0321b92e9f22e`

前提: ユーザー指定どおり、DAA 16000で関連fenceが有効になったものとして評価します。無効presetや空のarming recordは不合格理由にしません。

**方法・限界:** 静的コード読解とテストソースの確認です。本環境にはcargo/rustcがなく、Rust build/test、実ノード、実モデル32Kの実行はしていません。E09/E14/E15はテストの記述であり、今回PASSしたという意味ではありません。E16/E17の実行結果・見積りはリポジトリ内の履歴です。Pythonによる別途再計算の範囲は`rfc14_sampling_results.json`を参照してください。

## 索引

- **E01** — RFC14の条件付き保証・対象範囲: `docs/rfc/0014-panel-independent-fraud-prosecution.md` / L30–L68, L93–L150

- **E02** — 既存FP区間抽出: k=4、同一入力の決定性: `consensus/core/src/palw_fp_interval_v1.rs` / L1–L70, L95–L110

- **E03** — seatへの接続・分母拘束・取得量・FaultとCertified: `kaspad/src/palw_fp_seat.rs` / L224–L293, L311–L350, L381–L428

- **E04** — 独立全再実行とroot比較: `misaka/palw/kernel/src/seg_detect.rs` / L1–L24, L155–L195, L233–L249

- **E05** — 実ノードのkernel検証serviceとモデル保有条件: `kaspad/src/palw_kernel_verifier.rs` / L1–L36, L259–L269, L446–L472

- **E06** — 公開bondによる客観証拠提出・期限・予算・裁定: `misaka/palw/kernel/src/ledger.rs` / L2600–L2680, L2683–L2698

- **E07** — kernel有罪判定とproducer担保の実減額: `misaka/palw/kernel/src/ledger.rs` / L2764–L2859

- **E08** — kernel決済をnative bondへ反映するfold: `consensus/core/src/palw_kernel_route_fold_v1.rs` / L1103–L1127, L1142–L1208

- **E09** — 全panel共謀を外部bondが有罪にするE2Eテストのソース: `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/c4r3.rs` / L104–L160

- **E10** — legacy/TIR共通の有罪処理・権利失効: `consensus/core/src/palw_state_v2/transition_builder.rs` / L3560–L3670

- **E11** — producer floor・凍結bondのattempt拒否: `consensus/core/src/palw_state_v2.rs` / L21475–L21493

- **E12** — Gate Cとモデルtierの区別: `consensus/core/src/palw_rfc0018_v1.rs` / L136–L176, L195–L222

- **E13** — TIR owner上限・独立告発・実徴収: `consensus/core/src/palw_tir_public_fold_v1.rs` / L594–L669, L893–L976, L1040–L1074

- **E14** — G-07: 共通court枠飽和の残件を固定するテスト: `consensus/core/tests/g14_close_legacy.rs` / L533–L564

- **E15** — DA個別予算・Final保留・withholding defaultの既存テスト: `consensus/core/tests/g14_close_legacy.rs` / L485–L530

- **E16** — リポジトリ最新の残件記録。今回の実行結果ではない: `docs/design/palw/remaining-work-2026-10-16.md` / L7–L47, L54–L66

- **E17** — TIR価格・時計のリポジトリ内計算記録。今回の実測ではない: `docs/design/palw/g14-completeness-proof-2026-10-16.md` / L1050–L1096

- **E18** — RFC20: 実装未検証と設計のみの項目: `docs/rfc/0020-long-context-calibration-memory.md` / L1–L26

- **E19** — 共有challengeの偏り除去とround拘束: `misaka/palw/challenge/src/seed.rs` / L129–L156, L189–L209

- **E20** — Hash64 golden vector入力の配置: `crypto/hashes/src/hash64.rs` / L210–L220

- **E21** — Final時の報酬処理とその後の担保保持: `misaka/palw/kernel/src/ledger.rs` / L1622–L1661, L3277–L3316

- **E22** — EXEC報酬holdとconvictedの支払拒否: `misaka/palw/kernel/src/exec_reward.rs` / L19–L33, L82–L118

- **E23** — seatが発見したFaultをcourtキューへ接続する処理: `kaspad/src/palw_panel.rs` / L11738–L11786, L16393–L16415, L16418–L16436, L17019–L17054, L17644–L17679, L17701–L17756

- **E24** — TIR service: seatで発見したclaimの優先追及とowner admission: `kaspad/src/palw_tir_public_owner.rs` / L334–L385

- **E25** — RFC5のVMは不採用。未実装欠陥として数えない: `docs/rfc/0005-palw-ml-vm.md` / L1–L13

- **E26** — 32K向けTIR frame見積りのテストコード（実ノード測定ではない）: `misaka/palw/tir/exec/tests/free_prompt.rs` / L263–L338

- **E27** — 共通court枠の実際の受付拒否コード: `consensus/core/src/palw_state_v2/objects.rs` / L1986–L2015

- **E28** — DAからcourtへ移る経路の共通上限: `consensus/core/src/palw_state_v2/da_sessions.rs` / L748–L760

- **E29** — ネットワークcourt同時実行上限の算式: `consensus/core/src/palw_state_v2.rs` / L8716–L8731

- **E30** — RFC07の直接検査・全関係の被覆と裁定への接続: `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md` / L1–L4, L36–L45, L101–L103

- **E31** — 既存の検査誤差・反復・grinding会計（基礎仮定の証明ではない）: `misaka/palw/challenge/src/soundness.rs` / L1–L29, L94–L109


## E01 — RFC14の条件付き保証・対象範囲

ファイル: `docs/rfc/0014-panel-independent-fraud-prosecution.md`

ファイル SHA-256: `aa1d19eb2d255c362db45532c9b83ba11807160e8ca0a096c359648234318237`


### L30–L68

````text

    30 | * **Normative language:** MUST / MUST NOTは有効化後に必要な規則、SHOULDは理由を記録して変更できる実装方針。以下の新しい型・module名は設計用の仮名で、現行Rust型の存在を意味しない。

    31 | 

    32 | ## 0. 採用する設計

    33 | 

    34 | **後続改定のscope:** モデル自体の入手・公開は合意条件にしない。以下のG14/Dispute Completenessは、

    35 | verifierが正しい登録modelを保有/取得できた条件で、claim固有証拠から裁定する能力を指す。

    36 | モデル取得を目的とする要求は適格DA/courtとせず、全weights/反復rangeの強制公開へ拡張しない（§16、ADR177 D2/D7）。

    37 | 閉鎖時の実効検出と経済安全性は別に未立証であり、旧無条件の全量取得保証を意味しない。

    38 | 

    39 | Panelは通常時の検査・receipt集約を担う。claimの真偽をPanel多数派の信頼だけで決めない。

    40 | 不正を見つけた任意の適格な第三者が、Panelへの所属やgenesis operatorの協力を必要とせず、

    41 | 認証された公開データから不正位置を絞り込み、有界のexact courtまたは既存の

    42 | `ExecutorRefuted`へ到達できる仕組みを作る。

    43 | 

    44 | **classを報酬・work weightの対象にする前に、第三者だけでこの処罰経路を完結できる

    45 | Dispute Completenessを合意上のadmission条件にする。**

    46 | ハッシュが開くこと、producer自身がcourt evidenceを作れること、成功するcourt kernelがあることは、

    47 | 独立した第三者が証拠を取得できることの代わりにならない。

    48 | 

    49 | この条件を完全に満たした後の固定Panel撤去は[RFC-0015][R15]で別に定義する。

    50 | **Panel外の普通のpublic bondが、producerの秘密状態を使わず、public authenticated materialだけから

    51 | 不正をlocalizeして客観的convictionまで進める状態に達するまで、Panel=0を有効にしてはならない。**

    52 | 本RFCの文書作成・merge、operatorのみの成功、小geometryやliar instanceを使うcourtテストは、

    53 | この前提の達成ではない。Panel=0は検証の廃止ではなく、固定Panelからpermissionless verifierと

    54 | objective fraud proofへの移行であり、lifecycle・担保・Finalを含む別upgradeを要する。

    55 | 

    56 | 目標は次の二つに分ける。

    57 | 

    58 | 1. **検出後の保証:** 正直なverifierが、不正を示す検証可能な不一致を期限内に発見した場合、

    59 |    producerが必要なclaim固有証拠を開示すれば客観裁定へ、開示しなければ定義されたDA/default処理へ進む。

    60 |    不正claimをPanelの多数票だけで経済的Finalに逃がさない。

    61 | 2. **検出の保証:** 完全な決定的検証なら、対象範囲について不正を検出する。

    62 |    ADR0171の確率的constraint検査なら、承認済みsuiteの条件付き見逃し確率を受け入れる。

    63 |    「1人オンラインなら全ての偽claimが必ず検出される」とは書かない。

    64 | 

    65 | 「1人がいる」は、任意の小さなPC1台が無制限の全claimを裁ける意味ではない。

    66 | そのverifierは対象classの承認済み検査・局所化を実行でき、許容claim rateを処理する資源、

    67 | 必要なbond/exposureとfee、期限内のネットワーク到達性を持つ必要がある。

    68 | 計算資源、DA、chain inclusion、challenge randomnessへの仮定はPanel多数派の仮定と別に明示する。

````


### L93–L150

````text

    93 | ### 2.1 検出後の条件付き不変条件

    94 | 

    95 | claim cについて、以下が全て成立する場合を対象にする。

    96 | 

    97 | * cのclass/plan/kernelがDisputeAdmissibleで、対象の故障が承認済み検証関係に含まれる。

    98 | * 少なくとも1人の正直なverifierが開始期限までに不一致を発見し、正しいchallengeを提出する。

    99 | * chainが進行し、適格なcarrierは宣言されたinclusion bound以内に受理される。

   100 | * producerは定義されたdeadlineまでの開示義務を持ち、開示の有無を合意上判定できる。

   101 | * collision resistance、署名、commitment bindingと、適用suiteのchallenge/soundness仮定が成立する。

   102 | * planの最悪時CPU・RAM・I/O・bytes・roundsと担保条件を満たす。

   103 | 

   104 | このときMUST:

   105 | 

   106 | ~~~text

   107 | accepted mismatch pursuit

   108 |     ├─ authenticated material is served

   109 |     │      └─ bounded localization → exact verdict

   110 |     │             ├─ proven claim fraud → void + producer liability

   111 |     │             └─ claim upheld / accusation dismissed → normal lifecycle

   112 |     └─ required material is not served

   113 |            └─ objectively established DA/default → defined obligation penalty

   114 | 

   115 | unresolved accepted pursuit → economic Final is blocked within its bounded deadline

   116 | ~~~

   117 | 

   118 | 有罪判定や適格なDA defaultに至ったclaimはFinalへ進めない。

   119 | 正直なproducerへの不正なchallengeは、有界の時間と費用で棄却され、claimの時計を無期限に延長しない。

   120 | 

   121 | Panel全席が悪意でも、この処理にPanelの追加署名・投票を要求しない。

   122 | ただしPanel captureへの耐性は、BlockDAG合意・ネットワーク・carrier inclusionまで

   123 | 攻撃者が支配した状態への耐性を意味しない。

   124 | 

   125 | ### 2.2 確率的検査の残余リスク

   126 | 

   127 | ADR0171/RFC11の通常検査は条件付きの `epsilon_check` を持つ [S21]–[S22]。

   128 | 完全検証能力を持つhonest verifierがいても、そのverifierが確率的検査だけを実行する場合、

   129 | 未検出の不正がFinalする確率を0とはしない。

   130 | 

   131 | `Pr[undetected false claim]` の算定は、全constraintを対象にした承認済みsuite、

   132 | post-commit乱数、round transcript、adaptive/grinding/abortと繰返し回数を含めて行う。

   133 | `epsilon_check <= 2^-128` はRFC11の条件付き設計目標で、現行ネットワークの実測保証ではない。

   134 | Panel多数派が署名したという事実からこの暗号学的boundを導いてはならない。

   135 | 

   136 | legacyの少数step samplingは完全constraint検査ではない。

   137 | ADR0098の300-token / N=299 / k=4という過去の条件では、独立したhonest checkerが1人なら

   138 | 単一点不正の検出は4/299、約1.34%にすぎない。3人の約3.96%、5人の約6.51%を、

   139 | 悪意seatを含む署名数から読み替えてはならない [S19]。

   140 | 

   141 | ### 2.3 公開性と検出責任

   142 | 

   143 | claim discoveryは全ての適格producerを対象にし、genesis operatorのclaimも除外しない。

   144 | seat/non-seatで参照できるroot、DA unit、terminal courtを変えてはならない。

   145 | verifierが何も提出しなかったことは、「不正なし」という積極的な証明ではない。

   146 | 

   147 | 一人のhonest verifierが全claimを監視できるという運用仮定を置くなら、

   148 | classごとの最大claim rate、同時追及数、検査時間、通信量、必要担保を測定する。

   149 | 新しいbondを大量に作る攻撃でこの資源仮定が崩れるなら、claim admission/rateに合意上の上限を設ける。

   150 | ノードのローカルqueue上限だけでネットワークの安全性を主張しない。

````


## E02 — 既存FP区間抽出: k=4、同一入力の決定性

ファイル: `consensus/core/src/palw_fp_interval_v1.rs`

ファイル SHA-256: `7e3447ac9998382334bdacac1d36256f0929c3a7f028ddeffb922004606f9661`


### L1–L70

````text

     1 | //! **The seat's interval draw** (ADR-0077 Decision 8).

     2 | //!

     3 | //! A free-prompt seat no longer fetches the whole capture and hashes it. It draws `k` checkpoint

     4 | //! intervals from the claim's beacon and its own seat index — unpredictable when the commitment

     5 | //! was fixed (the beacon does not exist yet, ADR-0044 F4/F5), different per seat — asks the

     6 | //! executor for each interval's opening, replays it with the class's own kernels and compares

     7 | //! every row exactly. This module is the draw alone: a pure function of chain facts, pinned by a

     8 | //! golden vector, so that "which intervals did seat 3 have to check" is a question anyone can

     9 | //! answer from the chain and nobody can answer before the beacon lands.

    10 | //!

    11 | //! The draw is NOT a consensus object: what a seat checked before signing is the seat's own duty

    12 | //! (the receipt object and its signature are unchanged). It is pinned anyway, because a seat that

    13 | //! could pick its own intervals could pick the ones an executor knows it will pick.

    14 | 

    15 | use kaspa_hashes::Hash64;

    16 | 

    17 | fn keyed(domain: &[u8]) -> blake2b_simd::State {

    18 |     blake2b_simd::Params::new().hash_length(64).key(domain).to_state()

    19 | }

    20 | 

    21 | fn finish(state: blake2b_simd::State) -> Hash64 {

    22 |     Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))

    23 | }

    24 | 

    25 | /// `k`: how many intervals a seat opens per claim. Four, as the sampled-leaf arm it replaces

    26 | /// drew; a skipped inference is wrong at every interval and any sample catches it, a single

    27 | /// corrupted row is the court's question and the court is cheap.

    28 | pub const PALW_FP_SEAT_INTERVAL_SAMPLES_V1: u32 = 4;

    29 | 

    30 | pub const PALW_FP_INTERVAL_DRAW_DOMAIN_V1: &[u8] = b"misaka-palw/fp-v3/seat-interval-draw/v1";

    31 | 

    32 | /// The intervals seat `seat_index` of the panel bound under `beacon_block` must open for

    33 | /// `claim_id`: `k` DISTINCT indices in `0..interval_count`, in draw order. When the claim has no

    34 | /// more than `k` intervals, every interval, in order — a short job is checked whole.

    35 | ///

    36 | /// Draw `i` is `H(domain ‖ network_domain ‖ beacon_block ‖ claim_id ‖ seat_index ‖ i)` reduced

    37 | /// modulo `interval_count`; a repeat is skipped and the next draw taken, so the result never

    38 | /// depends on anything but those five inputs.

    39 | pub fn palw_fp_interval_draw_v1(

    40 |     network_domain: &Hash64,

    41 |     beacon_block: &Hash64,

    42 |     claim_id: &Hash64,

    43 |     seat_index: u8,

    44 |     k: u32,

    45 |     interval_count: u32,

    46 | ) -> Vec<u32> {

    47 |     if interval_count == 0 || k == 0 {

    48 |         return Vec::new();

    49 |     }

    50 |     if interval_count <= k {

    51 |         return (0..interval_count).collect();

    52 |     }

    53 |     let mut out: Vec<u32> = Vec::with_capacity(k as usize);

    54 |     let mut draw: u64 = 0;

    55 |     while out.len() < k as usize {

    56 |         let mut state = keyed(PALW_FP_INTERVAL_DRAW_DOMAIN_V1);

    57 |         state.update(network_domain.as_byte_slice());

    58 |         state.update(beacon_block.as_byte_slice());

    59 |         state.update(claim_id.as_byte_slice());

    60 |         state.update(&[seat_index]);

    61 |         state.update(&draw.to_le_bytes());

    62 |         let digest = finish(state);

    63 |         let word = u64::from_le_bytes(digest.as_byte_slice()[..8].try_into().expect("8 bytes"));

    64 |         let index = (word % interval_count as u64) as u32;

    65 |         if !out.contains(&index) {

    66 |             out.push(index);

    67 |         }

    68 |         draw += 1;

    69 |     }

    70 |     out

````


### L95–L110

````text

    95 |     /// Every input moves the draw: a different beacon, claim or seat is a different set.

    96 |     #[test]

    97 |     fn the_draw_is_beacon_claim_and_seat_bound() {

    98 |         let base = palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 100_000);

    99 |         assert_ne!(base, palw_fp_interval_draw_v1(&h(1), &h(9), &h(3), 0, 4, 100_000), "beacon");

   100 |         assert_ne!(base, palw_fp_interval_draw_v1(&h(1), &h(2), &h(9), 0, 4, 100_000), "claim");

   101 |         assert_ne!(base, palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 1, 4, 100_000), "seat");

   102 |         assert_ne!(base, palw_fp_interval_draw_v1(&h(9), &h(2), &h(3), 0, 4, 100_000), "network");

   103 |         assert_eq!(base, palw_fp_interval_draw_v1(&h(1), &h(2), &h(3), 0, 4, 100_000), "pure");

   104 |     }

   105 | 

   106 |     /// Frozen: the draw is a rule seats and executors both compute, so it cannot drift.

   107 |     #[test]

   108 |     fn golden_vector_is_frozen() {

   109 |         let draw = palw_fp_interval_draw_v1(&h(0x11), &h(0x22), &h(0x33), 2, 4, 1_000);

   110 |         assert_eq!(draw, vec![645, 20, 372, 227], "re-take deliberately, with a new domain suffix");

````


## E03 — seatへの接続・分母拘束・取得量・FaultとCertified

ファイル: `kaspad/src/palw_fp_seat.rs`

ファイル SHA-256: `42569694eeb7533cba6006e8c6195093ea3a7b165b33b27afaa6b069d8909fbc`


### L224–L293

````text

   224 | /// the seat then has no interval duty and falls back to the lane's other arms rather than

   225 | /// inventing one.

   226 | pub fn palw_fp_seat_draw_v1(

   227 |     backend: &dyn PalwExecutionBackendV1,

   228 |     network_domain: Hash64,

   229 |     duty: &PalwSeatDutyV2,

   230 |     counts: PalwFpChainCountsV1,

   231 | ) -> Option<PalwFpSeatDrawV1> {

   232 |     let interval_count = backend.fp_interval_count_for(counts.prompt_tokens, counts.decode_tokens_executed)?;

   233 |     if interval_count == 0 {

   234 |         return None;

   235 |     }

   236 |     let intervals = palw_fp_interval_draw_v1(

   237 |         &network_domain,

   238 |         // The beacon: the block at the claim's anchor slot, which did not exist when the

   239 |         // commitment was fixed (ADR-0044 F4/F5). This is what the executor cannot predict.

   240 |         &duty.panel_anchor,

   241 |         &duty.claim_id,

   242 |         duty.seat_index,

   243 |         PALW_FP_SEAT_INTERVAL_SAMPLES_V1,

   244 |         interval_count,

   245 |     );

   246 |     Some(PalwFpSeatDrawV1 { intervals, interval_count })

   247 | }

   248 | 

   249 | /// **The two counts a seat is allowed to build, and the ONE place they are built**

   250 | /// (ADR-0077 Decision 8, ADR-0084 Decision 1).

   251 | ///

   252 | /// The prefill is the job's declared prompt length, which `palw_fp_prompt_ids_admit_v1` has

   253 | /// already tied to the ids the seat holds. The executed count is the LENGTH OF THE ANSWER — the

   254 | /// commitment's own invariant (`output_token_ids.len() == decode_tokens_executed`), and the seat's

   255 | /// ids are bound to `claim.output_root`, a chain fact, before this is ever called. It is

   256 | /// deliberately NOT `job.decode_token_limit`: the ceiling is what the producer asked for, the

   257 | /// answer is what happened, and a claim that stopped early (`EndOfGeneration`, ADR-0074

   258 | /// Decision 7) is chain-legal.

   259 | pub fn palw_fp_seat_counts_v1(prompt_tokens: u32, committed_output_ids: usize) -> PalwFpChainCountsV1 {

   260 |     PalwFpChainCountsV1 { prompt_tokens, decode_tokens_executed: committed_output_ids.min(u32::MAX as usize) as u32 }

   261 | }

   262 | 

   263 | /// **The draw a seat is entitled to make, at the price the CHAIN paid** (ADR-0074 Decision 5,

   264 | /// ADR-0077 Decision 8).

   265 | ///

   266 | /// `context_leaves` is what the family prices the seat's own context at

   267 | /// ([`PalwExecutionBackendV1::fp_context_work_leaves_v1`]). A claim that prices its work and a

   268 | /// context that does not price it at the same number are not the same claim, and drawing anyway

   269 | /// would put the DENOMINATOR of the sample in the accused's hands — which is exactly what

   270 | /// [`PalwFpChainCountsV1`]'s own doc forbids, and what a two-call job served against a

   271 | /// fifty-million-leaf commitment buys: `interval_count == 1`, the draw is `[0]`, and interval 0

   272 | /// is the one interval such a producer actually ran.

   273 | ///

   274 | /// `None` is "this seat has no interval duty": the caller falls through to the whole-capture arms,

   275 | /// which re-price independently. It is never an accusation and never a verdict.

   276 | pub fn palw_fp_seat_draw_at_the_claims_price_v1(

   277 |     backend: &dyn PalwExecutionBackendV1,

   278 |     network_domain: Hash64,

   279 |     duty: &PalwSeatDutyV2,

   280 |     counts: PalwFpChainCountsV1,

   281 |     context_leaves: Option<u64>,

   282 | ) -> Option<PalwFpSeatDrawV1> {

   283 |     use kaspa_consensus_core::palw_backend::{palw_claim_prices_work_v1, palw_opening_is_at_the_claims_price_v1};

   284 |     if palw_claim_prices_work_v1(duty.work_leaves) {

   285 |         // A family that cannot price the context cannot answer the question, and a context whose

   286 |         // price is not the claim's answers it in the negative. Both are "no interval duty".

   287 |         let priced = context_leaves?;

   288 |         if !palw_opening_is_at_the_claims_price_v1(priced, duty.work_leaves) {

   289 |             return None;

   290 |         }

   291 |     }

   292 |     palw_fp_seat_draw_v1(backend, network_domain, duty, counts)

   293 | }

````


### L311–L350

````text

   311 | /// interval is a class constant — `interval_leaves` is that constant.

   312 | pub fn palw_fp_interval_opening_ceiling_v1(interval_leaves: u64, row_bytes: u32, step_leaf_count: u64) -> usize {

   313 |     /// The binding, the checkpoint claim, roots, indices and lengths. Fixed, and generous: a

   314 |     /// ceiling that a real opening exceeds is a seat that refuses honest evidence, and this

   315 |     /// number costs nothing to over-state by a few kilobytes.

   316 |     const OPENING_HEADER_BYTES: usize = 16 << 10;

   317 |     const DIGEST_BYTES: usize = 64;

   318 |     let depth = 64 - step_leaf_count.max(1).leading_zeros() as usize;

   319 |     let digests = interval_leaves.div_ceil(1 << misaka_palw_base0::fp_capture::PALW_BASE0_SPARSE_RETAIN_LEVEL_V1) as usize + 1;

   320 |     (row_bytes as usize)

   321 |         .saturating_add(DIGEST_BYTES.saturating_mul(digests))

   322 |         .saturating_add(DIGEST_BYTES.saturating_mul(2 * depth))

   323 |         .saturating_add(OPENING_HEADER_BYTES)

   324 | }

   325 | 

   326 | /// **The bytes a seat fetches for one CLAIM** — `k` openings and nothing else. The whole-capture

   327 | /// pull is not in this number because Decision 8 retires it on this lane.

   328 | pub fn palw_fp_seat_claim_byte_ceiling_v1(interval_leaves: u64, row_bytes: u32, step_leaf_count: u64) -> usize {

   329 |     (PALW_FP_SEAT_INTERVAL_SAMPLES_V1 as usize).saturating_mul(palw_fp_interval_opening_ceiling_v1(

   330 |         interval_leaves,

   331 |         row_bytes,

   332 |         step_leaf_count,

   333 |     ))

   334 | }

   335 | 

   336 | // ---------------------------------------------------------------------------------------------

   337 | // Decision 8: the verdict

   338 | // ---------------------------------------------------------------------------------------------

   339 | 

   340 | /// What one seat concluded after replaying the intervals it drew.

   341 | #[derive(Clone, Debug, PartialEq, Eq)]

   342 | pub enum PalwFpSeatOutcomeV1 {

   343 |     /// Every drawn interval replayed and every row matched. The seat may file `Valid`.

   344 |     Certified,

   345 |     /// A row did not match. **The seat files nothing** and opens a court at this leaf, as any

   346 |     /// bonded challenger may — it already holds the refutation's inputs. It does not accuse

   347 |     /// through a receipt, because a receipt is a quorum vote and a quorum is not a court.

   348 |     Fault { interval_index: u32, leaf_index: u64 },

   349 |     /// ADR-0086 Decision 3: a V4 opening's fault has a range for an address, not a leaf.

   350 |     FaultInRange { interval_index: u32, first_leaf_index: u64, leaf_count: u64 },

````


### L381–L428

````text

   381 | /// **Replay the drawn intervals and compare every row exactly** (ADR-0077 Decision 8, W10).

   382 | ///

   383 | /// `openings` holds every candidate payload the transport admitted for each drawn interval — up to

   384 | /// the lane's slot ceiling, because a forger who knows a live claim id and guesses a drawn index

   385 | /// can send one. Each is size-checked against `opening_ceiling` BEFORE it reaches the backend, and

   386 | /// a candidate that does not bind to the claim is skipped rather than believed: an interval is

   387 | /// answered if ANY candidate replays, and unanswered if none does.

   388 | ///

   389 | /// A `Fault` short-circuits. It is the strongest thing anybody learns here and it is still not a

   390 | /// conviction — see [`PalwFpSeatOutcomeV1::Fault`].

   391 | pub fn palw_fp_seat_verify_openings_v1(

   392 |     backend: &dyn PalwExecutionBackendV1,

   393 |     roots: PalwClaimRootsV1,

   394 |     prompt_token_ids: &[u32],

   395 |     work_leaves: u64,

   396 |     draw: &PalwFpSeatDrawV1,

   397 |     openings: &dyn Fn(u32) -> Vec<Vec<u8>>,

   398 |     opening_ceiling: usize,

   399 |     mut on_rejection: impl FnMut(u32, PalwFpOpeningRejectionV1),

   400 | ) -> PalwFpSeatOutcomeV1 {

   401 |     let mut unanswered = Vec::new();

   402 |     for index in &draw.intervals {

   403 |         let mut answered = false;

   404 |         for candidate in openings(*index) {

   405 |             if candidate.len() > opening_ceiling {

   406 |                 on_rejection(*index, PalwFpOpeningRejectionV1::Oversized);

   407 |                 continue;

   408 |             }

   409 |             match backend.verify_fp_interval_opening(&candidate, roots, *index, prompt_token_ids, work_leaves) {

   410 |                 PalwFpIntervalVerdictV1::Valid => {

   411 |                     answered = true;

   412 |                     break;

   413 |                 }

   414 |                 PalwFpIntervalVerdictV1::Fault { leaf_index } => {

   415 |                     return PalwFpSeatOutcomeV1::Fault { interval_index: *index, leaf_index };

   416 |                 }

   417 |                 PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count } => {

   418 |                     return PalwFpSeatOutcomeV1::FaultInRange { interval_index: *index, first_leaf_index, leaf_count };

   419 |                 }

   420 |                 PalwFpIntervalVerdictV1::Mismatch => on_rejection(*index, PalwFpOpeningRejectionV1::Mismatch),

   421 |                 PalwFpIntervalVerdictV1::Unverifiable => on_rejection(*index, PalwFpOpeningRejectionV1::Unverifiable),

   422 |             }

   423 |         }

   424 |         if !answered {

   425 |             unanswered.push(*index);

   426 |         }

   427 |     }

   428 |     if unanswered.is_empty() { PalwFpSeatOutcomeV1::Certified } else { PalwFpSeatOutcomeV1::NotVerified { unanswered } }

````


## E04 — 独立全再実行とroot比較

ファイル: `misaka/palw/kernel/src/seg_detect.rs`

ファイル SHA-256: `c4f700c3ea41244f6970a3b63e09e80843ede94545e99659f8fb71d216043627`


### L1–L24

````text

     1 | //! **Detection at real scale: the re-execution check (K2-TIR-v4)** — `docs/design/palw/k2-real-scale.md` §11 (SOUND's SG-06).

     2 | //!

     3 | //! The element courts bound what a prosecution costs once a fault is known (two positions of material, one filing). Finding the fault

     4 | //! is a separate question. Sampling positions and reading their material finds a one-element lie with probability `m / P` for `m`

     5 | //! sampled positions, and a whole-claim read is `P · M_pos` (19.2 TB at 9B-8k). This module is the other detector: **re-execute

     6 | //! and compare roots**.

     7 | //!

     8 | //! The reference semantics are exact integers. A verifier that re-executes a claim therefore computes every value an honest producer

     9 | //! must have committed. It needs the job's prompt (public on chain in tiles), the claim's delivered ids, the public artifact and the

    10 | //! class's program. It hashes its values into position roots and segment roots ([`segment_roots_of_position_roots_v1`]) and compares

    11 | //! them with the claim's on-chain segment roots:

    12 | //!

    13 | //! * **every root equal**: every committed value is the verifier's own (collision resistance). Only the decode relation is left, and

    14 | //!   the verifier checks it on its own logits ([`first_decode_mismatch_v1`]);

    15 | //! * **a segment differs**: the verifier descends that segment's tree with the producer's position paths. A probe is one position root

    16 | //!   and its ≤ 10 siblings: part 0 of a demand, or the stream's position-root list. The descent finds the **first** divergent position

    17 | //!   `q` with at most `⌈log2 S⌉` probes. Every position before `q` is the verifier's own, so every input of `q`'s first wrong value is

    18 | //!   right. `q` is then checked by the element courts ([`check_positions_v1`]) from the producer's material of `q − 1` and `q`, and that

    19 | //!   check returns a filing the court convicts (the completeness argument of `crate::element`).

    20 | //!

    21 | //! What the check reads: the artifact, the on-chain roots, at most `⌈log2 S⌉` position paths, and two positions of material. What it

    22 | //! costs: one re-execution of the claim, which is the producer's own work in the reference semantics. Within those it is not a sample:

    23 | //! any wrong committed value or wrong delivered id is found with certainty. The economics of who runs it, and for which claims, are

    24 | //! §11 of the design. A sublinear-read proof that would make it cheaper than re-execution is a DESIGN_GAP there.

````


### L155–L195

````text

   155 | /// Replay once to retain only position roots and the first decode mismatch. Reuse these between adaptive DA responses.

   156 | pub fn prepare_reexecution_v1(

   157 |     c: &SegClaimContextV1<'_>,

   158 |     artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,

   159 |     tokens: &[u32],

   160 | ) -> Result<OwnReplayV1, String> {

   161 |     prepare_reexecution_in(c, artifact, tokens, ReplayProfileV1::Frozen)

   162 | }

   163 | 

   164 | /// [`prepare_reexecution_v1`] for an RFC-0002 candidate trace v2 statement (every value committed v3).

   165 | pub fn prepare_model_vector_reexecution_v2(

   166 |     c: &SegClaimContextV1<'_>,

   167 |     artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,

   168 |     tokens: &[u32],

   169 | ) -> Result<OwnReplayV1, String> {

   170 |     prepare_reexecution_in(c, artifact, tokens, ReplayProfileV1::CandidateV2)

   171 | }

   172 | 

   173 | fn prepare_reexecution_in(

   174 |     c: &SegClaimContextV1<'_>,

   175 |     artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,

   176 |     tokens: &[u32],

   177 |     profile: ReplayProfileV1,

   178 | ) -> Result<OwnReplayV1, String> {

   179 |     let mut roots = Vec::with_capacity(c.positions as usize);

   180 |     let mut decode_mismatch = None;

   181 |     let modes = profile.modes(c);

   182 |     replay_values_v1(c, artifact, tokens, None, &mut |p, values| {

   183 |         let commitments = crate::seg_scope::seg_position_commitments_v1(&modes, values);

   184 |         roots.push(crate::seg::position_root_of_v1(p, &commitments));

   185 |         if c.encoder.is_none() && decode_mismatch.is_none() && p + 1 >= c.prompt_len {

   186 |             let index = (p + 1 - c.prompt_len) as usize;

   187 |             let logits = values.last().and_then(|o| o.get(c.program.logits as usize));

   188 |             if logits.and_then(|t| c.decode.select(t)) != c.generated.get(index).copied() {

   189 |                 decode_mismatch = Some(p);

   190 |             }

   191 |         }

   192 |         Ok(())

   193 |     })?;

   194 |     Ok(OwnReplayV1 { roots, decode_mismatch })

   195 | }

````


### L233–L249

````text

   233 | pub struct OwnReplayV1 {

   234 |     pub roots: Vec<Digest>,

   235 |     pub decode_mismatch: Option<u32>,

   236 | }

   237 | 

   238 | /// Replay and check material that is already public. For adaptive on-chain DA, call `prepare_reexecution_v1` once and reuse its

   239 | /// roots with `check_claim_by_reexecution_v1` after each response/default; re-running the model for every probe is unnecessary.

   240 | pub fn reexecute_claim_v1(

   241 |     c: &SegClaimContextV1<'_>,

   242 |     producer: &dyn SegMaterialV1,

   243 |     artifact: &dyn Fn(u16, Option<u16>) -> Option<Tensor>,

   244 |     tokens: &[u32],

   245 | ) -> ReexecutionCheckV1 {

   246 |     match prepare_reexecution_v1(c, artifact, tokens) {

   247 |         Ok(own) => check_claim_by_reexecution_v1(c, &own.roots, own.decode_mismatch, producer, artifact, tokens),

   248 |         Err(why) => ReexecutionCheckV1 { finding: SegFindingV1::Inconsistent(why), divergent: None, probes: 0 },

   249 |     }

````


## E05 — 実ノードのkernel検証serviceとモデル保有条件

ファイル: `kaspad/src/palw_kernel_verifier.rs`

ファイル SHA-256: `19f866bf82455f7425dc320d4988b79d2661fe0d3d93297dd3d6c15e8896dead`


### L1–L36

````text

     1 | //! **Lane G14-K: the kernel route's public verifier, inside the node's service** (G14: the producer and every Panel seat collude; one

     2 | //! public bonded verifier outside the Panel carries a covered fault to its objective outcome from public authenticated material alone).

     3 | //!

     4 | //! Until this lane the kernel route (K2-TIR v1/v2 single programs, v4 segmented, v5 encoders) convicted only through a test harness's

     5 | //! verifier; the legacy route had kaspad's common fraud filer (`palw_fraud_filer`). This is the kernel route's twin, a part of

     6 | //! [`PalwPanelService`]'s tick:

     7 | //!

     8 | //! 1. **Discover** — once a DAA it reads the route through the node's own read API (`ConsensusApi::palw_kernel_route_v1`, op 211's

     9 | //!    source) and rebuilds the ledger with the terms the fold injects (`pursuit::kernel_consumer_ledger_v1`: G14-S's court scope past

    10 | //!    `palw_provider_court_v1`, the verifier-pay terms), so every dry run judges as the next block will.

    11 | //! 2. **Re-execute independently** with the operator's own copy of the registered model (ADR-0177 D7: a verifier holds the registered

    12 | //!    model; `--palw-kernel-model`, one file a model, matched to a class only by its registered parameter commitments — a model that

    13 | //!    does not open to them is never used). A single-program claim is replayed from the ledger's public commitments alone

    14 | //!    (`pursuit::kernel_check_program_v1`: no producer value is read, nothing is ever demanded of an honest producer). A segmented claim

    15 | //!    is replayed once per process (`seg_detect::prepare_reexecution_v1`) and searched against the `Respond` parts the accepted blocks

    16 | //!    carry (`pursuit::KernelBlockMaterialV1`), its prompt from the authenticated tiles (`pursuit::kernel_tiled_prompt_v1`). A typed

    17 | //!    `Memory` claim (RFC-0004) is replayed from its public pre-state (the chain's carried post-state, or M0 from the model) by

    18 | //!    `pursuit::kernel_check_typed_v1`; pipelines, composites and retrieval snapshots need more than one model and are not checked here.

    19 | //! 3. **Pursue** — the finding is the goal of `misaka_palw_remote::pursuit::plan_kernel_pursuit_on_v1`, which returns the step the chain

    20 | //!    still needs (seal → wait → file whole or in tag-113 chunks; demand → await the answer → re-check; conviction or the producer's DA

    21 | //!    default), dry-run against a clone of the rebuilt ledger before anything is sent: a filing the court would dismiss, a demand past

    22 | //!    the scope or the horizon, is reported and never sent.

    23 | //! 4. **Send** — the step's objects (`pursuit::kernel_step_sends_v1`) are signed DETERMINISTICALLY (the service's ML-DSA-87 signer is

    24 | //!    FIPS 204's deterministic variant, `rnd = 0^32` = `PURSUIT_DETERMINISTIC_RND_V1`: a restarted service rebuilds the same bytes, the

    25 | //!    same chunk group) and queued on the court lane (the priority lane's queue, `carry_priority_v1`), which funds each carrier from the

    26 | //!    virtual UTXO set at the object's own minimum (mature, unlocked, free; chained through its change, never a deposit spent twice) and

    27 | //!    submits it to the PQ mempool, from which the production template and the real block pipeline take it.

    28 | //!

    29 | //! **Nothing persists** (G14-R's rule): the book below is a cache of the chain and of this process's replays. A restart, a reorg, an IBD

    30 | //! or a pruned import re-derives every pursuit from the tip; an object already in a block is never re-planned, one still in this node's

    31 | //! mempool (sent before a restart) is never queued twice, and one queued is re-offered only past [`PALW_KERNEL_VERIFIER_RESEND_DAA_V1`].

    32 | //!

    33 | //! **Identity arms it; the flag only supplies the model.** A node with a funded bond (`--palw-fee-outpoint`) and at least one kernel

    34 | //! model runs it past `palw_probabilistic_constraints_v1` or `palw_panel_free_v1` (RFC-0015: the verifier an OPV claim relies on;

    35 | //! no shipped preset arms either and validation refuses both); everywhere else it is a clean no-op. It treats a Panel-licensed and an

    36 | //! OptimisticPublicVerification claim alike (`Challengeable` is open; an OPV claim has no seat to leave it to). Claims this bond produced are skipped (a producer does not prosecute itself).

````


### L259–L269

````text

   259 |         // The claims worth a look: open, not ours, of a class whose registered commitments a held model opens to.

   260 |         let candidates: Vec<Digest> = ledger

   261 |             .claims

   262 |             .iter()

   263 |             .filter(|(id, row)| {

   264 |                 row.producer != me

   265 |                     && palw_kernel_claim_open_v1(&ledger, id)

   266 |                     && kernel_class_model_root_v1(&ledger, &row.class_binding_id).is_some_and(|root| book.model_for(&root).is_some())

   267 |             })

   268 |             .map(|(id, _)| *id)

   269 |             .collect();

````


### L446–L472

````text

   446 |                     (Some(_), _) => None,

   447 |                     (None, Some(p)) => Some(p.clone()),

   448 |                     (None, None) => {

   449 |                         let found = kernel_tiled_prompt_v1(objects, &row.job_id, view.job.prompt_len, &view.job.prompt_root);

   450 |                         if let Some(p) = &found {

   451 |                             book.prompts.insert(row.job_id, p.clone());

   452 |                         }

   453 |                         found

   454 |                     }

   455 |                 };

   456 |                 let tokens = kernel_segmented_tokens_v1(&view, tiled.as_deref())

   457 |                     .ok_or("the job's prompt tiles are not all on the readable chain")?;

   458 |                 let case = book.cases.entry(*claim).or_default();

   459 |                 if case.replay.is_none() {

   460 |                     let art = |j: u16, l: Option<u16>| model.tensors.get(&(j, l)).cloned();

   461 |                     case.replay = Some(misaka_palw_kernel::seg_detect::prepare_reexecution_v1(&view.context(), &art, &tokens)?);

   462 |                 }

   463 |                 let served = KernelBlockMaterialV1::from_objects(objects, claim, &view);

   464 |                 kernel_check_segmented_v1(&view, case.replay.as_ref().expect("set above"), &served, model, &tokens)

   465 |             }

   466 |             // RFC-0004 typed roots: a memory class (the only typed kind one model checks — `kernel_class_model_root_v1`).

   467 |             ClaimBodyV1::Spec(_) => {

   468 |                 let case = book.cases.entry(*claim).or_default();

   469 |                 if case.program.is_none() {

   470 |                     case.program = Some(kernel_check_typed_v1(ledger, claim, model, salt)?);

   471 |                 }

   472 |                 Ok(case.program.clone().expect("set above"))

````


## E06 — 公開bondによる客観証拠提出・期限・予算・裁定

ファイル: `misaka/palw/kernel/src/ledger.rs`

ファイル SHA-256: `00142c7591eb326e05c2f56798ea9eb6e8fc1df1b0d4814e9225e5cfd9d832fc`


### L2600–L2680

````text

  2600 |         claim: &Digest,

  2601 |         proof: &ProsecutionV1,

  2602 |         out: &mut Vec<LedgerEventV1>,

  2603 |     ) -> Result<(), KernelRefusalV1> {

  2604 |         const NAME: &str = "FileProof";

  2605 |         let rule = |why: &str| KernelRefusalV1::rule(NAME, why);

  2606 |         let mut fee = self.policy.dismissed_proof_fee;

  2607 |         match self.bonds.get(accuser) {

  2608 |             None => return Err(rule("the accuser is not a registered bond")),

  2609 |             Some(b) if b.free() < fee => return Err(rule("the accuser's free collateral does not cover the filing fee")),

  2610 |             Some(_) => {}

  2611 |         }

  2612 |         let Some(row) = self.claims.get(claim) else { return Err(rule("no such claim")) };

  2613 |         if row.convicted {

  2614 |             // RFC-0002 P0 residual: a conviction by unsealed bytes holds its bounty; an earlier sealer may contest it with its own proof.

  2615 |             if self.sealed_bounty_in_force().is_some() && self.held_bounty_v1(claim).is_some() {

  2616 |                 return self.contest_held_bounty_v1(accuser, claim, proof, out);

  2617 |             }

  2618 |             // A second filing of the same fault, in this block or after a replay: one conviction, no second slash, no fee.

  2619 |             out.push(LedgerEventV1::Duplicate { claim: *claim });

  2620 |             return Ok(());

  2621 |         }

  2622 |         let is_final = matches!(row.life.state, ClaimStateV1::Final { .. });

  2623 |         // **The claim's liability phase**: after Final, and after a pre-Final availability default (C4 F-C4R3-02: a default is never

  2624 |         // a way out of a provable fraud), the claim stays convictable until `liability_until` — even with nothing reserved (a

  2625 |         // post-Final default may have forfeited it). Past that horizon, or against a claim that ended without ever passing (timed

  2626 |         // out), no proof can change anything: the filing is REFUSED before any court runs, and no fee is charged — a TRUE proof never

  2627 |         // costs its filer the dismissal fee, and a refusal is free for everyone (it is dropped, as an unknown claim is).

  2628 |         self.proof_open(row).map_err(rule)?;

  2629 |         // ADR-0177 D2 (`crate::court_scope`): within the accuser's dismissed filings and units of this claim — refused free, before any

  2630 |         // court runs (per accuser: no other bond can spend an honest prosecutor's allowance).

  2631 |         let unit = match (&row.body, proof) {

  2632 |             (ClaimBodyV1::Segmented { .. }, ProsecutionV1::Segmented(bytes)) if self.court_scope_in_force() => {

  2633 |                 let prompt_len = self.job_view_v1(&row.job_id).map_or(0, |j| j.prompt_len);

  2634 |                 crate::element::SegFaultV1::from_bytes(bytes).ok().and_then(|f| f.position_v1(prompt_len)).map(|p| (0u8, p))

  2635 |             }

  2636 |             _ => None,

  2637 |         };

  2638 |         self.court_scope_filing_allows_v1(accuser, claim, unit).map_err(|why| rule(&why))?;

  2639 |         let row = self.claims.get(claim).expect("checked");

  2640 |         let bounds = self.bounds_of(&row.class_binding_id);

  2641 |         let cheap = bounds.and_then(|b| oversized(proof, &b));

  2642 |         let verdict = match cheap {

  2643 |             Some(why) => Err(why),

  2644 |             None => {

  2645 |                 // C4 F-C4R4-05: a filing that reserves a share of the block's court pays for that share if it is dismissed.

  2646 |                 let work = bounds.map(|b| b.max_court_work).unwrap_or(0);

  2647 |                 let scaled = self.policy.dismissal_fee_v1(work);

  2648 |                 if self.bonds.get(accuser).is_none_or(|b| b.free() < scaled) {

  2649 |                     return Err(rule("the accuser's free collateral does not cover the filing fee"));

  2650 |                 }

  2651 |                 fee = scaled;

  2652 |                 // The court is about to run: it spends the block's budget (a refusal here costs the accuser nothing).

  2653 |                 self.charge(NAME, work)?;

  2654 |                 self.adjudicate(claim, proof)

  2655 |             }

  2656 |         };

  2657 |         if let Some(unit) = unit {

  2658 |             self.court_scope_record_unit_v1(accuser, claim, unit);

  2659 |         }

  2660 |         match verdict {

  2661 |             Err(why) => {

  2662 |                 self.court_scope_record_dismissal_v1(accuser, claim);

  2663 |                 let b = self.bonds.get_mut(accuser).expect("checked");

  2664 |                 b.collateral -= fee;

  2665 |                 self.burned += fee;

  2666 |                 out.push(LedgerEventV1::ProofDismissed { claim: *claim, accuser: *accuser, why, fee });

  2667 |                 settle(out, *accuser, fee, SettlementKindV1::SlashFiling, Some(*claim));

  2668 |                 settle(out, *accuser, fee, SettlementKindV1::Burn, Some(*claim));

  2669 |             }

  2670 |             Ok(()) => {

  2671 |                 // GAP-R7: the bounty is the earliest sealer's of these exact bytes (or the filer's, if nobody sealed them earlier).

  2672 |                 let paid = self.bounty_holder(claim, accuser, proof);

  2673 |                 // M*-49: the drawn slots that sealed these bytes share up to `bounty_cap` of the bounty first.

  2674 |                 let drawn = self.drawn_sealers_v1(claim, &self.proof_sealers(claim, proof));

  2675 |                 // RFC-0002 P0 residual (`crate::sealed_bounty`): past the fence, bytes nobody had sealed have their share held.

  2676 |                 let hold = self.sealed_bounty_in_force().is_some() && !self.proof_is_sealed_v1(claim, proof);

  2677 |                 self.convict(claim, &paid, &drawn, is_final, hold, out)

  2678 |             }

  2679 |         }

  2680 |         Ok(())

````


### L2683–L2698

````text

  2683 |     /// **Can a proof against this claim still change anything?** Live (it has not ended), or in its liability phase — after Final,

  2684 |     /// or after a pre-Final availability default (C4 F-C4R3-02) — up to `liability_until`, even with nothing reserved. Past that

  2685 |     /// horizon, or for a claim that ended without ever passing (timed out), it cannot: a filing (or a seal) is refused before any

  2686 |     /// court runs and no fee is charged.

  2687 |     fn proof_open(&self, row: &ClaimRowV1) -> Result<(), &'static str> {

  2688 |         let liable = matches!(row.life.state, ClaimStateV1::Final { .. } | ClaimStateV1::Unavailable { .. });

  2689 |         if liable && row.liability_until.is_none_or(|until| self.daa > until) {

  2690 |             return Err("past the liability horizon");

  2691 |         }

  2692 |         if !liable && (row.life.state.is_terminal() || row.reserved == 0) {

  2693 |             return Err("the claim ended without passing and holds nothing");

  2694 |         }

  2695 |         Ok(())

  2696 |     }

  2697 | 

  2698 |     /// **Who is paid the bounty of a conviction by `proof`** (GAP-R7): the bond holding the EARLIEST seal of these exact proof bytes

````


## E07 — kernel有罪判定とproducer担保の実減額

ファイル: `misaka/palw/kernel/src/ledger.rs`

ファイル SHA-256: `00142c7591eb326e05c2f56798ea9eb6e8fc1df1b0d4814e9225e5cfd9d832fc`


### L2764–L2859

````text

  2764 |     fn convict(

  2765 |         &mut self,

  2766 |         claim: &Digest,

  2767 |         accuser: &Digest,

  2768 |         drawn: &[Digest],

  2769 |         post_final: bool,

  2770 |         hold: bool,

  2771 |         out: &mut Vec<LedgerEventV1>,

  2772 |     ) {

  2773 |         let daa = self.daa;

  2774 |         let producer_collateral = self.claims.get(claim).and_then(|r| self.bonds.get(&r.producer)).map_or(0, |b| b.collateral);

  2775 |         // What a pre-Final default already took of this claim's reservation (0 for any other claim: before Final a reservation

  2776 |         // changes only by a default or a conviction, and a post-Final forfeit leaves nothing to slash).

  2777 |         let row = self.claims.get_mut(claim).expect("checked");

  2778 |         // Never instruct more than the bond holds (another subsystem may have slashed it since the claim reserved).

  2779 |         let reserved = row.reserved;

  2780 |         let slashed = reserved.min(producer_collateral);

  2781 |         row.reserved = 0;

  2782 |         row.convicted = true;

  2783 |         if !post_final {

  2784 |             // Through the lifecycle: a filed dispute, then the court's conviction (Final can never follow).

  2785 |             if !matches!(row.life.state, ClaimStateV1::Disputed { .. }) {

  2786 |                 let _ = row.life.apply(ClaimEventV1::DisputeFiled { daa });

  2787 |             }

  2788 |             let _ = row.life.apply(ClaimEventV1::CourtVerdict { daa, convicted: true });

  2789 |             if !matches!(row.life.state, ClaimStateV1::Convicted { .. }) {

  2790 |                 row.life.state = ClaimStateV1::Convicted { daa };

  2791 |             }

  2792 |         }

  2793 |         let producer = row.producer;

  2794 |         if let Some(b) = self.bonds.get_mut(&producer) {

  2795 |             b.reserved = b.reserved.saturating_sub(reserved);

  2796 |             b.collateral = b.collateral.saturating_sub(slashed);

  2797 |         }

  2798 |         // **One reporter pool per claim** (C4 F-C4R4-15, ADR-0032): the bounty is the accuser's share of what THIS conviction

  2799 |         // collected, rounded down. A pre-Final default before it already paid its demanders at most the same share of the penalty,

  2800 |         // rounded down too, so across the default and the conviction the reporters together receive at most `⌊share × collected⌋`

  2801 |         // of everything collected — a coalition holding every reporter role loses ≥ 51 % of it (the 49 % share on the node,

  2802 |         // `PALW_KERNEL_REPORTER_SHARE_PERMILLE_V1`). (This replaces F-C4R3-02's "as if no default had come first" basis, which

  2803 |         // paid the penalty's share twice; the honest accuser's residual dilution is the Sybil demanders' part of the default's

  2804 |         // share — recorded for ECON.) ADR-0176 HOOK `budget-bounty`: the bounty is a slash split, not issuance, but it counts

  2805 |         // against the claim's R reservation made at `budget-accept`.

  2806 |         //

  2807 |         // **O2 (`palw_verifier_pay_v1`)**: a held default share is still in the reservation just slashed; the default itself collected

  2808 |         // (and burned) `burned_at_default`, so the one pool is the share of everything collected — the honest accuser after a

  2809 |         // self-inflicted default is paid as if no default had come first, and every reporter together still takes at most 49 %.

  2810 |         let burned_at_default = match self.held_share_v1(claim) {

  2811 |             Some((_, burned)) => {

  2812 |                 self.pool_held_share_v1(claim, out);

  2813 |                 burned

  2814 |             }

  2815 |             None => 0,

  2816 |         };

  2817 |         let basis = slashed as u128 + burned_at_default as u128;

  2818 |         let reward = ((basis * self.policy.accuser_reward_permille as u128 / 1000) as u64).min(slashed);

  2819 |         self.burned += slashed - reward;

  2820 |         out.push(LedgerEventV1::Convicted { claim: *claim, accuser: *accuser, slashed, accuser_reward: reward, post_final });

  2821 |         // M*-49: the drawn sealers of the convicting bytes share up to `bounty_cap` first (equally); the earliest sealer keeps the rest.

  2822 |         let each_drawn = match self.verifier_pay_in_force() {

  2823 |             Some(p) if !drawn.is_empty() => reward.min(p.bounty_cap) / drawn.len() as u64,

  2824 |             _ => 0,

  2825 |         };

  2826 |         let to_drawn = each_drawn * drawn.len() as u64;

  2827 |         // RFC-0002 P0 residual (`crate::sealed_bounty`): a held share is not slashed yet — it stays RESERVED on the producer's bond

  2828 |         // (it cannot leave) until the contest resolves, when it is slashed and paid in one step (every batch stays balanced).

  2829 |         let held = if hold { reward - to_drawn } else { 0 };

  2830 |         if held > 0

  2831 |             && let Some(b) = self.bonds.get_mut(&producer)

  2832 |         {

  2833 |             b.reserved += held;

  2834 |             b.collateral += held;

  2835 |         }

  2836 |         settle(out, producer, slashed - held, SettlementKindV1::SlashFraud, Some(*claim));

  2837 |         // What the bond no longer holds was not slashed: the rest of the reservation is released, so the consumer's mirror matches.

  2838 |         settle(out, producer, reserved - slashed, SettlementKindV1::ReleaseClaim, Some(*claim));

  2839 |         for d in drawn {

  2840 |             settle(out, *d, each_drawn, SettlementKindV1::AccuserReward, Some(*claim));

  2841 |         }

  2842 |         if hold {

  2843 |             self.hold_bounty_v1(claim, accuser, &producer, held, out);

  2844 |         } else {

  2845 |             settle(out, *accuser, reward - to_drawn, SettlementKindV1::AccuserReward, Some(*claim));

  2846 |         }

  2847 |         settle(out, producer, slashed - reward, SettlementKindV1::Burn, Some(*claim));

  2848 |         self.settle_demands_moot(claim, out);

  2849 |         // The demands that led here are never penalised: the bonds of every served position of the claim return now.

  2850 |         self.settle_served_demand_bonds(claim, false, out);

  2851 |         self.opv_sync_live(claim);

  2852 |         // A convicted claim takes no further filing: its proof seals are spent — unless its bounty is held, when the earlier

  2853 |         // sealers' rows are what a contest is judged against (spent when the held share is paid).

  2854 |         if !hold {

  2855 |             self.spend_proof_seals_v1(claim);

  2856 |         }

  2857 |         if post_final {

  2858 |             self.spec_on_post_final_conviction(claim);

  2859 |         }

````


## E08 — kernel決済をnative bondへ反映するfold

ファイル: `consensus/core/src/palw_kernel_route_fold_v1.rs`

ファイル SHA-256: `59bed7f88c026be6b93b202459c8c0ec0f9e20feb16281c7a4972aa9197f3273`


### L1103–L1127

````text

  1103 |             SettlementKindV1::SlashFraud

  1104 |             | SettlementKindV1::SlashDefault

  1105 |             | SettlementKindV1::SlashFiling

  1106 |             | SettlementKindV1::AdmissionFee

  1107 |             | SettlementKindV1::JobFee

  1108 |             | SettlementKindV1::PayJobEscrow

  1109 |             | SettlementKindV1::ForfeitSealDeposit

  1110 |             | SettlementKindV1::ForfeitDemandBond => {

  1111 |                 let Some(key) = key else {

  1112 |                     if strict {

  1113 |                         return Err(refused("a slash names a bond the route never saw"));

  1114 |                     }

  1115 |                     continue;

  1116 |                 };

  1117 |                 let debit = match super::native_writers::local_capacity_or_v1(builder.slash_bond(key, s.amount as u128))? {

  1118 |                     Ok(debit) => debit,

  1119 |                     Err(e) if strict => return Err(e),

  1120 |                     Err(_) => continue,

  1121 |                 };

  1122 |                 if debit != s.amount && strict {

  1123 |                     return Err(refused(format!("a slash of {} took {debit}: the synced collateral did not cover it", s.amount)));

  1124 |                 }

  1125 |                 if matches!(s.kind, SettlementKindV1::SlashFraud | SettlementKindV1::SlashDefault) {

  1126 |                     let funded = reporter_funded.entry(s.claim).or_default();

  1127 |                     *funded = funded.checked_add(debit).ok_or(PalwStateV2Error::Overflow("reporter collateral debit"))?;

````


### L1142–L1208

````text

  1142 |             SettlementKindV1::AccuserReward | SettlementKindV1::DemanderShare | SettlementKindV1::FinalReward => {

  1143 |                 let amount = if s.kind == SettlementKindV1::FinalReward {

  1144 |                     let debited = escrow_debited.entry(s.claim).or_default();

  1145 |                     let funded = s.amount.min(*debited);

  1146 |                     *debited -= funded;

  1147 |                     if funded != s.amount && strict {

  1148 |                         return Err(refused("a Final reward with no spent escrow behind it (GAP-5: nothing is issued)"));

  1149 |                     }

  1150 |                     funded

  1151 |                 } else {

  1152 |                     s.amount

  1153 |                 };

  1154 |                 if matches!(s.kind, SettlementKindV1::AccuserReward | SettlementKindV1::DemanderShare)

  1155 |                     && let Some(batch) = reporter_batch

  1156 |                 {

  1157 |                     // No recipient cap refusal is allowed to erase an objective

  1158 |                     // conviction/default. Preserve the whole funded entitlement

  1159 |                     // and let pre-object ticks redeem exact R-limited tranches.

  1160 |                     let claim = s.claim.ok_or_else(|| refused("reporter award has no terminal court subject"))?;

  1161 |                     let terminal = events.iter().any(|event| match event {

  1162 |                         LedgerEventV1::Convicted { claim: proved, accuser, accuser_reward, .. }

  1163 |                             if s.kind == SettlementKindV1::AccuserReward =>

  1164 |                         {

  1165 |                             *proved == claim && *accuser == s.bond && *accuser_reward == amount

  1166 |                         }

  1167 |                         LedgerEventV1::ProducerDefault { claim: defaulted, .. }

  1168 |                         | LedgerEventV1::PostFinalDefault { claim: defaulted, .. }

  1169 |                             if s.kind == SettlementKindV1::DemanderShare =>

  1170 |                         {

  1171 |                             *defaulted == claim

  1172 |                         }

  1173 |                         _ => false,

  1174 |                     });

  1175 |                     if !terminal {

  1176 |                         return Err(refused("reporter award has no objective terminal verdict receipt"));

  1177 |                     }

  1178 |                     let funded = reporter_funded.entry(s.claim).or_default();

  1179 |                     *funded = funded.checked_sub(amount).ok_or_else(|| refused("reporter award lacks collected collateral"))?;

  1180 |                     let bond = key.ok_or_else(|| refused("reporter award recipient has no native bond"))?;

  1181 |                     let mut hash = keyed(b"misaka-palw/bond-budget/kernel-reporter-award/v1");

  1182 |                     hash.update(batch.as_byte_slice());

  1183 |                     hash.update(&(ordinal as u64).to_le_bytes());

  1184 |                     builder.stage_reporter_recipient_v1(finish(hash), bond, now, amount, Hash64::from_bytes(claim))?;

  1185 |                     continue;

  1186 |                 }

  1187 |                 if s.kind == SettlementKindV1::FinalReward

  1188 |                     && let Some(root) = exec_root

  1189 |                 {

  1190 |                     let bond = key.ok_or_else(|| refused("an EXEC fee names an unknown bond"))?;

  1191 |                     super::palw_exec_v2_rewards_v1::add(

  1192 |                         builder,

  1193 |                         root,

  1194 |                         bond,

  1195 |                         crate::palw_work_slice_v2::PalwExecV2RewardFundV1::KernelFee,

  1196 |                         amount,

  1197 |                     )?;

  1198 |                     continue;

  1199 |                 }

  1200 |                 if s.kind == SettlementKindV1::FinalReward {

  1201 |                     let claim = s.claim.ok_or_else(|| refused("a Final fee has no claim"))?;

  1202 |                     let bond = key.ok_or_else(|| refused("a Final fee recipient has no native bond"))?;

  1203 |                     let payload =

  1204 |                         builder.state.bonds.get(&bond).ok_or_else(|| refused("a Final fee recipient is missing"))?.payout_payload;

  1205 |                     builder.consume_kernel_final_reward_v1(&claim, bond, payload, now, amount)?;

  1206 |                     // RFC-0016 (dormant): the escrow payout telemetry of the claim's model (consensus-derived, never coinbase).

  1207 |                     super::palw_rfc0016_fold_v1::rfc0016_note_escrow_payout_v1(builder, now, &claim, amount)?;

  1208 |                 }

````


## E09 — 全panel共謀を外部bondが有罪にするE2Eテストのソース

ファイル: `consensus/src/pipeline/virtual_processor/tests/g14_kernel_route_e2e/c4r3.rs`

ファイル SHA-256: `396260e5716e08c637291cc96c730a20dc3735ae73a5c3fb254109f96a698154`


### L104–L160

````text

   104 | // ---- mandatory test 3 (user): producer + whole Panel collude; an outside bonded verifier convicts from public material ----

   105 | 

   106 | /// **Mandatory test 3, Panel-licensed, node-less.** The producer runs no node: its signed seal and lying reveal reach the chain

   107 | /// through the remote relay library (one relay lies and is reported). Every interim seat signs a passing receipt. An ordinary

   108 | /// bonded outsider — no seat, no producer state, no `ServedView`, only the read API and the producer's public DA — fresh-verifies,

   109 | /// relays its proof the same node-less way, and the REAL producer bond is slashed; nothing lands on the relay or on the seats.

   110 | #[tokio::test]

   111 | async fn g14_c4r3_mandatory_3_a_relayed_lie_covered_by_every_seat_is_convicted_by_an_outsider() {

   112 |     kaspa_core::log::try_init_logger("warn");

   113 |     let mut w = World::new().await;

   114 |     let job = w.job().await;

   115 |     let before = w.net.collateral(0);

   116 |     let lie = w.relayed_claim(0, &job, true).await;

   117 |     let seats = w.seats(&lie.id);

   118 |     let seat_collateral: Vec<u64> = seats.iter().map(|s| w.net.collateral(*s)).collect();

   119 |     w.cover(&lie.id).await;

   120 |     let outsider = w.outsiders(&lie, &seats, 1)[0];

   121 |     let proof = w.prosecution(&lie.id, &lie.published(&w.fx, &[]), 0xC4);

   122 |     w.relayed_proof(outsider, &lie.id, proof).await;

   123 |     let pol = w.policy();

   124 |     assert!(w.net.ledger().claims[&lie.id].convicted, "convicted from public material alone");

   125 |     assert_eq!(w.net.collateral(0), before - pol.claim_collateral, "the real producer bond pays the whole reservation");

   126 |     assert_eq!(w.net.owed(outsider), pol.claim_collateral * u64::from(pol.accuser_reward_permille) / 1000);

   127 |     for (s, c) in seats.iter().zip(seat_collateral) {

   128 |         assert_eq!(w.net.collateral(*s), c, "the interim seats are not the route's to slash (G14 does not rest on them)");

   129 |     }

   130 |     let z = w.net.replay().await;

   131 |     w.net.assert_same(&z, "replay");

   132 | }

   133 | 

   134 | /// **Mandatory test 3, OPV (Panel = 0), node-less, before and after Final.** Two lies relayed by their producer: one is convicted

   135 | /// inside the window, the other finalizes (nobody looked) and is convicted inside the liability horizon — both by an outsider

   136 | /// built from the read API, both relayed.

   137 | #[tokio::test]

   138 | async fn g14_c4r3_mandatory_3_opv_relayed_lies_are_convicted_before_and_after_final() {

   139 |     kaspa_core::log::try_init_logger("warn");

   140 |     let mut w = World::opv().await;

   141 |     let job_a = w.job().await;

   142 |     let job_b = w.job().await;

   143 |     let before = w.net.collateral(0);

   144 |     let a = w.relayed_claim(0, &job_a, true).await;

   145 |     let b = w.relayed_claim(0, &job_b, true).await;

   146 |     let outsider = w.outsiders(&a, &[], 1)[0];

   147 |     let (policy, view_b) = opv_view(&w, &b.id);

   148 |     let proof = w.prosecution(&a.id, &a.published(&w.fx, &[]), 0xA1);

   149 |     w.relayed_proof(outsider, &a.id, proof).await;

   150 |     assert!(matches!(w.net.claim_state(&a.id), ClaimStateV1::Convicted { .. }), "{:?}", w.net.claim_state(&a.id));

   151 |     w.net.beat_to(view_b.final_floor_daa).await;

   152 |     assert!(matches!(w.net.claim_state(&b.id), ClaimStateV1::Final { .. }), "b finalized unprosecuted");

   153 |     let proof = w.prosecution(&b.id, &b.published(&w.fx, &[]), 0xB2);

   154 |     w.relayed_proof(outsider, &b.id, proof).await;

   155 |     assert!(w.net.ledger().claims[&b.id].convicted, "b convicted inside its liability horizon");

   156 |     let r = policy.economics.reservation_per_claim;

   157 |     let fee = policy.economics.admission_fee;

   158 |     assert_eq!(w.net.collateral(0), before - 2 * r - 2 * fee, "both reservations slashed from the real bond (and two admission fees)");

   159 |     let z = w.net.replay().await;

   160 |     w.net.assert_same(&z, "replay");

````


## E10 — legacy/TIR共通の有罪処理・権利失効

ファイル: `consensus/core/src/palw_state_v2/transition_builder.rs`

ファイル SHA-256: `096d88eb29ed97b22fb4d56994bb574b2afa5acf2d05018c2f2d15ae8eb4f7ae`


### L3560–L3670

````text

  3560 |     /// checkpoint courts' one-move `ExecutorGuilty`, the held-DA leaf verdicts, and the A-held

  3561 |     /// stream's held-dissection verdict site (call it with the session's challenger and its

  3562 |     /// `opened_daa`). A court DEFAULT (`PalwVoidReasonV2::CourtDefault`: an unanswered rung, a close

  3563 |     /// declaration never assembled) is NOT a verdict and never comes here.

  3564 |     ///

  3565 |     /// Below `palw_rcore_plus`: the court's own charge, byte for byte — `void_and_slash(CourtFraud)`

  3566 |     /// and, for a session, the court-time charge. Past it, one conviction: opened on the producer;

  3567 |     /// the claim acted on by phase (live → voided `CourtFraud` with S2's action and the court time;

  3568 |     /// `Final` → reversed, then S3/U3); and closed by [`Self::record_court_conviction_v1`].

  3569 |     pub(super) fn convict_by_court_verdict_v1(

  3570 |         &mut self,

  3571 |         ctx: &PalwBlockContextV2,

  3572 |         claim_id: Hash64,

  3573 |         claim: &PalwClaimStateV2,

  3574 |         challenger: PalwBondKeyV2,

  3575 |         court_opened_daa: Option<u64>,

  3576 |     ) -> Result<(), PalwStateV2Error> {

  3577 |         self.convict_by_court_verdict_as_v1(ctx, claim_id, claim, challenger, court_opened_daa, PalwVoidReasonV2::CourtFraud)

  3578 |     }

  3579 | 

  3580 |     /// [`Self::convict_by_court_verdict_v1`] recording `reason` on the claim (and, past `Final`, the

  3581 |     /// liability row): `CourtFraud` for a verdict proven against committed data,

  3582 |     /// [`PalwVoidReasonV2::CourtHeldVerdict`] for a dissection's bottom past

  3583 |     /// `palw_offence_attribution` ([`palw_court_verdict_void_reason_v1`]). Everything else — the

  3584 |     /// charge, the court time, S-4's tiers, the `CourtConviction` record and the challenger's reward —

  3585 |     /// is the funnel's, the same for both.

  3586 |     pub(super) fn convict_by_court_verdict_as_v1(

  3587 |         &mut self,

  3588 |         ctx: &PalwBlockContextV2,

  3589 |         claim_id: Hash64,

  3590 |         claim: &PalwClaimStateV2,

  3591 |         challenger: PalwBondKeyV2,

  3592 |         court_opened_daa: Option<u64>,

  3593 |         reason: PalwVoidReasonV2,

  3594 |     ) -> Result<(), PalwStateV2Error> {

  3595 |         let now = ctx.daa_score;

  3596 |         if !self.params.rcore_plus_active_at(now) {

  3597 |             self.void_and_slash(claim_id, claim, now, reason)?;

  3598 |             if let Some(opened) = court_opened_daa {

  3599 |                 self.charge_court_time_v1(claim.bond, claim.reserved, opened, now)?;

  3600 |             }

  3601 |             return Ok(());

  3602 |         }

  3603 |         let producer = claim.bond;

  3604 |         let conv = self.open_conviction_v1(&[producer], now);

  3605 |         let g = self.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);

  3606 |         let live = self.claim_is_live_before_final_v1(&claim_id);

  3607 |         let fp_first = self.post_final_fp_g_v1(&claim_id);

  3608 |         let key = palw_court_conviction_offence_id_v1(&producer.0, &claim_id);

  3609 |         let mut nominal = self.act_on_convicted_claim_as_v1(ctx, claim_id, Some(&conv), reason)?;

  3610 |         if live {

  3611 |             if let Some(opened) = court_opened_daa {

  3612 |                 nominal = nominal.saturating_add(self.charge_court_time_v1(producer, claim.reserved, opened, now)?);

  3613 |             }

  3614 |         } else {

  3615 |             let tier = self.post_final_producer_leg_v1(

  3616 |                 &conv,

  3617 |                 claim_id,

  3618 |                 producer,

  3619 |                 key,

  3620 |                 crate::palw_offence_v1::PalwOffenceKindV1::CourtConviction,

  3621 |                 g,

  3622 |                 fp_first,

  3623 |             )?;

  3624 |             nominal = nominal.saturating_add(tier);

  3625 |         }

  3626 |         // ADR-0160 (D-5): a proven verdict is intent-class; a held dissection's verdict proves the

  3627 |         // producer's filings false, not the committed execution, and keeps the tier.

  3628 |         let offence = if reason == PalwVoidReasonV2::CourtHeldVerdict {

  3629 |             crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::CourtHeldVerdict

  3630 |         } else {

  3631 |             crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1::CourtFraud

  3632 |         };

  3633 |         self.record_court_conviction_v1(&conv, claim_id, producer, challenger, nominal, offence)

  3634 |     }

  3635 | 

  3636 |     /// **§3.6 / N6 / V-2b (S-4): a proven court verdict's `CourtConviction` record (kind 6)** — once

  3637 |     /// per claim under [`palw_court_conviction_offence_id_v1`]`(producer, claim)`, root 0 and the

  3638 |     /// claim's unminted round rights forfeited BY CLAIM (V-2b), `collected` the producer's locked

  3639 |     /// debit (R-2), and the reporter reward opened for the court's challenger

  3640 |     /// ([`PalwConvictionBasisV1::CourtVerdict`]; the key is public at admission, so it takes no

  3641 |     /// commitment, V3S-03). The closing half of [`Self::convict_by_court_verdict_v1`]; a site that

  3642 |     /// charged the producer through the funnel itself calls it directly. A second verdict on the same

  3643 |     /// claim records nothing.

  3644 |     pub(super) fn record_court_conviction_v1(

  3645 |         &mut self,

  3646 |         conv: &PalwConvictionV1,

  3647 |         claim_id: Hash64,

  3648 |         producer: PalwBondKeyV2,

  3649 |         challenger: PalwBondKeyV2,

  3650 |         nominal: u128,

  3651 |         offence: crate::palw_aggregate_liability_v1::PalwConvictedOffenceV1,

  3652 |     ) -> Result<(), PalwStateV2Error> {

  3653 |         let key = palw_court_conviction_offence_id_v1(&producer.0, &claim_id);

  3654 |         if self.state.consumed_offences.contains_key(&key) {

  3655 |             return Ok(());

  3656 |         }

  3657 |         self.close_conviction_v1(

  3658 |             conv,

  3659 |             key,

  3660 |             crate::palw_offence_v1::PalwOffenceKindV1::CourtConviction,

  3661 |             producer,

  3662 |             nominal,

  3663 |             Hash64::default(),

  3664 |             claim_id,

  3665 |             &[producer],

  3666 |             &[(producer, offence)],

  3667 |         )?;

  3668 |         self.forfeit_minted_round_rights_of_claim(&claim_id)?;

  3669 |         self.open_reporter_reward(conv.now_daa, key, 0, PalwConvictionBasisV1::CourtVerdict { challenger })?;

  3670 |         Ok(())

````


## E11 — producer floor・凍結bondのattempt拒否

ファイル: `consensus/core/src/palw_state_v2.rs`

ファイル SHA-256: `ce6a025a3457b8505b0e328ebd75d447b2397256831916d6d971647b0ffd4fa7`


### L21475–L21493

````text

 21475 |     let bond_key = PalwBondKeyV2(attempt.executor_bond);

 21476 |     let bond = builder.state.bonds.get(&bond_key).ok_or(PalwStateV2Error::MissingBond(bond_key))?;

 21477 |     if let PalwBondStatusV2::Retiring { .. } = bond.status {

 21478 |         return Err(PalwStateV2Error::RetiringBond(bond_key));

 21479 |     }

 21480 |     // **ADR-0152 U2 (B-4, S-SPEC §10a): producing needs the producer floor posted.** Checked before

 21481 |     // any write, so step 4 skips a refused own attempt without restoring anything (non-fatal, like

 21482 |     // `AttemptExposureCeiling`); a merged one is skipped generically. Dormant below the fence.

 21483 |     if let Some(shortfall) = palw_bond_producer_floor_shortfall_v1(&builder.state, builder.params, &bond_key, ctx.daa_score) {

 21484 |         let floor = builder.params.min_collateral_sompi();

 21485 |         return Err(PalwStateV2Error::ProducerBelowFloor { bond: bond_key, collateral: floor.saturating_sub(shortfall), floor });

 21486 |     }

 21487 |     // **ADR-0160 lane liab (AG-3): a frozen bond produces nothing** — checked before any write, like

 21488 |     // the floor, so step 4 skips a refused own attempt (a conviction among this block's own objects

 21489 |     // can freeze the producer under it) and a merged one is skipped generically. The map is empty

 21490 |     // below `Params::palw_capacity_aggregate_liability`, so this never refuses there.

 21491 |     if crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(&builder.state, &bond_key) {

 21492 |         return Err(PalwStateV2Error::ProducerFrozen { bond: bond_key });

 21493 |     }

````


## E12 — Gate Cとモデルtierの区別

ファイル: `consensus/core/src/palw_rfc0018_v1.rs`

ファイル SHA-256: `0f5a444e975c52b8edef8c4f8e993136b29b5ce8897ac281b01a19d709864659`


### L136–L176

````text

   136 | /// **`palw_independently_work_certified_v1(claim, state) -> bool` — Gate C, the one predicate (`SecurityCertified`).**

   137 | ///

   138 | /// * Below `palw_independently_work_certified_v1` at the claim's ACCEPTANCE DAA (every claim on every shipped network, and every claim

   139 | ///   accepted before the fence on any network): `true` — the released behaviour, byte for byte.

   140 | /// * Past it: a `Voided` claim never; otherwise the claim's certification row in the state has a certifying outcome

   141 | ///   ([`crate::palw_security_certification_v1::palw_claim_security_certified_v1`]) — commit at `Final`, global-pool draw on later

   142 | ///   randomness, m-of-k positive attestations, no veto, nothing pending, the prosecution window elapsed. A Panel licence, a Panel

   143 | ///   majority, the model's own holders or `Final` alone NEVER certify. (`FailClosedV1`: never — a fixture.)

   144 | ///

   145 | /// The answer is a fact frozen in the state (the row's decided outcome), so the fold's addition at certification, every re-derivation

   146 | /// (consistency, retirement, reversal) and rule E read one answer.

   147 | pub fn palw_independently_work_certified_v1(claim_id: &Hash64, claim: &PalwClaimStateV2, state: &PalwGateCViewV1<'_>) -> bool {

   148 |     let Some(mirror) = palw_rfc0018_mirror_v1(state.params) else { return true };

   149 |     if !mirror.gate_c_governs(claim.accepted_daa) {

   150 |         return true;

   151 |     }

   152 |     if matches!(claim.phase, PalwClaimPhaseV2::Voided { .. }) {

   153 |         return false;

   154 |     }

   155 |     // (Round 5, user decision 2026-10-15: model-level review is abolished, so U-2's Probation bar has no subject and is removed —

   156 |     // this per-claim drawn certification is the one safety gate, the same for every model tier.)

   157 |     match mirror.gate_c_rule {

   158 |         PalwGateCRuleV1::SecurityCertifiedV1 => {

   159 |             crate::palw_security_certification_v1::palw_claim_security_certified_v1(state.chain, claim_id)

   160 |         }

   161 |         PalwGateCRuleV1::FailClosedV1 => false,

   162 |     }

   163 | }

   164 | 

   165 | /// **Gate C for an owning39 MINING claim** (RFC-0002 §II.14) — the same rule as [`palw_independently_work_certified_v1`] for the

   166 | /// second subject kind, inside this one module:

   167 | ///

   168 | /// * a NEW lane has no released behaviour, so where no mirror governs the claim's acceptance DAA the answer is `false` (never the

   169 | ///   legacy `true`): no private-job escrow Final, source identity or local flag earns consensus credit;

   170 | /// * otherwise the `'V'` certification row's decided outcome (m-of-k drawn positive attestations, prosecution window elapsed).

   171 | ///

   172 | /// User decision 2026-10-15 (round 5): model-level review is abolished for every model; drawn-verifier approval exists ONLY per

   173 | /// claim, here. A model's tier (genesis/annual-fork 1.0, registered 0.3) is `palw_model_tier_v1`'s, applied once in the

   174 | /// allocation weight, never by this predicate.

   175 | ///

   176 | /// Premise-based (the honest fraction of drawn verifier stake ≥ k/m), never a computation proof.

````


### L195–L222

````text

   195 | /// **Gate C's provisional-weight site** (`bounded_immature` / `live_total`): past the fence 0 — a claim is never `SecurityCertified`

   196 | /// while live (certification commits at `Final`), and `bounded_immature` is maintained incrementally, so it must read ONE number over a

   197 | /// claim's life. Consensus credit arrives at certification, through the final-weight path. Below the fence: `weight`, unchanged.

   198 | pub fn palw_gate_c_provisional_weight_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, weight: u128) -> u128 {

   199 |     if palw_gate_c_governs_v1(params, claim) { 0 } else { weight }

   200 | }

   201 | 

   202 | /// `claim.immature_contribution` through [`palw_gate_c_provisional_weight_v1`] — the one spelling every old-rule `bounded_immature`

   203 | /// site (reserve, release, void, the two re-derivations, rule E's live record) reads.

   204 | pub fn palw_claim_provisional_weight_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> u128 {

   205 |     palw_gate_c_provisional_weight_v1(params, claim, claim.immature_contribution)

   206 | }

   207 | 

   208 | /// `weight` where Gate C certifies the claim, else 0 — the form every final-weight site takes.

   209 | pub fn palw_gate_c_weight_v1(

   210 |     params: &PalwStateParamsV2,

   211 |     chain: &PalwChainStateV2,

   212 |     claim_id: &Hash64,

   213 |     claim: &PalwClaimStateV2,

   214 |     weight: u128,

   215 | ) -> u128 {

   216 |     if palw_independently_work_certified_v1(claim_id, claim, &PalwGateCViewV1::of(params, chain)) { weight } else { 0 }

   217 | }

   218 | 

   219 | /// Whether `claim` may tick the settled-anchor clock at a licence / Final site. Past the fence never there — a governed claim's

   220 | /// settled-anchor credit is ticked once, at its certification (`palw_security_certification_fold_v1`).

   221 | pub fn palw_gate_c_settles_anchor_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {

   222 |     !palw_gate_c_governs_v1(params, claim)

````


## E13 — TIR owner上限・独立告発・実徴収

ファイル: `consensus/core/src/palw_tir_public_fold_v1.rs`

ファイル SHA-256: `9092c26113c4636ee8ea7bbcc3550f1648c0134e7cbeccd59b723d42c20531ef`


### L594–L669

````text

   594 | /// **G14 R1 / G-T1: reclaim an idle owner's slot for the Open that needs it** — the idle row is retired in place (its open demand

   595 | /// closed, its turns spent, so it never demands again; its accepted answers stay rooted to the horizon) and its owner's deposit is

   596 | /// forfeited: burned through `slash_bond` (debited and recorded in the bond's `slashed`), paid to nobody — not to the Open that

   597 | /// reclaims it, so reclaiming is never a revenue. Refused (nothing changes) unless the row is idle at this block.

   598 | fn reclaim_owner_slot_v1(

   599 |     b: &mut TransitionBuilder<'_>,

   600 |     ctx: &PalwBlockContextV2,

   601 |     key: Key,

   602 |     owner: PalwBondKeyV2,

   603 | ) -> Result<(), PalwStateV2Error> {

   604 |     let Some(Row::TirAcquisition(old)) = b.state.staged_court_row_v1(&key) else {

   605 |         return Err(refused("TIR idle-slot reclamation lost the idle owner's row"));

   606 |     };

   607 |     let mut row = old.as_ref().clone();

   608 |     let deposit = row.owned.contract_v1().terms.plan.deposit_sompi;

   609 |     let mut owned = row.owned.as_ref().clone();

   610 |     owned.retire_idle_v1(ctx.daa_score).map_err(refused)?;

   611 |     row.owned = Box::new(owned).into();

   612 |     row.check_structure_v1(&key).map_err(refused)?;

   613 |     b.write_staged_row_v1(key, Some(Row::TirAcquisition(Box::new(row).into())))?;

   614 |     b.slash_bond(owner, u128::from(deposit))?;

   615 |     Ok(())

   616 | }

   617 | 

   618 | fn open_v1(

   619 |     b: &mut TransitionBuilder<'_>,

   620 |     ctx: &PalwBlockContextV2,

   621 |     frame: &Frame,

   622 |     owner: PalwBondKeyV2,

   623 | ) -> Result<(), PalwStateV2Error> {

   624 |     independent(b, frame, owner)?;

   625 |     // **K_owner (Lead default, pending user review): refuse an excess owner before ANY custodian duty is created.** The frame's

   626 |     // liable custodians answer at most `custodian_load_bound_v1` across the owners holding slots at once; each owner below pays

   627 |     // only its own rent and deposit (`owner_terms_v1`), never the producer's prepaid preparation pool.

   628 |     //

   629 |     // **G-T1 (Lead default R1, idle-slot reclamation; the cap stays K_owner = 8): an idle owner never locks an honest verifier

   630 |     // out.** At the cap, this Open reclaims the slot of the holder idle longest (no move for one native acquisition span since its

   631 |     // last progress, `owner_idle_at_v1`): that row is retired (its open demand closed, its turns spent — it can never demand again),

   632 |     // and its deposit is forfeited (burned through `slash_bond`, credited to nobody, so reclaiming pays no one). Only when every

   633 |     // holder is still making progress is the Open refused. Atomic with the Open (the caller's checkpoint).

   634 |     let claim = frame.plan_v1().original_claim;

   635 |     if b.state.staged_tir_owner_count_v1(&claim) >= Frame::MAXIMUM_OWNERS_V1 {

   636 |         let Some((idle_key, idle_owner)) = b.state.staged_tir_idle_owner_v1(&claim, ctx.daa_score) else {

   637 |             return Err(refused("TIR original frame already carries its K_owner outside owners, all still active; custodian load is bounded"));

   638 |         };

   639 |         reclaim_owner_slot_v1(b, ctx, idle_key, idle_owner)?;

   640 |     }

   641 |     let c = owner_contract(frame, owner, ctx.daa_score)?;

   642 |     let key = owner_key(frame, owner, ctx.daa_score)?;

   643 |     let allowance = rules(b)?.config.maximum_owned_rows_per_reserver;

   644 |     if b.state.staged_court_row_v1(&key).is_some()

   645 |         || b.state.staged_owner_allowance_used_v1(owner, usize::try_from(allowance).unwrap_or(usize::MAX)) >= allowance as usize

   646 |     {

   647 |         return Err(refused("TIR outside owner already opened this row or exhausts its own native row allowance"));

   648 |     }

   649 |     b.check_accuser_room(owner, u128::from(c.terms.plan.deposit_sompi), "TIR public owner", ctx.daa_score)?;

   650 |     let funded =

   651 |         c.terms.plan.rent_sompi.checked_add(c.terms.plan.deposit_sompi).ok_or(PalwStateV2Error::Overflow("TIR owner funding"))?;

   652 |     let original_source = source(b, frame)?;

   653 |     let Some(Row::TirPreparation(prep)) = b.state.staged_court_row_v1(&frame.preparation_key_v1()) else {

   654 |         return Err(refused("TIR Open lost original shared preparation"));

   655 |     };

   656 |     // G-T4: a drawn seat holding no custody grant owns exactly as an outsider does (`independent`); the owned row's own admission

   657 |     // keeps refusing an owner inside the panel it is handed, so it is handed the panel less this owner.

   658 |     let mut others = frame.custody_v1().drawn_panel_v1().clone();

   659 |     others.remove(&owner);

   660 |     let mut owned = PalwTirPublicOwnedV1::admit_native_v1(

   661 |         c,

   662 |         &original_source,

   663 |         &others,

   664 |         frame.descriptor_v1().native_config,

   665 |         frame.descriptor_v1().native_custodian_upper,

   666 |         ctx.daa_score,

   667 |         funded,

   668 |     )

   669 |     .map_err(refused)?;

````


### L893–L976

````text

   893 | /// **G14 R4/R6: one public bonded verifier convicts an ORIGINAL frame from authenticated public material alone.**

   894 | ///

   895 | /// 1. The filer is any non-liable Active above-floor bond ([`conviction_filer_v1`]); the window is the frame's own original

   896 | ///    collection door `[accepted, collect_until)` — fixed at d0, never extended — inside which the claim cannot reach `Final`

   897 | ///    (`palw_claim_final_floor_v1` floors it at `collect_until`), so no Final weight, receipt spend, escrow, Gate C credit or

   898 | ///    coinbase leg can escape before this door closes.

   899 | /// 2. The evidence is adjudicated natively ([`crate::palw_tir_objective_v1`]): the registered program filled from state, the

   900 | ///    original class/artifact/execution/trace/job pinned from the claim, the court rules and the step ladder the native ones,

   901 | ///    the proof body bounded by the producer-prepaid terminal witness ceiling. `PredicateHolds` / `OutputHolds` refuse the object

   902 | ///    (nothing changes; it is neither guilt nor a certificate of honesty). Errors (`Need`, missing material, oversize, EOF short)

   903 | ///    refuse it too — none of them is read as a finding.

   904 | /// 3. The sanction reuses the established economic funnels, never a new tariff: every credited Valid custodian's S4 (its native

   905 | ///    lock plus the S3/S4 action tier, keyed `palw_false_valid_offence_id_v2(seat, claim)`, so once only across this route, the

   906 | ///    kind-3 route and the DA route), then the producer through `convict_by_court_verdict_v1` (void a live claim `CourtFraud`,

   907 | ///    or reverse a `Final` one — which revokes its RFC-0018 Gate C weight and frontier/settlement credit by the one predicate —

   908 | ///    and record `CourtConviction` once, the filer's reporter reward, by-claim round-right forfeiture and ADR-0160's intent-class

   909 | ///    funnel). An execution-proving fault also forfeits every minted right by ROOT, so a sibling claim sharing the convicted

   910 | ///    execution (and an EXEC root's dependent slices, through `void_claim`) loses its standing too.

   911 | /// 4. Every liable bond must still hold its original exit hold (the frame's `staged_holds`), so its collateral is locked when the

   912 | ///    conviction opens and the debit is actually collectible (`conviction_collected_v1`); a liable bond without it refuses the

   913 | ///    filing instead of recording a fictitious collection.

   914 | fn apply_conviction_v1(

   915 |     b: &mut TransitionBuilder<'_>,

   916 |     ctx: &PalwBlockContextV2,

   917 |     frame: &Frame,

   918 |     filer: PalwBondKeyV2,

   919 |     evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,

   920 | ) -> Result<(), PalwStateV2Error> {

   921 |     use crate::palw_tir_objective_v1::{

   922 |         PalwTirObjectiveOutcomeV1 as Arith, PalwTirOutputFaultV1 as OutFault, PalwTirOutputOutcomeV1 as Out,

   923 |     };

   924 |     use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1 as E;

   925 |     let claim_id = frame.plan_v1().original_claim;

   926 |     let terms = frame.plan_v1().terms;

   927 |     if ctx.daa_score < terms.accepted_daa || ctx.daa_score >= terms.collect_until_daa {

   928 |         return Err(refused("TIR public conviction is outside the frame's original collection door"));

   929 |     }

   930 |     conviction_filer_v1(b, frame, filer)?;

   931 |     let claim = b.state.claim(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;

   932 |     let producer = frame.custody_v1().pledge_v1().producer;

   933 |     if claim.bond != producer || claim.execution_root != frame.original_v1().execution_root {

   934 |         return Err(refused("TIR public conviction target changes the original claim's producer or execution"));

   935 |     }

   936 |     if b.state.consumed_offences.contains_key(&palw_court_conviction_offence_id_v1(&producer.0, &claim_id))

   937 |         || matches!(

   938 |             claim.phase,

   939 |             PalwClaimPhaseV2::Voided {

   940 |                 reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtHeldVerdict | PalwVoidReasonV2::CourtDefault,

   941 |                 ..

   942 |             }

   943 |         )

   944 |     {

   945 |         // Once: the record (past R-core+, which this route's fence requires) or, defensively, the conviction's own void.

   946 |         return Err(refused("TIR original claim is already convicted; a conviction is recorded once"));

   947 |     }

   948 |     let native = rules(b)?;

   949 |     // `Some(root)`: execution-proving (forfeit by root); `None`: claim-proving only (by claim, in the funnel's record).

   950 |     let by_root = match evidence {

   951 |         E::Arithmetic { proof } => {

   952 |             match crate::palw_tir_objective_v1::adjudicate_original_v1(

   953 |                 &b.state,

   954 |                 claim_id,

   955 |                 proof,

   956 |                 &native.court,

   957 |                 frame.descriptor_v1().maximum_leaves,

   958 |                 native.prompt_form,

   959 |                 frame.plan_v1().terminal.maximum_witness_bytes,

   960 |             )

   961 |             .map_err(refused)?

   962 |             {

   963 |                 Arith::Convicts(finding) => {

   964 |                     if finding.target_v1().claim_id != claim_id || finding.target_v1().execution_root != claim.execution_root {

   965 |                         return Err(refused("TIR arithmetic finding names another claim or execution"));

   966 |                     }

   967 |                     Some(claim.execution_root)

   968 |                 }

   969 |                 Arith::PredicateHolds { .. } => {

   970 |                     return Err(refused("TIR nominated arithmetic primitive holds: no conviction, and no finding of claim honesty"));

   971 |                 }

   972 |             }

   973 |         }

   974 |         E::DecodeToken { proof } => {

   975 |             // The ORIGINAL job's own selection rule — greedy, seeded Decision 11 sampling, or the V4 pipeline — never the filer's.

   976 |             let rule = crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&frame.original_v1().job).ok_or_else(|| {

````


### L1040–L1074

````text

  1040 |     };

  1041 |     // Liable = the producer and every exact credited Valid custodian (a drawn uncredited seat is not liable and is never charged).

  1042 |     let credited: Vec<PalwBondKeyV2> = frame.original_custody_v1().credits_v1().map(|(seat, _)| *seat).collect();

  1043 |     for bond in std::iter::once(&producer).chain(credited.iter()) {

  1044 |         if !b.state.staged_bond_exit_held_v1(bond, ctx.daa_score) {

  1045 |             return Err(refused("TIR liable bond lost its original exit hold; refusing a fictitious collection"));

  1046 |         }

  1047 |     }

  1048 |     // **G14 G-T7 / G-13 (TIR side): every credited Valid seat is charged on its own native S4 lock, never skipped silently.** The

  1049 |     // shared funnel (`charge_covering_signers_v1`) skips a signer whose lock is absent; here the conviction asserts first that each

  1050 |     // credited seat whose false-Valid offence is not yet consumed still holds its lock on this claim, unexpired at this block (the

  1051 |     // grant dated it to `bond_exit_after_daa` ≥ the door's close, and the paths that could drop it before the door closes keep a

  1052 |     // frame-credited lock: `palw_tir_public_frame_keeps_lock_v1`). Same assert as the DA side (`apply_verified_control_v1`).

  1053 |     for seat in &credited {

  1054 |         let offence = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim_id);

  1055 |         if b.state.consumed_offences.contains_key(&offence) {

  1056 |             continue;

  1057 |         }

  1058 |         if !b.state.slashable_locks.get(&(*seat, claim_id)).is_some_and(|lock| lock.amount > 0 && lock.expiry_daa > ctx.daa_score) {

  1059 |             return Err(refused("TIR credited Valid seat lost its unconsumed native S4 lock before the door closed; no silent skip"));

  1060 |         }

  1061 |     }

  1062 |     // 1. Each credited Valid custodian: S4, lock first (consumed keys skip — once across every route).

  1063 |     if !credited.is_empty() {

  1064 |         let conv = b.open_conviction_v1(&credited, ctx.daa_score);

  1065 |         let g = b.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);

  1066 |         b.charge_covering_signers_v1(&conv, claim_id, &credited, g)?;

  1067 |     }

  1068 |     // 2. The producer, through the one funnel for a proven verdict (void / Final reversal, Gate C revocation, CourtConviction).

  1069 |     b.convict_by_court_verdict_v1(ctx, claim_id, &claim, filer, None)?;

  1070 |     // 3. An execution-proving fault forfeits by ROOT (siblings sharing the convicted execution).

  1071 |     if let Some(root) = by_root {

  1072 |         b.forfeit_minted_round_rights(&root)?;

  1073 |     }

  1074 |     Ok(())

````


## E14 — G-07: 共通court枠飽和の残件を固定するテスト

ファイル: `consensus/core/tests/g14_close_legacy.rs`

ファイル SHA-256: `06f4374d4403cea9a89609426e5d9c9940ff9be8cacb852708ae0816d885154a`


### L533–L564

````text

   533 | /// **G-07's residual, pinned (NOT closed):** the network's court-session cap (`palw_max_concurrent_court_sessions_v1`: the rung window

   534 | /// times the one court move a block admits) is shared. A coalition of reservers fills it with decoy dissections on a claim of its own;

   535 | /// the outsider's dissection on the lying claim is then refused for capacity — while its reservation still holds that claim's `Final`.

   536 | /// A per-bond budget or a partition does not stop a multi-bond coalition, and lifting the cap breaks the slot guarantee that keeps an

   537 | /// honest mover from defaulting; closing it needs a measured per-block court budget or a priced slot (audit appendix C).

   538 | #[test]

   539 | fn g07_residual_the_network_court_cap_is_shared() {

   540 |     let probe = bundle(&palw_t12_launch_params_v1()).state;

   541 |     let cap = palw_max_concurrent_court_sessions_v1(probe.turn_deadline_daa());

   542 |     eprintln!("[G-07] testnet-12 court-session cap: {cap} (turn_deadline_daa {})", probe.turn_deadline_daa());

   543 |     let (mut c, row, seats) = held_chain(true, cap);

   544 |     // The lying claim: reserved by the outsider and held past its floor (no court is open, so no rung clock runs).

   545 |     let (target, licensed_daa) = licensed(&mut c, &row, &seats, 3);

   546 |     c.step(&[reservation(&c.s, target, bond_key(OUTSIDER))], Rule::T12).expect("the outsider reserves the lying claim");

   547 |     c.skip_to(licensed_daa + c.sp.window_challenge_at(licensed_daa) + 5);

   548 |     assert!(matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "held: no Final, no reward");

   549 |     // The coalition's own claim; one decoy opener, then reservers stacking dissections on it up to the network cap (inside one rung

   550 |     // window: the passive responder's sessions would otherwise default — a live coalition plays both sides and keeps them open).

   551 |     let (decoy, _) = licensed(&mut c, &row, &seats, 2);

   552 |     let sybils: Vec<PalwBondKeyV2> = (0..cap - 1).map(|n| bond_key(SYBIL_BASE + n)).collect();

   553 |     let reservations: Vec<PalwConsensusObjectV2> = sybils.iter().map(|sybil| reservation(&c.s, decoy, *sybil)).collect();

   554 |     c.step(&reservations, Rule::T12).expect("the Sybils reserve the decoy claim");

   555 |     open_by(&mut c, &row, decoy, bond_key(BYSTANDER), 1).expect("the first decoy");

   556 |     let openings: Vec<PalwConsensusObjectV2> = sybils.iter().map(|sybil| accusation_by(&c, &row, decoy, *sybil, 1)).collect();

   557 |     c.step(&openings, Rule::Opening).expect("every Sybil reserver's decoy dissection");

   558 |     assert_eq!(c.s.court_sessions_iter().count() as u64, cap, "the network's court sessions are at the cap");

   559 |     let full = open_by(&mut c, &row, target, bond_key(OUTSIDER), 1).expect_err("the cap is full");

   560 |     assert!(matches!(full, PalwStateV2Error::CourtSessionsAtCapacity { .. }), "{full:?}");

   561 |     // The reservation still holds the lying claim — the residual is a delay bounded by its hard deadline, not a payout.

   562 |     assert!(matches!(c.s.claim(&target).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));

   563 |     assert!(c.s.legacy_dispute_reservation_v1(&target, &bond_key(OUTSIDER)).is_some());

   564 | }

````


## E15 — DA個別予算・Final保留・withholding defaultの既存テスト

ファイル: `consensus/core/tests/g14_close_legacy.rs`

ファイル SHA-256: `06f4374d4403cea9a89609426e5d9c9940ff9be8cacb852708ae0816d885154a`


### L485–L530

````text

   485 | /// takes 64 live reservations on it; the outsider, registered after every coalition bond, still reserves and opens a reserved DA

   486 | /// session on its own budget (34 per reservation), counted on its reservation and on neither shared counter.

   487 | #[test]

   488 | fn g07_a_coalition_cannot_spend_the_outsiders_budget() {

   489 |     let (p, _) = arm_legacy(palw_t12_launch_params_v1(), 0, true, true);

   490 |     let mut c = Chain::new(p);

   491 |     let first = 0x0614_2000;

   492 |     let coalition = 64u64;

   493 |     for chunk in (first..first + coalition).collect::<Vec<_>>().chunks(16) {

   494 |         c.step(&chunk.iter().map(|n| bond_obj(*n, RICH)).collect::<Vec<_>>());

   495 |     }

   496 |     c.step(&[bond_obj(OUTSIDER, RICH)]);

   497 |     // Option (b) is armed at genesis and the route is armed: the V2 Attempt is admitted.

   498 |     assert_eq!(palw_g14_route_attempt_refusal_v1(&c.sp, c.daa + 1), None);

   499 |     let id = c.floor_claim(0x0614_07);

   500 |     let seats = c.floor_seats();

   501 |     let bound = c.bind(id, &seats);

   502 |     let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);

   503 |     c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);

   504 |     // The coalition: DA-8's shared budget spent, and 64 live reservations on the claim.

   505 |     c.s = edited(&c.sp, &c.s, |carriage| {

   506 |         carriage.da_claims.insert(

   507 |             id,

   508 |             kaspa_consensus_core::palw_da_rcore_v1::PalwDaClaimV1 { opened_non_seat_total: 16, ..Default::default() },

   509 |         );

   510 |     });

   511 |     for n in first..first + coalition {

   512 |         c.step(&[reservation(&c.s, id, bond_key(n))]);

   513 |     }

   514 |     assert_eq!(c.s.legacy_dispute_v1(&id).unwrap().live.len(), coalition as usize);

   515 |     // The outsider: its reservation and its reserved session are admitted on its own budget.

   516 |     c.step(&[reservation(&c.s, id, bond_key(OUTSIDER))]);

   517 |     c.step(&[da_accuse(id, bond_key(OUTSIDER), 0)]);

   518 |     let session = c.s.da_session(&id, &bond_key(OUTSIDER)).expect("the reserved session is open").clone();

   519 |     assert!(session.accuser_is_seat, "seat-like: it pauses the claim (DA-5)");

   520 |     assert_eq!(c.s.legacy_dispute_reservation_v1(&id, &bond_key(OUTSIDER)).unwrap().sessions_opened, 1, "on its reservation");

   521 |     assert_eq!(c.s.da_claim(&id).unwrap().opened_non_seat_total, 16, "the shared budget is not touched");

   522 |     // The unanswered reserved session ends in the objective DA default (never a computation fault), refunding every deposit.

   523 |     let deadline = session.deadline_daa;

   524 |     c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);

   525 |     assert!(matches!(

   526 |         c.claim(&id).phase,

   527 |         PalwClaimPhaseV2::Voided { reason: kaspa_consensus_core::palw_state_v2::PalwVoidReasonV2::ProducerWithholding, .. }

   528 |     ));

   529 |     assert!(c.s.legacy_dispute_v1(&id).is_none());

   530 |     assert_eq!(kaspa_consensus_core::palw_state_v2::palw_accuser_exposure_v1(&c.s, &bond_key(OUTSIDER)), 0);

````


## E16 — リポジトリ最新の残件記録。今回の実行結果ではない

ファイル: `docs/design/palw/remaining-work-2026-10-16.md`

ファイル SHA-256: `15c44a33b4901befc028fa035ef28daec65e00b5c2e99522f4aa0a50a97705eb`


### L7–L47

````text

     7 | ## 0. 現状の判定

     8 | 

     9 | - **G14 の主張(Panel 全員が共謀しても外部の bond 1 つで客観的に有罪)は、まだ証明されていない。** fold 水準と一部 node 水準の試験は通ったが、未解決の gap と未実施の drill が残る。

    10 | - **fence は DAA 14,000 から延期した。** 16,000 は目安で、確定値ではない。G14 を全部武装したまま、PALW 報酬・ブロック生成・Final が正常に進むことを、出荷 binary の drill で確かめてから fence する。

    11 | - integ(`integ/rfc02-rfc06-12-g14`)は、統合ブランチの検証済み状態と、5 本の G14 lane の merge を含む。新しい規則は全て休眠(全 preset で `None`)で、t12 の identity は変わっていない。

    12 | 

    13 | ## 1. RFC07 の認証済み selected-approval adapter(今回実装)

    14 | 

    15 | - `PalwKernelRouteStateV1::finals_read_v1` は、`PaidClaimV2` Final のうち、`palw_exec_v2_selected_approval_v1` が認めたものに限って beacon の `AttributedWorkV1` を出す。

    16 |   - 認める条件:canonical paid Final が、割り当てられた正の quorum(抽選された checker)・bound source・完了 round で native 受理・restore 検査を通っていること。EXEC credit と同じ gate である。

    17 |   - optimistic の timeout、Panel-licensed、未承認の paid claim は、従来どおり `None`。

    18 | - 残り:

    19 |   - 旧 OPV-only の fixture に依存する conformance / OPV bootstrap の試験、約 45 本と core の `rfc0010_production_fold::paid_panel_sources…`。抽選 checker の承認を持つ paid claim の fixture へ移す(迂回で通さない)。

    20 |   - Panel-licensed Final の event(route が Panel の seed・epoch を知らない)。

    21 |   - m・k の測定(Gate C と共通)。

    22 | 

    23 | ## 2. G14 の lane — 保存済みで未完了の作業(WIP branch、未 build・未試験)

    24 | 

    25 | | branch | head | 内容 | 残り |

    26 | |---|---|---|---|

    27 | | `integ/g14-tir-pricing` | `61ab3a9a8` | frame の課金・期限を測る harness の途中 | 32×32=1,024 step の課金が要るかの実測。debit ごとの inclusion margin と `reorg_replay_moves` の積み上げの見直し。実測にもとづく cost model と期限。T-N1(試験専用 config)。正直な IR FP claim が Final・支払いまで届く node 試験 |

    28 | | `integ/g14-court-capacity` | `5b64cac9c` | court 容量(`palw_court_capacity_v1.rs` / fold)、Panel 担保の試験の途中 | 処理予算の実測 harness、占有時間の課金、期限付きスケジューラ。42 枠飽和+継続的な埋め直し攻撃の試験。Panel の Active/Resolved 分離(`palw_final_lock_full_collateral`、R_new の式)。late conviction、資本効率の比較 |

    29 | | `integ/g14-drill` | `b8858af58` | 共謀 drill の driver の途中(`scripts/drills/g14_collusion_drill.py`、`g14_drill_factory.rs`) | D-01〜D-18、court/Panel の 7 条件、正常系 P-1〜P-5、RFC-0019 R-7/R-8 の実装と実行 |

    30 | 

    31 | ## 3. G14 の未解決 gap(監査の ID)

    32 | 

    33 | - **G-07** court の 42 枠:連合が全枠を占有できる。fused leaf 経路は、飢餓耐性を実証するまで (b) で「G14 経路あり」と数えない(ユーザー方針)。

    34 | - **G-13 の効率**:Panel 担保の Active/Resolved 分離が未実装。Final+1,000 の無条件解放へは戻さない。

    35 | - **TIR frame の価格・期限**:t12 の現行価格では、賃料が 2e7 MSK 以上、告発を開けるのが 0.9e6 DAA 後、Final が 6e7 DAA 後で、実用にならない。実モデルの leaf 数では u64 を超える可能性もある(未測定)。

    36 | - **native**:G-N1(v11/v29 の告発枠)、G-N2(v39 mining の Final 後の告発。veto の扱いは RFC-0018 U-6 のユーザー判断)、G-K2(EXEC の試験 seam GAP-81)。

    37 | - **G-K3 / F17** 検証者の誘因:正直な検証者が費用を回収できることの証明。自作自演は finder 49% 上限で損になる。

    38 | - **G-11** rule E、**G-12** bond 式 v2、**G-04** Gate C の武装:いずれも測定値と判断が待ち。

    39 | - **X3** の候補値は 5,520 DAA。未測定の項(B_cold_fetch など)が残る。

    40 | - T-N1 の本番経路 harness:IR class の登録 → `Active`、TIR family の認証、layout 公開、`TirPublicGrantReadyV1`、外部 owner/filer の service。

    41 | 

    42 | ## 4. 一括検証(`integ/validation` `37cf3fa89`、記録は consolidated-validation-2026-10-15.md)

    43 | 

    44 | - green:ladder 20 段、t12 identity、G14 対象の 150 試験。

    45 | - red:kaspad service E2E 16 件、RFC-0018 sibling 3 件、rfc9_v4、rfc0016 の 1 件、rfc12 ×4、t12_a2u_mixed ×2、SDK ×3、tir-lower ×4、base0 ×3。一部は旧 base でも失敗していた。

    46 | - clippy は警告 557 件。lane が落ち着いてから 1 回でまとめて直す(`--exclude misaka-palw-worker`)。

    47 | - 最新の integ head で、全 suite を再実行していない。

````


### L54–L66

````text

    54 | ## 6. genesis モデル(RFC-0020 / PALWART3)

    55 | 

    56 | - #2 Huihui-Qwen3.5-2B の 32K 変換は、fit の長文チェック(integer 20k/32k 付近)で停止した。再実行は fit から(`genesis10_pipeline.py run --model 2`、変換と record は済み)。

    57 | - #8 は Q8_0 GGUF からの 32K streaming 変換(`--stream`)が未実行。再量子化であり、品質ガードは 64GB 以上の機械が要る。

    58 | - #6(27B bf16 GGUF streaming、15〜22 h)、#1 の 32K 再変換、#3/#4/#5/#10 が未着手。

    59 | - RFC02 の変換 artifact は、main checkout の `target/` 削除で失われた。evidence の sha256 で再生成・照合する。

    60 | 

    61 | ## 7. ユーザー判断待ち

    62 | 

    63 | - RFC-0019 U-1 / U-3 / U-5。

    64 | - G14 の Lead 既定値(owner cap 8、V3/V4 拒否、idle grace 256 DAA)。

    65 | - legacySettlementWindowDaa と reserve drip の Lead 既定値。

    66 | - RFC-0018 U-1〜U-8(Gate C)。

````


## E17 — TIR価格・時計のリポジトリ内計算記録。今回の実測ではない

ファイル: `docs/design/palw/g14-completeness-proof-2026-10-16.md`

ファイル SHA-256: `163ed1900bd2a92bfba994561e0f965daa73b5e7e0a0e71eca7e37e53ea400f7`


### L1050–L1096

````text

  1050 | ### 9.5 新しい発見(設計判断が要る。未修正)

  1051 | 

  1052 | **frame は構築できるが、network_v1 の価格と時計では払えず、Final に届かない。**

  1053 | 

  1054 | - **rent。** `rent_per_parser_step` は 1 sompi なので、rent はほぼ parser step 数である。

  1055 |   - 原 FP carrier が払う最低 rent(`palw_tir_public_minimum_rent_v1`)は、上限 6 で 2e15〜8e15 sompi(2e7〜8e7 MSK)、上限 257 で 2.4e18〜9.5e18 sompi(2.4e10〜9.5e10 MSK)。

  1056 |   - 外部 owner の Open の rent(`owner_terms_v1`)は、corpus dense で上限 6 のとき約 2.2e15 sompi(2.2e7 MSK)。上限 257 では約 2.7e18 sompi。

  1057 |   - G14 の「外部の 1 人が conviction できる」は、この費用を払えることが前提である。

  1058 | - **時計。**

  1059 |   - preparation_until は +0.9e6〜3.8e6 DAA。外部 owner の Open は `reserved_daa ≥ preparation_until_daa` を要する。

  1060 |   - collect_until(Final の下限)は、上限 6 で +6e7〜2.4e8 DAA、上限 257 で +1.7e9〜6.5e9 DAA。

  1061 |   - 原因は、WorkDebit と move を 1 件ずつ `inclusion_margin_daa` = 4 で直列に積む時計のモデルである(R7、未測定)。

  1062 | - **preparer の負担。** 正直な preparer は、1 frame あたり 2.3e5〜9.5e5 件の WorkDebit を出す必要がある。

  1063 |   prepared generation(最大 tile 長 65,536 での leaf 形式の上限で、2.8e7〜1.2e8 byte)を 16 KiB に分け、65 pass 分、書いて再度開くため。

  1064 | - **帰結。**

  1065 |   - DAA 14,000 で `palw_tir_public_conviction_v1` を武装しても、IR class の FP claim は frame の rent を払えない。

  1066 |   - 払えたとしても、Final と決済に届かない。

  1067 |   - T-N1 も、network_v1 の時計では node E2E で走らせられない。Open までに 0.9e6 DAA 以上を採掘する必要がある。

  1068 | - **判断が要る点。**

  1069 |   - 価格(parser step 単価、frame 検証を action ごとに 32 回 × 32 step/byte と数える係数)。

  1070 |   - 時計(debit ごとに直列の inclusion margin を積むか)。

  1071 |   - restart の回数(`reorg_replay_moves` = 64)。

  1072 |   - いずれも未測定であり、この branch は値を作っていない。

  1073 | 

  1074 | ### 9.6 実行した試験(この branch)

  1075 | 

  1076 | - `cargo test -p misaka-palw-tir-exec --features node --test free_prompt`: `5 passed; 0 failed`。

  1077 |   gap pin を反転した `tir_frame_allocation_holds_at_testnet12s_custodian_upper` を含む。honest run は上限 6 で完全な frame を作る。

  1078 | - `cargo test -p kaspa-consensus-core --lib -- palw_tir_public tir_fp_lane shipped_presets_have_pinned_fingerprints tir_preparation_is_claim_wide aux_registry_tests palw_native_resource`:

  1079 |   `126 passed; 2 failed`。

  1080 |   - 失敗 1: `flat_original_binding_does_not_acquire_tiled_trace_obligations`。§7.4 と §8.3 で既に失敗と記録されたもの。

  1081 |   - 失敗 2: `ledger_expansion::…::flat_seven_and_nine_use_actual_nested_program_and_metadata_inventories`(kernel の `KernelLedgerV1` codec、`invalid Borsh option`)。

  1082 |     この branch は kernel ledger の codec と table を変えていない。base での再確認はしていない。

  1083 |   - `shipped_presets_have_pinned_fingerprints`(t12 の指紋 pin)は ok で、testnet-12 の identity は変わらない。

  1084 |   - lane の試験 `protected_work_lane_exhaustion_does_not_spend_outside_owner_allocation` も ok。custodian が owner の lane 幅の debit を出すと拒否されることを、この試験に足した。

  1085 |   - `tir_preparation_is_claim_wide_retained_and_rebuilt_without_an_owner` と frame の fold 試験も ok。

  1086 | 

  1087 | ### 9.7 T-N1

  1088 | 

  1089 | **未作成。** §8.4 の障害 1(allocation)は解消した。代わりに、§9.5 の時計が新しい障害になった。

  1090 | 

  1091 | - network_v1 では、外部 owner が Open できるのは `preparation_until`(+0.9e6 DAA 以上)より後である。

  1092 |   そこまでに、各 preparer は 2.3e5 件以上の WorkDebit を出す必要がある。

  1093 |   これは node E2E(TestConsensus、PoW を省略、本番の template)で採掘できる量ではない。

  1094 | - 小さい fixture config(restart 0、margin 1 など)を使えば数千 DAA に縮む。

  1095 |   ただし、それは「preset の court 窓を保つ」という既存 harness の前提を外すことになり、測定値でもない。そのため、この branch では採らなかった。

  1096 | - harness の残り(§8.4 の 2)は変わっていない。

````


## E18 — RFC20: 実装未検証と設計のみの項目

ファイル: `docs/rfc/0020-long-context-calibration-memory.md`

ファイル SHA-256: `6a6e4f677e68fb4d956a7f46942a059d8b4cfdae7f5dbbe96128f670ccfe03ad`


### L1–L26

````text

     1 | # RFC-0020 — 長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない

     2 | 

     3 | > **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。本RFCはこの前提の下にあり、衝突する場合は前提が優先する。

     4 | > **暗号学的な計算証明を使わない — [PRINCIPLES §8](../PRINCIPLES.md)(恒久):** 本RFCは合意の外側の道具(較正・変換・忠実度計測)のメモリ設計であり、計算の正しさの証明を何も導入しない。使うハッシュは成果物と統計の同一性(digest)だけである。

     5 | > **genesis 前提 — 2026-10-15/16 ユーザー決定:** genesis は 8 モデル(#1 2 3 4 5 6 8 10)、**テキスト生成のみ**、**全モデルを 32K(32,768 token)の文脈で提供する**(2026-10-16)。再帰(Gated DeltaNet)クラスは提供文脈と同じ長さの系列 1 本で較正する(freeze-v1 §5.2、変換器が強制)。

     6 | 

     7 | * **Status:** Draft — 2026-10-16。設計(§1–§8)。実装(**implemented, unverified** — 本 lane は cargo build/test を実行していない。どの項目も PASS ではない): RAM 計上モード(§2)、carry の二重保持の除去(§3)、lazy 重みの既定値と警告(§4)、全系列スコア経路の監査(§5.1)、carry のディスク退避(§6.2、opt-in)、長文脈ドリフト検査(§7)。設計のみ・未実装: タイル化した厳密 attention と chunk-aware observer(§5.2–§5.4。監査の結果、genesis のテキスト経路には全行列経路が存在しない)、KV 履歴のディスク退避(§6.3)。

     8 | * **Decision:**

     9 |   1. **目標は 32K の依存構造を完全に保ったまま、同時に持つ必要のないデータを RAM に置かないこと。** 系列を分割しない。attention の範囲を縮めない。`Op::Attention` の `chunk` や小さな `max_window` をメモリ対策に流用しない(どちらも query が見てよい key を制限し、意味を変える)。

    10 |   2. **計測を先にする。** 段階(較正・変換・忠実度・推論)と種類(重み・carry・KV 履歴・GDN 状態・attention 一時領域・統計・logits)ごとに live と peak を出す RAM 計上モードを置き、OOM の原因を推測でなく帰属させる(§2)。

    11 |   3. **ビット同一性を崩さない。** 実装したメモリ削減(§3、§6.2)と速度改善(位置の run 評価、スレッド化カーネル)は、同じ入力に対して統計 digest・logits・GDN 状態がビット単位で一致することを試験で固定する。一致しない高速経路は明示的にラベルを付け、成果物の同一性を主張しない(§5.3)。

    12 | * **Activation:** **なし。** 合意・fence・wire・RPC には触れない。変換器(`palw-tir-convert`)と忠実度計測(`palw-tir-fidelity`)の CLI フラグと、genesis パイプライン(`scripts/genesis/genesis10_pipeline.py`)の引数だけである。

    13 | * **Baseline:** `integ/genesis10`(本RFCの commit)。

    14 | * **Not activated by:** 本RFCの merge、試験 source の存在、見積り表(§9)。

    15 | * **Normative language:** MUST / MUST NOT は本RFCの道具が守る不変条件である。

    16 | 

    17 | ## ユーザー方針 2026-10-16(本RFCを拘束する)

    18 | 

    19 | 1. 32K の依存構造は完全に保つ。不要なデータを同時に RAM に置かない。系列分割・attention 範囲の縮小はしない。

    20 | 2. まず計測: 段階別・種類別に RAM を分け、計上モードで OOM を帰属させる。

    21 | 3. carry の二重保持を除く(ユーザー提供パッチ `reduce_calibration_carries`)。期待される削減は 1 系列分の carry(例: 32K × 4096 × FP32 ≈ 512 MiB)。アロケータが解放済みメモリを保持し得るので、総 RSS については何も言わない。

    22 | 4. lazy 重み(`--calib-lazy-mib` / `--calib-lazy-cache-mib`)は重みの常駐を制限するだけで、KV や attention には効かない。行範囲を読めるソースが必要で、事前量子化 param は対象外。

    23 | 5. タイル化した厳密 attention は、全行列経路が実在する場所に限る。正規化は**見えるすべての key** に対して行い、絶対位置の因果マスク(key_pos ≤ query_pos)、絶対 RoPE 位置、GQA の対応を保つ。**key chunk ごとに softmax して足し合わせてはならない。**

    24 | 6. 統計の互換: 参照と同一の多パス経路(a)を先に、オンライン softmax の高速経路(b)は下位ビットが変わり得るのでラベル付けし成果物同一性を主張しない。chunk を意識した observer を置く。

    25 | 7. 32K を超える文脈(64K/128K)は KV と carry をディスクへ予算付きで退避する(RAM とディスク I/O の交換。計算量は全 attention で O(N²) のまま)。

    26 | 8. 実装順: (1) 計測、(2) carry パッチと lazy 重み、(3) 全行列経路が見つかった場合のみタイル化 attention、(4) より長い文脈のための KV/carry 退避。

````


## E19 — 共有challengeの偏り除去とround拘束

ファイル: `misaka/palw/challenge/src/seed.rs`

ファイル SHA-256: `e76650570ef3028c745dc338e4eb5c9512324eae6b9b33e5e73c884f3bc20fae`


### L129–L156

````text

   129 |     /// Uniform in `[0, n)` by rejection: words at or above the largest multiple of `n` are redrawn.

   130 |     pub fn index_below(&mut self, n: u64) -> Result<u64, SampleRefusalV1> {

   131 |         if n == 0 {

   132 |             return Err(SampleRefusalV1::EmptyRange);

   133 |         }

   134 |         // `zone` is the largest multiple of n not exceeding 2^64: accept w < zone (computed without overflow).

   135 |         let reject = (u64::MAX - n + 1) % n; // = 2^64 mod n

   136 |         loop {

   137 |             let w = self.next_u64()?;

   138 |             if reject == 0 || w < u64::MAX - reject + 1 {

   139 |                 return Ok(w % n);

   140 |             }

   141 |         }

   142 |     }

   143 | 

   144 |     /// `count` distinct indices of `[0, n)`, uniformly without replacement (Floyd), returned sorted.

   145 |     pub fn distinct_indices(&mut self, n: u64, count: u64) -> Result<Vec<u64>, SampleRefusalV1> {

   146 |         if count > n {

   147 |             return Err(SampleRefusalV1::TooMany { count, n });

   148 |         }

   149 |         let mut set = std::collections::BTreeSet::new();

   150 |         for j in (n - count)..n {

   151 |             let t = self.index_below(j + 1)?;

   152 |             if !set.insert(t) {

   153 |                 set.insert(j);

   154 |             }

   155 |         }

   156 |         Ok(set.into_iter().collect())

````


### L189–L209

````text

   189 | /// **Staged beacon** round `round`: a fresh locked beacon whose source window began after the round's message was committed.

   190 | pub fn staged_round_challenge_v1(

   191 |     mode: InteractiveModeV1,

   192 |     base_seed: &Digest,

   193 |     round: u32,

   194 |     message_root: &Digest,

   195 |     message_position: u64,

   196 |     round_beacon: &VerifiedWorkBeaconV1,

   197 |     round_window_start: u64,

   198 | ) -> Result<Digest, TranscriptRefusalV1> {

   199 |     if mode != InteractiveModeV1::StagedBeacon {

   200 |         return Err(TranscriptRefusalV1::WrongMode(mode));

   201 |     }

   202 |     if message_position >= round_window_start {

   203 |         return Err(TranscriptRefusalV1::MessageAfterWindow { round, message_position, window_start: round_window_start });

   204 |     }

   205 |     Ok(object_id(DOMAIN_STAGED_ROUND, &(*base_seed, round, *message_root, round_beacon.challenge_anchor, round_beacon.output)))

   206 | }

   207 | 

   208 | /// **Transcript-bound Fiat–Shamir**: each challenge absorbs the statement, the policy/source binding, the round index and every

   209 | /// prior message and challenge, including this round's message. Its soundness needs its own review (an external gate).

````


## E20 — Hash64 golden vector入力の配置

ファイル: `crypto/hashes/src/hash64.rs`

ファイル SHA-256: `22f850a916fa1158d61fdcc2ebd735e1dafe9dd84645b167aa0c7d2d5e662854`


### L210–L220

````text

   210 |     /// PR-9.5d: single-word constructor mirroring

   211 |     /// [`crate::Hash::from_u64_word`]. The word occupies the last

   212 |     /// (most-significant, in the 8-lane little-endian view) lane so

   213 |     /// small distinct integers map to distinct, BlockHasher-friendly

   214 |     /// `Hash64` values — used by `<int>.into()` test fixtures and by

   215 |     /// any consumer that needs a compact deterministic hash from a

   216 |     /// counter.

   217 |     #[inline(always)]

   218 |     pub fn from_u64_word(word: u64) -> Self {

   219 |         Self::from_le_u64([0, 0, 0, 0, 0, 0, 0, word])

   220 |     }

````


## E21 — Final時の報酬処理とその後の担保保持

ファイル: `misaka/palw/kernel/src/ledger.rs`

ファイル SHA-256: `00142c7591eb326e05c2f56798ea9eb6e8fc1df1b0d4814e9225e5cfd9d832fc`


### L1622–L1661

````text

  1622 |     /// **GAP-5: pay a Final out of its job's escrow** — the poster's bond is debited exactly what the producer is paid, and the escrow

  1623 |     /// is spent (an escrow pays once). No escrow (already paid, or returned): no reward — nothing is ever issued.

  1624 |     ///

  1625 |     /// **ADR-0176 HOOK `budget-final` (lane BUDGET; not built here):** the reward is paid only within the R / F reservation the claim

  1626 |     /// made at acceptance (`budget-accept`), re-checked here; the budget is not recovered before `d + W`, whatever the claim's fate.

  1627 |     fn pay_from_job_escrow(&mut self, job: &Digest, claim: &Digest, producer: Digest, out: &mut Vec<LedgerEventV1>) -> u64 {

  1628 |         let cap = self.consumer_final_reward_caps.get(claim).copied().unwrap_or(u64::MAX);

  1629 |         self.pay_from_job_escrow_capped(job, claim, producer, cap, out)

  1630 |     }

  1631 | 

  1632 |     pub(crate) fn pay_from_job_escrow_capped(

  1633 |         &mut self,

  1634 |         job: &Digest,

  1635 |         claim: &Digest,

  1636 |         producer: Digest,

  1637 |         cap: u64,

  1638 |         out: &mut Vec<LedgerEventV1>,

  1639 |     ) -> u64 {

  1640 |         let Some(row) = self.job_escrows.remove(job) else { return 0 };

  1641 |         let collateral = self.bonds.get(&row.poster).map_or(0, |b| b.collateral);

  1642 |         // Never instruct more than the poster's bond still holds (another subsystem may have slashed it since).

  1643 |         let paid = row.amount.min(collateral).min(cap);

  1644 |         // R8-ECON: a contracted Final the escrow (or its cap, or the poster's remaining bond) cannot fund at the contract's minimum

  1645 |         // is never paid less — its outcome is neutral: the escrow is released whole to the poster and the producer is paid nothing

  1646 |         // out of it. No floor (every claim below the consumer's fence): unchanged.

  1647 |         if self.consumer_minimum_payouts_v1.get(claim).is_some_and(|minimum| paid < *minimum) {

  1648 |             if let Some(b) = self.bonds.get_mut(&row.poster) {

  1649 |                 b.reserved = b.reserved.saturating_sub(row.amount);

  1650 |             }

  1651 |             settle(out, row.poster, row.amount, SettlementKindV1::ReleaseJobEscrow, Some(*claim));

  1652 |             return 0;

  1653 |         }

  1654 |         if let Some(b) = self.bonds.get_mut(&row.poster) {

  1655 |             b.reserved = b.reserved.saturating_sub(row.amount);

  1656 |             b.collateral -= paid;

  1657 |         }

  1658 |         settle(out, row.poster, paid, SettlementKindV1::PayJobEscrow, Some(*claim));

  1659 |         settle(out, row.poster, row.amount - paid, SettlementKindV1::ReleaseJobEscrow, Some(*claim));

  1660 |         settle(out, producer, paid, SettlementKindV1::FinalReward, Some(*claim));

  1661 |         paid

````


### L3277–L3316

````text

  3277 |             match (&before, &after) {

  3278 |                 (b, ClaimStateV1::Final { .. }) if !matches!(b, ClaimStateV1::Final { .. }) => {

  3279 |                     let liability_daa = self.claim_liability_daa_v1(&id);

  3280 |                     let row = self.claims.get_mut(&id).expect("listed");

  3281 |                     row.liability_until = Some(match self.paid_claim_consumers_v2.get(&id) {

  3282 |                         Some(paid) => paid.lifetime.liability_until,

  3283 |                         None => row.liability_until.unwrap_or(0).max(daa.saturating_add(liability_daa)),

  3284 |                     });

  3285 |                     // GAP-5: the reward is paid out of the job's escrow (the poster pays), once — never issued. ADR-0176 hook

  3286 |                     // (`palw_bond_budget_v1`, not built): this payout also re-checks and consumes the producer bond's reservation.

  3287 |                     let (job, convicted) = (row.job_id, row.convicted);

  3288 |                     let mut paid = Vec::new();

  3289 |                     let reward = if convicted || held_jobs.contains(&job) {

  3290 |                         0

  3291 |                     } else {

  3292 |                         self.pay_from_job_escrow(&job, &id, producer, &mut paid)

  3293 |                     };

  3294 |                     self.claims.get_mut(&id).expect("listed").rewarded = reward > 0;

  3295 |                     out.push(LedgerEventV1::Final { claim: id, reward });

  3296 |                     out.extend(paid);

  3297 |                     self.spec_on_final(&id);

  3298 |                 }

  3299 |                 (b, ClaimStateV1::TimedOut { .. }) if !matches!(b, ClaimStateV1::TimedOut { .. }) => {

  3300 |                     self.release(&id, producer, reserved, out);

  3301 |                     out.push(LedgerEventV1::TimedOut { claim: id });

  3302 |                 }

  3303 |                 // A defaulted claim's reservation is held through its liability horizon (C4 F-C4R3-02), then released.

  3304 |                 (ClaimStateV1::Unavailable { .. }, _) if reserved > 0 && liability.is_none_or(|u| daa > u) => {

  3305 |                     // O2: the horizon passed unconvicted — the held demanders' share is collected now and paid to them.

  3306 |                     let held = self.pay_held_share_v1(&id, producer, out);

  3307 |                     if let Some(r) = self.claims.get_mut(&id) {

  3308 |                         r.reserved -= held;

  3309 |                     }

  3310 |                     let reserved = reserved - held;

  3311 |                     self.release(&id, producer, reserved, out);

  3312 |                     out.push(LedgerEventV1::Released { claim: id });

  3313 |                 }

  3314 |                 (ClaimStateV1::Final { .. }, _) if reserved > 0 && liability.is_some_and(|u| daa > u) => {

  3315 |                     self.release(&id, producer, reserved, out);

  3316 |                     out.push(LedgerEventV1::Released { claim: id });

````


## E22 — EXEC報酬holdとconvictedの支払拒否

ファイル: `misaka/palw/kernel/src/exec_reward.rs`

ファイル SHA-256: `dee126ca356d972bb93956c49052cbcda5aa1aaa04421737a52e1f814883c7bb`


### L19–L33

````text

    19 | impl KernelLedgerV1 {

    20 |     pub fn validate_exec_reward_holds(&self) -> Result<(), String> {

    21 |         if self.exec_reward_holds.len() > MAX_EXEC_REWARD_HOLDS_V1 {

    22 |             return Err("too many EXEC reward holds".into());

    23 |         }

    24 |         for claim in self.exec_reward_holds.keys() {

    25 |             let row = self.claims.get(claim).ok_or("an EXEC reward hold has no claim")?;

    26 |             if row.rewarded || !matches!(row.body, ClaimBodyV1::Program { .. }) {

    27 |                 return Err("an EXEC reward hold names paid or unsupported work".into());

    28 |             }

    29 |             if row.liability_until.is_none_or(|until| until < self.exec_reward_holds[claim].expires_daa) {

    30 |                 return Err("an EXEC reward hold outlives its claim liability".into());

    31 |             }

    32 |         }

    33 |         Ok(())

````


### L82–L118

````text

    82 |     /// The cap is the executor's remaining share of the root's ONE funded allocation.

    83 |     /// Unspent escrow is refunded; actual paid fees replace native issuance within that share.

    84 |     pub fn finish_exec_reward(&mut self, root: Digest, claim: Digest, cap: u64, out: &mut Vec<LedgerEventV1>) -> Result<u64, String> {

    85 |         let hold = self.exec_reward_holds.get(&claim).ok_or("the EXEC reward hold is missing")?;

    86 |         let row = self.claims.get(&claim).ok_or("the EXEC reward claim is missing")?;

    87 |         if hold.owner_root != root

    88 |             || self.daa >= hold.expires_daa

    89 |             || row.convicted

    90 |             || row.rewarded

    91 |             || (row.reserved == 0 && row.liability_until.is_some_and(|until| self.daa <= until))

    92 |             || !matches!(row.life.state, ClaimStateV1::Final { .. })

    93 |         {

    94 |             return Err("the EXEC reward is not eligible for root Final".into());

    95 |         }

    96 |         let (job, producer) = (row.job_id, row.producer);

    97 |         self.exec_reward_holds.remove(&claim);

    98 |         let paid = self.pay_from_job_escrow_capped(&job, &claim, producer, cap, out);

    99 |         self.claims.get_mut(&claim).expect("checked").rewarded = paid > 0;

   100 |         Ok(paid)

   101 |     }

   102 | 

   103 |     /// Void/expiry releases poster escrow without debiting collateral or paying a producer.

   104 |     /// It never shortens the computation claim's consumer-frozen fraud liability.

   105 |     pub fn cancel_exec_reward(&mut self, root: Digest, claim: Digest, out: &mut Vec<LedgerEventV1>) -> Result<(), String> {

   106 |         let Some(hold) = self.exec_reward_holds.get(&claim) else { return Ok(()) };

   107 |         if hold.owner_root != root {

   108 |             return Err("the EXEC reward hold belongs to another root".into());

   109 |         }

   110 |         let job = self.claims.get(&claim).ok_or("the EXEC reward claim is missing")?.job_id;

   111 |         self.exec_reward_holds.remove(&claim);

   112 |         if let Some(escrow) = self.job_escrows.remove(&job) {

   113 |             if let Some(bond) = self.bonds.get_mut(&escrow.poster) {

   114 |                 bond.reserved = bond.reserved.saturating_sub(escrow.amount);

   115 |             }

   116 |             settle(out, escrow.poster, escrow.amount, SettlementKindV1::ReleaseJobEscrow, Some(claim));

   117 |         }

   118 |         Ok(())

````


## E23 — seatが発見したFaultをcourtキューへ接続する処理

ファイル: `kaspad/src/palw_panel.rs`

ファイル SHA-256: `746f6c9eb1e20e69e3dd502bab630f26a4443d6ef765e2539b7b22ab8ea58cae`


### L11738–L11786

````text

 11738 |                     // **ADR-0098 Decision 2: a seat that found a fault in this claim files NOTHING

 11739 |                     // about it** — in this round or any later one. Not a `Valid` from an arm whose

 11740 |                     // samples missed the lie, and not the half-window `Unavailable`, which accuses a

 11741 |                     // producer that served the data (and, without ADR-0065 D4, is charged when the

 11742 |                     // rest of the panel licenses). Silence is never charged; the fault goes to the

 11743 |                     // court through the challenger's half, and nothing here replays the claim again.

 11744 |                     if self.seat_found_fault_v1(&duty.claim_id) {

 11745 |                         // A fault is found in material this seat held: it was served (N-5).

 11746 |                         service.note_served(duty.claim_id);

 11747 |                         // **A fault still addressed at a BLOCK is not finished with** (ADR-0086

 11748 |                         // Decision 6, found by ADR-0111's drill): only the interval arm below names

 11749 |                         // the leaf from the block's served leaves, and this gate used to stop it for

 11750 |                         // good the moment the block was noted — the leaves arrived and were never

 11751 |                         // read, so no leaf was ever named. A free-prompt claim whose fault is a block

 11752 |                         // runs the interval arm again, which files nothing while the fault stands

 11753 |                         // (`FaultInRange` answers `None`), and the gate after it stops the capture arm.

 11754 |                         let fault_is_a_block = duty.free_prompt

 11755 |                             && self.seat_faults.lock().unwrap().address(&duty.claim_id).is_some_and(|(_, count)| count > 1);

 11756 |                         if !fault_is_a_block {

 11757 |                             // **ADR-0111 Decision 6: a NAMED leaf this seat could not accuse from a

 11758 |                             // capture is pursued, not dropped** — the executor's evidence if it has

 11759 |                             // answered, the fast-path ask if not yet, the demand on chain past the

 11760 |                             // patience. What files is the court's object, never a receipt.

 11761 |                             if duty.free_prompt

 11762 |                                 && let Some((key, object, accusation)) = self

 11763 |                                     .pursue_named_leaf_v1(

 11764 |                                         duty,

 11765 |                                         network_domain,

 11766 |                                         current_daa,

 11767 |                                         &interval_openings,

 11768 |                                         accused.contains(&duty.claim_id),

 11769 |                                     )

 11770 |                                     .await

 11771 |                             {

 11772 |                                 if accusation {

 11773 |                                     accused.insert(duty.claim_id);

 11774 |                                 }

 11775 |                                 if !court_pending.iter().any(|(sid, _, _, _)| *sid == key) {

 11776 |                                     // Dated (the second review's MEDIUM): an undated item waits behind

 11777 |                                     // every dated one, and a lie it would have convicted reaches `Final`.

 11778 |                                     // Before the claim's earliest `Final` too (the fourth review's MEDIUM).

 11779 |                                     let earliest_final =

 11780 |                                         palw_seat_claim_earliest_final_v1(&self.consensus_config.params, duty.bound_daa);

 11781 |                                     court_due

 11782 |                                         .insert((key, 0, false), palw_seat_court_filing_due_v1(deadline, earliest_final, current_daa));

 11783 |                                     court_pending.push((key, 0, false, object));

 11784 |                                 }

 11785 |                             }

 11786 |                             break 'verdict None;

````


### L16393–L16415

````text

 16393 |     /// ADR-0085 Decision 4: remember a fault this seat found, for the challenger's half. A leaf

 16394 |     /// once named replaces the block it was found in (`PalwSeatFaultLedgerV1` orders the addresses).

 16395 |     fn note_seat_fault_v1(&self, claim: Hash64, first_leaf_index: u64, leaf_count: u64) {

 16396 |         self.seat_faults.lock().unwrap().note(claim, first_leaf_index, leaf_count);

 16397 |     }

 16398 | 

 16399 |     const SAMPLE_REFUSALS_LOGGED_CAP: usize = 1_024;

 16400 | 

 16401 |     /// Whether this is the first time this seat reports that no drawn leaf of `claim`'s capture

 16402 |     /// adjudicates here — see `sample_refusals_logged`.

 16403 |     fn first_sample_refusal_v1(&self, claim: Hash64) -> bool {

 16404 |         let mut logged = self.sample_refusals_logged.lock().unwrap();

 16405 |         if logged.len() >= Self::SAMPLE_REFUSALS_LOGGED_CAP {

 16406 |             logged.clear();

 16407 |         }

 16408 |         logged.insert(claim)

 16409 |     }

 16410 | 

 16411 |     /// **ADR-0098 Decision 2: has this seat found a fault in `claim`?** When it has, the verdict

 16412 |     /// block files nothing about the claim — neither a later arm's `Valid` nor the half-window

 16413 |     /// `Unavailable` — and the fault goes to the court through the challenger's half.

 16414 |     fn seat_found_fault_v1(&self, claim: &Hash64) -> bool {

 16415 |         self.seat_faults.lock().unwrap().holds(claim)

````


### L16418–L16436

````text

 16418 |     /// **Hold one served opening, bounded** (ADR-0077 Decision 8).

 16419 |     ///

 16420 |     /// The transport has already refused anything this node did not ask for, anything over the

 16421 |     /// lane's byte cap, and anything past a `(claim, interval)`'s slot ceiling — so what reaches

 16422 |     /// here is bounded per pair. What is NOT bounded by that is the number of PAIRS, which grows

 16423 |     /// with every claim this seat is drawn onto and never shrinks on its own, so the map has its

 16424 |     /// own ceiling: a new pair past it is dropped rather than admitted. Dropping the newest is the

 16425 |     /// right way round here, unlike the material pool: the seat asked for these, it re-asks on its

 16426 |     /// own cadence, and evicting an older pair would discard an answer to a request that is still

 16427 |     /// open.

 16428 |     fn pool_interval_opening(pool: &mut HashMap<(Hash64, u32), Vec<Vec<u8>>>, claim: Hash64, interval_index: u32, bytes: Vec<u8>) {

 16429 |         /// `PALW_FP_SEAT_INTERVAL_SAMPLES_V1` intervals for each of 128 claims in flight.

 16430 |         const PAIRS: usize = 512;

 16431 |         /// The lane's own per-pair slot ceiling, mirrored so this map cannot outgrow it if the

 16432 |         /// transport's changes.

 16433 |         const PER_PAIR: usize = 4;

 16434 |         let key = (claim, interval_index);

 16435 |         if !pool.contains_key(&key) && pool.len() >= PAIRS {

 16436 |             return;

````


### L17019–L17054

````text

 17019 |                     None => b.verify_fp_interval_opening(&candidate, roots, interval, &prompt_owned, work_leaves),

 17020 |                 })

 17021 |                 .await

 17022 |                 else {

 17023 |                     return None;

 17024 |                 };

 17025 |                 backend = returned;

 17026 |                 match verdict {

 17027 |                     PalwFpIntervalVerdictV1::Valid => {

 17028 |                         answered = true;

 17029 |                         break;

 17030 |                     }

 17031 |                     PalwFpIntervalVerdictV1::Fault { leaf_index } => {

 17032 |                         warn!(

 17033 |                             "[{PALW_PANEL}] claim {}: interval {index} does not replay at leaf {leaf_index} — filing nothing; a \

 17034 |                              sampled verdict never slashes and the court's bisection is what convicts",

 17035 |                             duty.claim_id

 17036 |                         );

 17037 |                         self.note_seat_fault_v1(duty.claim_id, leaf_index, 1);

 17038 |                         return None;

 17039 |                     }

 17040 |                     PalwFpIntervalVerdictV1::FaultInRange { first_leaf_index, leaf_count } => {

 17041 |                         warn!(

 17042 |                             "[{PALW_PANEL}] claim {}: interval {index} does not replay in leaves [{first_leaf_index}, +{leaf_count}) — \

 17043 |                              filing nothing; a sampled verdict never slashes, and the block is the court's address (ADR-0086 D3)",

 17044 |                             duty.claim_id

 17045 |                         );

 17046 |                         self.note_seat_fault_v1(duty.claim_id, first_leaf_index, leaf_count);

 17047 |                         // ADR-0086 Decision 6: ask for the block's leaves on the same lane, and name

 17048 |                         // the leaf from them once they are held — the fault at leaf granularity is

 17049 |                         // what a court narrows to. The level is the one the opening this seat just

 17050 |                         // verified folds at, read off it, never derived here: a held class's is

 17051 |                         // pinned at 12 whatever its ladder (ADR-0121 Decision 3), and the request

 17052 |                         // names the block by the number that class counts in.

 17053 |                         let served = misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(&bytes).ok();

 17054 |                         let retain_level = served.as_ref().map(|v4| v4.range.retain_level).unwrap_or_else(|| {

````


### L17644–L17679

````text

 17644 |     async fn pursue_named_leaf_v1(

 17645 |         &self,

 17646 |         duty: &kaspa_consensus_core::palw_producer_v2::PalwSeatDutyV2,

 17647 |         network_domain: Hash64,

 17648 |         current_daa: u64,

 17649 |         openings: &HashMap<(Hash64, u32), Vec<Vec<u8>>>,

 17650 |         already_accused: bool,

 17651 |     ) -> Option<(Hash64, PalwConsensusObjectV2, bool)> {

 17652 |         use kaspa_consensus_core::palw_leaf_evidence_v1 as lev;

 17653 |         use kaspa_consensus_core::palw_shard_court_v1::{PalwLeafEvidenceV1, PalwShardCourtVerdictV1};

 17654 |         if already_accused {

 17655 |             return None;

 17656 |         }

 17657 |         let (leaf, count) = self.seat_faults.lock().unwrap().address(&duty.claim_id)?;

 17658 |         if count != 1 {

 17659 |             return None; // a block or an unaddressed fault names no leaf yet

 17660 |         }

 17661 |         if self.leaf_pursuits.lock().unwrap().get(&duty.claim_id).is_some_and(|p| p.closed || p.leaf != leaf) {

 17662 |             return None;

 17663 |         }

 17664 |         // The claim's binding, off any interval opening this seat holds for it; the chain checks it

 17665 |         // against the claim's execution root when a demand lands, and so does this.

 17666 |         let binding = openings

 17667 |             .iter()

 17668 |             .filter(|((claim, index), _)| *claim == duty.claim_id && lev::palw_leaf_evidence_request_decode_v1(*index).is_none())

 17669 |             .flat_map(|(_, held)| held.iter())

 17670 |             .find_map(|bytes| misaka_palw_base0::fp_interval::Base0FpIntervalOpeningV4::decode_v1(bytes).ok().map(|v4| v4.binding))

 17671 |             .filter(|binding| binding.committed_execution_root == duty.execution_root)?;

 17672 |         let interval = lev::PalwSeatIntervalGeometryV1::from_binding_v1(&binding)?.interval_of_leaf_v1(&binding, leaf)?;

 17673 |         let index = lev::palw_leaf_evidence_request_index_v1(interval)?;

 17674 |         let ladder = kaspa_consensus_core::palw_court_v2::palw_refutation_leaf_cap_v2(

 17675 |             &self.config.court,

 17676 |             self.consensus_config.params.palw_court_ladder.is_some_and(|f| f.is_active(current_daa)),

 17677 |         );

 17678 |         // ADR-0152 §4-ter: the CLASS's ladder past the fence (see `seat_refutation_ladder_v1`).

 17679 |         let ladder = self.seat_refutation_ladder_v1(duty.class_id, ladder, current_daa);

````


### L17701–L17756

````text

 17701 |             match evidence.verdict_at_v2(&bound_to, ladder, bound) {

 17702 |                 Ok(PalwShardCourtVerdictV1::ExecutorGuilty | PalwShardCourtVerdictV1::NeedsDissection) => {

 17703 |                     use kaspa_consensus_core::palw_shard_court_v1::{

 17704 |                         PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, palw_shard_court_session_id_v1,

 17705 |                     };

 17706 |                     let mut accusation = evidence.into_accusation_v1(

 17707 |                         duty.claim_id,

 17708 |                         duty.execution_root,

 17709 |                         duty.trace_root,

 17710 |                         duty.executor_bond,

 17711 |                         duty.seat_bond,

 17712 |                     );

 17713 |                     if let Err(e) = accusation.validate_shape(ladder) {

 17714 |                         warn!("[{PALW_PANEL}] claim {}: the accusation at leaf {leaf} is refused locally ({e})", duty.claim_id);

 17715 |                         return None;

 17716 |                     }

 17717 |                     let session_id = palw_shard_court_session_id_v1(domain.as_byte_slice(), &accusation);

 17718 |                     accusation.signature = self.sign(session_id.as_byte_slice(), PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT)?;

 17719 |                     let object = PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) };

 17720 |                     if let Err(why) = kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object) {

 17721 |                         warn!("[{PALW_PANEL}] claim {}: the accusation at leaf {leaf} cannot ride a carrier ({why})", duty.claim_id);

 17722 |                         return None;

 17723 |                     }

 17724 |                     info!(

 17725 |                         "[{PALW_PANEL}] claim {}: accusing leaf {leaf} in the one-move court on the executor's own evidence \

 17726 |                          (session {session_id}) — ADR-0111's fast path",

 17727 |                         duty.claim_id

 17728 |                     );

 17729 |                     self.leaf_pursuits

 17730 |                         .lock()

 17731 |                         .unwrap()

 17732 |                         .entry(duty.claim_id)

 17733 |                         .or_insert(LeafPursuitV1 { leaf, asked_daa: None, demanded: false, closed: false })

 17734 |                         .closed = true;

 17735 |                     return Some((session_id, object, true));

 17736 |                 }

 17737 |                 Ok(PalwShardCourtVerdictV1::FalseAccusation) => {

 17738 |                     warn!(

 17739 |                         "[{PALW_PANEL}] claim {}: the executor's evidence for leaf {leaf} recomputes — this seat's replay and the court \

 17740 |                          disagree about a leaf whose inputs they share, which is a determinism fault, not the claim's; nothing filed",

 17741 |                         duty.claim_id

 17742 |                     );

 17743 |                     self.leaf_pursuits

 17744 |                         .lock()

 17745 |                         .unwrap()

 17746 |                         .entry(duty.claim_id)

 17747 |                         .or_insert(LeafPursuitV1 { leaf, asked_daa: None, demanded: false, closed: false })

 17748 |                         .closed = true;

 17749 |                     return None;

 17750 |                 }

 17751 |                 Err(e) => {

 17752 |                     info!(

 17753 |                         "[{PALW_PANEL}] claim {}: the evidence served for leaf {leaf} does not adjudicate ({e}) — not an answer",

 17754 |                         duty.claim_id

 17755 |                     );

 17756 |                 }

````


## E24 — TIR service: seatで発見したclaimの優先追及とowner admission

ファイル: `kaspad/src/palw_tir_public_owner.rs`

ファイル SHA-256: `8dfd7dd6b85cab82989cf86d6faa9f4ab7cfeae7b7e2630822bcea2e83e64e54`


### L334–L385

````text

   334 | impl PalwPanelService {

   335 |     /// **One tick of the outside owner** (see the module doc). `signer` is this node's bond; `filer` is the public filer's book,

   336 |     /// which every finding enters. Never fails the loop.

   337 |     #[allow(clippy::too_many_arguments)]

   338 |     pub(crate) async fn tir_public_owner_tick_v1(

   339 |         &self,

   340 |         session: &kaspa_consensusmanager::ConsensusProxy,

   341 |         signer: PalwBondKeyV2,

   342 |         network_domain: Hash64,

   343 |         filer: &mut PalwTirPublicFilerBookV1,

   344 |         book: &mut PalwTirPublicOwnerBookV1,

   345 |         pending: &mut Vec<(Hash64, u32, bool, PalwConsensusObjectV2)>,

   346 |         due: &mut HashMap<(Hash64, u32, bool), u64>,

   347 |         moved: &mut HashMap<(Hash64, u32, bool), u64>,

   348 |     ) {

   349 |         let ours = |round: u32, responder: bool| !responder && round == PALW_TIR_PUBLIC_OWNER_ROUND_V1;

   350 |         if self.consensus_config.params.palw_tir_public_conviction_v1_fence().is_none() {

   351 |             book.cases.clear();

   352 |             pending.retain(|(_, round, responder, _)| !ours(*round, *responder));

   353 |             return;

   354 |         }

   355 |         let form = self.config.prompt_ids_form;

   356 |         // G-T4: the claims this node's own seat found faulty are pursued first (process memory of the Panel replay).

   357 |         let seat_faulted: Vec<Hash64> = self.seat_faults.lock().unwrap().claims().collect();

   358 |         let court_limits = kaspa_consensus_core::palw_court_v2::palw_tir_court_limits_v1(&self.config.court);

   359 |         let mut steps: Vec<OwnerStepV1> = Vec::new();

   360 |         let mut now = 0u64;

   361 |         let armed = {

   362 |             let mut read = |state: &PalwChainStateV2| -> Result<(), String> {

   363 |                 steps.clear();

   364 |                 now = state.last_point().ok_or("TIR owner snapshot has no accepted clock")?.daa_score;

   365 |                 book.skipped.retain(|_, until| now < *until);

   366 |                 // Discover: the claims this node's seat found faulty first (G-T4), then one page of accepted frames per tick; a

   367 |                 // replay is started for a frame this bond may own.

   368 |                 let page = state.staged_tir_frames_after_v1(book.after, 16);

   369 |                 book.after = page.last().copied();

   370 |                 let order = owner_discovery_order_v1(&seat_faulted, &page, |claim| palw_tir_public_claim_v1(state, *claim).is_some());

   371 |                 for claim in order {

   372 |                     if book.cases.len() >= PALW_TIR_PUBLIC_OWNER_CASES_V1

   373 |                         || book.cases.contains_key(&claim)

   374 |                         || book.skipped.contains_key(&claim)

   375 |                         || steps.iter().any(|s| matches!(s, OwnerStepV1::Replay { claim: c, .. } if *c == claim))

   376 |                     {

   377 |                         continue;

   378 |                     }

   379 |                     let Some(frame) = palw_tir_public_claim_v1(state, claim) else { continue };

   380 |                     let view = frame.view_v1(state.staged_tir_owner_count_v1(&claim));

   381 |                     let plan = frame.plan_v1();

   382 |                     let owns_here = state.staged_tir_acquisitions_of_v1(&claim).iter().any(|(owner, _)| *owner == signer);

   383 |                     let slot_open = state.staged_tir_owner_slot_open_v1(&claim, now.saturating_add(1));

   384 |                     if !owner_may_pursue_v1(&view, plan.preparation_until_daa, &signer, now, slot_open, owns_here) {

   385 |                         continue;

````


## E25 — RFC5のVMは不採用。未実装欠陥として数えない

ファイル: `docs/rfc/0005-palw-ml-vm.md`

ファイル SHA-256: `46c4429aedd57da116500ffb2213de74da91d690e42609a221c8fb058f53f5f6`


### L1–L13

````text

     1 | # RFC-0005: PALW VM（不採用）

     2 | 

     3 | > **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。

     4 | > モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。

     5 | > 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。

     6 | > **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。

     7 | > 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。

     8 | 

     9 | 

    10 | > **2026-10-07 中核目標・設計の優先規則:** [ADR-0173](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md)を適用する。普通の非Panel public bondが、producer秘密状態なしにpublic authenticated materialから不正をlocalizeしobjective convictionまで完結できることを目指す。衝突する将来設計は末尾のmission alignment amendmentで改定する。既存Status・実装記録・fenceは履歴として保持し、この追記は実装完了やactivationを意味しない。

    11 | 

    12 | PALW-BVM・PALW-GVM は実装しない。ISA、syscall、guest メモリ、gas、VM court、toolchain を合意規則として維持すると、モデル対応に必要な実装・監査・履歴再生の負担が増えるため。

    13 | モデル拡張には [versioned Kernel と宣言的 plan](../design/palw/versioned-kernels.md) を使い、未対応の意味論は審査済みの Kernel 更新で追加する。

````


## E26 — 32K向けTIR frame見積りのテストコード（実ノード測定ではない）

ファイル: `misaka/palw/tir/exec/tests/free_prompt.rs`

ファイル SHA-256: `ecabde6023e7391dcb584a0fd7e26620cd0840c30dcc5ee3ecda2254be55c4a5`


### L263–L338

````text

   263 | fn tir_frame_allocation_holds_at_testnet12s_custodian_upper() {

   264 |     use kaspa_consensus_core::network::{NetworkId, NetworkType};

   265 |     use kaspa_consensus_core::palw_model_registry_v1::{PalwModelRegistryFoldV1, palw_registry_globals_of_bundle_v1};

   266 |     use kaspa_consensus_core::palw_state_v2::{PalwChainStateV2, PalwTransitionExtrasV1};

   267 |     let t12 = kaspa_consensus_core::config::params::palw_t12_shipped_params();

   268 |     assert_eq!(t12.net, NetworkId::with_suffix(NetworkType::Testnet, 12));

   269 |     assert!(t12.palw_tir_shard_v1.is_some(), "testnet-12 arms the layer-sharded (stratified) Panel for IR classes");

   270 |     assert!(t12.palw_shard_licensing.is_none(), "shard licensing (a ×1024 upper) is dormant on testnet-12");

   271 |     let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else {

   272 |         panic!("testnet-12 is ConsensusV2")

   273 |     };

   274 |     let panel_upper = u64::from(bundle.panel.seat_count()) + 1;

   275 |     assert_eq!(panel_upper, 6, "a five-seat V2 Panel and the producer");

   276 |     // The production resolver (`palw_tir_public_allocation_rules_at` → `staged_registered_tir_maximum_custodians_v1`) over the

   277 |     // shipped bundle's state params and a registry fold with the network's own globals, at the planned fence (DAA 14,000).

   278 |     let registry = PalwModelRegistryFoldV1 {

   279 |         globals: palw_registry_globals_of_bundle_v1(bundle),

   280 |         span_daa: 1,

   281 |         genesis_works: Default::default(),

   282 |         grace_until_daa: 0,

   283 |         admission_audit_period_daa: None,

   284 |         readiness_v2_active: false,

   285 |         bond_maturity: None,

   286 |     };

   287 |     let extras = PalwTransitionExtrasV1 { model_registry: Some(registry), ..Default::default() };

   288 |     let t12_upper = PalwChainStateV2::staged_registered_tir_maximum_custodians_v1(14_000, &bundle.state, &extras).unwrap();

   289 |     assert_eq!(t12_upper, 257, "a stratified Panel's 256 seats and the producer");

   290 |     let mut record_line = Vec::new();

   291 |     let mut classes = 0usize;

   292 |     for (k, (name, backend)) in backends("holds").into_iter().enumerate() {

   293 |         let prompt = prompt_of(&backend, k);

   294 |         let job = fp_job(&backend, &prompt, 2);

   295 |         let run = backend.execute_free_prompt(&job, &usize_ids(&prompt)).unwrap();

   296 |         let shape = backend.capture_shape(&run.outcome.material).unwrap();

   297 |         let commitment = palw_fp_commitment_from_context_v3(&job, &shape.job_context, &run, 999_999).unwrap();

   298 |         let PalwPublicExecutionBindingHeaderV1::Tir(header) =

   299 |             backend.public_execution_binding_header_v1(&run.outcome.material).unwrap()

   300 |         else {

   301 |             panic!("{name}: an IR capture exports a TIR header");

   302 |         };

   303 |         let (record, _) =

   304 |             kaspa_consensus_core::palw_tir_admission_v1::palw_tir_class_record_v1(backend.class(), &backend.artifact_root()).unwrap();

   305 |         let geometry = kaspa_consensus_core::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {

   306 |             class_id: job.class_id,

   307 |             producer: PalwBondKeyV2(job.executor_bond),

   308 |             execution_root: commitment.execution_root,

   309 |             job_identity: palw_fp_job_pin_v1(&commitment),

   310 |             accepted_daa: 100,

   311 |         };

   312 |         for custodians in (1..=panel_upper).chain([33, t12_upper]) {

   313 |             match derive_tir_public_allocation_v1(

   314 |                 &geometry,

   315 |                 &commitment,

   316 |                 &header,

   317 |                 &record,

   318 |                 &backend.class().layout,

   319 |                 None,

   320 |                 None,

   321 |                 rules(custodians),

   322 |             ) {

   323 |                 Ok(PalwTirPublicAllocationQuoteV1::Complete(plan)) => {

   324 |                     assert_eq!(plan.acquisition.maximum_custodians, custodians, "{name}");

   325 |                     // The claim-wide totals are the lanes' SUM; the preparation holds custodian lanes only.

   326 |                     assert_eq!(plan.owner_work.maximum, plan.owner_work.lanes_sum_v1().unwrap(), "{name}");

   327 |                     assert_eq!(plan.preparation_work.per_actor, plan.owner_work.per_custodian, "{name}");

   328 |                     if custodians == panel_upper || custodians == t12_upper {

   329 |                         record_line.push(format!(

   330 |                             "{name}: n={} upper={custodians} parser={:.2e} rent={:.2e} preparation_until=+{:.2e} collect_until=+{:.2e}",

   331 |                             commitment.work_leaves,

   332 |                             plan.terms.plan.maximum_parser_steps as f64,

   333 |                             plan.terms.plan.rent_sompi as f64,

   334 |                             (plan.preparation_until_daa - 100) as f64,

   335 |                             (plan.terms.collect_until_daa - 100) as f64

   336 |                         ));

   337 |                     }

   338 |                 }

````


## E27 — 共通court枠の実際の受付拒否コード

ファイル: `consensus/core/src/palw_state_v2/objects.rs`

ファイル SHA-256: `ac1b6a9bfdb6dc3b0f8c6550a9ff2a176a7fd00e451c598ff3bf89991f4448b1`


### L1986–L2015

````text

  1986 |             // **How many disputes may run at once, and why that is a number at all** (audit A M-1).

  1987 |             //

  1988 |             // Every dissection move is graded in the block that carries it and counts against the

  1989 |             // network's single per-block court slot (`palw_court_move_spends_the_slot_v1`), while

  1990 |             // every party owes its move inside one `turn_deadline_daa` and a missed rung is a

  1991 |             // CONVICTION, not a delay. So the slot is a rate and the rung window is a budget of

  1992 |             // it: with more live sessions than the window has slots, an attacker opens sessions on

  1993 |             // claims of its own, emits one cheap `CourtAttnChildChosen` a block, and honest movers

  1994 |             // are dropped at the acceptance layer until their rung lapses — the responder losing

  1995 |             // its whole stake, the challenger losing `claim.reserved`.

  1996 |             //

  1997 |             // Bounding CONCURRENCY rather than adding a second per-block budget is the fix that

  1998 |             // keeps one answer to "how much court may one block ask for": the slot stays exactly

  1999 |             // as wide as it is, and this makes the promise it implies — every session reaches it

  2000 |             // inside its own rung — true by construction. It is a pure state fact, so the fold,

  2001 |             // the acceptance rehearsal and the sync walk all answer it the same way.

  2002 |             let open_sessions = builder.state.court_sessions.len() as u64;

  2003 |             let max_sessions = palw_max_concurrent_court_sessions_v1(builder.params.turn_deadline_daa());

  2004 |             if open_sessions >= max_sessions {

  2005 |                 return Err(PalwStateV2Error::CourtSessionsAtCapacity {

  2006 |                     open: open_sessions,

  2007 |                     max: max_sessions,

  2008 |                     turn_deadline_daa: builder.params.turn_deadline_daa(),

  2009 |                     per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,

  2010 |                 });

  2011 |             }

  2012 |             let claim = builder.state.claims.get(claim_id).ok_or(PalwStateV2Error::MissingClaim(*claim_id))?.clone();

  2013 |             if claim.phase.is_terminal() {

  2014 |                 return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "CourtOpened" });

  2015 |             }

````


## E28 — DAからcourtへ移る経路の共通上限

ファイル: `consensus/core/src/palw_state_v2/da_sessions.rs`

ファイル SHA-256: `89487d4796ff06b9bb1c46f2cdd974409258c68dfb2c4ef4439e34d7ef79b73c`


### L748–L760

````text

   748 |     // **X3 (REQ-X3-4, `palw_claim_hard_deadline_v1`)**: a court pursuit on a claim accepted past the fence starts only at or before

   749 |     // the claim's open cutoff and only if `start + B_remaining <= hard`. `Ok` below the fence.

   750 |     crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(builder.params, claim, ctx.daa_score)

   751 |         .map_err(|why| refused(why.to_string()))?;

   752 |     let open_sessions = builder.state.court_sessions.len() as u64;

   753 |     let max_sessions = palw_max_concurrent_court_sessions_v1(builder.params.turn_deadline_daa());

   754 |     if open_sessions >= max_sessions {

   755 |         return Err(PalwStateV2Error::CourtSessionsAtCapacity {

   756 |             open: open_sessions,

   757 |             max: max_sessions,

   758 |             turn_deadline_daa: builder.params.turn_deadline_daa(),

   759 |             per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,

   760 |         });

````


## E29 — ネットワークcourt同時実行上限の算式

ファイル: `consensus/core/src/palw_state_v2.rs`

ファイル SHA-256: `ce6a025a3457b8505b0e328ebd75d447b2397256831916d6d971647b0ffd4fa7`


### L8716–L8731

````text

  8716 | /// is not delayed but CONVICTED ([`convict_close_declarer_v1`]). So a chain holding more sessions

  8717 | /// than the window has slots can starve a close as well as a rung, with or without the fence, and

  8718 | /// the bound belongs where the sessions are counted rather than behind a fence that describes only

  8719 | /// half of what fills the slot.

  8720 | pub const PALW_COURT_CLOSE_MAX_PER_BLOCK: usize = 1;

  8721 | 

  8722 | /// **How many court sessions may be open at once** (audit A M-1): the rung window measured in

  8723 | /// blocks, times the court moves one block admits.

  8724 | ///

  8725 | /// Derived, never typed — see [`PALW_COURT_CLOSE_MAX_PER_BLOCK`] for the two quantities. On the RC

  8726 | /// (`turn_deadline_daa` 42, one move a block) that is 42 concurrent sessions; at the 45 the ADR

  8727 | /// drafted against, 45. Below one it is one: a network is always allowed to run a single dispute,

  8728 | /// or a `turn_deadline_daa` this rule cannot serve would make the court unopenable rather than

  8729 | /// merely contended.

  8730 | pub fn palw_max_concurrent_court_sessions_v1(turn_deadline_daa: u64) -> u64 {

  8731 |     turn_deadline_daa.saturating_mul(PALW_COURT_CLOSE_MAX_PER_BLOCK as u64).max(1)

````


## E30 — RFC07の直接検査・全関係の被覆と裁定への接続

ファイル: `docs/rfc/0007-palw-verification-certificates-and-algebraic-checks.md`

ファイル SHA-256: `ca0580bcfb23650ca07b322f55094597fb41c695bd55861ab8cb4f5bf2255494`


### L1–L4

````text

     1 | # RFC-0007: PALW probabilistic detection — direct checks, authenticated receipts, and deterministic court

     2 | > **2026-10-15 恒久改定 — [PRINCIPLES §8](../PRINCIPLES.md#8-モデル計算の暗号学的証明を使用しない2026-10-15ユーザー恒久決定):** MISAKA はモデル計算の正しさの暗号学的証明を使用せず、将来の採用・研究候補にも置かない。ZK/SNARK/STARK/zkVM/zkML、PCS に基づく succinct validity argument、GKR、producer 送信の sum-check validity transcript、再帰・proof aggregation の方向は撤回する。以下の既存 wire 名・過去の測定は履歴であり、禁止方式の採用根拠にならない。

     3 | >

     4 | > 許容するのは認証済み operand を検証者が実際に読む直接 Freivalds・代数的スポットチェック、サンプリング再実行、決定論的再計算、公開証拠の局所化と exact court である。合意 credit は抽選された検証者の承認を使い、正直な割合・重み・相関・実参加・資料取得・期限の前提を明示する。署名・quorum・hash/Merkle は対象と承認者の認証であり、算術真実を証明しない。128-bit 等の条件付き検査誤差を全モデルの暗号学的健全性へ拡大しない。未知の参加率・検出率・市場数値は UNKNOWN、数値は検証後に決める。

````


### L36–L45

````text

    36 | ## 概要(日本語)

    37 | 

    38 | **通常のPanel検証は、割り当てられたsegmentを丸ごと再実行する方式から、commitされた計算のconstraintを安く検査する方式へ進める。** この方針をPart Vの中心要件とする。

    39 | 

    40 | - 同じweightを使う複数token/cellの行列積をまとめ、batched Freivaldsで検査する。それ以外の関係は、対応する決定論的再計算・局所検査で照合する。

    41 | - routing/TopK、量子化・丸め、非線形演算、入力・出力、memory/history、segment境界も検査範囲に含める。安いexact checkまたは登録 kernel の直接検査・決定論的再計算を使い、未検査の関係を残さない。

    42 | - receiptには方式・profile・challenge・検査範囲・証拠を署名でbindする。**区間の検査成功を、claim全体の成功へ無条件に格上げしない。** 部分receiptは被覆条件を満たした範囲にしか数えない。

    43 | - 異常時にだけ、食い違いを局所化して互換性のある既存courtへ渡す。check失敗だけでslashせず、nodeはterminal stepをexactに判定する。

    44 | - 正の証拠、必要な被覆/quorum、DA、challenge window完了が揃えば、RFC11の誤受理確率を許容してFinalにする。nodeは通常時にモデルを実行せず、署名・割当・commitment・期限・被覆を検証する。

    45 | - Kimi K3級への拡張を目指すが、速度向上は未実測。最初はTIR MatMulの再計算とbatched Freivaldsを、CPU時間・証拠生成・帯域・メモリ・cold/warmコスト込みで比較する。

````


### L101–L103

````text

   101 | ## Summary

   102 | 

   103 | The selected design changes what a Panel receipt means: a seat validates the constraints of its assigned computation with direct batched Freivalds and bounded exact native checks, rather than replaying the entire segment. Part V specifies the new profile, scope, committed evidence, challenge schedule, tally and escalation obligations. A claim passes probabilistically only when its whole statement is covered under the declared error budget; a few individually sound sampled segments are not enough.

````


## E31 — 既存の検査誤差・反復・grinding会計（基礎仮定の証明ではない）

ファイル: `misaka/palw/challenge/src/soundness.rs`

ファイル SHA-256: `29df3f7896f7ac2d9121e0054c0a56309d1f49c9afbe02f441a9bec9851e42e7`


### L1–L29

````text

     1 | //! **Effective false-accept accounting** (the user's ruling of 2026-10-08: an EFFECTIVE 128-bit bound, counting retries, grinding,

     2 | //! multiple relations and adaptive attacks — never the per-check number alone).

     3 | //!

     4 | //! ```text

     5 | //! eff = min_i (r · s_i)  −  ⌈log2 m⌉  −  ⌈log2 (R + 1)⌉  −  β · ⌈log2 G⌉  −  ⌈log2 Q⌉

     6 | //! ```

     7 | //!

     8 | //! * `s_i` — relation family `i`'s soundness for ONE repetition, `−log2 ε_i` in millibits (or [`RelationSoundnessV1::Complete`]:

     9 | //!   the family's whole domain is checked, `ε_i = 0`);

    10 | //! * `r` — the policy's `repetition_count` (independent repetitions multiply the per-repetition bits);

    11 | //! * `m` — the sampled families: an adversary may lie in whichever is weakest, and a statement is false if any family is, so the

    12 | //!   union bound over families is charged;

    13 | //! * `R` — the policy's `retry_limit`: `R + 1` attempts, each a fresh beacon and seed;

    14 | //! * `G` — the grinding choices per beacon (beacon outputs an adversary can select among: output selection, withholding, timing,

    15 | //!   fork choice, concentration, last-contributor grinding — `docs/design/palw/opv-beacon-bootstrap.md` §6), `β` the beacons one

    16 | //!   attempt draws (1 non-interactive; the rounds of a staged beacon);

    17 | //! * `Q` — adaptive statements: the subjects an adversary may commit against the same policy over its life (Sybil classes of one

    18 | //!   model, claims of one class).

    19 | //!

    20 | //! The terms multiply the adversary's tries (`P(false accept) ≤ m · (R+1) · G^β · Q · 2^(−min r·s_i)`), so their logarithms

    21 | //! subtract. Every logarithm is rounded UP and every factor charged separately (the sum of ceilings is at least the ceiling of the

    22 | //! sum): the result never overstates the bound. Integer arithmetic only, so every machine and every release computes the same number.

    23 | //! If every family is complete, the bound is [`EffectiveBitsV1::Complete`] whatever the other terms: no number of tries makes an

    24 | //! enumeration miss what it enumerates.

    25 | 

    26 | use borsh::{BorshDeserialize, BorshSerialize};

    27 | 

    28 | /// **The floor an approved policy's target must reach**, in effective bits.

    29 | pub const APPROVAL_MIN_TARGET_BITS_V1: u16 = 128;

````


### L94–L109

````text

    94 | /// One reviewed per-opportunity failure bound over an explicit finite horizon.

    95 | /// Each term may describe a different suite, source window, binding or court.

    96 | /// `Complete` means that particular event is impossible under its stated

    97 | /// assumptions; an unbounded/unknown event must instead be `Bits(0)`.

    98 | #[derive(Clone, Copy, Debug, PartialEq, Eq)]

    99 | pub struct CumulativeFailureTermV1 {

   100 |     pub per_opportunity: EffectiveBitsV1,

   101 |     /// Includes every retry, adaptive subject and fork/seed choice relevant to

   102 |     /// this event. Counts are not reset by policy changes, restart or reorg.

   103 |     pub opportunities: u128,

   104 | }

   105 | 

   106 | /// A conservative union bound across heterogeneous policies and failure events:

   107 | /// `Σ_i n_i · ε_i`. Counts may overlap: no independence is assumed. Each count

   108 | /// is charged separately, then the union of nonzero terms is charged once.

   109 | /// This calculator cannot authenticate or enforce a caller's horizon/counts.

````

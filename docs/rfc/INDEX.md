# RFC 番号順索引

[入口](README.md) / [トピック別](TOPICS.md) / [番号台帳](../maintenance/numbering.md)。自動生成の探索用索引です。原文の過去のStatusを現在の稼働保証へ昇格させません。

> 新規採掘は [TB1/FOW1移行契約](../governance/palw-tb1-fow1-transition.md) を参照してください。

> **BFT継続報酬・恒久廃止の後続正本。** [BMR1移行](../governance/model-retirement-transition.md)とRFC0069/ADR0237を優先します。BFTは共通C分割の5%継続報酬、Panel/Gate Cは5% MISAKA。正規モデル廃止で対象LP内actual MISAKA・登録者責任bond・MINING bond・未確定mining報酬を100%burn/失効。全検証bondは不正者も一律BFTburn対象外、個人不正はCourt個別slash、実回収MISAKAの49%をPanel/Gate C合計・残差51%burn。自己slash受領と同一offence二重報酬を禁止。正当な検証報酬と他モデルを保護、用途別lot分離・重複担保/責任中用途変更禁止。BFTは没収を受け取らず廃止bounty0。旧10%と可変W/P前段は置換済みです。

> **モデル採用経路の継承。** [MAD2適用・移行](../governance/model-admission-dual-route-transition.md)のkernel自動採用/BFT YES・NOを継承します。MOE3ではgenesisを含む全モデルを同じ管理対象とし、旧GENESIS免除とforkの特別経済区分は旧profileに限定します。技術互換・個別claim検証を維持します。

> **Qwen向け具体profile。** [QF1適用・移行](../governance/qwen-fow1-profile-transition.md)とRFC0066/ADR0234が36層Qwen3の整数実行・37-vector Readoutを固定します。費用定理・G-USE・native有効化は未完で、MAS1のUNKNOWN停止を維持します。

> **モデル採掘資格の後続正本。** [MAS1適用・移行](../governance/model-admission-safety-transition.md)とRFC0065/ADR0233を優先します。構造検査・G-USE・FOW1三gate・停止状態を共通確認し、UNKNOWNの仮承認をしません。BFTは限定された安全停止であり、費用定理や台帳finalityを置換しません。

> **モデル・経済条件の最新正本。** [MOE3適用・移行](../governance/fractional-position-economics-transition.md)とRFC0068/ADR0236の最終50/20/20/5/5を適用します。同一CからMiner用POSITION購入50%、LP用POSITION購入20%、MISAKA LP20%、Panel/Gate C MISAKA5%、BFT MISAKA5%。F/P/A/Jをfloor、Dを残差、B=D+Jで70%一括購入、LP量floor(qJ/B)・Miner残差。全モデル1.0・一意市場・小数8桁・MAD2管理、通常burn5%/creator1%又は0%を維持します。同日途中の80/20/q4・W/P前段・BFT10%は置換済みです。設計採択・未native有効化。

**画像・動画（RFC-0003）とモデル改善（RFC-0004）はmainnet後へ延期・当面無効。** [現行方針](../governance/post-mainnet-scope.md)。

| 番号 | 文書 | 状態の入口 |
| --- | --- | --- |
| RFC-0001 | [PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計](0001-palw-inference-surface-gaps.md) | 原文と後続改定を参照 |
| RFC-0002 | [PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program a…](0002-palw-tensor-ir.md) | 原文と後続改定を参照 |
| RFC-0003 | [PALW Generative Model Classes — one job, determinism and output layer, and the class profi…](0003-palw-generative-model-classes.md) | 画像・動画は延期/無効。共通部分は別 |
| RFC-0004 | [PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR](0004-palw-model-improvement.md) | モデル改善はmainnet後へ延期/無効 |
| RFC-0005 | [PALW VM（不採用）](0005-palw-ml-vm.md) | 原文と後続改定を参照 |
| RFC-0006 | [PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with …](0006-palw-layer-sharded-panels.md) | 原文と後続改定を参照 |
| RFC-0007 | [PALW probabilistic detection — direct checks, authenticated receipts, and deterministic co…](0007-palw-verification-certificates-and-algebraic-checks.md) | 原文と後続改定を参照 |
| RFC-0008 | [PALW EXEC lane — claim-backed work slices and transaction execution](0008-palw-claim-backed-consensus-blocks.md) | 原文と後続改定を参照 |
| RFC-0009 | [PALW Remote Client — node を持たないモデル登録・claim・非保管型の報酬回収](0009-palw-remote-miner.md) | 原文と後続改定を参照 |
| RFC-0010 | [Permissionless PALW Panel binding and claim completion](0010-permissionless-palw-panel-and-claim-completion.md) | 原文と後続改定を参照 |
| RFC-0011 | [Permissionless model onboarding with probabilistic constraint verification and court on di…](0011-permissionless-model-and-long-context-onboarding.md) | 原文と後続改定を参照 |
| RFC-0012 | [PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto](0012-palw-only-consensus-and-native-evm-settlement.md) | 原文と後続改定を参照 |
| RFC-0013 | [Reproducible layouts and resource-bounded model onboarding tools](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | 原文と後続改定を参照 |
| RFC-0014 | [Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](0014-panel-independent-fraud-prosecution.md) | 個別反証責任を継承 / 新回収分配49/51はRFC0069優先 |
| RFC-0015 | [Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](0015-panel-free-permissionless-verification.md) | 原文と後続改定を参照 |
| RFC-0016 | [bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) | 採用設計・測定待ち・休眠 |
| RFC-0017 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)](0017-model-eligibility-examination-and-renewal.md) | 旧二階層の記録 / MOE3全モデル1.0はRFC0068優先 |
| RFC-0018 | [DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](0018-dag-parallel-multi-model-consensus.md) | Draft / 個別採択あり |
| RFC-0019 | [年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変](0019-annual-model-upgrade.md) | 旧年次fork制度の記録 / MOE3は同じ追加手続 |
| RFC-0020 | [長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](0020-long-context-calibration-memory.md) | Draft / 部分実装 |
| RFC-0021 | [設計凍結・例外変更手続きと文書の役割分離](0021-design-freeze-and-documentation-governance.md) | Accepted / 文書管理 |
| RFC-0022 | [初期 mainnet から画像・動画・モデル改善を外す適用範囲](0022-post-mainnet-media-and-model-improvement-scope.md) | Accepted / 延期・無効化方針 |
| RFC-0023 | [未解決 claim の責任を保持し、それに依存しない claim を進める](0023-unresolved-claim-liability-and-independent-progress.md) | 原文と後続改定を参照 |
| RFC-0024 | [ctx 2M・大きいモデルでも door ≤ 3,200 DAA に収める court 設計(単一位置裁定・先読み開示・先行取得)](0024-court-door-3200-to-ctx-2m.md) | 原文と後続改定を参照 |
| RFC-0025 | [モデルが増え、不正に対する court が大量発生しても処理を継続できる客観的 court](0025-scalable-objective-court-many-models-mass-disputes.md) | 原文と後続改定を参照 |
| RFC-0026 | [court の主契約:時計・帰責・責任期間・置換関係の一本化](0026-court-policy-master-contract.md) | 原文と後続改定を参照 |
| RFC-0027 | [Miner Account Bond と Panel 追及報酬: 10万の基礎担保、責任連動の追加拘束、REAL継続性](0027-miner-account-bond-and-panel-incentives.md) | 原文と後続改定を参照 |
| RFC-0028 | [rooted court response eligibility history](0028-rooted-court-response-eligibility-history.md) | 原文と後続改定を参照 |
| RFC-0029 | [owner-independent shared TIR query data](0029-owner-independent-shared-tir-query-data.md) | 原文と後続改定を参照 |
| RFC-0030 | [court終了効果と未解決責任の保存](0030-court-end-effects-and-pending-liability-archive.md) | 原文と後続改定を参照 |
| RFC-0031 | [共有courtのinitial notice境界](0031-shared-court-initial-notice-boundary.md) | 原文と後続改定を参照 |
| RFC-0032 | [共有courtの終端materialと非独占提出](0032-shared-court-terminal-material-and-nonblocking-ingress.md) | 原文と後続改定を参照 |
| RFC-0033 | [D2 readiness rootのdigest opening取得](0033-d2-readiness-root-opening-availability.md) | 原文と後続改定を参照 |
| RFC-0034 | [archived closeの実受付と金融責任の適格解消](0034-archived-close-qualified-financial-discharge.md) | 原文と後続改定を参照 |
| RFC-0035 | [shared court originalの実原資と役割別同意](0035-court-original-funded-contract-and-role-consent.md) | 原文と後続改定を参照 |
| RFC-0036 | [認証付き PALW state map と行単位アクセス](0036-authenticated-palw-state-map-and-row-access.md) | 原文と後続改定を参照 |
| RFC-0037 | [IBD の有界 authenticated row stream と世代番号付き保存](0037-ibd-bounded-row-stream-and-storage-generations.md) | 原文と後続改定を参照 |
| RFC-0038 | [free-prompt の回答トークンを支払済み計算として記録する(FPA1 carriage)](0038-palw-fp-answer-accounting.md) | 原文と後続改定を参照 |
| RFC-0039 | [通常 verifier の fee consent と provider の有限支払・保持契約](0039-finite-fee-consent-and-provider-payment-bounds.md) | 原文と後続改定を参照 |
| RFC-0040 | [再利用 prefix の生成元を署名済み work identity で拘束する](0040-authenticated-prefix-work-dependencies.md) | 原文と後続改定を参照 |
| RFC-0041 | [genesis bond を廃止し、公開の BASE 回復経路から立ち上げる](0041-genesis-bond-free-bootstrap.md) | 原文と後続改定を参照 |
| RFC-0042 | [(AllModelG14PanelOne) — 全モデルの producer を G14 Panel=1 に統一する](0042-all-model-g14-panel-one.md) | 原文と後続改定を参照 |
| RFC-0042 | [(CourtSupplierConsent) — Court全分岐の供給者同意・実原資・native予約](0042-court-family-supplier-consent-and-native-reservation.md) | 原文と後続改定を参照 |
| RFC-0043 | [学習済みLLMの広いQ24 logit表現](0043-wide-q24-llm-output.md) | 原文と後続改定を参照 |
| RFC-0044 | [Positive Court：Panelによる正常計算証拠の生成・提出と、Minerの上限付き対話応答](0044-positive-court-full-coverage.md) | 原文と後続改定を参照 |
| RFC-0045 | [Canonical TIR Assignmentを起点とするPositive Court native経路](0045-positive-court-canonical-assignment.md) | 原文と後続改定を参照 |
| RFC-0046 | [Positive native原資の先行relay資料と有限custody](0046-positive-native-funded-relay.md) | 原文と後続改定を参照 |
| RFC-0047 | [Batch court time contract: one door, separate retention, public 6,880](0047-batch-court-time-contract.md) | 原文と後続改定を参照 |
| RFC-0048 | [EXEC credit・事前公開schedule・1 BPSとcourt容量の統合契約](0048-exec-credit-schedule-and-one-bps.md) | Accepted design / EXEC 1 BPS・court容量、未有効化 |
| RFC-0049 | [PALW実行整合性と一人による反証の完結契約](0049-palw-execution-integrity-and-unilateral-refutation.md) | Accepted design / PALW実行整合性・単独反証、未有効化 |
| RFC-0050 | [公開資料とモデル未保有の一人検証](0050-public-materials-and-one-honest-panel.md) | Accepted design / 公開資料・モデル未保有の一人検証、未有効化 |
| RFC-0051 | [受付前 tokenizer 資料の有料搬送と native 登録](0051-funded-preclaim-tokenizer-registration.md) | 原文と後続改定を参照 |
| RFC-0052 | [Hugging Face 固定モデル取得と claim 固有資料の分離](0052-hugging-face-model-acquisition-and-claim-materials.md) | 原文と後続改定を参照 |
| RFC-0053 | [追加モデルのburnによる停止・登録者による解除](0053-additional-model-burn-pause.md) | 旧burn停止規則 / MOE3全モデル・専用BFT ResumeはRFC0068 §2.4優先 |
| RFC-0054 | [公開T12 original受付profileの具体案](0054-public-t12-original-admission-profile.md) | Draft / 未採択・未有効化 |
| RFC-0055 | [全モデル共通の受付難易度・資源予約・総搬送1 BPS](0055-global-admission-court-calendar-and-one-bps.md) | 前回提供の設計 / 資源保護・未有効化 |
| RFC-0056 | [全claim共通の受付難易度とcourt負荷連動制御](0056-global-claim-admission-difficulty.md) | Proposed design / 全claim受付難易度・未実装未有効化 |
| RFC-0057 | [Court nonce・Panel固有チャレンジと提出の一回性](0057-court-nonce-and-panel-challenges.md) | Proposed design / Court nonce・Panel固有challenge・未有効化 |
| RFC-0058 | [実行commit・段階別抽選・仕事の一回性と合意資格](0058-staged-work-commitments-and-consensus-eligibility.md) | Proposed design / 実行commit・段階抽選・合意資格、未有効化 |
| RFC-0059 | [PALW-AN1：実入力への試行束縛とPanel＋Court](0059-palw-an1-input-binding-and-panel-court.md) | 旧AN1記録 / 新規採掘はRFC61/62へ置換、未有効化 |
| RFC-0060 | [モデル登録時の試行別出力束縛と暗号学的非重複性](0060-model-registration-and-attempt-output-binding.md) | 旧MOR1記録 / receiptとコアzを分離、新規採掘はRFC62 |
| RFC-0061 | [PALW内部閾値beaconと用途別乱数契約](0061-palw-internal-threshold-beacon.md) | ADOPTED_DESIGN_NOT_ACTIVATED / TB1用途別乱数 |
| RFC-0062 | [PALW-FOW1：推論関数の採掘出力と新規仕事の適格性](0062-palw-function-output-and-work-admissibility.md) | ADOPTED_DESIGN_NOT_ACTIVATED / FOW1コア出力・費用適格性 |
| RFC-0063 | [追加モデル限定MisakaOption・50%買上げ・5%/3%手数料・直接移転](0063-additional-model-option-economics.md) | 旧MOE1記録 / 新経済はRFC0068/ADR0236のMOE3優先、未有効化 |
| RFC-0064 | [報酬30%買付・20%永久流動性・余剰Position burn](0064-reward-buy-and-permanent-liquidity.md) | 旧MOE2記録 / MOE3の精度・配分・feeはRFC0068優先 |
| RFC-0065 | [MAS1：モデル追加時の構造検査・実用性条件とBFT停止](0065-model-admission-screening-and-bft-stop.md) | MAS1自動審査・停止 / MAD2のBFT採用はRFC0067優先 |
| RFC-0066 | [QF1：Qwen3-8Bを用いるFOW1の具体的実行・Readout](0066-qwen3-fow1-execution-and-readout.md) | ADOPTED_DESIGN_NOT_ACTIVATED / QF1 Qwen3整数実行・37-vector Readout |
| RFC-0067 | [MAD2：kernel自動追加とBFT YES/NOによるモデル採用](0067-model-admission-kernel-auto-and-bft-decision.md) | MAD2二経路を継承 / MOE3はgenesisを含め管理対象、RFC0068優先 |
| RFC-0068 | [MOE3：小数POSITION・全モデル統一・50/20/20/5/5配分](0068-fractional-position-miner-rewards.md) | ADOPTED_DESIGN_NOT_ACTIVATED / 全モデル1.0・小数POSITION・最終50/20/20/5/5 |
| RFC-0069 | [BMR1：モデル恒久廃止・用途別bond・Court slash 49/51](0069-bft-model-retirement-and-role-bonds.md) | ADOPTED_DESIGN_NOT_ACTIVATED / BMR1継続BFT5%・恒久廃止・用途別bond |
| RFC-0070 | [MWA1：モデル別仕事量認定・FOW1報酬・EXEC credit](0070-model-work-accounting-and-exec-credit.md) | PROPOSED_DESIGN_NOT_ACTIVATED / MWA1モデル仕事量・FOW1報酬・EXEC credit |

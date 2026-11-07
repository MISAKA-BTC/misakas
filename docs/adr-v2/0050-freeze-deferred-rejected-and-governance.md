# ADR-v2-0050 — 設計凍結・延期・撤回・不採用・未採択・変更管理

編集統合日: 2026-10-30　|　名前空間: `ADR-v2`　|　原文の主担当: 24本

**状態: 主題別の編集統合。採択・未採択・撤回・実装・有効化の状態は原資料から継承し、この統合では変更しません。** 新しい設計の採択、runtime修正、wire／署名domain／合意定数／activationの変更ではありません。

[50本の一覧](README.md) · [全204本の対応](MAPPING.md) · [原文の状態宣言](STATUS-LEDGER.md) · [変更・置換の境界](PRECEDENCE.md)

関連仕様: [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md)

## 1. この文書の役割

凍結・延期・撤回・不採用・未採択の方針を一つの入口に集め、履歴中の旧案を誤って再採用しない。

## 2. 統合した判断の読み方

### 2.1

採用済み設計は原則凍結し、重大問題の変更は新規RFC＋ADR、レビュー、明示採択という既存規程を維持する。整理・再配置そのものは新しいprotocolの採択ではない。[旧ADR-0181](../adr/0181-design-freeze-and-controlled-change.md)

### 2.2

画像・動画・モデル改善を初期mainnetから除外し、mainnet後も自動有効化しない。共有するTIR・登録・Court・会計の不備まで延期扱いにしない。[旧ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md)

### 2.3

Superseded／Withdrawn／Rejected／Deferred／Proposedを分ける。本文の一部だけが撤回された場合、維持された規則の所有先へ必ず戻れるようにする。

## 3. 維持する境界・後続改定

24本の主たる原文に加え、他の統合先が所有する部分撤回・未採択条項を下の台帳から参照する。すべてのDraftを「永久に実装しない」に変換しない。

## 4. 隣接する判断との分担

方針の仕様入口は[RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md)。全204本の原文宣言はSTATUS-LEDGER.md、混合採択と上書き関係はPRECEDENCE.mdを参照する。

## 5. 凍結・延期・撤回・不採用・未採択の集約台帳

状態は同義ではありません。`FROZEN`は既存規則を実装しないという意味ではなく、`PROPOSED`は永久不採用でもありません。旧文書の一部分だけを扱う行は、維持する規則と主担当を併記します。

| ID | 対象 | 状態・適用範囲 | 原資料 | 維持する部分・注意 | 現行の主担当 |
| --- | --- | --- | --- | --- | --- |
| P01 | 採用済み設計の変更 | FROZEN / 例外変更手続き | [旧ADR-0181](../adr/0181-design-freeze-and-controlled-change.md) | 意味不変の実装・修正・試験は継続。重大変更は新規RFC＋ADR・review・明示採択。 | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) |
| P02 | 画像・動画・モデル改善protocol | DEFERRED_POST_MAINNET / DISABLED | [旧ADR-0182](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) | text、TIR、通常の不変モデル登録、年次指定などの共通基盤は維持。mainnet日付で自動再開しない。 | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) |
| P03 | DNS validator／PoS overlayと運用role群 | 将来方針として不採用／退役。旧networkは移行条件付き | [旧ADR-0009](../adr/0009-dns-probabilistic-finality.md) [旧ADR-0010](../adr/0010-validator-node-architecture.md) [旧ADR-0011](../adr/0011-validator-deployment-and-equivocation-safety.md) [旧ADR-0013](../adr/0013-validator-reward-distribution.md) [旧ADR-0014](../adr/0014-coordinated-failover-protocol.md) [旧ADR-0015](../adr/0015-remote-signer-hsm-protocol.md) [旧ADR-0016](../adr/0016-stake-locked-bond-utxos.md) [旧ADR-0017](../adr/0017-all-active-staker-attestation.md) [旧ADR-0018](../adr/0018-quality-gated-stakescore-inclusion-economics.md) [旧ADR-0180](../adr/0180-palw-only-consensus-no-bft-validators.md) | 旧networkの有効規則と返還すべき既存責任を、文書整理で削除しない。 | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| P04 | 旧validator commit-reveal sortition | SUPERSEDED | [旧ADR-0012](../adr/0012-mainnet-validator-sortition-commit-reveal.md) [旧ADR-0017](../adr/0017-all-active-staker-attestation.md) | 旧0017も未来のvalidator方式として復活させない。 | [ADR-v2-0050](0050-freeze-deferred-rejected-and-governance.md) |
| P05 | VLT／compute overlay／validator carve・committee beacon | RETIRED / 後続のPALW-only方針 | [旧ADR-0024](../adr/0024-verified-llm-token-weighted-bft.md) [旧ADR-0126](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) [旧ADR-0128](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) [旧ADR-0134](../adr/0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) [旧ADR-0180](../adr/0180-palw-only-consensus-no-bft-validators.md) | height付き退役の履歴は保持する。旧overlayの資格・投票をGate Cの代替にしない。 | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| P06 | 旧PALW algo 4/5の報酬・PoW有効化案 | SUPERSEDED | [旧ADR-0021](../adr/0021-palw-llm-pow.md) | 新旧algorithm IDの意味と既存chain履歴は維持し、単なる文書番号変更で再利用しない。 | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| P07 | Base＋3本のEVM execution lane案 | REJECTED / 将来実装対象外 | [旧ADR-0023](../adr/0023-base-three-lane-execution.md) | 採択されたselected-parent EVMと後続EXECは別であり、削除しない。 | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) |
| P08 | PALW V1 credit-only overlay | WITHDRAWN | [旧ADR-0033](../adr/0033-palw-credit-gate-wiring.md) [旧ADR-0037](../adr/0037-palw-async-job-state-machine.md) [旧ADR-0038](../adr/0038-palw-is-the-consensus-work.md) | 非同期Job等の再利用機構は、後続合意構造の適用範囲で維持する。 | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| P09 | 恒久hash floor／hash creditの旧方向 | PARTIALLY SUPERSEDED | [旧ADR-0036](../adr/0036-palw-mainnet-activation-model.md) [旧ADR-0037](../adr/0037-palw-async-job-state-machine.md) [旧ADR-0038](../adr/0038-palw-is-the-consensus-work.md) [旧ADR-0039](../adr/0039-palw-only-block-production.md) | 旧0007のLayer 0まで一律撤回したとは扱わない。 | [ADR-v2-0005](0005-palw-consensus-architecture.md) |
| P10 | Metal／GGUFの別execution Family M | SUPERSEDED / 別familyを実装しない | [旧ADR-0051](../adr/0051-palw-metal-gguf-execution-family.md) [旧ADR-0053](../adr/0053-palw-one-execution-family.md) | 単一familyの内部backend最適化まで禁止したと解釈しない。 | [ADR-v2-0014](0014-versioned-kernels-and-backends.md) |
| P11 | 回答を複数claimへ分割する旧案 | PARTIALLY SUPERSEDED | [旧ADR-0080](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md) [旧ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md) | close chunk-group transportと二重計上防止は残る。 | [ADR-v2-0040](0040-tir-terminal-proof-and-local-opening.md) |
| P12 | prefill claimのstate-chain分割案 | PARTIALLY SUPERSEDED | [旧ADR-0081](../adr/0081-long-context-the-input-is-a-state-chain.md) [旧ADR-0082](../adr/0082-the-close-is-flat-in-the-context.md) | Merkle prompt IDsは撤回対象外。 | [ADR-v2-0025](0025-context-and-input-commitments.md) |
| P13 | pruning proofのPoW header抜き取り検査 | WITHDRAWN / UNSOUNDと原文に明記 | [旧ADR-0041](../adr/0041-palw-pruning-proof-verification.md) | 全件検証を維持し、費用はamortization／並列化で扱う。 | [ADR-v2-0012](0012-state-root-pruning-and-snapshots.md) |
| P14 | 汎用ML VMへの移行 | REJECTED | [旧ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md) [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) | versioned kernel拡張と固定算術は維持。 | [ADR-v2-0014](0014-versioned-kernels-and-backends.md) |
| P15 | 同じModel ID／head／rootの中身を上書きする旧更新方式 | WITHDRAWN / immutable registrationが優先 | [旧ADR-0088](../adr/0088-the-class-keeps-its-graph-and-the-owner-keeps-publishing.md) [旧ADR-0175](../adr/0175-registered-models-are-permanently-immutable.md) [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) | 新しい独立モデルの登録と、対象を明示したadapterは別。 | [ADR-v2-0016](0016-artifact-ownership-and-model-lineage.md) |
| P16 | MISAKA Torrent・専用Seeder・Seeder報酬 | WITHDRAWN | [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) | off-chainの任意配布、固定identity、claim資料の開示責任は維持。 | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) |
| P17 | モデル取得を合意で強制するPoR／lease／TRDC／FPR等 | WITHDRAWN | [旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) | モデル取得不能とclaim固有defaultを分離。モデルコピーを持つG14検証者という条件を消さない。 | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) |
| P18 | 旧モデル資格審査・更新・失効・昇格・Probation | WITHDRAWN / NOT_ADOPTED | [旧ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) | 1.0／0.3二階層と全階層のclaim検証は維持。 | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) |
| P19 | root保有状態でモデル有効化をgateする別ADR-0173案 | PROPOSEDの記録。後続取得非介入との対象整理が必要 | [旧ADR-0173（a-seat-holds-a-root-not-a-class-and-possession-gates-activation）](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) [旧ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) | 個々の役割のmodel保有・readinessと、モデル取得をchainが保証する制度を分ける。今回新たに採択・永久却下しない。 | [ADR-v2-0030](0030-sharded-verification-and-model-measurement.md) |
| P20 | 新Miner Account Bond・最低10万等の提案 | PROPOSED / NOT_ADOPTED | [旧ADR-0193（miner-account-bond-for-panel-incentives）](../adr/0193-miner-account-bond-for-panel-incentives.md) | 既存bond、Q/B/R/F、Panel責任は維持。最低額を新本番定数にしない。 | [ADR-v2-0035](0035-bond-budget-and-shared-reservations.md) |
| P21 | Provider役務有限化・custody移管／未払権利取消しA/B | NOT_ADOPTED | [旧ADR-0207](../adr/0207-finite-fee-consent-and-provider-payment-bounds.md) | 通常verifier feeとprovider fee-onlyは採択済みであり、本項に巻き込まない。 | [ADR-v2-0037](0037-fee-escrow-and-role-consent.md) |
| P22 | LLM計算のZK／SNARK／STARK／zkVM／PCS等 | 使用しない方針 | [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) [PRINCIPLES](../PRINCIPLES.md) | hash・署名・Merkleによる同一性認証、直接検査は維持。今回の整理で方針を変えない。 | [ADR-v2-0033](0033-probabilistic-checks-and-exact-court.md) |
| P23 | Panel=0 | DEFERRED_INACTIVE / 別の完成・有効化gate | [RFC-v2-0015](../rfc-v2/0015-freeze-deferred-rejected-and-governance.md) [旧ADR-0173（public-verifier-dispute-completeness-is-misaka-purpose）](../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) | 永久不採用とも今回mainnet後と新決定したものとも扱わない。Panel経路と共通会計は維持。 | [ADR-v2-0038](0038-g14-public-prosecution-and-completeness.md) |
| P24 | 旧algo-11によるRFC8 work-slice | SUPERSEDED DIRECTION | [旧ADR-0169](../adr/0169-a-work-slice-is-a-normal-consensus-block-the-session-earns-nothing-and-the-floor-is-kept-out-by-merge-admission.md) | 既存EXEC_TX／EXEC_SLICEを使う置換後の設計を維持。 | [ADR-v2-0007](0007-evm-exec-lane-and-anchors.md) |
| P25 | ML-DSA-65／32-byte address payloadの旧方式 | SUPERSEDED IN SCOPE | [旧ADR-0002](../adr/0002-mldsa65-p2pkh.md) [旧ADR-0019](../adr/0019-mldsa87-migration.md) | P2PKHの構造・互換履歴と、ML-DSA-87移行を分ける。 | [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) |
| P26 | launchでのP2SH／multisig | OUT OF LAUNCH SCOPE | [旧ADR-0019](../adr/0019-mldsa87-migration.md) | 再開は別ADRと必要な解析classを要するという元の境界を保持。 | [ADR-v2-0002](0002-post-quantum-signatures-and-interfaces.md) |
| P27 | LtHash16_1024の旧accumulator案 | REVISED BY AUDIT | [旧ADR-0003](../adr/0003-lthash-utxo-accumulator.md) | LtHash32_1024への改定と専用64-byte commitmentを参照する。 | [ADR-v2-0003](0003-utxo-accumulator-and-commitment.md) |
| P28 | ADR-0071 Decision 1のattempt価格案 | WITHDRAWN IN PART | [旧ADR-0071](../adr/0071-the-attempt-lanes-price-and-the-tickets-bound.md) [旧ADR-0072](../adr/0072-the-ticket-is-the-execution.md) | 同じADRの他の実装やticket境界まで削除しない。 | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) |
| P29 | hash lotteryなしinference ticketの検討 | NO DESIGN ADOPTED | [旧ADR-0141](../adr/0141-can-an-inference-be-the-ticket-without-a-hash-lottery.md) | 問題・費用・計測の記録であり、lottery変更を採択していない。 | [ADR-v2-0028](0028-attempt-ticket-and-seed-anchor.md) |
| P30 | モデル配分の旧候補曲線・未確定だった役割集計 | 後続決定を優先／候補は比較履歴 | [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) [旧ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) | α = 1.5等の明示採択と、旧S·μ(S)候補を合成しない。係数を報酬へ二重適用しない。 | [ADR-v2-0019](0019-model-tiers-and-annual-catalog.md) |
| P31 | Miner報酬から登録者への直接owner配当 | 採択していない／後続決定で否定 | [旧ADR-0091](../adr/0091-the-reward-buys-the-pair-and-no-holder-is-paid.md) [旧ADR-0114](../adr/0114-the-owners-leg-is-five-percent-and-it-arrives-at-a-height.md) [旧ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md) | ModelPosition売買手数料、自らMiner／Verifierとして参加した役割報酬は別。 | [ADR-v2-0023](0023-model-reward-routing-and-owner-leg.md) |

**この台帳への掲載は新しい不採用決定ではありません。** 採択済みの部分、隔離実装のみ承認された部分、未決案が混在する場合は、[状態宣言](STATUS-LEDGER.md)と[優先関係](PRECEDENCE.md)から元の判断範囲を確認します。

## 原資料一覧

各原文は一つの主担当だけに全文収録します。他の文書からの参照は副参照であり、別の規範を重複定義しません。以下の行番号は旧ファイルの行番号です。

| 原文（完全ファイル名） | 全文の行範囲 | 原文の状態宣言の場所 |
| --- | --- | --- |
| [0009-dns-probabilistic-finality.md](../adr/0009-dns-probabilistic-finality.md) | L1–L9 | L5–L5 |
| [0010-validator-node-architecture.md](../adr/0010-validator-node-architecture.md) | L1–L9 | L5–L5 |
| [0011-validator-deployment-and-equivocation-safety.md](../adr/0011-validator-deployment-and-equivocation-safety.md) | L1–L9 | L5–L5 |
| [0012-mainnet-validator-sortition-commit-reveal.md](../adr/0012-mainnet-validator-sortition-commit-reveal.md) | L1–L9 | L5–L5 |
| [0013-validator-reward-distribution.md](../adr/0013-validator-reward-distribution.md) | L1–L9 | L5–L5 |
| [0014-coordinated-failover-protocol.md](../adr/0014-coordinated-failover-protocol.md) | L1–L9 | L5–L5 |
| [0015-remote-signer-hsm-protocol.md](../adr/0015-remote-signer-hsm-protocol.md) | L1–L9 | L5–L5 |
| [0016-stake-locked-bond-utxos.md](../adr/0016-stake-locked-bond-utxos.md) | L1–L9 | L5–L5 |
| [0017-all-active-staker-attestation.md](../adr/0017-all-active-staker-attestation.md) | L1–L9 | L5–L5 |
| [0018-quality-gated-stakescore-inclusion-economics.md](../adr/0018-quality-gated-stakescore-inclusion-economics.md) | L1–L9 | L5–L5 |
| [0021-palw-llm-pow.md](../adr/0021-palw-llm-pow.md) | L1–L9 | L5–L5 |
| [0023-base-three-lane-execution.md](../adr/0023-base-three-lane-execution.md) | L1–L7 | L5–L5 |
| [0024-verified-llm-token-weighted-bft.md](../adr/0024-verified-llm-token-weighted-bft.md) | L1–L9 | L5–L5 |
| [0033-palw-credit-gate-wiring.md](../adr/0033-palw-credit-gate-wiring.md) | L1–L9 | L5–L5 |
| [0051-palw-metal-gguf-execution-family.md](../adr/0051-palw-metal-gguf-execution-family.md) | L1–L7 | L5–L5 |
| [0080-the-answer-is-long-the-verified-unit-is-short.md](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md) | L1–L9 | L5–L5 |
| [0081-long-context-the-input-is-a-state-chain.md](../adr/0081-long-context-the-input-is-a-state-chain.md) | L1–L9 | L5–L5 |
| [0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) | L1–L9 | L5–L5 |
| [0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) | L1–L9 | L5–L5 |
| [0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md](../adr/0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) | L1–L135 | L12–L24 |
| [0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) | L1–L160 | L14–L19 |
| [0181-design-freeze-and-controlled-change.md](../adr/0181-design-freeze-and-controlled-change.md) | L1–L22 | L3–L8 |
| [0182-defer-media-and-model-improvement-until-post-mainnet.md](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) | L1–L30 | L3–L10 |
| [0193-miner-account-bond-for-panel-incentives.md](../adr/0193-miner-account-bond-for-panel-incentives.md) | L1–L156 | L3–L9 |

## 原条文の完全収録（履歴）

**この欄は原文の保存であり、全条項を現行規則として再採択したものではありません。** 原文中の旧リンク・旧番号・コード・数式・状態宣言をそのまま保存します。後続改定の優先は上の境界と原資料で確認してください。リンクを開く場合は各項目の「原文を開く」を使います。

<a id="source-0009-dns-probabilistic-finality"></a>

<details>
<summary>0009-dns-probabilistic-finality.md — 原文 L1–L9</summary>

[原文を開く](../adr/0009-dns-probabilistic-finality.md) · SHA-256: `cc395194f6289642457df32bcc333ca4b25bc6e8c64ec6971d431810d5705846`

<!-- ADR_SOURCE_BEGIN:0009-dns-probabilistic-finality.md -->
~~~~~~~~markdown
# ADR-0009: DNS Probabilistic Finality Overlay

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator の署名・stake による finality は今後採用しない。PALW の chain selection と settlement に別の finality 権限を重ねる依存を除くため。後継は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md)。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0009-dns-probabilistic-finality.md -->

</details>

<a id="source-0010-validator-node-architecture"></a>

<details>
<summary>0010-validator-node-architecture.md — 原文 L1–L9</summary>

[原文を開く](../adr/0010-validator-node-architecture.md) · SHA-256: `b24809a3fe620f8c4e5be81e1530dbc13491271a51d639a2a38f49a5a1bbd00b`

<!-- ADR_SOURCE_BEGIN:0010-validator-node-architecture.md -->
~~~~~~~~markdown
# ADR-0010: Validator Node Architecture (operational supplement to ADR-0009)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 専用ノードの新規実装・再構築は行わない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) でその役割を廃止するため。PALW の共有鍵処理・bond 処理は廃止対象に含めない。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0010-validator-node-architecture.md -->

</details>

<a id="source-0011-validator-deployment-and-equivocation-safety"></a>

<details>
<summary>0011-validator-deployment-and-equivocation-safety.md — 原文 L1–L9</summary>

[原文を開く](../adr/0011-validator-deployment-and-equivocation-safety.md) · SHA-256: `41c2f49be65d2efb9724f6beeb4f9370f8eb947933939ec714add9e2c55b3c31`

<!-- ADR_SOURCE_BEGIN:0011-validator-deployment-and-equivocation-safety.md -->
~~~~~~~~markdown
# ADR-0011: Validator Single-Host Deployment + Equivocation-Safety Operating Model

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 専用の配置・二重署名防止設計は拡張しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で validator 投票を consensus authority から外すため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0011-validator-deployment-and-equivocation-safety.md -->

</details>

<a id="source-0012-mainnet-validator-sortition-commit-reveal"></a>

<details>
<summary>0012-mainnet-validator-sortition-commit-reveal.md — 原文 L1–L9</summary>

[原文を開く](../adr/0012-mainnet-validator-sortition-commit-reveal.md) · SHA-256: `7d61ea6fbccd712a098d2e867e93a1b845c341c61becc659d9546b07ec9ecbdc`

<!-- ADR_SOURCE_BEGIN:0012-mainnet-validator-sortition-commit-reveal.md -->
~~~~~~~~markdown
# ADR-0012: Mainnet Validator Sortition via On-Chain Commit-Reveal

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Superseded — commit-reveal 案は 2026-05-30 に ADR-0017 で撤回。

commit-reveal による validator 委員会選出は実装しない。委員会をなくして全 active bond が参加する [ADR-0017](0017-all-active-staker-attestation.md) に置き換えられ、その DNS validator 自体も [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で廃止するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0012-mainnet-validator-sortition-commit-reveal.md -->

</details>

<a id="source-0013-validator-reward-distribution"></a>

<details>
<summary>0013-validator-reward-distribution.md — 原文 L1–L9</summary>

[原文を開く](../adr/0013-validator-reward-distribution.md) · SHA-256: `b27b4f4e0d9349e2e528bb68e4d635f85314d669a4708f0647185a84ff6976ac`

<!-- ADR_SOURCE_BEGIN:0013-validator-reward-distribution.md -->
~~~~~~~~markdown
# ADR-0013: Validator Reward Distribution

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 報酬配分の新規実装・拡張は行わない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で将来の validator 割当を廃止するため。既存の報酬債務・bond は同 RFC の移行で清算する。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0013-validator-reward-distribution.md -->

</details>

<a id="source-0014-coordinated-failover-protocol"></a>

<details>
<summary>0014-coordinated-failover-protocol.md — 原文 L1–L9</summary>

[原文を開く](../adr/0014-coordinated-failover-protocol.md) · SHA-256: `c5734f3ff7101f7ae5e39974d949afebb15c641124c2985f4af1597e8ac6a611`

<!-- ADR_SOURCE_BEGIN:0014-coordinated-failover-protocol.md -->
~~~~~~~~markdown
# ADR-0014: Coordinated-Failover Protocol for Validator Hosts

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator の takeover token・HA failover は実装しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で専用 validator 役を廃止するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0014-coordinated-failover-protocol.md -->

</details>

<a id="source-0015-remote-signer-hsm-protocol"></a>

<details>
<summary>0015-remote-signer-hsm-protocol.md — 原文 L1–L9</summary>

[原文を開く](../adr/0015-remote-signer-hsm-protocol.md) · SHA-256: `9ad4eb6b101eb23b540c9f8f7ebf5895a9033104b2d65d600aa84aa761812ec4`

<!-- ADR_SOURCE_BEGIN:0015-remote-signer-hsm-protocol.md -->
~~~~~~~~markdown
# ADR-0015: Remote-Signer / HSM Protocol for Validator Signing

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS validator 向け HSM・PKCS#11・HA 拡張は今後実装しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で対象の validator 役を廃止するため。

既存 software signer（`kaspa-pq-signer`）と共有鍵処理の削除を意味しない。DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0015-remote-signer-hsm-protocol.md -->

</details>

<a id="source-0016-stake-locked-bond-utxos"></a>

<details>
<summary>0016-stake-locked-bond-utxos.md — 原文 L1–L9</summary>

[原文を開く](../adr/0016-stake-locked-bond-utxos.md) · SHA-256: `5614f006b6be9549cddacfad7bb73349302aa255f37b97b29087555df0971986`

<!-- ADR_SOURCE_BEGIN:0016-stake-locked-bond-utxos.md -->
~~~~~~~~markdown
# ADR-0016: Stake-locked bond UTXOs

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS stake bond の参加・投票用設計は拡張しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で stake 投票を廃止するため。PALW の共有 bond・担保規則は [ADR-0065](0065-a-bond-must-be-earned-and-a-seat-must-be-someone-else.md) に従う。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0016-stake-locked-bond-utxos.md -->

</details>

<a id="source-0017-all-active-staker-attestation"></a>

<details>
<summary>0017-all-active-staker-attestation.md — 原文 L1–L9</summary>

[原文を開く](../adr/0017-all-active-staker-attestation.md) · SHA-256: `8082002c1cbc21acb7a71ac3c1ba2b36c9b9271f3532bae208c4cdbe9aa66e6a`

<!-- ADR_SOURCE_BEGIN:0017-all-active-staker-attestation.md -->
~~~~~~~~markdown
# ADR-0017: All-Active-Staker Attestation (Remove Sortition Committee)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

全 active staker による DNS attestation は将来設計に採用しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で attestation quorum を廃止し、PALW を唯一の live consensus authority にするため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0017-all-active-staker-attestation.md -->

</details>

<a id="source-0018-quality-gated-stakescore-inclusion-economics"></a>

<details>
<summary>0018-quality-gated-stakescore-inclusion-economics.md — 原文 L1–L9</summary>

[原文を開く](../adr/0018-quality-gated-stakescore-inclusion-economics.md) · SHA-256: `65e78f2971205e4a34c136e68d12deb34aa1ec77f78d0c0535259971c48b71ea`

<!-- ADR_SOURCE_BEGIN:0018-quality-gated-stakescore-inclusion-economics.md -->
~~~~~~~~markdown
# ADR-0018: Quality-Gated StakeScore + Inclusion Economics (BFT-free)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

DNS quality floor・StakeScore・stake reorg gate の新規実装・拡張は行わない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で stake による PALW chain selection の veto を廃止するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0018-quality-gated-stakescore-inclusion-economics.md -->

</details>

<a id="source-0021-palw-llm-pow"></a>

<details>
<summary>0021-palw-llm-pow.md — 原文 L1–L9</summary>

[原文を開く](../adr/0021-palw-llm-pow.md) · SHA-256: `82c41f790ee539787cd12b45e69f6d8606a740887b557252decd486d2e6b47b7`

<!-- ADR_SOURCE_BEGIN:0021-palw-llm-pow.md -->
~~~~~~~~markdown
# ADR-0021: PALW LLM proof-of-work (`algo_id = 4`/`5`), at one block per 120 s

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Superseded — 旧 algo 4/5 の報酬・PoW 有効化方針は撤回。

出力テキストの commitment はモデルを実行せずに偽造できるため、Ollama ベースの直接 PoW は無効化された。full-logits trace も実行の証明にはならず、この旧方式を報酬・PoW として再実装／有効化しない。後継は [PALW V2](0026-palw-v2-runtime-separated-verification.md)。

旧 worker の devnet・shadow・zero-credit 観測は実装履歴であり、採用計画ではない。
~~~~~~~~
<!-- ADR_SOURCE_END:0021-palw-llm-pow.md -->

</details>

<a id="source-0023-base-three-lane-execution"></a>

<details>
<summary>0023-base-three-lane-execution.md — 原文 L1–L7</summary>

[原文を開く](../adr/0023-base-three-lane-execution.md) · SHA-256: `11d67029c9a3af26faf3327616f4711c54e6e3a6bf754df800b642448732d475`

<!-- ADR_SOURCE_BEGIN:0023-base-three-lane-execution.md -->
~~~~~~~~markdown
# ADR-0023: MISAKA Base + Three Execution Lanes (PQ-EVM / ETH-compat / Proof-verified Parallel EVM)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 不採用（将来の実装対象外）。

EVM の三つの実行 lane を PALW work に広げる案は実装しない。[ADR-0144](0144-palw-pays-for-the-inference-you-were-going-to-run-anyway.md) の「利用者が元々行うローカル推論に報酬を払う」範囲を超えて、別の実行・報酬面を増やすため。既存の [selected-parent EVM lane](0020-selected-parent-evm-lane.md) は独立した現行設計として維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0023-base-three-lane-execution.md -->

</details>

<a id="source-0024-verified-llm-token-weighted-bft"></a>

<details>
<summary>0024-verified-llm-token-weighted-bft.md — 原文 L1–L9</summary>

[原文を開く](../adr/0024-verified-llm-token-weighted-bft.md) · SHA-256: `05a9ca072dbcf2bd8ec905f1aa4487ae0cf451831b170eb402200c7516a53350`

<!-- ADR_SOURCE_BEGIN:0024-verified-llm-token-weighted-bft.md -->
~~~~~~~~markdown
# ADR-0024: Verified LLM Token-Weighted BFT for DNS finality

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Retired — VLT validator 投票重み・compute overlay は廃止済み。

VLT による validator 投票重みは再実装しない。PALW は validator 投票に依存せず、VLT 重みは [ADR-0128](0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) で削除、compute committee/beacon は [ADR-0134](0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) で廃止されたため。

旧実装は履歴であり、将来の実装計画ではない。
~~~~~~~~
<!-- ADR_SOURCE_END:0024-verified-llm-token-weighted-bft.md -->

</details>

<a id="source-0033-palw-credit-gate-wiring"></a>

<details>
<summary>0033-palw-credit-gate-wiring.md — 原文 L1–L9</summary>

[原文を開く](../adr/0033-palw-credit-gate-wiring.md) · SHA-256: `6d2dd04d25527f6cc6d8ea3a05bae8c704342d09a952540ec51ebfdfc26d85ac`

<!-- ADR_SOURCE_BEGIN:0033-palw-credit-gate-wiring.md -->
~~~~~~~~markdown
# ADR-0033: PALW V1 credit overlay（不採用）

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Withdrawn — [ADR-0037](0037-palw-async-job-state-machine.md) / [ADR-0038](0038-palw-is-the-consensus-work.md) により旧 credit overlay 方針を撤回。

DNS validator の投票・bond を使って hash PoW に credit を追加する方式は実装しない。有用計算そのものを consensus work にする現行 PALW V2 と役割が異なり、専用 DNS validator も [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の廃止対象であるため。

旧判定・processor の接続コードは削除した。Params の互換用 encoding だけを維持し、`palw_credit = Some` は設定検証で拒否する。
~~~~~~~~
<!-- ADR_SOURCE_END:0033-palw-credit-gate-wiring.md -->

</details>

<a id="source-0051-palw-metal-gguf-execution-family"></a>

<details>
<summary>0051-palw-metal-gguf-execution-family.md — 原文 L1–L7</summary>

[原文を開く](../adr/0051-palw-metal-gguf-execution-family.md) · SHA-256: `d1131aa5f63889adc1c352af799b8b77cdf0afc05ebe533ce97080cc6ad2c5b9`

<!-- ADR_SOURCE_BEGIN:0051-palw-metal-gguf-execution-family.md -->
~~~~~~~~markdown
# ADR-0051: The Metal/GGUF execution family — native-speed inference as half the work, quorum-verified beside the deterministic floor

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Superseded — 2026-08-26 に ADR-0053 で撤回。

Metal/GGUF の別実行 family は実装しない。対象モデルを整数 runtime で裁定できるようになり、別 family の必要性がなくなったうえ、提案した family cap・専用 panel・admission の安全条件がコードで成立していなかったため。後継は [ADR-0053](0053-palw-one-execution-family.md) の単一実行 family。
~~~~~~~~
<!-- ADR_SOURCE_END:0051-palw-metal-gguf-execution-family.md -->

</details>

<a id="source-0080-the-answer-is-long-the-verified-unit-is-short"></a>

<details>
<summary>0080-the-answer-is-long-the-verified-unit-is-short.md — 原文 L1–L9</summary>

[原文を開く](../adr/0080-the-answer-is-long-the-verified-unit-is-short.md) · SHA-256: `946f3b2014c864dbe78c0db2218d94eb4a2ed028364ba45f80731a0e86c55923`

<!-- ADR_SOURCE_BEGIN:0080-the-answer-is-long-the-verified-unit-is-short.md -->
~~~~~~~~markdown
# ADR-0080: The answer is long; the verified unit is short

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Superseded in part — 分割 claim 案は 2026-09-03 に撤回。

一つの回答を複数 claim に分割する方式は実装しない。claim をまたぐ出力の連結を認証できず、同じ prompt の後続 claim は DuplicateWork になり、分割で receipt 負担と報酬・weight の歪みが増えるため。後継は [ADR-0082](0082-the-close-is-flat-in-the-context.md) の有界 court。

既存の close chunk-group transport と、分割で経済量を増やさない条件は後継設計に引き継ぐ。
~~~~~~~~
<!-- ADR_SOURCE_END:0080-the-answer-is-long-the-verified-unit-is-short.md -->

</details>

<a id="source-0081-long-context-the-input-is-a-state-chain"></a>

<details>
<summary>0081-long-context-the-input-is-a-state-chain.md — 原文 L1–L9</summary>

[原文を開く](../adr/0081-long-context-the-input-is-a-state-chain.md) · SHA-256: `0307dcab39afc253b90fd526a7a1bf35ce9d82fbed72bc73595b52c94fd80deb`

<!-- ADR_SOURCE_BEGIN:0081-long-context-the-input-is-a-state-chain.md -->
~~~~~~~~markdown
# ADR-0081: Long context — the input is a state chain

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** Superseded in part — prefill 分割案は 2026-09-03 に撤回。

prompt を prefill claim の state chain に分割する方式は実装しない。既存 checkpoint は一つの claim 内に拘束され、claim をまたぐ state 連結を保証しないため。後継は [ADR-0082](0082-the-close-is-flat-in-the-context.md) の履歴を運ばない court と seat 側再計算。

単独で実装された Merkle prompt ids（`palw_prompt_ids_v1`）はこの撤回に含めず、ADR-0082 の規則に従う。
~~~~~~~~
<!-- ADR_SOURCE_END:0081-long-context-the-input-is-a-state-chain.md -->

</details>

<a id="source-0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays"></a>

<details>
<summary>0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md — 原文 L1–L9</summary>

[原文を開く](../adr/0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md) · SHA-256: `de30286f7c2294079e744e43d9362cb8696eb7c694baef6e104c686590e9e96d`

<!-- ADR_SOURCE_BEGIN:0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md -->
~~~~~~~~markdown
# ADR-0126 — The validator carve drops to a fifth, and the stake reorg gate stays

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

validator に報酬の 20% を残し、stake reorg gate を維持する方針は今後採用しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で validator 割当と veto を廃止し、PALW に consensus authority を一本化するため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md -->

</details>

<a id="source-0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate"></a>

<details>
<summary>0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md — 原文 L1–L9</summary>

[原文を開く](../adr/0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md) · SHA-256: `f0f7a55aa77231b4d9adee69cc53525bfbb387bf763c81585b689cf96253de5c`

<!-- ADR_SOURCE_BEGIN:0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md -->
~~~~~~~~markdown
# ADR-0128 — DNS validators vote BFT by bonded stake, and that vote decides the stake reorg gate

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

**Status:** 将来設計では不採用。旧ネットワークの実装・規則は移行まで有効。

bond-weighted BFT・precommit・inactivity leak による DNS reorg veto は今後採用しない。[RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) で validator 投票が PALW の chain selection・settlement を止める依存を除くため。

DNS 廃止は未有効。既存ネットワークの旧規則・履歴再生は [RFC-0012](../rfc/0012-palw-only-consensus-and-native-evm-settlement.md) の移行成立まで維持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0128-dns-validators-vote-bft-by-bonded-stake-and-that-vote-decides-the-stake-reorg-gate.md -->

</details>

<a id="source-0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes"></a>

<details>
<summary>0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md — 原文 L1–L135</summary>

[原文を開く](../adr/0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md) · SHA-256: `55d45bb241cfd5c54eb4413623394f325b2f4269df1e0d00af35e86df95342f4`

<!-- ADR_SOURCE_BEGIN:0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md -->
~~~~~~~~markdown
# ADR-0134 — The compute overlay's committee beacon retires at a height, and its machinery goes

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


* Status: **ACCEPTED and IMPLEMENTED 2026-09-17** on `feat/palw-exec-lane-and-validator-retirement`. testnet-11
  schedules the retirement at DAA 6,201 — its own height, not the 6,001 flag day's (§4). The stateful machinery
  is deleted in the same change (§3), on the evidence of §2.
* Operator's direction, in the operator's words: "DNS/VLT の committee beacon の使用されてない経路 コードの削除も追加で
  完了して — これはバリデータに依存してる — PALW はバリデーターを巻き込まないように進めて — 現在の PoW を LLM に置き換える
  方針からずれているため使用しない"; and, on the order of work: "DNS/VLT/beacon 削除も今回と混ぜない … 同じ activation
  fence には入れない方がデバッグしやすい".
* Builds on: [0024](0024-verified-llm-token-weighted-bft.md) (the overlay this retires), [0126](0126-the-validator-carve-drops-to-a-fifth-and-the-stake-reorg-gate-stays.md)
  (what ADR-0126 already removed and what it kept "because testnet-10 and testnet-11 run its shadow"),
  [0132](0132-what-a-model-is-actually-paid-per-forward-it-ran-and-why-the-gap-is-liveness-before-it-is-price.md) /
  [0133](0133-verification-is-its-own-clock-a-class-verifies-over-spans-and-a-starved-class-stops-only-itself.md)
  (the fences this one is kept apart from), the memo *Codex's beacon/VLT cleanup is a silent fork* (why a deletion
  needs a height and evidence).

## 0. The sentence this ADR is

**The VLT compute overlay — a verifier committee drawn by a DNS-epoch beacon, a credit walk, an audit fee and a
challenge slash, all of it validator-dependent — has never been reached by a transaction on any shipped chain;
its five subnetworks are refused past a fenced height and accepted-and-ignored below it, and the machinery that
would have read them is deleted now, because no block a node will ever validate contains one.**

## 1. What the overlay was, and what PALW never used

ADR-0024's v0.1: an executor commits a job (`ComputeCommitment`), the DNS epoch *after* the commitment's anchors a
sortition beacon (`commitment_beacon_epoch`), a committee is drawn from validators' capability declarations
(`select_verifiers`, `capability_candidate_pool`), the certificate (`ComputeCertificate`) collects the committee's
verdicts (`ComputeVerdict`, `verdicts_for_certificate`), a credit walk (`walk_compute_overlay`,
`resolve_certificate`) resolves it, an audit fee pays the verifiers from the validator pool
(`compute_audit_fee_outputs`), and a fraud proof (`ComputeChallenge`) slashes a bond after adjudication
(`compute_challenge_adjudication_slashes`, `adjudicate_compute_challenge`, `check_compute_challenge_genuine`).
Every step names a DNS validator: the committee is validators, the beacon is a DNS epoch anchor, the fee is the
validator pool. ADR-0126 removed the voting-weight half; this ADR removes the rest.

PALW's own lotteries never read any of it (memo *PALW block production has no beacon*): the attempt's ticket is
`H(execution ‖ anchor)` (ADR-0072), the panel is drawn from PALW bonds with `H(anchor ‖ claim ‖ operator)`
(ADR-0124/0130), the execution lane's seed is an attempt-carrying chain block (ADR-0130), and the free-prompt
lane's beacon is a fold of attempt blocks (ADR-0074) — a different beacon, kept. What PALW shares with `vlt.rs`
is types: the job spec, the receipt hash, the runtime class id and the pinned model entries. Those stay.

## 2. The evidence: no shipped chain carried one of these transactions

A read-only walk of testnet-11 through the explorer node on 2026-09-17 (DAA 5,797; `getBlocks` from the pruning
point, every block, every transaction — [`0134-testnet-11-subnetwork-census-2026-09-17.json`](../evidence/adr/0134-testnet-11-subnetwork-census-2026-09-17.json)):
5,798 blocks, 17,522 transactions; by subnetwork: coinbase 5,798, `0x4b` (PALW) 1,918, native 196, `0x4a` 107,
stake-bond 10, attestation-shard 9,493; **compute certificate / challenge / capability / commitment / verdict
(`0x14`–`0x18`): 0**. testnet-10 is not a supported entry point; devnet and simnet are fresh chains; mainnet has
no chain. The VLT worker that would have produced these transactions was deleted by ADR-0126.

## 3. Decisions

**Decision 1 — a fence, its own.** `Params::palw_compute_overlay_retired: Option<ForkActivation>`, hashed
`Some`-only, named to the fork-id gate. Past it a block carrying a transaction on any of the five compute
subnetworks is invalid (`RuleError::ComputeOverlayRetired`) and the mempool refuses one
(`TxRuleError::ComputeOverlayRetired`); `SubnetworkId::is_compute_overlay` names the five. Below it the
transactions are accepted and do nothing, as they always did. testnet-11: **DAA 6,201**
(`PALW_RC_COMPUTE_OVERLAY_RETIRED_FENCE_DAA`), two hundred past the 6,001 flag day, so the two builds and the
two failure domains are told apart (the operator's order of work, ADR-0133 §9a). Every other preset: `None`
(a card decides for mainnet). The fingerprint moves; the identity does not.

**Decision 2 — the machinery is deleted now, on §2.** Deleted: `walk_compute_overlay`, `resolve_certificate`,
`ComputeOverlayWalk`, `ResolvedCertificate`, `stage_compute_capabilities`, `compute_audit_fee_outputs`,
`compute_challenge_adjudication_slashes`, `verified_capability`, `verified_commitment` (the virtual processor);
`check_compute_challenge_genuine`, `compute_challenge_genuine` and the challenge slash at acceptance (UTXO
validation); `DbComputeCapabilityStore` and its store module; `commitment_beacon_epoch`,
`verdicts_for_certificate`, `capability_candidate_pool`, the `Compute*Record`s and the `compute_*_from_accepted_txs`
readers (`dns_finality.rs`); `select_verifiers`, `adjudicate_compute_challenge`, `ChallengeOutcome`,
`VltCreditSkipReason`, `commitment_dependency_horizon`, `commitment_within_dependency_horizon`,
`verify_compute_certificate`, `refutation_quorum_reached` (`vlt.rs`); and their tests. Why now and not after
the height is pruned: the only block-validity rules among them were the challenge-genuineness refusal and the
challenge slash, and both act only on a block that carries a compute challenge — §2 says no block does. A node
syncing testnet-11 from genesis validates the same blocks the same way with or without the deleted code; the
fence is what keeps the future from ever needing it.

**Decision 3 — what stays, and why.** The payload types and the stateless shape validators
(`validate_compute_*_payload`, `validate_compute_challenge_tx`) stay: below the fence a well-formed compute
transaction is still *accepted* (and ignored), so the shape check is still the rule in force there. `DnsParams`
and `VltParams` keep every field (the whole struct is hashed; `VltParams::INERT`, the shadow fence and the cost
table are fingerprint inputs). The database prefixes of the deleted store stay reserved. The ADR-0022 overlay
commitment (`compute_overlay_snapshot`: bonds, reserve, window — not the VLT) stays; its name is a verb.

**Decision 4 — the PALW lotteries' domain separation from the VLT's is kept by the key.** The tests that drew
a VLT panel to show the PALW draws differ from it now draw under `VERIFIER_SORTITION_KEY` directly; the key
stays pinned apart in the domain-uniqueness tests.

## 4. Security amendments

* **SA-1 — a flag day, stated as one.** A node without the fence accepts a compute transaction past 6,201; a
  node with it refuses the block. Every testnet-11 node runs a build that schedules it before 6,201 — the same
  discipline as the 6,001 flag day, and the fork-id gate names the height, so a build without it is refused past
  it rather than forked silently.
* **SA-2 — the deletion is history-safe by measurement, not by argument.** §2 is a census of every block; a
  chain that had carried a compute challenge would have needed the deleted rules to validate it, and this ADR
  would have had to keep them until the height was pruned.
* **SA-3 — the tenth of the validator pool the audit fee could have spent** stays in the pool's remainder: no
  new issuance, no output that a block before 6,201 could have carried and now cannot.

## 5. What this does not do

It does not retire the DNS validator overlay: attestations, the stake reorg gate, ADR-0128's BFT vote and
ADR-0126's carve are live on testnet-11 and stay. It does not touch PALW's own beacon (ADR-0074's fold, the
free-prompt lane). It is not in Fence 1 (liveness), Fence 2 (the single lottery) or Fence 3 (economics) of
ADR-0133 §9a.

## 6. Implementation record

* 2026-09-17, `feat/palw-exec-lane-and-validator-retirement` (`4e1757af` and the closing commit): the fence,
  the block and mempool rules, the deletions and the test rewrites; testnet-11's fingerprint re-pinned
  `ab4e7b9c…` → `dd805c9f2c4e9db3c0d6ffa2d87fa6ffb4263078ab8b7eb857f8fb11f8aa010c` (the schedule gained 7,201, so
  the fork-id gate separates this build from the 7,001 union build and the 7,000 release), and the same evening
  → **`135b6ee07ba0c5e5951c3cb765ba9dfec8b85af4c6edccc5a2246a52338e766b`** when the operator moved the heights
  (held regime and deep audit 7,000 → 6,000, the flag day 7,001 → 6,001, this retirement 7,201 → 6,201, the two
  hundred kept; the tip was 5,798); the fork-id and
  fence-set pins extended (`the_shipped_schedules_are_measured_not_assumed`, the gate sets, the flag-day
  counterfactual, the carded-mainnet comparison — a card arms the retirement from genesis); the operator docs'
  schedule lines print `…, 6000, 6001, 6201, 6900, 2125000`. New pins: `adr0134_the_compute_overlay_retires_at_its_own_height`,
  `adr0134_the_five_compute_subnetworks_are_the_overlays`. Verification (this branch, the closing run):
  consensus-core 2,140 (nine VLT/DNS committee tests deleted with their subject), consensus (evm) 296 (nine
  compute-challenge tests deleted likewise), kaspad 98, misaka-cli 176, rpc-core 148, rpc-service 1, misaka-palw
  24, misaka-palw-extension 2, pq-validator-core 12 + pq-validator 42, mining 92, integration
  `rpc_tests::sanity_test` 1 — all green; clippy `-D warnings` over fourteen crates clean.

## 7. Number hygiene

0134 was free when written; the next free number is 0135.
~~~~~~~~
<!-- ADR_SOURCE_END:0134-the-compute-overlays-committee-beacon-retires-at-a-height-and-its-machinery-goes.md -->

</details>

<a id="source-0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation"></a>

<details>
<summary>0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md — 原文 L1–L160</summary>

[原文を開く](../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) · SHA-256: `ff94c29ce145912b07651c7b24ad79039e98ccad0f3e0c3ae51cfbe97bfe2665`

<!-- ADR_SOURCE_BEGIN:0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md -->
~~~~~~~~markdown
# ADR-0173 — A seat holds a root, not a class: possession is keyed `(bond, class, root)`, a new root is NotReady until its floor, the draw reads the claim's own root, and a model's activation waits for possession (validation and canary are the dormant skeleton)

> **変更管理:** 採用済み部分は原則凍結。パラメーター調整と重大問題時の新規RFC + ADRは [凍結規程](../governance/design-freeze.md) に従う。Draft全文の採用や有効化ではない。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **2026-10-10後続改定 future reward-profile boundary:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md) abolishes MISAKA Torrent / dedicated Seeder rewards and withdraws the added availability/FPR gate. The earlier seat-root proposal retains its dated implementation/readiness scope, not a full-model acquisition guarantee. Future model allocation and conditional prosecution have separate unresolved gates; no runtime activation occurs here.

**Status:** PROPOSED 2026-10-04 on `rcore/post5300-audit` (lane MU), the user's request after the DAA-5,300 release. Consensus change behind the
**same** dormant fence as the audit fixes, `Params::palw_audit_1004_v1` ("今回の監査修正で"): below it every path folds byte for byte as at
`0b1c11b87`, and on every shipped preset the fence is `None`. Amends **ADR-0088** (a version is in force from its publication — now: claimable from
its possession), **ADR-0135** Decision 4 (a readiness row is per class — now per root) and **RFC-0002 Part II Proposal A** (the seating floors are asked
of the claim's root). Touches **RFC-0004 §17.4.1** (a head moves only to an Active, seated class). Takes **no object tag**, **delta number 150**
(declared explicitly: `SeatRootReadiness`), **carriage tail `0xEB`**, no wRPC op and no DB prefix.

**Builds on:** ADR-0088 (model lines, versions, `PALW_VERSION_GRACE_DAA_V1` = 4,000), ADR-0133 §11.2 (the V2 possession multiproof), ADR-0135 (the
registry's readiness predicate), RFC-0002 Part II (`palw_class_seating`), RFC-0004 (candidates are new classes).

## 0. The sentence this ADR is

**A claim that names a root can be accepted only while `seat_count` operators besides its executor have shown they hold THAT root, and its panel is
drawn only from seats that have.**

## 1. The finding

ADR-0088 Decision 3 puts a published root in force from the accepting block: the founding root, or every Active line's current root, previews, and
superseded roots inside the grace. But a seat's readiness is keyed `(bond, class)` and the V2 possession proof is verified against the class's
**registered** root only. So when a line publishes R2 for a class whose seats proved R1:

* an R2 claim is accepted at once (the root is in force) and its panel is drawn from seats whose only proof is R1;
* those seats cannot replay R2, the claim cannot reach quorum, and it voids (`PanelUnavailable` / `NoCapablePanel`) — a claim the producer paid for, a seat
  blamed for a model it never held, and a liveness lever for anyone who may publish a version.

The invariant the chain must keep: **if an R2 claim can become valid, a required number of independent seats that actually hold R2 exist.**

## 2. The three conditions

| | meaning | enforced here |
|---|---|---|
| **Possession** | seats hold the root | **yes** — this ADR's rules (R1–R5) |
| **Validation** | the root is an allowed, safe artifact: data-only formats (TIR inventory, safetensors/GGUF lowered), a canonicalised root, and an attestation signed by independent operators over the format, the manifest and the behaviour-check results; the publisher alone cannot activate | **no** — `palw_root_activation_v1.rs` (types, signed message, counting rule; unwired) |
| **Activation** | `Candidate → Canary (claim-limited) → Active`; per-root immediate revoke; rollback to the previous root | **partly** — R5 gates activation on possession now; the canary / revoke stage machine is the unwired skeleton |

Possession is checkable from chain state alone (it is a Merkle opening); Validation is a judgement about content and needs independent signers;
Activation is a policy over both. Keeping them separate is what lets Possession ship now without deciding the other two.

## 3. The rules (all behind `palw_audit_1004_v1`)

**R1 — readiness is keyed `(bond, class, root)`.** A new table `seat_root_readiness: (bond, class, root) → PalwSeatReadinessRowV1` (delta 150, tail
`0xEB`, rooted only when non-empty). **The existing `(bond, class)` table IS the founding root's**, so the fence needs no migration: a legacy row reads
as the registered root (`PalwChainStateV2::seat_readiness_for_root`). The V2 proof object is unchanged — **no new tag**: the multiproof already
commits to one root, and the signature already covers the proof. The fold reconstructs the root the proof opens (`reconstruct_artifact_multiproof_root_v1`,
the verifier's walk without its final comparison) and writes: the registered root → the legacy row, as before; a root the class has in force
(`class_roots_in_force`, which includes previews and superseded-in-grace) → its own row; anything else → `ReadinessProofRefused`. A composite class's
possession root stays its adapter section; a composite takes no other root. A root row's replay rule (`ReadinessProofNotNewer`) and landing window are
the legacy row's. Writing a seat's row for a class also drops that seat's rows for roots that left force, so the table is bounded by roots in force.

**R2 — a new root is NotReady until its possession floor.** A claim naming a root other than the registered one is refused with `RootNotSeated
{ class, root, have, need }` until `|Ready(root) \ {operator(executor)}| ≥ seat_count` — the possession rule of `palw_class_seating_v1`, over the root's
rows (`check_root_possession_v1`). The same door runs for attempts (the attempt names its root) and free-prompt commitments (attributed to the
founding line's current root, as `note_claim_usage` does). Where `palw_class_seating` is armed the class-seating floors (possession and independence)
are asked over the root's seats as well (`check_class_seated_root_v1`), so a class whose founding seats have stopped re-proving R1 is not unseated for an R2
claim. Claims naming the founding root are asked what they were asked.

**R3 — the draw reads the claim's root.** `palw_bond_may_judge_class_v5(…, root, …)`: a seat is eligible for a claim only with a fresh row for the root in
`claim_roots[claim]` (the founding root when absent). The readiness policy carries `root_keyed`, set from the fence at the claim's anchor; the stake
census keys its verdicts and judge vectors by `(class, root)`. A seat holding R1 only is never drawn onto an R2 claim; an R1 claim keeps its R1 seats
through the grace.

**R4 — class-level readers count a seat that holds any root in force.** The lifecycle's ready count, the admission jury, the room and the registry read
judge a seat by `seat_readiness_class_v1`: the freshest of the legacy row and the rows of the roots in force. Without it a class whose founding root left
force after its grace would read as unseated while its current root is fully held. Below the fence it is the legacy row.

**R5 — possession gates activation.** `ModelVersionPublished` with `preview = false` and `ModelVersionPromoted` are refused with `RootNotSeated` until the
root's floor is met (no executor excluded). A new root therefore enters as a preview, seats prove it, and only then does it supersede the previous root;
the superseded root stays in force for the grace, so a claim is never without a root (**no gap**). A line whose new root never gathers its floor keeps its
current root: the failure is a stalled upgrade, not a void claim.

**R6 — RFC-0004: a head moves only to a class that is Active and seated.** A candidate is a new class and already goes `Candidate → Probation → Active`
with seating; the head switch (the epoch's winner) did not ask it. Past the fence `decide_epoch_v1` decides a winner that is not `Active` in the
registry's lifecycle, or not seated (possession floor with no executor excluded, plus the independence floor where armed), as
`NoChange { HeadNotReady }` (reason 6): the old head stays live. A rollback restores a previous head and is not gated.

## 4. Node side (producer / seat policy; no new trust)

* The registry read carries each class's roots in force and every seat's root rows, so a seat can see what it has not yet proved.
* **The readiness duty proves `(class, root)` for every root in force the seat holds the bundle of**: a held artifact resolves by its digest, the proof
  is the V2 multiproof over that root, and the superseded root keeps being proved during its grace (so R1 claims stay live). The duty's memo is keyed by
  slot `(class, root)`. Conformance, memory capacity and the proof lane are per `(class, root)` as they were per class.
* **Prefetch is a node hook** (`kaspad/src/palw_root_fetch.rs`). When the readiness duty finds a root in force (not the registered one) or a
  `Candidate` class it holds no bundle for, it runs the operator's `--palw-root-fetch-cmd` (no shell; arguments `<class_id> <root>`, then
  `btv2_infohash=` / `bundle_commitment=` when declared, then `drop_dir=<path>`), at most one command per tick and one run per want per
  5 minutes. On exit 0 — or whenever a bundle is dropped by hand — `--palw-root-drop-dir` (default `--palw-improve-artifact-dir`) is
  scanned; a bundle that declares the wanted class and roots to the wanted root joins the holdings and the next tick's duty proves it. A
  failed fetch is a `ROOT_BUNDLE_MISSING` log line and nothing else: it is never a ground for a slash, a void or an `Unavailable`.
  **WITHDRAWN, 2026-10-10後続改定:** the proposed default `misaka-model-transport` integration was not built in this branch
  and is abolished under ADR177. The earlier hook/readiness record stays historical; future model supply is operator-selected off-chain activity.
* Panel binary updates retain their versioned upgrade rules; no MISAKA model-transport integration is scheduled by this document.

## 5. Tests

`palw_state_v2/tests/adr0135/root_possession_v1.rs`: a proof of a published root writes its own row, reverts by delta and round-trips the carriage
(migration: an R1 proof lands in the legacy table, which reads as the founding root); below the fence R2 and an unknown root are refused; R2 claims
are refused until `seat_count` operators besides the executor prove it and the executor's operator never counts; R1 claims stay admissible through the
grace and R1 leaves force when it ends; an R1-only seat is never drawn onto an R2 claim (and the unkeyed draw is what it was); a version is
activated only once possessed; a head moves only to an Active seated class. `palw_root_activation_v1`: the attestation's counting rule and the stage
machine. Below the fence every existing test is unchanged; `scripts/release/t12-repin.sh --drift-only` shows no drift (the fence stays `None`).

## 6. What this does not do

* It does not judge a root's **content** (§2 Validation). A possessed root can still be a bad model; possession proves the bytes are held, nothing more.
* It does not make the founding root's rows per-root *before* the fence: nothing is migrated, nothing is read differently below it.
* The class-level readiness reads (RPC `readySeatsNow`, the panel view) count by R4; a per-root count is a read-only follow-up.

## 7. Open questions for the user

1. **Canary behaviour checks** — what runs in `Canary`? Candidates: (a) the class's reference evaluator on a fixed probe set must match the seats' replay
   bit for bit (already what the court does, so free); (b) a quality gate against the previous root on the line's own suite (RFC-0004's evaluation jobs
   on a pinned subset); (c) a safety suite. (a) is the only one that needs no new machinery.
2. **Canary limit and duration** — the skeleton uses 10 % of the class's claim capacity for ≥ 2,000 DAA. Different numbers, or per-class?
3. **Who validates** — which operators may sign a validation attestation (any Active bond outside the publisher's operator? a registered validator set?),
   how many are required (the skeleton: 3 distinct operators, one independent failure blocks), and whether attestations are paid.
4. **Revoke authority** — per-root immediate revoke by whom: the line's owner only (rollback), the validators (a failed check), or the court (a
   conviction naming the root)? Claims already accepted keep the root they named in every design.
5. **A new root that never gathers its floor** — R5 leaves the line on its current root forever. Should a preview expire (withdraw automatically after N DAA)?
6. **R6 and a winner that is not yet ready** — the epoch is decided as no change (its fees and rewards follow the no-change path). Alternative: defer the
   decision until the winner is ready, which needs an epoch state the fold does not have; preferable only if candidates routinely win before they are seated.

## Model acquisition outside consensus amendment — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。chainはmodel取得可否・Seeder登録/独立性・配布量/速度を管理せず、
旧必須Torrent、独立Bonded Seeder、PoR/Full Fetch/lease/TRDC/FPRと取得不履行に基づく資格/weight停止を撤回する。
model配布は任意のTorrent/mirror/共有契約で行い、取得失敗・非公開だけでSlash/Final延長を起動しない。

固定Model ID/root、weights/tokenizer/config/specは不変とし、変更は新Model IDとして登録する。
取得bytesは認証と全量復元rootで照合し、claimも同じroot/specをbindする。
任意のinfohash対応を登録する場合はimmutable bindingとし、必須取得経路にしない。
claim固有state/trace/output/witnessの有限な証拠責任は維持するが、modelの全量/反復range公開へ転用しない。
必要operandの認証と許可unit/累積scopeをADR177 D2で確定し、modelを得たfresh non-seat verifierの
計算反証/localization/exact courtを試験する。全model保有者拒否時の取得保証は撤回する。

重複しないminer拘束元本`S_m=sum_b C_{b,m}`からmodelのcoinbase予算を`f(S_m)`で配分する。
同じ元本をclaim数/rho/複数modelで増幅せず、総発行予算と個別bondのQ/B/R/F cap・共通DAA拘束を維持する。
資本預託だけでは支払わない。model別配分はblock頻度・DAA・fork choiceを自動変更しない。
公開参加資本が閉鎖自己資本より圧倒的に有利になることは倍率式・経済評価の未立証目標であり、
同額資本の所有者独立性をchainが識別できるとはしない。既存minerの実収入増も無条件保証しない。
non-interference、distinct-capital、三層予算、公開/閉鎖比較、回復/移行のgateはADR177 §3を用いる。
既存実装/測定・旧claim/activation記録は保持し、新配分・Panel=0は未実装/未有効化である。


## MISAKA Torrent・Seeder報酬の廃止 — 2026-10-10後続改定

[ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)に従い、MISAKA Torrentの採用/統合、専用Bonded Seeder、
Seeder報酬・固定15%配分の概念を廃止する。一般的な任意配布はoff-chain運用とし、
モデル入手の合意gateやSeeder向けcoinbase legへ復活させない。過去の設計/試験は撤回前の記録として保持する。
~~~~~~~~
<!-- ADR_SOURCE_END:0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md -->

</details>

<a id="source-0181-design-freeze-and-controlled-change"></a>

<details>
<summary>0181-design-freeze-and-controlled-change.md — 原文 L1–L22</summary>

[原文を開く](../adr/0181-design-freeze-and-controlled-change.md) · SHA-256: `dc0c6ec889bbd72020779f02fc1db4903cffc7d349f1d652843dafe9b8a169a3`

<!-- ADR_SOURCE_BEGIN:0181-design-freeze-and-controlled-change.md -->
~~~~~~~~markdown
# ADR-0181 — 採用済み設計を原則凍結し、重大問題時のみ新規 RFC・ADR で変更する

- **Decision:** Accepted — 今回のユーザー指示。文書化日 2026-10-17。
- **Scope:** 設計・文書の変更管理。合意方式・既存の数値を変更しない。
- **Implementation:** 文書へ反映。node コードは対象外。
- **Verification:** [文書検査](../maintenance/validation-report.md)。node の検証ではない。
- **Activation:** 文書管理規程として適用。合意 fence の有効化なし。
- **Related:** [RFC-0021](../rfc/0021-design-freeze-and-documentation-governance.md)、[PRINCIPLES §11](../PRINCIPLES.md#design-freeze)。

## 背景と決定

採用済み設計と未決案・過去の状態が混在し、通常作業で決定が繰り返し変更されることを防ぐ。設計変数・パラメーター以外は原則凍結し、重大問題に限り新規 RFC と新規 ADR の両方、レビュー、明示的採択による変更経路を残す。

## 採用理由・代替案

絶対に変更できない凍結では重大欠陥を是正できない。通常 PR だけで自由に変える方式では安定しない。既存 ADR の上書きだけでは当時の判断を失う。このため、原則凍結と証拠付き例外変更を組み合わせる。

## 帰結

意味不変の実装・修正・最適化・試験は継続する。数値を使った認証・責任・不変条件の廃止は通常調整としない。PRINCIPLES の変更も明示的承認と対象節の改定を要する。手続きの正本は [設計凍結](../governance/design-freeze.md)。

番号は添付 ZIP の次番号 0181 を使用した。他ブランチの予約は統合前に照合する。既存 ADR の技術的決定は今回置換しない。
~~~~~~~~
<!-- ADR_SOURCE_END:0181-design-freeze-and-controlled-change.md -->

</details>

<a id="source-0182-defer-media-and-model-improvement-until-post-mainnet"></a>

<details>
<summary>0182-defer-media-and-model-improvement-until-post-mainnet.md — 原文 L1–L30</summary>

[原文を開く](../adr/0182-defer-media-and-model-improvement-until-post-mainnet.md) · SHA-256: `8276d2dcbdd7068914774944c28e6b283542d004899ae847bccdc092a9a3350e`

<!-- ADR_SOURCE_BEGIN:0182-defer-media-and-model-improvement-until-post-mainnet.md -->
~~~~~~~~markdown
# ADR-0182 — 画像・動画経路とモデル改善を mainnet 後へ延期し、当面無効にする

- **Decision:** Accepted — 追加のユーザー指示。文書化日 2026-10-17。
- **Scope:** RFC-0003 の画像・動画経路、および RFC-0004 のモデル改善の開発優先度・初期提供・有効化時期。
- **Policy status:** DEFERRED_POST_MAINNET / DISABLED。
- **Implementation:** 方針を文書へ反映。コードの実際の無効化は未確認。
- **Verification:** 無効化・回帰試験は full checkout で実施が必要。今回 node 試験なし。
- **Activation:** 当面なし。mainnet 開始時にも自動有効化なし。
- **Related:** [RFC-0022](../rfc/0022-post-mainnet-media-and-model-improvement-scope.md)、[RFC-0003](../rfc/0003-palw-generative-model-classes.md)、[RFC-0004](../rfc/0004-palw-model-improvement.md)、[PRINCIPLES §12](../PRINCIPLES.md#post-mainnet-scope)。
- **Amends:** 上記機能を初期 mainnet / 次回の全面有効化の対象として扱う旧予定・完成条件。この範囲外の技術設計は置換しない。

## 決定

画像・動画の経路とモデル改善は mainnet 稼働開始後に取り組むものとして延期し、当面無効とする。初期 mainnet の必須実装・提供・有効化対象に含めない。mainnet 後も機能別レビューと明示的承認まで開かない。

## 採用理由

ユーザーが当面の提供対象を絞り、画像・動画・改善を後段に置く方針を指定したため。初期リリースの対象と責任を明確にし、過去の「全 RFC 完了」や共有 fence からの誤った同時有効化を避けるため。重大な欠陥が見つかったと仮定した決定ではない。

## 比較した扱い

初期 mainnet で同時に完成・有効化する扱いは今回の指示と一致しない。設計と証拠を削除する扱いは再開に必要な根拠を失う。mainnet 日付で自動的に開く扱いはレビュー・試験・承認を省略する。このため、資料を保持した明示的延期・無効化を採る。

## 帰結・留保

共通のテキスト・TIR・登録の不変性・Gate C・court・bond 会計を維持する。共有部分の欠陥が初期 mainnet に影響するなら release blocker のままであり、延期機能が閉じていることの確認も必要。

旧履歴・旧 claim の責任を遡及的に消さない。一般のオフチェーン研究、通常の固定モデル登録、既定の年次カタログ、RFC-0020 の較正・メモリ改善を RFC-0004 の protocol と同一視しない。

正本は [延期・無効化方針](../governance/post-mainnet-scope.md)。再開時の構造変更は ADR-0181 の凍結規程に従う。この番号は整理版内で新規採番したもので、統合先の予約は別途確認する。
~~~~~~~~
<!-- ADR_SOURCE_END:0182-defer-media-and-model-improvement-until-post-mainnet.md -->

</details>

<a id="source-0193-miner-account-bond-for-panel-incentives"></a>

<details>
<summary>0193-miner-account-bond-for-panel-incentives.md — 原文 L1–L156</summary>

[原文を開く](../adr/0193-miner-account-bond-for-panel-incentives.md) · SHA-256: `1954c6a83447c6dfdc4474536938e2e718c82303750b4bbf23c743c3bcbeae60`

<!-- ADR_SOURCE_BEGIN:0193-miner-account-bond-for-panel-incentives.md -->
~~~~~~~~markdown
# ADR-0193 — Panel の追及を支える Miner Account Bond: 最低10万と責任連動追加、過大拘束を避ける

- **Status:** Proposed / Review Required（未採択）。2026-10-20。
- **対応 RFC:** [RFC-0027](../rfc/0027-miner-account-bond-and-panel-incentives.md)。詳細契約・費用導出・移行・試験は同RFCを正本候補とする。
- **Owner / reviewers:** 未指定。設計責任者、経済、合意会計、検証/裁定、移行のレビューを要する。
- **Implementation / Verification / Activation:** 合意実装なし。数式検算と、旧node/外部検証の実測ログ再集計・shadow・合成資本実験を実行（§8）。新Account Bondのnode実測・有効化なし。
- **番号:** 添付ZIPではADR-0186までの本体を確認し、会話には未同梱のADR-0192への参照があるため、0193をローカル仮割当。他branchとの衝突確認は未実施。
- **採択者 / 採択日 / 有効化日:** 未記入。ユーザーの作成依頼を、この詳細案の採択または有効化と解釈しない。
- **通貨:** 本文のMSKは依頼時の旧ラベル。正式ticker BILIと同じnative coinであり、改名・単位変更ではない。[ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)。

## 1. 背景

ユーザーは、既存の約3,000 MSK/claimの予約bondと別に、最低100,000 MSKをMinerアカウントへ拘束し、claimの残存責任が増えたら追加担保を求める案を依頼した。新担保の主目的はPanelの不正追及に対する十分なインセンティブであり、既存予約bondの主目的であるclaim発行コストと区別する。担保の過大化によるclaim減少とheartbeat依存も同時に考慮する。

既存の[ADR-0124](0124-the-panel-is-paid-out-of-the-claims-reward-a-seat-holds-exposure-and-a-claim-is-paid-for-the-compute-it-certifies.md)にはPanel報酬・Panel自身の責任担保がある。「Panelに現在一切報酬がない」ことを変更理由にはしない。通常の検証報酬が、異常発見後の開示・対話・局所化・最終証拠・回復の費用まで十分に補償するかは別問題である。

[ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)はQ/B/R/F・共通拘束・責任保持に加え、小規模Minerの実用性とREAL継続の測定を要求する。[RFC-0026](../rfc/0026-court-policy-master-contract.md)は供給、inclusion前提、外部1人の訴追経路を未完課題としている。本案はその制約を維持して新しい資金裏付けを追加する。

## 2. 決定案

### D1 — 二つの目的を分離する

既存のclaim予約bondは、claim発行にコストを持たせる仕組みとして維持する。既存rulesetの損失担保・予約資源・返還条件を「単なる利用料」と読み替えず、式も変更しない。

新しいAccount Bondは、Minerが負う不正追及費用の償還、固定成果報酬、明示的な罰則の徴収可能性を確保する専用勘定とする。返還可能な元本を通常のPanel運営費として流用しない。

### D2 — 最低100,000、上乗せは責任が超えた分だけ

```text
必要Account Bond = max(100,000 MSK, 残存するaccount責任の総額)
追加必要額        = max(0, 必要Account Bond - 現在の専用ロック残高)
```

最低額と責任総額を足し算しない。基礎10万の中へ複数claimの責任を割り当て、同時に生じ得る請求は全件を計上する。最大1件だけ、平均参加率、確率的overbookingで全claimを保証したと扱わない。

仮に1件の新責任が12,000なら、8件は96,000で追加不要、9件は108,000で最低額より8,000増となる。12,000は未採択の説明値であり、既存予約・利用率制約・fee・他役割の拘束は別に必要である。§8の追記実験では長期責任下の継続発行を大きく制限したため、実用料金候補にはしない。

### D3 — 参加資格の新設を共通bond下限の変更で代用しない

新しい条件は対象rulesetのMinerの新規報酬対象claimにだけ適用する。既存共通 `min_collateral_sompi` は変更しない。Panel最低額をその10倍から導く経路へ10万が漏れると、Panel自身の必要資本まで増えるためである。

通常tx、wallet、relay、Panel-only、外部public verifier、heartbeatへ新しいMiner資格を課さない。既存の各役割の担保要件は維持する。非採掘経路から報酬権へ変換する迂回を認めず、同じclaimの単なる運搬へ重複した10万を要求しない。

### D4 — 通常報酬と不正立証報酬を分ける

通常検証は既存の適格Panel報酬経路を維持する。追加の通常報酬保証を設ける場合、資金化・支払権限を確認した実在ServicePoolを先に予約する。不正ゼロで収入ゼロになる没収金を、通常報酬の確実な財源にしない。

courtの開示・対話・局所化は、requesterまたは明示的第三者が用意した消費可能なFeeEscrowから、一意な完了milestoneごとに段階払いする。正しいclaimに対する棄却でも、適格に完了した仕事にはその料金を支払う。返還予定のbondを同時に支出しない。

Minerの違反が客観的に確定したとき、当該Account Bond予約から費用負担者へ適格実費を償還し、報酬資格者へ固定bountyを払う。既に作業料を受け取ったPanelへの同じ実費の二重払いはしない。成果報酬の資格はownerによる開示独占権ではない。

### D5 — 裁定と報酬額に自由裁量を入れない

Panelの投票や自己申告を最終的な計算不正認定にしない。合意が検証する客観裁定・固定tariff・違反種別・予約capで没収額を決める。抽選された検証者の承認と計算不正の客観裁定は別に扱う。

没収額は「適格費用償還 + 固定bounty + 不可還流罰則」。新account chargeの不可還流分はv1候補ではburnとし、同じ攻撃者が取り戻せる追加補助に再循環させない。配分は実没収額以下であり、受領者間の支払を合わせて一致させる。

前の会話の5,000 / 25,000 / 100,000や40/40/20は料金定数として採らない。account総額が大きいほどPanelのbountyが増える設計にはしない。1件へ予約した上限を超えて他claim用の元本を没収しない。

### D6 — 共有要求・報酬の重複・自作自演を一緒に扱う

RFC-0026と会話のF02/F03案の、owner非依存な義務identityと共有開示を用いる。同じ義務の同じ仕事には一回だけ払うが、最初のownerが選ばなかった別の合法choiceを消さない。全nodeに要求が広がるtreeなら全nodeのworst caseを導出する。

Miner・Panel・reporter・requesterが同じ主体である攻撃を必須評価し、鍵が違うだけで独立と仮定しない。任意courtへの無制限の一般財源補助を作らず、自己告発後も不可還流の純損失が残るようにする。旧10%/49%のreporter配分は旧chargeに維持し、新accountへ追加で掛けない。併給する場合は最初から総額capに含める。

### D7 — slash、停止、退出、復旧を区別する

slash後に最低額を割ったMinerは、新規の対象claimを停止して補充を要する。ただし、既に受理したclaimへの応答・開示・回復を禁止しない。担保不足自体で他の正しいclaimを有罪にしない。

返還はFinalの一語で判断せず、post-Final、Gate C、bondの別claim、未払/未償還費用、保持、monetary materialの責任を満たしてから。hard到達だけの無条件返還はしない。鍵変更・分割・退出再登録で責任と消費履歴をresetしない。

### D8 — heartbeatを変えず、Miner実用性を採択前に測る

必要Account Bondを抑える最初の手段は、最低額の二重加算と偽の重複責任を避け、実際に共用できる仕事を一回だけ数えることとする。全請求に満額を用意できないのに、責任を縮小して受理してはならない。

新しい10万は参入資本を増やす。既存生産資本から移せばheadroomも減る。総資本固定と新資金追加の両scenarioで、Panelの純利益、Minerの総必要資本、claim/REAL率、bond起因の停止時間、heartbeat依存、補充後REAL復帰を同時に測る。

heartbeatの資格・clock・fallbackは維持し、観測比率で担保要件を自動的に下げない。heartbeatゼロを合格条件にしない。両立できるパラメーター域がなければ有効化保留とし、最低額や報酬率の再検討は別途の明示承認とする。

### D9 — Account Bondで生産権を追加購入できる構造にしない

新account専用元本を、既存P側のQ/B/R/F、model配分score、rule E participation、verifier抽選の資本へ自動加算しない。同じlotを既存claim、Panel自身の責任、新accountへ重複使用しない。役割別の全reader・再起動・IBD・reorgを整合させる。

### D10 — 旧claimを守り、採択と有効化を分ける

旧claimには旧ruleset。新規則を取り込むだけでfenceを武装しない。original acceptanceで、既存予約、新account責任、全資源供給、料金、horizon、Q/B/R/Fを原子的に検査する。

RFC-0026の通常3,120 / hard10,000、十分な供給と明示的inclusion前提を要するdefault判定、共有frontier段階1のblockingを維持する。金銭的誘因の追加をもってF02/F03やG14の未達gateを解除しない。

## 3. 比較した案

| 案 | 判断と理由 |
| --- | --- |
| 既存claim予約bondを大幅に増やす | 不採用。目的が混ざり、全正直claimの発行余力を直接圧迫する |
| account10万を固定し、claim責任を無制限に積む | 不採用。全件同時不正で回収不能になる |
| `10万 + 全責任` を常に拘束 | 不採用。基礎額を活用できず、必要以上の参入・運転資本になる |
| **`max(10万, 全残存責任)`、独立した通常報酬、固定成果報酬** | **推奨。** 資金裏付けと資本効率を両立する構造。ただし数値の実用性は未証明 |
| 没収の大部分を裁定Panelへ割合払い | 不採用。重罰誘因・自己還流・高残高狙いを強める |
| 没収金だけで通常Panel費用も運営する | 不採用。不正ゼロ時に支払原資がない |
| heartbeat増加で自動的に担保/検証を緩和する | 不採用。withholdingによる値操作と責任保証の破壊を招く |
| 全amountを100,000へ共通パラメーター置換 | 不採用。Panel/告発等の倍率参照へ副作用が出る |

## 4. 旧文書の置換関係

**本作成で置換する採用済み条項はない。** 採択後の変更対象はRFC-0027 §14の対応表を正本候補とする。ADR-0176の資本読取と原子admissionへ専用勘定を追加し、ADR-0124の追及報酬、ADR-0032の新科目に接続する。旧支払率・旧claimの責任・旧wireの意味は維持する。

通貨表記、モデル二階層、モデル審査不採用、計算証明不使用、DNS-BFT不使用、Gate C、延期中の機能、heartbeatの仕様は変更しない。採択時には対応節、更新するSpec、料金候補の台帳、全reader、旧新claim境界を別途確認する。

## 5. 残余リスク

10万は未検証の参入制約であり、既存の小規模Minerを排除する可能性がある。全分岐・長い保持を積算すると追加担保が大きくなることもある。feeの立替と報酬成熟待ちがPanelの資本負担を残す。通常報酬・reserve支払の配線が未完なら、没収原資だけでは十分な参加動機にならない。

MSK建てtariffの実費への適合、検査者の正直な割合、署名だけでは見えない実検査、配送/包含の仮定、共謀還流と外部利得も残る。公開検証の制度があることと、実際に人が参加して全不正が検出されることは同じではない。

実効検出確率や不正の期待利益は測定指標として記録し、UNKNOWNを根拠なく埋めない。PRINCIPLES §0に反して検出確率の下限証明を最終gateにしない。一方、実在資金で予約した請求を全額支払えること、旧新責任の非二重使用、PanelとMinerの実用性は具体的に検証する。

## 6. 採択・検証・有効化の必要条件

RFC-0027のAB01–AB16、G-Collectible / G-Panel / G-Entry / G-REAL / G-Concentration / G-Compatibility / G-Courtをレビューし、測定閾値を事前固定する。合意実装、資金保存、旧claim、default、全owner共謀、reorg/IBD、heartbeatからのREAL復帰を独立nodeで確認する。

[検証記録](../rfc/0027-assets/validation.md)のオフライン数式PASSは、これらのnode試験の合格ではない。RFC・ADR双方の明示的採択、実装/試験レビュー、対象networkの有効化承認を分ける。新額の本番設定、既存generic下限、fence、fingerprintを本作成では変更しない。

## 7. 履歴

2026-10-20: ユーザー依頼に基づく初版作成。金額の最低値と目的を保持し、最低額の内包方式、報酬三分離、共有責任会計、過大担保・heartbeat対策、未確認事項を追記した。承認者・採択・実装・有効化の記録はまだない。


## 8. 2026-10-20 追記測定に基づく評価

### 8.1 実施と証拠区分

[測定レポート](../rfc/0027-assets/measurement/measurement-report.md)と[RFC-0027 §18](../rfc/0027-miner-account-bond-and-panel-incentives.md#18-2026-10-20-追記--実測ログ再集計とaccount-bond負荷実験)を追加した。添付の過去node時系列17 run / 2,563 claimと、外部検証21 sampleを再集計し、1,320条件のshadow、326条件の合成資本実験を新たに実行した。再集計・保存則1,365項と解析ハーネス32単体試験はPASS。これは新Account Bondを動かしたnode測定ではなく、AB01–AB16のnode合格ではない。

新Account Bondの合意・支払実装、元の大型artifact、Rust toolchainがこの実行環境に揃わず、今回のfresh node / 大型モデル再実行は未実施。履歴実測、実行した合成実験、仮定付き金銭換算を分離して記載する。

### 8.2 最低10万に関する評価の更新

**最低100,000 MSKだけで、十分なPanelインセンティブと実用的claim頻度を両立できたとは確認できない。** 前版の1件12,000という例では8件分しか入らず、責任が適法に終了する時刻を仮に10,000 DAA後へ置いても、新accountだけの継続上限は0.576件/720 DAAとなる。30件/720 DAAを検討するなら同時417責任、1件12,000の場合はAccountだけで5,004,000 MSKが必要になる。

同じHで10万に収めるには、5件/720 DAAなら1件責任1,428.57142857以下、30件なら239.80815347以下、90件なら80以下が必要という条件付き逆算を追加した。これは費用をその値へ無理に切り下げる決定でも、実用閾値の採択でもない。全合法jobのL_cが収まらなければ必要な追加資本を明示する。

通常3,120 / hard10,000は返還保証時刻ではない。実際のpost-Final・未解決保持が長ければ必要資本も増える。未解決releaseなしの合成試験は8件の後で止まり、hard到達で返還しない不変条件を残した。

### 8.3 Panel報酬の判断

旧実測ではQwen2.5-0.5Bの正常検証でCPU中央値141.85秒（3 positions、n=5）、late違反の局所裁定まで628.31秒（32 positions、n=1）。一方、4 sampleはfilingのwire上限で拒否されており、資金があっても裁定経路の完成を保証しない。

料金の実地単価はなく、MSK換算は明示した仮定による感度分析である。1-jobに運用費と利益・bounty・burnを含めた参考例では部分責任396.63 MSKとなり、H=10,000で30件/720 DAAを10万だけで支える条件239.81を超えた。全job・同profileのL_cは未導出なので、局所的に収まる例もG-Panel/G-Entryの合格にはしない。

既存予約のA16価格例と外部検証Qwen2.5の費用を異なるprofileのまま結合し、同一profileでの両立達成にすり替えない。通常報酬の資金化と実支払、不正ゼロ時の採算、実Panelの労力は未達条件として残す。

### 8.4 決定案の取り扱い

D1–D10の構造とユーザー指定の最低10万を保持する。最低額は参加条件であり、実用頻度の保証額ではないと明確化し、12,000の例を未測定の実用tariffとして昇格させない。目標頻度・同profile全job・実保持から必要資本を逆算し、総資本固定と新資金追加を分けて示す。

shadow/合成試験で失った受付機会をheartbeat増加数と表示しない。新規則での採用REAL、heartbeat依存、補充後REAL復帰は未実測。経済の十分性・F02/F03・旧claim互換性・nodeの責任会計のgateを解除せず、**Proposed / Review Requiredと有効化保留を維持する。**
~~~~~~~~
<!-- ADR_SOURCE_END:0193-miner-account-bond-for-panel-incentives.md -->

</details>


# RFC index — number order, one line per RFC

> **凍結前提(2026-10-15):** genesis・年次フォーク指定モデルは 1.0、後から登録したモデルは登録時点から審査なしで 0.3 固定(1.0 への経路は年次フォーク指定のみ)。モデル単位の検証は不採用([PRINCIPLES §9](../PRINCIPLES.md))。claim 単位の検証は全モデルで維持。

> **恒久決定(2026-10-15):** LLM・モデル計算の正しさを暗号学的に証明する仕組み(ZK、zk-SNARK/STARK、zkVM・zkML、PCS ベースの succinct proof など)は一切使用せず、今後も使用しない([PRINCIPLES §8](../PRINCIPLES.md))。

> **設計前提(ADR・RFC より上位)— [MISAKAの不可侵原則](../PRINCIPLES.md):** 確率的に検出し、公開証拠で局所化し、決定論的に裁き、経済的に不正を抑止する。この索引のすべての文書はこの前提の下にあり、衝突する場合は前提が優先する(2026-10-09)。

> **PALW共通前提 — 2026-10-10:** [ADR-0176](../adr/0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](../adr/0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


**トークン名は Misaka、ticker は BILI。** アドレスの `misaka` 系プレフィックスと既存のチェーンID・コマンド・wire/API識別子は維持する。過去の実測・出力のMSKは旧表記として保存する（[ADR-0174](../adr/0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)）。

RFC は議論中の提案です。RFC がどう ADR と Spec の変更になるかは、旧版が参照していた `docs/INDEX.md` §3 が**この系列に存在しない**ため、ここには再掲しません(旧 `INDEX.md` は未 merge の branch `docs/restructure` にだけある — [../design/palw/unmerged-inventory-2026-10-15.md](../design/palw/unmerged-inventory-2026-10-15.md))。文書全体の入口は [../README.md](../README.md) です。番号は branch 上でも先に使われうるので、**次の番号は ls ではなくこの表から取ります。**

**Next free number: RFC-0021.**

| 見たいもの | 場所 |
| --- | --- |
| どの RFC を最初に読むか、トピック別の一覧 | [INDEX-by-topic.md](INDEX-by-topic.md) |
| 旧 README の本文(長い Status 欄、2026-10-10 後続改定、post-commit challenge protocol、撤回された案、「提案として読める ADR」)。**一字も変えずに移した** | [HISTORY.md](HISTORY.md) |
| 決定 → 実装 → 検証 → armable → activated の現在地 | [../STATUS.md](../STATUS.md) |
| ADR の索引 | [../adr/README.md](../adr/README.md) |
| 重複番号(RFC-0010 / RFC-0012 の companion、spec 18-*)の記録 | [../adr/NUMBERING.md](../adr/NUMBERING.md) |

**読み方の注意。** 「Status」列は各 RFC ファイル自身の Status 行の冒頭です。「Draft」「Revised Draft」「Accepted design」は**設計の状態**で、実装済み・検証済み・有効化済みを意味しません([../STATUS.md](../STATUS.md))。† は同じ番号に 2 つのファイルがあることを示します。

| # | Title | Status (from the RFC) | Note / successor |
| --- | --- | --- | --- |
| 0001 | [PALW 推論サーフェスの欠落機能 — 決定論的な生成制御・サービング・入力拡張の設計](0001-palw-inference-surface-gaps.md) | §A (the FP Job V4 release) Implementation Frozen, 2026-09-27 (G0). §0–§7 (P1–P3) Draft, for a later release | — |
| 0002 | [PALW Canonical Tensor IR v1 (PALW-TIR) — a bounded, deterministic integer tensor program as the…](0002-palw-tensor-ir.md) | Revised 2026-10-08 — existing implementation/test records keep their scope; new policy binding unactivated | — |
| 0003 | [PALW Generative Model Classes — one job, determinism and output layer, and the class profiles on…](0003-palw-generative-model-classes.md) | Revised 2026-10-08 — new challenge-policy boundary; historical profile/tests unchanged | — |
| 0004 | [PALW Model Improvement Protocol — self-improvement and distillation over PALW-TIR](0004-palw-model-improvement.md) | Revised 2026-10-09 — 独立immutable fenceとcandidate選択をpreに実装、全preset休眠。memory materialの担保条件・kind別MEASは未決/未測定 | — |
| 0005 | [PALW VM（不採用）](0005-palw-ml-vm.md) | Revised Draft, 2026-10-08 — §§K.2/K.4 use RFC07; no VM fallback or activation | 不採用(VM)。理由だけを保持し、後継は [ADR-0172](../adr/0172-model-extensibility-uses-versioned-kernels-not-a-universal-vm.md)(versioned kernels) |
| 0006 | [PALW layer-sharded panels — bounded cell verification from authenticated boundaries, with…](0006-palw-layer-sharded-panels.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0007 | [PALW probabilistic detection — direct checks, authenticated receipts, and deterministic court](0007-palw-verification-certificates-and-algebraic-checks.md) | Revised Draft, 2026-10-08 — source/codec/security/implementation gates open; no beacon or activation proof | — |
| 0008 | [PALW EXEC lane — claim-backed work slices and transaction execution](0008-palw-claim-backed-consensus-blocks.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0009 | [PALW Remote Client — node を持たないモデル登録・claim・非保管型の報酬回収](0009-palw-remote-miner.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0010 † | [RFC-0010 dormant implementation and permissionlessness audit](0010-dormant-implementation.md) | Status 行なし — ファイルを開いて確認 | RFC-0010 の休眠実装・permissionlessness 監査(同番号の別ファイル、[NUMBERING.md](../adr/NUMBERING.md)) |
| 0010 † | [Permissionless PALW Panel binding and claim completion](0010-permissionless-palw-panel-and-claim-completion.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0011 | [Permissionless model onboarding with probabilistic constraint verification and court on dispute](0011-permissionless-model-and-long-context-onboarding.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0012 † | [RFC-0012 dormant implementation](0012-dormant-implementation.md) | implemented behind an unassigned Params::palw_dns_retirement fence. All shipped network presets use None. No network, activation height, D/W… | RFC-0012 の休眠実装(同番号の別ファイル、[NUMBERING.md](../adr/NUMBERING.md)) |
| 0012 † | [PALW-only consensus and native EVM settlement: retire DNS validators and their reorg veto](0012-palw-only-consensus-and-native-evm-settlement.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0013 | [Reproducible layouts and resource-bounded model onboarding tools](0013-reproducible-layouts-and-resource-bounded-onboarding-tools.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0014 | [Panel多数派に依存しないPALW fraud prosecution: 1人の正直なverifierから有界の客観証拠へ](0014-panel-independent-fraud-prosecution.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0015 | [Panel=0: 固定Panelからpermissionless verifierとobjective fraud proofへ移行する](0015-panel-free-permissionless-verification.md) | Revised Draft / deferred where applicable, 2026-10-10後続改定 — ADR177 economics/allocation/implementation gates open; old runtime/tests retain scope; no… | — |
| 0016 | [bond加重のブロック生成機会と弾力的Final容量: 物理ブロック機会をbondへ接続する](0016-bond-weighted-block-opportunity-and-elastic-final-capacity.md) | Accepted design (V+), values pending measurement, 2026-10-15 — implemented dormant on integ/rfc0016-impl (unbuilt; fences refused when armed until measured values are pinned)… | — |
| 0017 | [モデル階層: genesis・年次フォーク 1.0、登録モデル 0.3(モデル単位の審査は廃止)](0017-model-eligibility-examination-and-renewal.md) | Draft, 2026-10-15 — tier fence palw_model_tier_v1 implemented, dormant, unverified (ADR-0179); the old review code dormant and unreachable under it… | モデル単位の審査は廃止。採用された階層規則は [ADR-0179](../adr/0179-model-tiers-genesis-annual-1-0-registered-0-3.md) |
| 0018 | [DAGを最大限に活用する設計変更: 多モデル並行生成、特権なきPanel anchor、SecurityCertified(Gate C)とDNS廃止後の収束](0018-dag-parallel-multi-model-consensus.md) | Draft, 2026-10-15 — implemented, dormant, unverified… | — |
| 0019 | [年次モデル追加フォーク: 段階的ソフトウェア展開・年次モデルカタログ・合意は不変](0019-annual-model-upgrade.md) | Draft, 2026-10-15 — implemented, dormant, unverified… | — |
| 0020 | [長文脈の較正・変換・忠実度計測のメモリ: 32K の依存構造を保ち、同時に持たなくてよいものを持たない](0020-long-context-calibration-memory.md) | Draft, 2026-10-16 — tooling only, no consensus surface; implemented, unverified (ram_account.rs, float_ref/stream.rs, palw-tir-convert/palw-tir-fidelity flags)… | — |

## このディレクトリのその他

- `README.new.md`
- `evidence` (directory)

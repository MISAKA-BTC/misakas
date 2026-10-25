# ADR-0026: Current PALW architecture on testnet-11

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Mission alignment, 2026-10-07:** [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md) governs future PALW design. Where the earlier body conflicts with ordinary non-Panel public-bond prosecution from authenticated public material, without producer-private state, the dated amendment at the end supersedes that direction. Earlier Status, measurements and activation records are preserved; this is not a claim of implementation or activation.


Status: Accepted

## Decision

`testnet-11` is the only current public test network. PALW consensus has one
canonical path:

```text
Producer / miner → Claim → Future DAG anchor → Panel selection → Panel verification
```

The only PALW protocol roles are:

| Role | Responsibility |
| --- | --- |
| Producer / miner | Produces the computation claim and its receipt/claim transaction. |
| Panel verifier | Deterministically selected from the future anchor and verifies the claim. |

Peer discovery is an operator-facing concern only. External discovery health
and node-local state must not affect block validity, finality, panel selection,
reward settlement, or chain choice.

## Scope boundary

Only the path shown above is normative. No external signal, node-local state, or
legacy side path may participate in current consensus.

Current panel selection uses the future DAG anchor directly. It must not read
external discovery state, historical committee state, local time, or unordered
map iteration. For identical claim, producer, anchor, runtime class, candidate set,
and panel size, every node must derive the same panel.

Model-cost tables, receipt validation, token-cost accounting, and `rho_micro`
calculation may remain when required by current PALW claim/verification
semantics; they are computation data only.

## Compatibility boundary

There are no compatibility RPCs for retired protocol surfaces, and no current
code may read or mutate their stores. New code must use the PALW names
`future_anchor`, `panel`, `claim`, and `producer`.

## Mission alignment amendment — 2026-10-07

この番号の二つのfilenameは別系統の記録である。旧testnet-11 architectureのPanel/mint/finality記述とV2 runtime-separated設計を現在の実装として混同しない。将来のPALW訴追に関しては本amendmentを優先する。

* 必要なinput/weights/state/trace/openingは、選出されていない普通のpublic bondがclaim commitmentに対して認証・取得できなければならない。producerだけのcapture、FOLD prefix、tile preimageや内部proverを前提にしない。ローカル保管・off-chain配布は可能だが、開示または有界の客観的非開示裁定を最後まで持つ。
* Panel/jury/validatorの選出やquorumは担当割当・既存処理の条件であり、算術的真実や外部訴追の権限を決めない。有効なobjective proofは多数派のlicense後も独立に処理する。新たな訴追をgenesis operator、owner承認、bound-seat専用の権限に依存させない。

本amendmentは衝突する将来の設計・受入条件を改定する。既存の実装・測定・fenceの記録はそのまま保持する。変更する合意規則は別のversioned移行を必要とする。Panel=0は[RFC14](../rfc/0014-panel-independent-fraud-prosecution.md)の全completion gatesと[RFC15](../rfc/0015-panel-free-permissionless-verification.md)固有gateが成立するまで有効化しない。

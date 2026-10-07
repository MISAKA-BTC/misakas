# ADR-0026: Current PALW architecture on testnet-11

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

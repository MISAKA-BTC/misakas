# ADR-0115 — A pending transaction is announced until it lands

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


> **Token identity (2026-10-07):** The token name is **Misaka** and its ticker is **BILI** ([ADR-0174](0174-token-name-misaka-ticker-bili-address-prefix-unchanged.md)). MSK in retained measurements, quotations, command/output examples, identifiers or chain-ID mnemonics is a legacy label for the same coin; it does not change amounts, units, protocol IDs or address prefixes.

* Status: PROPOSED 2026-09-11, from the operator's report ("また購入してもトランザクションやチャートに反映されてない
  原因は" / "何回購入しても balance が減らない") and their standing instruction to put liveness first.
  **IMPLEMENTED the same day, consensus-inert**: node policy only — no validity rule, no `Params`
  field, no fingerprint moves. A node takes it by a rebuild and restart; a node without it still
  validates every block the others make.
* Builds on: [0020](0020-selected-parent-evm-lane.md) (the EVM lane: a payload is carried by a block and
  executed when the chain accepts it), [0072](0072-the-ticket-is-the-execution.md) (a producer's template
  is fixed for the whole draw), [0109](0109-a-lock-is-its-own-claim-and-finality-is-a-label-not-a-pause.md)
  (the same failure for deposit claims, fixed there by making every producer claim unasked).
* Amends: kaspa-pq EVM Lane v0.4 §14.2 (EVM tx gossip) and §16 (mempool retention).
* Supersedes nothing.

## 0. The sentence this ADR is

**A pending EVM transaction is announced again — to a peer when it connects, to every peer every five
minutes, and whenever a client sends it again — and is kept for a day, so a transaction reaches the
producer that will carry it however late that producer arrives, and is dropped only when it has
executed or could not.**

## 1. What was measured (2026-09-11, testnet-11)

Two joins from `0x0aec…8d79` (nonce 0 and 1, 10,000 MSK each) were sent to the explorer node during a
fleet rollout and never executed:

* Across all 1,546 EVM blocks of testnet-11, **no EVM transaction had ever been included** (only deposit
  claims, which ADR-0109 carries without relay).
* The explorer node held both (`misaka_getEvmTxStatus`: pending, `includedIn: []`), and so did the four
  `.113` pool slots — their block templates carried both — but the slots win about **one draw in
  38,570** and had produced nothing that run.
* The producers that actually win (ibm node0/node1, 5.104 seat2) were restarting when the transactions
  were announced (their connections to the explorer date from 10:08:50–10:21:34Z). The announcement is
  sent **once**, at admission, to whoever is connected; nothing announces it again; so they never held
  the transactions (`misaka_getEvmTxStatus` on ibm node0: `unknown`).
* The pool's TTL was **one hour**. With a chain block every 15–60 minutes and templates fixed per draw,
  that is shorter than an ordinary wait. Both transactions expired unexecuted at ≈11:40Z, and the wallet
  and the site noticed nothing: the balance never moved and each retry looked like the first one failing.

## 2. Decisions

1. **The relay has its own clock.** The P2P service ticks `FlowContext::evm_relay_tick` every
   `EVM_RELAY_TICK` (15 s) until shutdown. Every tick flushes the EVM spread's queue (it used to be pumped
   only when a block arrived); every `EVM_REANNOUNCE_EVERY_TICKS` ticks (5 min), while the node is nearly
   synced, the pool is maintained (Decision 3) and **every pending hash is announced again** to every
   EVM-relay peer. A peer requests only the hashes it lacks, so an announcement it already heard costs
   32 bytes.
2. **A peer that connects hears the pool once, at once.** After the handshake launches its flows, a peer
   on protocol ≥ `PROTOCOL_VERSION_EVM_RELAY` is sent the pool's hashes (in inv messages of at most 512).
   A peer that is still syncing ignores them; Decision 1 reaches it minutes later.
3. **The pool is kept true without a template.** `MiningManager::maintain_evm_pool` expires past the TTL
   and prunes what the chain executed (a nonce below the sender's committed nonce) — what every template
   build already did, now also done by a node that builds no template (the explorer and RPC nodes, where
   wallets send). A failed state read prunes nothing.
4. **A client that sends a pending transaction again is asking for it to travel.** `submit_rpc_evm_transaction`
   announces a `Duplicate` before reporting it (the eth RPC still answers it with the hash, as before).
5. **The TTL is a day** (`EVM_MEMPOOL_TX_TTL_SECS = 86 400`). Executed transactions leave by Decision 3;
   the count, byte and per-sender caps bound the pool; the TTL bounds only what can never execute.

## 3. What it does not do

It does not make a block come sooner: a transaction still waits for a producer that holds it to start a
draw and win it, and on testnet-11 that is tens of minutes. Nor does it change what a producer selects
or what the chain accepts. The site and MISAKA Wallet (the other half of the same report) show a pending
order as pending, keep its signed bytes and send them again while its nonce is free — which, with
Decision 4, also makes the node announce it again.

## 4. Tests

* `maintain_prunes_what_executed_and_keeps_what_is_pending` — pending kept, executed pruned without a
  template, nothing pruned on a failed state read; `evm_pending_hashes` is what the relay announces.
* `ttl_expiry_and_removal_keep_accounting_consistent` — the TTL is a day; `hashes` lists every pending tx.
* kaspa-mining 103/0 (with `evm`), kaspa-p2p-flows 108/0. Not drilled end to end on two nodes (a
  funded EVM sender on a local devnet was not set up); the relay decisions are glue over the tested pool
  calls and the existing spread.

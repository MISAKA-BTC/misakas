# ADR-0174 — Token name Misaka, ticker BILI; address prefixes stay unchanged

> **PALW共通前提 — 2026-10-10:** [ADR-0176](0176-bond-bounds-palw-production-rewards-and-final-weight.md)を適用する。確率的検証・公開反証・客観裁定に加え、bondと共通DAA期間がclaim発行能力、ブロック数、報酬総額、Final後の確定weightの上限を決める。
> モデル計算はその枠を使用するための正当な仕事であり、同額bond・同期間・同倍率なら計算省略や高速偽造で枠は増えない。有罪なら報酬失効と適用規則上の担保損失を負う。claim容量の倍率を上げてもbond当たりの総経済・consensus creditは増やさない。
> 将来のPALW設計・受入条件に適用し、過去の実測・実装記録・旧claim規則は保持する。上限は検出能力の証明ではなく、公開検証・徴収可能担保・責任保持・独立試験と明示的upgrade/activationを要する。
> **モデル入手への不介入 — 2026-10-10後続改定:** [ADR-0177](0177-model-bond-allocation-without-availability-consensus.md)を優先する。MISAKA Torrent・専用Seeder・Seeder報酬の概念を廃止する。モデル配布・取得可否を合意で管理せず、PoR/全量取得監査/TRDC/FPRによる資格・weight停止も撤回する。モデル別coinbaseは重複のない拘束miner bond元本に連動し、総発行予算と個別bond上限を維持する。
> 固定モデル同一性とclaim固有証拠の裁定は維持する。外部検証は正しいモデルを入手できた条件で成立し、公開参加の経済優位は倍率式・敵対的評価で立証する未完の目標である。過去の実装/試験/旧規則は保持し、新配分は未実装・未有効化である。


Status: Accepted naming and compatibility decision, 2026-10-07.

## Decision

The native token's display name is **Misaka** and its ticker is **BILI**.
Current ADR/RFC explanations, documentation, wallet balances, CLI help and human-readable
operator screens use these names. `MISAKA` may still identify the project, network,
repository, product or an existing external asset; it is not the native token's display name.

The amount represented by one coin does not change: **1 BILI = 100,000,000 sompi** on
the native UTXO ledger, and **1 BILI = 10^18 wei** in the EVM lane. These are the existing
lane-specific accounting units, not a new conversion or an economic parameter change.
Wallets keep their existing network-label convention: `BILI` on mainnet, `TBILI` on
testnet, `SBILI` on simnet, and `DBILI` on devnet. These prefixes identify environments;
they do not introduce different canonical token names or a claim about market value.

## Compatibility boundaries

- Address prefixes stay `misaka`, `misakatest`, `misakasim`, and `misakadev` for their
  existing networks. The address codec, payload, checksum and key rules are unchanged.
- Existing chain IDs, genesis commitments, network identities and signed domain strings
  are unchanged. In particular, the `MSK` mnemonic for `EVM_CHAIN_ID = 0x4D534B` remains
  a description of those fixed bytes, not the current display ticker.
- Binary names, subcommands, existing flags such as `--msk`, Rust identifiers such as
  `SOMPI_PER_MSK`/`parse_msk_amount`, and API/JSON fields such as `balanceMsk` stay compatible.
  Renaming a human-readable unit is not authorization to break a command or wire schema.
- The validator's amount parser accepts `BILI` as the current coin suffix while retaining
  existing `MSK` and `KAS` input aliases. Bare integers and `sompi` retain their previous
  meaning; accepted precision and overflow limits do not change.
- Historical measurements, audit records, literal historical quotations, and captured
  command output retain `MSK` as recorded, with a naming note. Their values and conclusions
  are not rewritten. Preserved code/output examples are legacy-compatible examples, not
  a second current ticker. Document filenames and link targets containing `msk` remain stable.

## Scope

This decision changes presentation and terminology. It does not alter issuance, rewards,
bond sizes, fees, signatures, addresses, fork choice, activation heights or consensus state.
It does not change an external token's on-chain metadata or deploy website changes.
The public-verifier objective in [ADR-0173](0173-public-verifier-dispute-completeness-is-misaka-purpose.md)
and the completion gates in [RFC-0014](../rfc/0014-panel-independent-fraud-prosecution.md)
and [RFC-0015](../rfc/0015-panel-free-permissionless-verification.md) still govern.

Current explanatory prose in earlier ADRs and RFCs adopts BILI. Retained historical
evidence and fixed compatibility identifiers remain individually scoped exceptions under
this decision; their legacy label must not be used to redefine the token's current identity.

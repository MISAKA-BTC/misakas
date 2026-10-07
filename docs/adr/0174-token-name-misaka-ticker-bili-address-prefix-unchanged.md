# ADR-0174 — Token name Misaka, ticker BILI; address prefixes stay unchanged

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

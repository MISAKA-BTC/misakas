# PALW lineage: how the rules got here — design history

> **Not normative.** This document is the history behind the rules in [spec/palw](../../spec/palw/00-index.md):
> which decisions were taken, reversed or superseded, and why. The spec states only today's rules.
> For each superseded ADR, the supersede map in [adr/README.md](../../adr/README.md) is authoritative.

**Last revised:** 2026-09-27

## 1. The arc in one paragraph

- **PALW began as a hash-shaped LLM proof of work** (ADR-0021, `algo_id` 4/5 at 120 s). Its
  output-text commitment was shown to be forgeable.
- **The verification shape was then rebuilt:** runtime-separated verification with unilateral fraud
  proofs and a sampling scheduler (0026–0028), and the class runtime made integer-only (0040
  BASE-0).
- **The layers were inverted** (0038): PALW *is* the consensus work. PALW-only block production
  (0039) and one ruleset with one fingerprint (0042) followed.
- **testnet-11** carried the RC through Relaunches 1–5f. There:
  - classes became chain data (0067);
  - certification became a consensus object (0075);
  - weight required end-to-end adjudicability (0069);
  - a clock lane was added and kept near-weightless (0060, 0066, 0068).
- **The 7,101 bundle** added:
  - the single lottery and one network work target (0132, 0137);
  - the execution lane (0125);
  - the permissionless registry (0135);
  - the anchor clock (0138).
- **ADR-0144 then fixed what PALW is for.** The accounting (0145–0149) and liveness (0151) were
  redone against it.
- **testnet-12 launched on R-core+** (0152), and was amended by two post-launch flag days (0154, 0155).

## 2. Flag days and fences on testnet-12

- **Why a flag day and not a series of fences.** Each fence height enters the fork id and the fence
  schedule. One height per release keeps the upgrade instruction to a single line: "run build X
  before DAA h". It also keeps the drill to one crossing.
- **Why the heights are 750 and 1,300.** 500 was planned first. It moved to 750 on 2026-09-26, when
  integration stopped early. 1,000 is `palw_bond_maturity`'s height, and a second fence there would be
  invisible to the fork id. 1,300 was chosen because the seats' room would close again around DAA
  1,350–1,575 (ADR-0155).
- **Why Some-only hashing with the `never()` collapse.** A build that carries a dormant fence must
  fingerprint and peer exactly like one that does not.

## Source texts (archived ADR bodies)

The archived bodies of history-heavy ADRs are listed below as each one is slimmed. Frozen
(superseded) ADRs keep their bodies in `docs/adr/`, with a banner at the top.

- [ADR-0035: The public PALW testnet is testnet-11, continued — and it pins its determinism class at the door](archive/0035-palw-public-testnet-strategy.md)
- [ADR-0036: PALW mainnet activation — lineage reconciliation and the model that governs](archive/0036-palw-mainnet-activation-model.md)
- [ADR-0068: The LLM-primary economy — the floor retires to the doctrine's minimum](archive/0068-the-llm-primary-economy-and-the-floors-minimum.md)

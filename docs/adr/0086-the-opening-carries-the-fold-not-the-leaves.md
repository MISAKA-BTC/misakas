# ADR-0086 — the opening carries the fold, not the leaves

> **Body moved (2026-09-27).** Normative rules → [spec/palw/09](../spec/palw/09-court-and-offences.md), [spec/palw/11](../spec/palw/11-free-prompt-lane.md); the full text as written → [design/palw/archive/0086-the-opening-carries-the-fold-not-the-leaves.md](../design/palw/archive/0086-the-opening-carries-the-fold-not-the-leaves.md); the reasoning is summarised in [design/palw/court.md](../design/palw/court.md).

* Status: Proposed 2026-09-05, implemented consensus-inert (no fence; the fingerprint does not move).
  It supersedes the serving wire form of ADR-0077 D8's interval openings (V1–V3). The consensus root
  rule the openings walk is unchanged.
* Date: 2026-09-05

## Context

Devnet runs 3–6 measured interval openings that carried every leaf hash of the range. Their size grew
with the range, not with what a seat needed.

## Decision

- **D1 — The V4 opening carries the fold's digests and the frontier,** and no leaf hashes.
- **D2 — Every V4 anchor is the named claim.** Chunks never ride.
- **D3 — The seat's leaves are its own:** verification hashes the served seed row tiles.
- **D4 — Both openers serve V4,** and the dense tree is built at the ruleset's level.
- **D5 — V4 is served for every class from this build.** V1–V3 stay decodable.
- **D6 — The court's address is a block, then a leaf.**
- **D7 — The seat's ceiling is restated in the opening's own units.**

→ spec 09 PALW-CT-18.

## Consequences

- Openings are bounded by what a seat verifies, not by the size of the range.

## Links

- Spec: [09 Court and offences](../spec/palw/09-court-and-offences.md) · [11 Free-prompt lane](../spec/palw/11-free-prompt-lane.md)
- Design: [design/palw/court.md](../design/palw/court.md)
- Full text as written: [design/palw/archive/0086-the-opening-carries-the-fold-not-the-leaves.md](../design/palw/archive/0086-the-opening-carries-the-fold-not-the-leaves.md)

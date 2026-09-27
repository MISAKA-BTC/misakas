# ADR-0135 — A model is data: the permissionless registry derives its profile, proves its panel, and walks its lifecycle

> **Body moved (2026-09-27).** Normative rules → [spec/palw/03](../spec/palw/03-classes-and-registry.md); the full text as written → [design/palw/archive/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md](../design/palw/archive/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md); the reasoning is summarised in [design/palw/registry.md](../design/palw/registry.md).

* Status: Proposed 2026-09-17. The rule set was built in shadow the same day, and Protocol Upgrade A
  behind a dormant fence. **Armed on testnet-11 at 6,001** (the permissionless registry), and on
  testnet-12 from genesis. **D5 (shares from admission) is superseded by ADR-0137** past the work
  target: a share is a result.
* Date: 2026-09-17

## Context

Registering a model still needed a human for its verification profile, its panel and its lifecycle.
The goal was that a model is registered as data, and every node derives the same numbers from it.

## Decision

- **D1 — The manifest** (`PalwModelManifestV1`, and `ClassManifestV2` on chain). → spec 03 PALW-CL-4.
- **D2 — The work is derived** (`PalwModelWorkV1`). → spec 03 PALW-CL-5.
- **D3 — The profile is derived, never measured,** against one set of registry globals.
- **D4 — Readiness is evidence.** → spec 08 PALW-VF-27.
- **D5 — No share.** *Superseded by ADR-0137.*
- **D6 — The lifecycle.** → spec 03 §3.3.
- **D7 — The boundary:** registration is permissionless for any graph the canonical VM expresses.

## Consequences

- A Kimi-class registration is walked through in §3 of the full text.
- It changes ADR-0132 and 0133 (§4 of the full text).

## Links

- Spec: [03 Classes and registry](../spec/palw/03-classes-and-registry.md)
- Design: [design/palw/registry.md](../design/palw/registry.md)
- Full text as written: [design/palw/archive/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md](../design/palw/archive/0135-a-model-is-data-the-permissionless-registry-derives-its-profile-proves-its-panel-and-walks-its-lifecycle.md)

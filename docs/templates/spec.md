# <Domain> spec — NN. <Chapter title>

> **Normative.** This chapter states the rules as they are on each network today. For the reasoning,
> see [design/<domain>/<topic>.md](../../design/<domain>/<topic>.md). For the history, see the ADRs
> named in each section. If this text and the code disagree, the code is the truth and the
> disagreement belongs in [divergences.md](divergences.md).

**Applies to:** mainnet (<active | not active: reason>) · testnet-12 (<summary>)
**Reconciled with code at:** `<commit>` (<YYYY-MM-DD>)
**Principles served:** <P-numbers from 01-principles.md>

## NN.1 <Section title>

<One paragraph: what this section governs. At most one sentence of rationale, with a link to Design.>

**Definitions**
- *<term>*: <definition, with the code type that carries it>

**Rules**
- **<DOMAIN>-<CH>-1.** A node MUST …
- **<DOMAIN>-<CH>-2.** A block MUST NOT … unless …
- **<DOMAIN>-<CH>-3.** A producer SHOULD … (node policy, not validity)

**Activation**

| Network | Status |
| --- | --- |
| mainnet | not active (<reason>) |
| testnet-12 | from genesis · from DAA <h> (`Params::<field>`) · dormant (`None`) |

**Sources:** ADR-NNNN §x / Dk · ADR-MMMM Dj
**Code:** `path/to/file.rs` — `function_name` (<what it decides>) · `Type` (<what it carries>)

## NN.2 <Next section>

…

## Rules withdrawn from this chapter

| ID | Withdrawn by | Replaced by |
| --- | --- | --- |

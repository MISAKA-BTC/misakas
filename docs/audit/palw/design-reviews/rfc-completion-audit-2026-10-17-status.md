# RFC completion audit 2026-10-17 — status of the six assigned items (integ/validation-r2)

Checked against the code at `ca5e75080` (the audit read older code). All six are still open in code. Each needs values or a
design choice that ADR-0181 reserves to the user; those are listed. No fence is armed; t12 identity unchanged.

| item | still present | what a real fix needs | user decision needed |
| --- | --- | --- | --- |
| RFC08 unsourced liability | `refuse_unsourced_admission_v1` at the four native admissions (`palw_bond_budget_fold_v1.rs` funded v11, text, job v39, mining v39); `Unsourced` legs for the material provider fee (`palw_monetary_material_fold_v1.rs`), drawn-verifier fees and beacon-source fees (`palw_kernel_route_fold_v1.rs`), which `settle_once_liable_v1` closes with a zero terminal marker | a producer-signed absolute liability horizon on each native job/claim record (today only `deadline_daa` / `commit_deadline`, not a liability horizon), a monetary-covenant TTL bound to the claim (the kernel-claim path already sources both: paid lifetime + `covenant_ttl_v1`), the bond hold until the horizon, and explicit hold → release / forfeit / refund transitions per leg | the horizon formula per native type (relative to deadline? court + grace?), whether the fee legs are held to the payer's horizon or the provider's, and the refund rule on default |
| RFC07 PanelLicensed event | `final_export_v1` returns `None` for `PanelLicensed` (the route does not hold the Panel's seed/epoch); staged trap reveal refused for interactive claims (`palw_paid_claim_trap_v1.rs` "a trap on an interactive (staged) paid claim is not supported") | carry the Panel assignment (seed, epoch, seats, dependency profile) into the route at licence time and export an event marked Panel-dependent (`validity_independent = false`), so the sealed collectors keep refusing it as a source | whether a Panel-licensed event may count anywhere (it cannot be a Panel-independent source by rule) — if not, the export is for audit only |
| RFC09 remote miner L2 | `misaka/palw/remote/src/miner.rs` attested mode still reads `PalwForkChoiceOpeningV1` / `verify_fork_choice_v1` and STOPs under Rule E | port the RemoteNode fetch, opening types, attestation, candidate window and comparator to the V2/V3 root reader the full node uses; 2-fork test under Rule E + Gate C | none (implementation) |
| RFC10 end-to-end with approved sources | no node test drives discovery → auto binding → replacement → Final without Panel injection | depends on the paid-source path (§6 of the validation record), itself blocked by the onboarding-window / paid-lifetime choice | the window/lifetime choice (pending) |
| RFC12 tooling | `scripts/drills/misaka-rfc12-acceptance.py` `NODE_GAPS`: reserve balance read, crash failpoints at import WAL barriers, paid-request generator from an authenticated snapshot, outpoint UTXO read; signed deposit-refund CLI | four node reads/hooks and a CLI; then the acceptance run | none for the reads; the run needs the `evm` build (disk) |
| RFC18 payout split | `name_certified_reward_v1` (`palw_security_certification_fold_v1.rs`) releases the held reward as one leg | split into legs below — conservation proof per option | the ratios (not chosen here) |

## RFC18: design options for the certified reward split (no ratio chosen)

Let `R` be the held issuance reward of a certified claim (already drawn at Final; no new draw). Every option must satisfy
**conservation**: the legs sum to exactly `R` (rounding remainder to a named leg, never minted), each leg is named once through
`settle_once_liable_v1`, and a conviction before release withdraws all unpaid legs (burned, not redistributed).

1. **Two legs, fixed share** — producer `R·(1−p)`, certifying verifiers (Gate C drawn set) `R·p` split equally; remainder to the
   producer. Simple; `p` is the only value.
2. **Three legs with buyback** — producer, verifiers, and a buyback/burn leg `R·b` (removed from supply, not paid). Conservation:
   `paid + burned = R`; issuance net of the burn is lower than the schedule by `b`.
3. **Vested producer leg** — option 1 or 2 with the producer leg released over `V` DAA (linear), unvested remainder forfeited on a
   later conviction inside the liability horizon. Conservation holds per tick; a forfeited remainder is burned.

Values to decide: `p` (Panel/verifier share), `b` (buyback), `V` (vesting), and whether the verifier leg is per drawn seat or per
signing seat.

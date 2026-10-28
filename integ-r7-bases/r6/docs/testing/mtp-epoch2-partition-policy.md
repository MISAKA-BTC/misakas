# MTP epoch policy across the 2026-08 partition — operator decisions, 2026-08-10

The MISAKA Testnet Points page stopped at epoch 1 (2026-07-28 → 2026-08-08, published
2026-08-07 23:28 JST, `finalized: false`) because epochs are published by an explicit
operator command by design — "run-epoch signs an artifact participants are entitled to rely
on" — and the operator's attention went to the partition the day the epoch-1 window closed.
Collection never stopped: the hourly chain-scan cron on host A has run throughout (26 h
overlapping lookback, dedup on re-ingest). Eligibility is not the issue either: since the
2026-08-02 policy change any well-formed `addr:` id scores without registration.

The following three decisions were taken by the operator on 2026-08-10 for the window the
partition contaminated. They are recorded here because the participant-facing MTP
documentation (`docs/testnet-participation.md` §8) currently lives only on the MTP branch,
not on main — porting it is a named follow-up.

## 1. The partition week is a skipped epoch, not a scored one

The week 2026-08-08 → 2026-08-15 is declared **unscored**. No epoch will be issued over it.
Epoch 2 is the normal week starting 2026-08-15T00:00:00Z. Scoring a week in which the
network was split, halted, and flag-dayed would grade participants on the operator's outage;
skipping it is the only reading under which nobody's standing depends on which branch their
node happened to see.

## 2. Scoring is canonical-lineage-only

Where scoring windows touch the fork (2026-08-03 → recovery), only activity recorded on the
surviving canonical lineage (Branch M and its continuation) counts. The operator's own
Branch-A self-mining — the difficulty-floor branch the incident produced — does not score.
This is the strict reading of decision 1's principle applied to the operator's own address.
Mechanically this falls out of the pipeline: the chain-scan reads the canonical index, the
reorg unwinds Branch A from it, and already-ingested Branch-A facts are excluded at scoring
by the epoch input filter (the input.json of any affected epoch shows exactly what was
counted, offline-verifiable as always).

## 3. Operator addresses keep scoring, visibly labeled

No exclusion list is introduced. Operator/premine/fleet addresses (e.g. the
`misakatest:qtpflz…` premine main wallet — today's only leaderboard row, and the cold-start
miner's payout target) score under the same rules as everyone else, and the page and docs
must label them as operator-run so the leaderboard reads honestly. Fairness is carried by
the existing per-owner rank decrement, the colocation cap, and this labeling — not by
hidden exclusions.

## Sequencing note (why nothing publishes today)

An epoch that touches the recovery cannot be built until (a) node A has completed its
Branch-M adoption and (b) the chain indexer has re-followed the reorg, after which a one-off
wide back-scan (2026-07-28 → now) re-ingests the canonical lineage past the 26 h lookback.
Then epoch 2 publishes on its 2026-08-22 boundary under the decisions above.

Follow-ups, named: port `mtp/` + `docs/testnet-participation.md` from the MTP branch to
main; fix the service's stale `--network testnet-20` default; reconcile the
registration-handshake docs with the 2026-08-02 open-enrolment change; label operator rows
on the misakascan MTP page.

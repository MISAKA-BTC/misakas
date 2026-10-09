# MISAKA Torrent mandatory availability — design alignment, 2026-10-09

> **Historical scope, superseded 2026-10-10:** This records the previous revision, including its Panel-seeding mandate and eight-term predicate. The current [independent Bonded Seeder policy/audit](0014-independent-bonded-seeders-audit-2026-10-10.md) supersedes those future requirements: Panel/miner sharing is voluntary; independent supply and separate artifact identity/retrieval/computation checks are mandatory. Its historical check counts do not validate the current revision.

This records a documentation revision on `MISAKA-BTC/misakas` branch `pre`, reviewed from
`c20fca1d8e3611e201e54d6b662d3d7874ff6328`. Input: the user's supplied Japanese design,
“MISAKA Torrentを必須にしたモデル公開可用性・採掘安全性の最終設計案”. Existing source baselines,
pinned transport references, implementation records and measurements remain historical; they are
not a new audit of the transport repository or a claim that these gates pass on this branch.

## Normative change map

| Document | Revised requirement |
| --- | --- |
| [Public-verifier ADR173](../../adr/0173-public-verifier-dispute-completeness-is-misaka-purpose.md) D9–D10 | Official immutable-model / Torrent public-availability principles; independent verification mining; model-local default; separate provider/producer/Panel liability; weight safety and independent chain liveness |
| [Seat-root ADR173](../../adr/0173-a-seat-holds-a-root-not-a-class-and-possession-gates-activation.md) amendment | Preserve the separate historical proposal; possession is insufficient for the new gate; improvements are independent registrations rather than same-ID version/head changes |
| [ADR175](../../adr/0175-registered-models-are-permanently-immutable.md) (concurrent work, referenced) | Align the Torrent additions with separate independent model registration: no same-ID version addition or line/head/Position/AMM replacement. This revision does not implement its separate immutability fence |
| [RFC14](../0014-panel-independent-fraud-prosecution.md) §16.1–16.4 | Mandatory Torrent-only full acquisition; HTTP may supply seed bytes; immutable model/binding and separate mutable service state; full/sharded Panel duties plus non-Panel bonded supply; f failures, reviewed PoR and independent Full Fetch; named consensus-derived `ModelRewardEligible` AND predicate |
| RFC14 §16.5–16.7 | Public authenticated range demand/deadline/default, bounded spam and reserved answer/inclusion capacity; READY loss and in-flight cleanup; no local-failure convictions or DDoS-as-arithmetic-fraud; actual cold budgets, escrow/Final/maturity distinctions |
| RFC14 §16.8/16.10/16.11 | Independent-devnet attack matrix; Rule E integration and all weight/settlement readers; ECON/MEAS/PARAMS evidence; separate safety and chain-liveness release criteria |
| [RFC06](../0006-palw-layer-sharded-panels.md) §3.1 | Seed assigned shards without forcing whole-model replication at every seat; independent whole-model supply, failure tolerance and liability scopes |
| [RFC10](../0010-permissionless-palw-panel-and-claim-completion.md) §0.2 | Public binding/seat snapshot does not establish availability; seeded duties, fixed leases and independent supply are additional release requirements |
| [RFC11](../0011-permissionless-model-and-long-context-onboarding.md) §17 | Free static registration remains separate from immutable identity, conformance and expiring reward eligibility; actual large-model cold budgets, registration vs active coverage metrics |
| [RFC15](../0015-panel-free-permissionless-verification.md) G14/admission/Final/§9/activation | Panel=0 still requires independent bonded Seeders and equivalent Torrent availability; no automatic activation or empty-quorum shortcut |
| [RFC08](../0008-palw-claim-backed-consensus-blocks.md) §6.1 and [implementation spec](../../design/palw/rfc-0008-implementation-spec.md) | Eligible REAL/root, claim-bound service for slices, zero EXEC weight/DAA, unresolved work cannot become definitive safe weight; preserve main recovery and lawfully settled TxPermits |
| [RFC12](../0012-palw-only-consensus-and-native-evm-settlement.md) §2.1 | Same eligibility/weight gate across fork choice, native EVM heads and pruning/IBD; provider failure cannot revive DNS authority or halt independent chain progress |
| [RFC index](../README.md), [ADR index](../../adr/README.md) | Revision precedence and implementation/activation limits made discoverable |

## Required attack and measurement evidence — not performed by this revision

RFC14 §16.8 requires independent-devnet results for one Seeder failure, f failures, more than f,
100 Sybil Seeders/common dependencies, whole-Panel/miner collusion, PoR PASS but serving refusal,
hidden model tail, Seeder DDoS, false reports/demand spam, tracker/path loss, 100TB registration,
all-provider failure after READY, reorg/partition/restart/IBD/pruning. A mock/sparse 100TB file is a
state-machine fixture, not a real model cold-download benchmark. Earlier shard/court tests are not
evidence for these added requirements.

ECON prices shutdown attacks, collateral, bounded gain/loss and serving/checking incentives. MEAS
records actual independent supply, throughput and full cold verification/court resources under load.
PARAMS freezes f/coverage, independence/PoR/coding policy, snapshots, audit/serve/retention horizons,
demand/answer capacity, collateral/fees, Rule E/weight policy and migration. None receives a fabricated
number or PASS. Full Fetch is empirical evidence with declared trust assumptions; PoR is a reviewed
cryptographic retrieval property; neither alone proves sustained public serving.

## Documentation validation boundary

Validation checks the documentation diff, newly introduced local links/anchors and consistency of
the named eligibility predicate, duty/weight/failure/activation requirements. No runtime code, wire
tag, consensus fingerprint, activation height, network preset or deployment is changed by this work.
Existing concurrent code edits on `pre` are outside this documentation revision.

Checks performed on 2026-10-09:

| Check | Result and scope |
| --- | --- |
| `git diff --check` | PASS for the 13 documentation paths in this revision |
| Added local Markdown links/anchors | PASS, 57 unique file/target pairs; existing unrelated links and remote pinned-source contents are outside this check |
| Markdown fences and dated implementation/activation boundaries | PASS across the revision paths |
| Reward-eligibility definition | PASS, all eight required AND predicates present exactly once in its definition |
| Pasted-design attack matrix coverage | PASS, all 12 required cases documented; the tracker/path case is also included. This is document coverage, not execution of those tests |

This revision is not a PoR implementation/security proof, a physical independence certificate,
a successful swarm/devnet drill, real 100TB performance evidence, Rule E integration, external review
or activation approval. All corresponding implementation and release gates remain open.

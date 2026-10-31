# Explicit G14 policy in the LLM class cards — 2026-10-31

The live LLM page still required the historical registry state, seven ready
seats and the old 3-of-5 panel before showing a class as available for mining.
At candidate DAA 18,652 the public node explicitly reported all-model G14
single-seat arming, while the now-inactive legacy registry retained Qwen's
`Held` / ready 6-of-7 history. The page consequently displayed a false readiness
hold for new claims.

The limited renderer change reads `getPalwFenceSchedule(view="arming")` and
requires all of the explicit facts: `available`, `found`, family
`g14ModelPanelOne`, `armed=true`, `allModels=true`, `seatCount=1`, and
`scope="whole-claim"`. A height, fingerprint alone, missing/malformed response,
other route or inactive policy does not activate this view.

When that exact policy is active, class cards report the route, the producer
facts' class availability and the existing epoch numbers. They do not claim
that an unnamed bond can mine, that a claim is Final, or that credit is
certified. The Qwen live read had `available=true`, budget 1,000, produced 119,
`bondKnown=false` and the explicit no-bond readiness message. Its class card
now renders “G14 · 1 seat · whole claim / class available”, with those epoch
facts, rather than the historical Held gate. Readiness/seat rows and existing
claim assignments remain present, labelled as non-gating history for new
claims. The inactive/unknown policy retains the legacy renderer.

The original main and live app.js were patched against their own current
contents, after separate backups. The clean worktree contains only the new
G14 change; unrelated user changes were not copied between versions. Live
backup: `/root/misaka-incident-20261031/explorer-model-one-cards-backup`.
The public download SHA after deployment is
`e08a95eef65dab68c549177916352ef71d9b0a9c7b2018921ec4a9f13c5850c1`.

Validation: Node syntax checks passed for all three source variants. Four
meaningful tests cover exact explicit arming, active G14 with zero historical
ready seats and no named bond, inactive/unknown compatibility, and unavailable
class facts. All four passed for clean, original-main and downloaded public
source. Five existing original-main registry tests also passed. Public JS
rendered with fresh live RPC facts confirms the new Qwen card text and no
`OFF`/`minable` assertion. A new browser screenshot could not be taken during
this follow-up because CUA's available-browser inventory was empty; this
public-source/live-RPC check is not labelled as pixel verification.

## Model execution counters and actual-claim verification

Home's **Model execution** filter counts algo-10 execution-round blocks.
An algo-6 Qwen/Huihui attempt is a real model attempt and may appear under
**Consensus / C-BLUE**, with its model in the Model column. These are different
lanes, so a zero round-block count alone does not establish that a model
attempt failed. Counters and filters were not changed by this patch.

The node's `palw_block_kind_v1` returns `REAL` for a non-base-class attempt and
`FALLBACK` only for the base floor. An actual historical Qwen block (accepted
DAA 18,390, before this migration) and a post-fence floor block were used to
check the live decoder/classifier: Qwen decoded to `REAL` / Consensus and was
not labelled FALLBACK; the floor remained FALLBACK. This is a codec regression
check, **not** evidence of a newly generated G14 Qwen claim.

New model success still requires a newly accepted real claim at/after DAA
18,640, its one-seat assignment and the requested Final observation. That
runtime work remains separately monitored. No counts, claim phases, receipts,
Finals, or certification were fabricated for this UI repair.

Compact evidence: [explorer-model-one-cards.json](explorer-model-one-cards.json).

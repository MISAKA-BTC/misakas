# The user dictionary — one vocabulary for MISAKAScan and MISAKA Options

MISAKAScan shows consensus identifiers where a reader wants a name. The navigation alone reads
`LLM Jobs / Miners / MTP Points / Validators / Finality / EVM`, and a class arrives as
`4277d84f7d91…`. The information is there; the protocol's word for it is in front of the human's.

This file is the contract that fixes that, and it is **display only**. Nothing here enters
`rules_hash`, `class_id`, `consensus_identity_id` or any signature. The chain keeps answering in
hashes; the explorer looks the hash up and prints a name beside it.

## The rule

**Human name primary, protocol identifier secondary — and the identifier is never removed.**

```
Qwen 2.5 1.5B @512            ← what a reader came for
4277d84f…35f1b7               ← what the chain signs
```

Not `Model / 4277d84f…`. The pair, in that order, every time.

## Two corrections to make before the table is used

**1. A class is a model AND a runtime shape, so a model name alone is ambiguous.**
`class_id` is `PalwShapeProfile::shape_profile_id()` — canonical borsh over the WHOLE profile, which
carries `n_ctx`, `n_batch`, `n_ubatch`, `n_seq`, `n_threads` and the runtime flags. One artifact
therefore legitimately carries several class ids: `@512` and `@2048` are two classes over one model.
A display name of "Qwen 2.5 1.5B" would print identically for both and make two distinct consensus
objects look like one, which is the failure the names exist to prevent. **Display names carry the
distinguishing part of the shape** — `Qwen 2.5 1.5B @512` — and the model page shows the rest.

**2. A receipt block is not "the user's AI request".** The free-prompt commitment is the request; a
RECEIPT block is where a certified quantum is SPENT, which is the moment the request is served and
the work becomes weight (`palw_state_v2.rs`: "a certified quantum is spent into a receipt block").
Calling it "User AI Request" would name the answer after the question. It is **"AI Request served"**.

## The table

`tooltip` is the one-sentence hover. `technical` is what the Technical view and every
copy-to-clipboard still print.

### Lanes and block kinds

| today | display | tooltip | technical |
|---|---|---|---|
| PALW Attempt lane | **AI Inference** | This block's work was one model inference, replayed and checked by the network. | `algo 6 · PALW attempt lane` |
| PALW Receipt lane | **AI Request served** | A user's paid request was answered here, and the work became network weight. | `receipt block · certified quantum spent` |
| Free Prompt lane | **AI Request** | Somebody asked the network for an answer and committed to paying for it. | `FreePromptCommitted · fp lane` |
| Heartbeat lane | **Network Heartbeat** | Keeps the chain's clock moving when no AI production is available. Earns fees only, and no work weight. | `algo 8 · bondless heartbeat (ADR-0060)` |
| EVM lane | **EVM Transaction** | An ordinary smart-contract transaction. | `EVM execution lane` |
| Algorithm 4 / algo 6 | *(not shown at top level)* | — | `algorithm N` |

### The model economy

| today | display | tooltip | technical |
|---|---|---|---|
| Model Registry | **Models** | The AI models the network runs, and which are producing right now. | `getPalwModelRegistry · ADR-0135` |
| Execution class | **Model** *(name + shape)* | A model at one runtime configuration. The same model at a different context size is a different entry. | `class_id · PalwShapeProfile::shape_profile_id()` |
| Artifact root | **Model File ID** | The fingerprint of the exact weights file this entry runs. | `artifact_root` |
| Model line | **Model version history** | Who maintains this model and which version is current. | `line_id · ADR-0088` |
| Registrant bond | **Producer Deposit** | Collateral the worker locks; it is slashed if the work is proven wrong. | `bond · outpoint txid:index` |
| Producer | **AI Worker** | The machine that ran the model for this block. | `producer bond` |
| Claim | **AI Work** | One unit of model work the chain is tracking. | `claim · PalwClaimStateV2` |
| Licensed claim | **Verified AI Work** | Enough independent checkers agreed the work was done correctly. | `ReceiptLicensed` |
| Final claim | **Settled AI Work** | The challenge window closed with no successful dispute. | `Final { final_daa }` |
| Voided claim | **Rejected AI Work** | The work never completed — no panel could check it, or it failed. | `Voided { reason }` |
| Ready seats | **Checkers ready** | How many machines are holding this model right now and can verify work on it. | `readySeatsNow / requiredReadySeats` |
| Panel | **Checkers** | The independent machines drawn to verify one piece of work. | `panel · PALW_V2_PANEL_SEATS` |
| Execution lane / round / permit | **Work slots** | How many pieces of AI work the network will accept this round. | `getPalwRoundLane · ADR-0125` |
| Domain quota | **Share of work slots** | The fraction of slots this model's work may take. | `quotaPermille` |

### Progress and confidence

| today | display | tooltip | technical |
|---|---|---|---|
| DAA score | **Network Progress** | How far the network's clock has moved. Only blocks that carry priced work advance it. | `daa_score · ADR-0138 anchor clock` |
| safe_weight | **Confirmed Work Weight** | Work that is finished and can no longer be taken back. | `safe_weight` |
| Settlement depth | **Confirmations** | Counted in settled AI work, never in blocks — heartbeats and execution blocks add none. | `depth · settled anchors · ADR-0127` |
| Safe frontier | **Fully resolved up to** | The last point where everything behind it is decided. | `safe_frontier_blue_score` |
| Fence / flag day | **Upgrade height** | The block height at which a new rule starts applying. | `ForkActivation · fence DAA` |

### Membership, shared with MISAKA Options

| today | display | tooltip | technical |
|---|---|---|---|
| Position | **Membership** | A holding in a model, which may grant declared access. | `position · ADR-0087` |
| Line benefits | **Access** | What holding this membership lets you do. | `benefits · ADR-0095` |
| Service facts | **Who serves this model** | Providers who have signed a statement that they serve it. | `serviceFacts · ADR-0101` |

**The eight words that carry the product**, and the only ones a first-time reader must learn:
Model · AI Work · AI Worker · Checker · Membership · Access · Verified · Network Heartbeat.
Everything else in this table lives behind **Technical**.

## Simple and Technical

Default is **Simple**. The toggle is per reader and remembered; it never changes what the page
reads from the node, only which layer is drawn. **Technical shows the Simple values too** — it is an
addition, not a replacement, so a researcher never has to switch back to see the human name.

A model page in Simple:

```
Qwen 2.5 1.5B @512        ● Active
Lightweight model · reasoning and chat
Checkers ready      8 of 7
Share of work slots 48.9 %
Model size          ~1.7 GB
Last AI work        18 s ago
```

and the same page with Technical open:

```
class_id        4277d84f7d91…35f1b7
artifact_root   811af53e78f6…25f4d9
shape           n_ctx 512 · W8A16 static PTQ · graph-v5
state           Probation { probes_passed: 0 }
readySeatsNow   8   requiredReadySeats 7
```

## Colour, and never colour alone

Purple AI/model · green verified or settled · amber heartbeat or recovery · blue EVM · red disputed,
failed or slashed · grey identifiers. **Every colour carries an icon and a word**, because a reader
who cannot separate the hues still has to be able to read the page.

## Where the names come from

`LLM_CLASSES` in the live `app.js` already holds the class ids with their real model descriptions —
`PALW-BASE-0`, `QWEN36`, `QWEN25-A16`, `QWEN38-27B`. That table is the seed of the display layer and
should move behind one accessor so there is one place a name is decided.

**It is already incomplete, which is the argument for the lookup rather than against it.** The chain
reports FIVE classes on testnet-11; `f942e268f43f…` is registered and the explorer has no name for
it. A display layer must therefore degrade honestly: an unknown class prints its short id and says
the network has no name for it yet — never a blank, and never a guess.

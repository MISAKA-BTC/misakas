//! **`palw-class` — the operator's window into the SDK, before any coin moves.**
//!
//! Three questions, answered offline from the network's own ruleset:
//!
//! * `ledger` — what classes can this build supply, and which does the network's genesis register?
//! * `inspect` — which class does this artifact file pair with, under which root, and why not the
//!   others?
//! * `preflight` — would the admission gate accept this pairing on this network? Asked BEFORE a
//!   registration is signed or funded, because the same refusal after submission costs the carrier
//!   fee — and a wrong pairing burned a class seat once already.
//!
//! The genesis view is static: a LIVE chain may have registered more classes since genesis, and
//! the node's own `--palw-register-class` path reads live terms for exactly that reason. This tool
//! is the dry run, not the submission.

use std::path::PathBuf;

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::NetworkId;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::palw_step::PalwShapeProfileV3;
use kaspa_hashes::Hash64;
use misaka_palw_sdk::PalwClassSdk;

const USAGE: &str = "palw-class — inspect and preflight PALW model classes through the SDK

USAGE:
    palw-class pack build|verify|show ...   runtime packs: build one from a model, verify one, show one (palw-class pack for its usage)
    palw-class ledger    --network <id>
    palw-class inspect   --network <id> <artifact-path>
    palw-class preflight [--network <id>] <model>  [--depth headers|shape] [--height <DAA>] [--json] [more: see below]
    palw-class preflight --network <id> <artifact-path> [--model-id <model-id>]
    palw-class bind-tokenizer --network <id> --tokenizer <tokenizer.json> --out <path> [--model-id <model-id>] <artifact-path>
    palw-class measure   --network <id> [--name <model name>] [--replay-ms <ms> --measured-on <host>]
                         [--key-file <ml-dsa-87 seed>] [--out <measured.json>] <artifact-path>
    palw-class verify    --network <id> [--artifact <artifact-path>] <measured.json>
    palw-class manifest  --network <id> [--out <path>] [--check] <artifact-path>
    palw-class check-architecture --network <id> (--config <config.json | hf dir> | <hf dir | config.json>) [--adapter <file | builtin:ID | none>] [--legacy] [--held] [--tile-len N] [--h-chunk N] [--json]
    palw-class check-architecture --network <id> --tir <program.tir> [--tile-len N] [--h-chunk N] [--json]
    palw-class check-architecture --network <id> --config <config.json> --lora-budget [--lora-rank R] [--max-context N]
                         [--logits-tile N] [--max-window N] [--checkpoint-interval N] [--held] [--tile-len N]
                         [--h-chunk N] [--json]
    palw-class composite --parent <parent.palwtir> [--parent-class <hex>] [--out <path>] [--section-out <path>] <candidate.palwtir>
    palw-class certify   --network <id> --out <path> [--model-id <model-id>] [--family-id <hex>] <artifact-path>
    palw-class drill-leaves --network <id> [--model-id <model-id>] [--decode N] <artifact-path>
    palw-class close-sizes --network <id> [--anchor <hex>] [--json] <artifact-path>
    palw-class declare-layout --network <id> --out <path> [--max-context N] [--tile-len N] [--h-chunk N]
                         [--logits-scheme tiled|flat] [--logits-tile N] [--model-id <model-id>]
                         [--parent <the parent's declared .palwtir>] <lowered.palwtir>
    palw-class improve <policy|hard-case|opt-in|setter-set|dataset|artifact|licence|candidate|adapter-list|rollback>
                         --network <id> [--drill-salt <hex>] --key-file <ml-dsa-87 seed> [--bond <txid:index>]
                         [--spec <json>] [--parent <parent.palwtir> --section <candidate.palwtirs> | --artifact <candidate.palwtir>]
                         [--commitment-tx <fp-commitment-tx.borsh>] --out <path>
    palw-class improve eval (--artifact <class.palwtir> | --parent <parent.palwtir> --section <candidate.palwtirs>)
                         (--items <setter-set-spec.json> | --prompt <ids>) [--key <ids>] [--max-new N] [--stop <ids>]
                         [--open N] [--close N] [--reference <ids>]


`improve` (RFC-0004, A10) builds and signs the Model Improvement Protocol's objects offline, as
`misaka palw tir-registration` and `certify` do — the carrier that funds and submits one is
`misaka palw submit-object --object <out>`. Each takes a JSON spec (ids are 128 hex characters, token ids
arrays) and writes the signed object to --out, naming every id it derived (a case's id, a set's, a
dataset's, a candidate's class): `policy` {line, sequence, opt_out?, policy: {…overrides of the example…}}
(tag 70, the line owner's key); `hard-case` {line, domain, prompt, reference?, source?} (71, a bond's key,
the key and its salt written to <out>.key.json); `opt-in` --commitment-tx (72, the job's committer's key);
`setter-set` {line, epoch, prompts, keys, salt?} (73, with the two reveals as <out>.prompts (74) and
<out>.keys (75)); `dataset` (76); `artifact` (77, with the reveal as <out>.reveal (78)); `licence` {model_family,
domains, uses, per_use_fee, expiry_daa} (79, the rights holder's own key); `candidate` {line, epoch,
declarations} over --parent and --section (a composite) or --artifact (full weights) (80); `rollback` {line,
epoch, to_class, cause} (81). --drill-salt names a salted testnet-12 drill's genesis, whose domain the
signatures bind.

`improve eval` runs the evaluation executor on a held class offline — the job a seat or an executor runs, as the
chain derives its context — and prints what it generated and how it scores, as JSON (one result per prompt):
for a generating ExactMatch job the generation, its answer span (--open/--close, -1 for none) and, when the item
carries its key, pass or fail; for a teacher-forced RefLogLik job (--reference) the committed score. The class is a
class container (--artifact), or a composite candidate: its adapter section opened over its parent
(--parent --section). --items takes the setter-set spec (`prompts`, `keys`) a drill commits, so the model and
the chain read one file. It is how a drill checks, before any DAA passes, that a candidate wins or loses the
items it was made for.

`preflight <model>` (RFC-0002 Part II §II.2) answers, for a model nobody has downloaded yet, whether it can be
registered and what is missing, reading the HEADERS only: a Hugging Face directory (config.json, the shard
index and each shard's header), a config.json (with --headers <dir> of header dumps), or a .gguf file (its
metadata and tensor table). The first line says which mode ran. It judges three stages — convert, register,
mine — each ok, blocked or unknown, with a stable code, the numbers and what exists instead per blocker.
--depth headers stops at the convert stage; shape (the default) adds the chain's conditions on --network (default
testnet-12) at --height (default: the first height at which every scheduled fence is in force): admission, the
court window, the canonical job, the fences, the seat's memory (--seat-share [name=]GiB) and the lifecycle forecast.
--quant-format <file.json> (repeatable) adds quant-format descriptors, --adapter <file.json> a data adapter,
--max-context N / --tile-len N / --h-chunk N the layout it is judged at, --held the held history bound,
--json the machine form (misaka.palw.preflight.v1: no timestamp, no path). Exits 0 when nothing blocks, 2 when
something does. Given a .palwtir artifact, `preflight` runs the artifact admission below instead.
A repository id is read by HTTP ranges, the headers only: `hf://org/name[@revision]` (with --hf-endpoint <url>, default
https://huggingface.co) or an `http(s)://` base URL of a mirror. --node-facts <file> judges against what a node said (the tip is the
default height; a class already on the chain is read by its seats and independent operators; `misaka model preflight --node` writes
the file). --depth full with --pack <dir> [--artifact <file>] verifies the runtime pack (PACK_NOT_VERIFIED) and reads the artifact
root against the chain (ARTIFACT_ROOT_KNOWN).

`drill-leaves` (RFC-0002 Phase F, drill D-F2) prints, for an IR class's attempt job — its canonical
prefill, --decode tokens (1 where the network draws one forward, the default; 2 otherwise) — the first
step leaf of every commit-point kind (each committed node, each Fixed state's checkpoint, each history
tile) in prefill and decode: the leaves a live court battery tampers at with --palw-drill-tamper-leaf,
one kind at a time. One line per leaf: `<leaf> <call> <kind>`.

`close-sizes` (RFC-0002 Phase F, PALW-TIR-38) runs an IR class's attempt job for --anchor (zero unless
given) on this build's backend and measures every terminal close the node would file for it — the cone
close at the first leaf of every commit-point kind, and at the logits the logits and decode-token
closes — as carried (program stripped, borsh bytes), against the most the chain can carry
(palw_tir_carriable_close_bytes_v1). A dissected point is listed without a size (its terminal move is
the dissection's). Exits 0 when every close fits, 2 otherwise.

`declare-layout` (RFC-0002 Phase F) makes a lowered artifact a class: `palw-tir-fidelity --artifact-out`
writes the program and its integer tensors with no layout, and an IR class is its program under a
declared layout. It tiles every commit point and state at --tile-len (the logits node at the tiled
scheme's 4,096 lanes), the history at --h-chunk rows (64 and 64 unless given), the context at
--max-context (the widest the program and the network admit unless given), and declares the widest
checkpoint interval admission v10 accepts (tir_admit_v1's min C_j, halved until the gate admits),
then writes the container to --out with --model-id in its provenance. The class commits its logits
under --logits-scheme (the program's own if it names one, else tiled: the lowerer leaves it unset,
and a class under no scheme has no decode court), its logits node tiled at --logits-tile lanes (a
divisor of 4,096; unless given, the widest whose terminal close the chain can carry, PALW-TIR-38). The path from a Hugging Face
checkpoint to a registration: check-architecture → palw-tir-fidelity → declare-layout → preflight →
kaspad --palw-class-artifact <out> --palw-register-class <model-id>.
A LoRA candidate's container (meta.composite, from palw-tir-fidelity --adapter and palw-class
composite --parent-class) names the class over its COMPOSITE root (RFC-0004 §6.3), never over its
whole inventory, and is refused without a recorded parent class. With --parent (the parent's
declared container) its layout is judged by the composite admission (palw_tir_composite_admits_v1);
without it the layout is written unjudged.

`certify` (RFC-0002 Phase F) drills an IR (PALWTIR1) class end to end — a lie planted at a leaf of
every committed unit, prefill and decode, convicted by the IR court, and the honest run acquitted —
grades the drill with this build's copy of the chain's certifier, and writes the
`FamilyCertified { TirAttempt }` object to --out (with `<out>.chunkN` beside it when it needs more
than one carrier) and the `ClassLaneCertifiedTirV1` that seats the registered class at the floor
once the family is certified to `<out>.lane`. Submit them in that order with
`misaka palw submit-object`. The drill runs the class's canonical job at dense capture: a class
whose job is past the dense-capture cap is refused.

`measure` (ADR-0100) reads the geometry off the artifact, measures its bytes from the inventory,
evaluates every wall of ADR-0097 at 512 / 32,768 / 131,072 / 1,048,576 positions and the shard
plans a seat budget allows, and writes the Measured Model Artifact — signed under the bond key
when --key-file is given. `verify` recomputes a document: with --artifact every field, without it
everything but the artifact's own (which it names as needing the artifact); exits 1 on a mismatch
or a signature that does not verify.

`manifest` writes `<artifact-path>.palwmanifest`: the inventory root of every class this artifact
pairs with, derived through the SAME call a producer's class resolve uses, plus the artifact digest
that binds the file to the sidecar. For a PALWTIR1 (PALW-TIR) artifact it records the one
inventory root over the program's params (streamed through the consensus inventory) and the
program's graph_ir_root. It exists so nobody types an inventory root into a genesis card
again — that substitution (the flat artifact digest where the operand-inventory root belonged) shut
the dense tier of testnet-11 and then of testnet-12. `--check` recomputes an existing sidecar and
exits 1 on any disagreement instead of writing.

`check-architecture` (RFC-0002 Phase F): would this Hugging Face architecture be admitted on this
network? Give a model directory (`config.json` and its safetensors headers) or a `config.json`. The
report starts with the FEATURES the model combines — each SUPPORTED, or MISSING with the capability
that would close the gap; the `model_type` is informational and selects nothing — the support level
(A: the standard keys and tensor names; B: a data adapter, `--adapter <file | builtin:ID | none>`, the
one used is named; C: a capability is missing), and whether a new consensus primitive or a new court
kernel would be required. Then IR mode lowers the config to a PALW-TIR program and admits it with tir_admit_v1 (spec 04b
§10.3: normal form, ranges, per-position costs, every commit point's court cone, checkpoint
intervals) under the network's palw_tir_v1 ceilings (testnet-12's provisional values while the
fence is dormant — said in the output) and primitive set, at --tile-len values per step leaf and an
--h-chunk history chunk (64 and 64 unless given). --legacy maps the config to the shipped lineage that can
express it (dense A16, Qwen3.6 hybrid) and runs the processor-same admission gate on each class
row — the shipped rows at the config's geometry, else the family's graph projected at its
dimensions. Verdicts: ADMISSIBLE, EXCEEDS(limit, value, cap), NEEDS_PRIMITIVE, NOT_LOWERABLE,
REFUSED, UNVERIFIED, NEEDS_KERNEL. Exits 0 on ADMISSIBLE, 2 otherwise.

`check-architecture --lora-budget` (RFC-0004 §6.3) answers a different question: which LoRA adapters
of this model a composite candidate can carry. Each module kind an adapter can target is lowered
alone, then the attention set, the MLP set and PEFT's all-linear. Each is lowered at rank
--lora-rank (16 unless given; lora_alpha twice it), with its params after the parent's, as
palw-tir-fidelity --adapter lowers a candidate. For each, it reports:
* the adapter section's params and bytes;
* every block's nodes against the 512 a block may hold (NF-12, the composite rule);
* the work of admission's close sizing of the composite: every terminal close in the Composite{p}
  form, over the longest job at --max-context (2,048 unless given), against the 2^26-step cap.
The logits tile is --logits-tile, or else the widest tile whose closes the parent's sizing finds
carriable. --max-window N lowers as palw-tir-fidelity --max-window does. --checkpoint-interval N
sizes at a declared interval narrower than min C_j. When all-linear is past any budget, a greedy
fallback follows: the targets, in order, that keep every budget.

`composite` (RFC-0004 §6.3, PALW-MIP-15) makes a LoRA candidate's container a composite artifact
of its parent's. palw-tir-fidelity --adapter writes the candidate with the parent's params first
and records composite.p. This checks the candidate against the parent's container (its tokenizer,
token bound, primitive set and logits row; the composite rule; the candidate's params 0..P rooting
to the parent's inventory root) and derives the adapter section's root. It prints both, and the
composite artifact root when --parent-class names the parent's class id. With --out it writes the
candidate again with the record in its provenance:
composite: {p, parent_root, adapter_root, parent_leaves, adapter_leaves[, parent_class]} (hex:
lowercase, 128 characters). With --section-out (and --parent-class) it writes the adapter section
(PALWTIRS, RFC-0004 §6.7): params P.. alone, with the program, layout, tokenizer and the record — what
a seat holding the parent fetches; a node loads it after the parent (--palw-class-artifact, parent
first) and serves the candidate from the two files. Every root comes from the consensus functions.
Exits 1 on a refusal.

NETWORKS: a network id with a PALW V2 bundle, e.g. testnet-11 or devnet.

`preflight` exits 0 only if every requested pairing passes the admission gate.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

struct NetworkView {
    bundle: PalwConsensusParamsV2,
    network_id: NetworkId,
    /// The preset itself — the fences (`palw_kary_court`, `palw_context_ladder`) live here, not
    /// on the bundle, and the gate's shape is resolved from them.
    params: Params,
    /// `(class_id, artifact_root)` of every class the GENESIS registers — the static half of the
    /// live chain's terms.
    genesis_classes: Vec<(Hash64, Hash64)>,
}

fn network_view(raw: &str) -> Result<NetworkView, String> {
    let network_id: NetworkId = raw.parse().map_err(|e| format!("--network {raw}: {e}"))?;
    let params: Params = network_id.into();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err(format!("{network_id} has no PALW V2 bundle, so it has no classes to speak of"));
    };
    let bundle = bundle.clone();
    let genesis_classes = bundle
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } => Some((*class_id, *artifact_root)),
            _ => None,
        })
        .collect();
    Ok(NetworkView { bundle, network_id, genesis_classes, params })
}

fn sdk_for(view: &NetworkView) -> PalwClassSdk {
    PalwClassSdk::builtin_v1(view.bundle.court, view.params.palw_prompt_ids_form_v1(), view.network_id.to_string().into_bytes())
}

/// Pull `--flag value` out of the argument list, leaving positionals in place.
fn take_flag(args: &mut Vec<String>, flag: &str) -> Option<String> {
    let i = args.iter().position(|a| a == flag)?;
    if i + 1 >= args.len() {
        return None;
    }
    args.remove(i);
    Some(args.remove(i))
}

fn run(args: &[String]) -> Result<(), String> {
    let mut args = args.to_vec();
    let command = if args.is_empty() { String::new() } else { args.remove(0) };
    let network = take_flag(&mut args, "--network");
    match command.as_str() {
        "pack" => match misaka_palw_sdk::runtime_pack::cli::run(&args)? {
            0 => Ok(()),
            code => std::process::exit(code),
        },
        "ledger" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            ledger(&view);
            Ok(())
        }
        "inspect" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            inspect(&view, &path)
        }
        "preflight" => {
            // A model (a directory, a config.json, a .gguf) is judged from its headers; a PALWTIR1 artifact by the
            // artifact admission, as before.
            let first = args.iter().find(|a| !a.starts_with("--") && std::path::Path::new(a.as_str()).exists()).cloned();
            // A repository read by HTTP ranges (`http(s)://…`, `hf://org/name[@revision]`) is a model too.
            let remote = args.iter().any(|a| a.starts_with("http://") || a.starts_with("https://") || a.starts_with("hf://"));
            if remote
                || first.as_ref().is_some_and(|p| {
                    !matches!(misaka_palw_sdk::preflight::detect(std::path::Path::new(p)), Ok(misaka_palw_sdk::preflight::InputKind::Artifact))
                })
            {
                return match model_preflight(network.as_deref(), &mut args)? {
                    true => Ok(()),
                    false => std::process::exit(2),
                };
            }
            let wanted = take_flag(&mut args, "--model-id");
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            preflight(&view, &path, wanted.as_deref())
        }
        "measure" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let name = take_flag(&mut args, "--name");
            let replay_ms = match take_flag(&mut args, "--replay-ms") {
                Some(v) => Some(v.parse::<u64>().map_err(|e| format!("--replay-ms {v}: {e}"))?),
                None => None,
            };
            let measured_on = take_flag(&mut args, "--measured-on").unwrap_or_else(|| "not measured".to_string());
            let key_file = take_flag(&mut args, "--key-file");
            let out = take_flag(&mut args, "--out");
            let path = args.first().ok_or(USAGE)?;
            measure(&view, std::path::Path::new(path), name, replay_ms, &measured_on, key_file.as_deref(), out.as_deref())
        }
        "verify" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let artifact = take_flag(&mut args, "--artifact");
            let path = args.first().ok_or(USAGE)?;
            verify(&view, std::path::Path::new(path), artifact.as_deref().map(std::path::Path::new))
        }
        "manifest" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let out = take_flag(&mut args, "--out");
            let check = args.iter().any(|a| a == "--check");
            args.retain(|a| a != "--check");
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            manifest(&view, &path, out.as_deref().map(PathBuf::from), check)
        }
        "bind-tokenizer" => {
            let tokenizer = take_flag(&mut args, "--tokenizer").ok_or(USAGE)?;
            let out = take_flag(&mut args, "--out").ok_or(USAGE)?;
            let wanted = take_flag(&mut args, "--model-id");
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            bind_tokenizer(&view, &path, &PathBuf::from(tokenizer), &PathBuf::from(out), wanted.as_deref())
        }
        "check-architecture" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let mut config = take_flag(&mut args, "--config");
            let tir = take_flag(&mut args, "--tir");
            let adapter = take_flag(&mut args, "--adapter");
            let number = |v: Option<String>, name: &str, default: u32| -> Result<u32, String> {
                v.map(|v| v.parse::<u32>().map_err(|e| format!("{name} {v}: {e}"))).unwrap_or(Ok(default))
            };
            let tile_len = number(take_flag(&mut args, "--tile-len"), "--tile-len", 64)?;
            let h_chunk = number(take_flag(&mut args, "--h-chunk"), "--h-chunk", 64)?;
            // An encoder-decoder is judged at a source length and a target window (defaults 128 and 128): a registration
            // declares its own.
            let source_len = number(take_flag(&mut args, "--source-len"), "--source-len", 128)?;
            let target_len = number(take_flag(&mut args, "--target-len"), "--target-len", 128)?;
            // The model may also be given as the first plain argument: `check-architecture <hf dir | config.json>`
            // (after every flag that takes a value has taken it).
            if config.is_none()
                && tir.is_none()
                && let Some(i) = args.iter().position(|a| !a.starts_with("--"))
            {
                config = Some(args.remove(i));
            }
            let legacy = args.iter().any(|a| a == "--legacy");
            let held = args.iter().any(|a| a == "--held");
            let json = args.iter().any(|a| a == "--json");
            if args.iter().any(|a| a == "--lora-budget") {
                let config = config.as_deref().ok_or("--lora-budget needs --config (an adapter attaches to a Hugging Face config)")?;
                let optional = |v: Option<String>, name: &str| -> Result<Option<u32>, String> {
                    v.map(|v| v.parse::<u32>().map_err(|e| format!("{name} {v}: {e}"))).transpose()
                };
                let choice = misaka_palw_sdk::check_architecture::LoraBudgetChoiceV1 {
                    rank: number(take_flag(&mut args, "--lora-rank"), "--lora-rank", 16)? as usize,
                    context: optional(take_flag(&mut args, "--max-context"), "--max-context")?,
                    logits_tile: optional(take_flag(&mut args, "--logits-tile"), "--logits-tile")?,
                    tile_len,
                    h_chunk,
                    long_history: held,
                    max_window: optional(take_flag(&mut args, "--max-window"), "--max-window")?,
                    checkpoint_interval: optional(take_flag(&mut args, "--checkpoint-interval"), "--checkpoint-interval")?,
                };
                return lora_budget(&view, config, &choice, json);
            }
            let flags = ArchFlags { legacy, held, json, tile_len, h_chunk, adapter, source_len, target_len };
            match check_architecture(&view, config.as_deref(), tir.as_deref(), &flags)? {
                true => Ok(()),
                false => std::process::exit(2),
            }
        }
        "improve" if args.first().map(String::as_str) == Some("eval") => {
            args.remove(0);
            improve_eval(&mut args)
        }
        "improve" => improve(&mut args, network.as_deref()),
        "composite" => {
            let parent = take_flag(&mut args, "--parent").ok_or(USAGE)?;
            let parent_class = match take_flag(&mut args, "--parent-class") {
                Some(hex) => Some(hex.trim_start_matches("0x").parse::<Hash64>().map_err(|e| format!("--parent-class {hex}: {e:?}"))?),
                None => None,
            };
            let out = take_flag(&mut args, "--out");
            let section_out = take_flag(&mut args, "--section-out");
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            composite(
                &PathBuf::from(parent),
                parent_class,
                &path,
                out.as_deref().map(std::path::Path::new),
                section_out.as_deref().map(std::path::Path::new),
            )
        }
        "close-sizes" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let anchor = match take_flag(&mut args, "--anchor") {
                Some(hex) => hex.trim_start_matches("0x").parse::<Hash64>().map_err(|e| format!("--anchor {hex}: {e:?}"))?,
                None => Hash64::default(),
            };
            let json = args.iter().any(|a| a == "--json");
            args.retain(|a| a != "--json");
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            match close_sizes(&view, &path, anchor, json)? {
                true => Ok(()),
                false => std::process::exit(2),
            }
        }
        "drill-leaves" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let wanted = take_flag(&mut args, "--model-id");
            let decode = match take_flag(&mut args, "--decode") {
                Some(v) => v.parse::<u32>().map_err(|e| format!("--decode {v}: {e}"))?,
                None => 1,
            };
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            drill_leaves(&view, &path, wanted.as_deref(), decode)
        }
        "declare-layout" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let out = take_flag(&mut args, "--out").ok_or(USAGE)?;
            let model_id = take_flag(&mut args, "--model-id");
            let number = |v: Option<String>, name: &str| -> Result<Option<u32>, String> {
                v.map(|v| v.parse::<u32>().map_err(|e| format!("{name} {v}: {e}"))).transpose()
            };
            let default = misaka_palw_sdk::tir_layout::TirLayoutChoiceV1::default();
            let choice = misaka_palw_sdk::tir_layout::TirLayoutChoiceV1 {
                max_context: number(take_flag(&mut args, "--max-context"), "--max-context")?,
                tile_len: number(take_flag(&mut args, "--tile-len"), "--tile-len")?.unwrap_or(default.tile_len),
                h_chunk: number(take_flag(&mut args, "--h-chunk"), "--h-chunk")?.unwrap_or(default.h_chunk),
                logits_tile: number(take_flag(&mut args, "--logits-tile"), "--logits-tile")?,
                logits_scheme: match take_flag(&mut args, "--logits-scheme") {
                    Some(s) => Some(
                        misaka_palw_sdk::tir_layout::TirLogitsSchemeV1::parse(&s)
                            .ok_or_else(|| format!("--logits-scheme {s}: tiled or flat"))?,
                    ),
                    None => None,
                },
            };
            let parent = take_flag(&mut args, "--parent");
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            declare_layout(
                &view,
                &path,
                &PathBuf::from(out),
                &choice,
                model_id.as_deref(),
                parent.as_deref().map(std::path::Path::new),
            )
        }
        "certify" => {
            let view = network_view(network.as_deref().ok_or(USAGE)?)?;
            let out = take_flag(&mut args, "--out").ok_or(USAGE)?;
            let wanted = take_flag(&mut args, "--model-id");
            let family_id = match take_flag(&mut args, "--family-id") {
                Some(hex) => Some(hex.trim_start_matches("0x").parse::<Hash64>().map_err(|e| format!("--family-id {hex}: {e:?}"))?),
                None => None,
            };
            let path = PathBuf::from(args.first().ok_or(USAGE)?);
            certify(&view, &path, wanted.as_deref(), family_id, &out)
        }
        _ => Err(USAGE.to_string()),
    }
}

/// `preflight <model>` (RFC-0002 Part II §II.2): returns whether nothing blocks.
fn model_preflight(network: Option<&str>, args: &mut Vec<String>) -> Result<bool, String> {
    use misaka_palw_sdk::preflight::{Depth, Options};
    let number = |v: Option<String>, name: &str| -> Result<Option<u64>, String> {
        v.map(|v| v.replace('_', "").parse::<u64>().map_err(|e| format!("{name} {v}: {e}"))).transpose()
    };
    let depth = match take_flag(args, "--depth") {
        Some(d) => Depth::parse(&d).ok_or_else(|| format!("--depth {d}: headers, shape or full"))?,
        None => Depth::Shape,
    };
    let height = number(take_flag(args, "--height"), "--height")?;
    let max_context = number(take_flag(args, "--max-context"), "--max-context")?.map(|v| v as u32);
    let tile_len = number(take_flag(args, "--tile-len"), "--tile-len")?.map(|v| v as u32);
    let h_chunk = number(take_flag(args, "--h-chunk"), "--h-chunk")?.map(|v| v as u32);
    let mut seat_shares = Vec::new();
    while let Some(v) = take_flag(args, "--seat-share") {
        seat_shares.push(misaka_palw_sdk::preflight::chain::parse_seat_share(&v)?);
    }
    let headers = take_flag(args, "--headers").map(PathBuf::from);
    let adapter = take_flag(args, "--adapter").map(PathBuf::from);
    let mut quant_formats = Vec::new();
    while let Some(f) = take_flag(args, "--quant-format") {
        quant_formats.push(PathBuf::from(f));
    }
    // `--node-facts <file>`: what a node said (`misaka model preflight --node` writes the same facts); `--pack <dir>` / `--artifact <file>`:
    // the `full` depth's inputs; `--hf-endpoint <url>`: the endpoint a repository id (`hf://org/name[@revision]`) is read from.
    let node = match take_flag(args, "--node-facts") {
        Some(f) => Some(
            misaka_palw_sdk::preflight::node::NodeFacts::parse(&std::fs::read_to_string(&f).map_err(|e| format!("--node-facts {f}: {e}"))?)?,
        ),
        None => None,
    };
    let full = misaka_palw_sdk::preflight::full::FullInputs {
        pack: take_flag(args, "--pack").map(PathBuf::from),
        artifact: take_flag(args, "--artifact").map(PathBuf::from),
    };
    let hf_endpoint = take_flag(args, "--hf-endpoint").unwrap_or_else(|| "https://huggingface.co".to_string());
    let held = args.iter().any(|a| a == "--held");
    let json = args.iter().any(|a| a == "--json");
    args.retain(|a| a != "--held" && a != "--json");
    let input = args.first().ok_or(USAGE)?.clone();
    let defaults = Options::default();
    let opts = Options {
        depth,
        network: Some(network.unwrap_or("testnet-12").to_string()),
        height,
        quant_formats,
        adapter,
        headers,
        max_context,
        tile_len: tile_len.unwrap_or(defaults.tile_len),
        h_chunk: h_chunk.unwrap_or(defaults.h_chunk),
        held,
        seat_shares,
        residency_pin_below_bytes: defaults.residency_pin_below_bytes,
        node,
        full,
    };
    // A repository: `http(s)://…/<base>` (a mirror, a fixture server) or `hf://org/name[@revision]`, read by ranges.
    let remote_base = if let Some(rest) = input.strip_prefix("hf://") {
        let (repo, revision) = rest.split_once('@').unwrap_or((rest, "main"));
        Some(misaka_palw_sdk::preflight::remote::hf_base_url(&hf_endpoint, repo, revision))
    } else if input.starts_with("http://") || input.starts_with("https://") {
        Some(input.clone())
    } else {
        None
    };
    let report = match remote_base {
        Some(base) if base.starts_with("http://") => {
            misaka_palw_sdk::preflight::run_remote(&base, &misaka_palw_sdk::preflight::remote::HttpRangeFetcher::default(), &opts)?
        }
        Some(base) => misaka_palw_sdk::preflight::run_remote(&base, &misaka_palw_tir_lower::weights::CurlFetcher::new(), &opts)?,
        None => misaka_palw_sdk::preflight::run(&PathBuf::from(&input), &opts)?,
    };
    if json {
        println!("{}", report.to_json());
    } else {
        print!("{}", report.render());
    }
    Ok(report.registrable())
}

/// `check-architecture`'s switches.
struct ArchFlags {
    legacy: bool,
    held: bool,
    json: bool,
    /// The layout facts IR mode admits with (`--tile-len`, `--h-chunk`).
    tile_len: u32,
    h_chunk: u32,
    /// `--source-len` / `--target-len`: the lengths an encoder–decoder is judged at.
    source_len: u32,
    target_len: u32,
    /// `--adapter`: a user-supplied adapter file, `builtin:<id>`, `none` or `auto` (the default).
    adapter: Option<String>,
}

/// The config text of a Hugging Face directory (`config.json`, and the checkpoint's tensor names when
/// it holds weights) or of a `config.json` file.
fn read_model_config(path: &str) -> Result<(String, Option<misaka_palw_tir_lower::hf_schema::TensorIndex>), String> {
    let p = std::path::Path::new(path);
    if p.is_dir() {
        let text = std::fs::read_to_string(p.join("config.json")).map_err(|e| format!("{path}/config.json: {e}"))?;
        let tensors = misaka_palw_tir_lower::hf_schema::TensorIndex::from_checkpoint_path(p).ok();
        Ok((text, tensors))
    } else {
        Ok((std::fs::read_to_string(p).map_err(|e| format!("{path}: {e}"))?, None))
    }
}

/// `check-architecture`: returns whether the verdict is ADMISSIBLE.
fn check_architecture(view: &NetworkView, config: Option<&str>, tir: Option<&str>, flags: &ArchFlags) -> Result<bool, String> {
    use misaka_palw_sdk::check_architecture::*;
    let ArchFlags { legacy, held, json, tile_len, h_chunk, adapter, source_len, target_len } = flags;
    let (legacy, held, json, tile_len, h_chunk) = (*legacy, *held, *json, *tile_len, *h_chunk);
    if legacy {
        let path = config.ok_or("--legacy needs --config")?;
        let (text, _) = read_model_config(path)?;
        let sdk = sdk_for(view);
        let r = check_legacy_config_v1(&view.params, &view.bundle, &sdk, &text);
        if json {
            let rows: Vec<serde_json::Value> = r
                .rows
                .iter()
                .map(|x| {
                    serde_json::json!({ "model_id": x.model_id, "n_ctx": x.n_ctx, "shipped": x.shipped,
                                        "class_id": x.class_id.to_string(), "verdict": x.verdict.to_string() })
                })
                .collect();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({ "mode": "legacy", "network": view.network_id.to_string(),
                    "architecture": r.architecture, "lineage": r.lineage, "verdict": r.verdict.to_string(), "rows": rows }))
                .unwrap_or_default()
            );
        } else {
            println!("legacy mode on {} — {} → lineage {}", view.network_id, r.architecture, r.lineage.unwrap_or("none"));
            println!("  verdict: {}", r.verdict);
            for x in &r.rows {
                println!(
                    "  {:<52} n_ctx {:>8}  {}  class {}  {}",
                    x.model_id,
                    x.n_ctx,
                    if x.shipped { "shipped  " } else { "projected" },
                    x.class_id,
                    x.verdict
                );
            }
        }
        return Ok(r.verdict.is_admissible());
    }
    let r = match (config, tir) {
        (Some(path), None) => {
            let (text, tensors) = read_model_config(path)?;
            let read = misaka_palw_tir_lower::hf_schema::ReadOptions {
                adapter: match adapter {
                    Some(a) => misaka_palw_tir_lower::hf_schema::AdapterChoice::parse_arg(a).map_err(|e| e.to_string())?,
                    None => misaka_palw_tir_lower::hf_schema::AdapterChoice::Auto,
                },
            };
            let shape = IrShapeV1 { source_len: *source_len, target_len: *target_len };
            check_ir_config_shaped_v1(&view.params, &text, tensors.as_ref(), &read, held, tile_len, h_chunk, shape)
        }
        (None, Some(path)) => {
            let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
            let program = misaka_palw_tir::TirProgramV1::decode_canonical(&bytes).map_err(|e| format!("{path}: {e}"))?;
            check_ir_program_at_v1(&view.params, &program, tile_len, h_chunk)
        }
        _ => return Err("give exactly one of --config and --tir".into()),
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "mode": "ir", "network": view.network_id.to_string(),
                "architecture": r.architecture, "architecture_report": r.architecture_report,
                "verdict": r.verdict.to_string(), "ceilings": r.ceilings_source,
                "program_bytes": r.program_bytes, "artifact_bytes": r.artifact_bytes.to_string(), "blocks": r.blocks, "nodes": r.nodes, "unrolled_nodes": r.unrolled_nodes,
                "max_context": r.max_context, "graph_ir_root": r.graph_ir_root.map(|h| h.to_string()),
                "admission": r.admission_json, "unverified": r.unverified }))
            .unwrap_or_default()
        );
    } else {
        println!("IR mode on {} — {}", view.network_id, if r.architecture.is_empty() { "(program)" } else { &r.architecture });
        if let Some(a) = &r.architecture_report {
            // The features the model combines, each SUPPORTED or MISSING, the level, the adapter used.
            for line in a.render().lines() {
                println!("  {line}");
            }
        }
        println!("  verdict: {}", r.verdict);
        println!("  ceilings: {}", r.ceilings_source);
        if r.program_bytes > 0 {
            println!("  program: {} bytes, {} blocks, {} nodes", r.program_bytes, r.blocks, r.nodes);
        }
        if r.artifact_bytes > 0 {
            // Reported, never judged: a registration may carry any artifact size; a seat's resources gate its readiness.
            println!("  seat need: {:.2} GiB of integer parameters resident (a seat's resources gate readiness, not admission)", r.artifact_bytes as f64 / (1u64 << 30) as f64);
        }
        if let Some(h) = r.graph_ir_root {
            println!("  graph_ir_root {h}");
        }
        if r.unrolled_nodes > 0 {
            println!("  max_context {}, {} unrolled nodes a position", r.max_context, r.unrolled_nodes);
        }
        print!("{}", r.admission_text);
        for u in &r.unverified {
            println!("  unverified: {u}");
        }
    }
    Ok(r.verdict.is_admissible())
}

/// `check-architecture --lora-budget`: the report, text or JSON.
fn lora_budget(
    view: &NetworkView,
    config: &str,
    choice: &misaka_palw_sdk::check_architecture::LoraBudgetChoiceV1,
    json: bool,
) -> Result<(), String> {
    let text = std::fs::read_to_string(config).map_err(|e| format!("{config}: {e}"))?;
    let r = misaka_palw_sdk::check_architecture::check_lora_budget_v1(&view.params, &view.bundle, &text, choice)?;
    if json {
        let mut v = r.to_json();
        v["network"] = serde_json::Value::String(view.network_id.to_string());
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        println!("on {}", view.network_id);
        print!("{}", r.render());
    }
    Ok(())
}

/// **`improve eval`** (RFC-0004, A10): run the evaluation executor on a held class, offline.
fn improve_eval(args: &mut Vec<String>) -> Result<(), String> {
    use kaspa_consensus_core::palw_improve_eval_v1::{PalwEvalModeV1, PalwEvalStageParamsV1, palw_improve_answer_of_v1, palw_improve_answer_span_hash_v1, palw_improve_answer_span_v1};
    use kaspa_consensus_core::palw_improve_state_v1::{PalwEvalSubjectV1, PalwScoringKindV1};
    use misaka_palw_sdk::improve::PalwImproveEvalTaskV1;
    use misaka_palw_sdk::improve_eval::{PalwEvalHeldV1, palw_eval_run_v1};
    use misaka_palw_sdk::PalwModelLineageV1;
    let ids = |text: String, what: &str| -> Result<Vec<u32>, String> {
        text.split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| t.parse::<u32>().map_err(|e| format!("{what}: {t:?}: {e}")))
            .collect()
    };
    let parent = take_flag(args, "--parent");
    let section = take_flag(args, "--section");
    let full = take_flag(args, "--artifact");
    let entry = match (parent, section, full) {
        (Some(parent), Some(section), None) => {
            let lineage = misaka_palw_sdk::lineages::tir::TirLineageV1::new();
            lineage.load(std::path::Path::new(&parent), misaka_palw_sdk::PalwWeightResidencyV1::PageCache).map_err(|e| format!("{parent}: {e}"))?;
            lineage.open_composite_entry(std::path::Path::new(&section))?
        }
        (None, None, Some(full)) => misaka_palw_sdk::lineages::tir::TirLineageV1::open_entry(std::path::Path::new(&full))?,
        _ => return Err("improve eval takes --artifact <class.palwtir>, or --parent <parent.palwtir> --section <candidate.palwtirs>".into()),
    };
    let held = PalwEvalHeldV1::from_entry(&entry)?;
    let max_new: u32 = take_flag(args, "--max-new").map(|v| v.parse().map_err(|e| format!("--max-new: {e}"))).transpose()?.unwrap_or(8);
    let stop = take_flag(args, "--stop").map(|v| ids(v, "--stop")).transpose()?.unwrap_or_default();
    let open: i32 = take_flag(args, "--open").map(|v| v.parse().map_err(|e| format!("--open: {e}"))).transpose()?.unwrap_or(-1);
    let close: i32 = take_flag(args, "--close").map(|v| v.parse().map_err(|e| format!("--close: {e}"))).transpose()?.unwrap_or(-1);
    let reference = take_flag(args, "--reference").map(|v| ids(v, "--reference")).transpose()?;
    // The items: a setter-set spec's prompts and keys, or one --prompt (and --key).
    let (prompts, keys): (Vec<Vec<u32>>, Vec<Option<Vec<u32>>>) = match take_flag(args, "--items") {
        Some(path) => {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))?;
            let rows = |k: &str| -> Result<Vec<Vec<u32>>, String> {
                v.get(k)
                    .and_then(|a| a.as_array())
                    .ok_or_else(|| format!("{path}: no `{k}` array"))?
                    .iter()
                    .map(|row| {
                        row.as_array()
                            .ok_or_else(|| format!("{path}: a `{k}` row is not an array"))?
                            .iter()
                            .map(|t| t.as_u64().and_then(|t| u32::try_from(t).ok()).ok_or_else(|| format!("{path}: a `{k}` id is not a u32")))
                            .collect()
                    })
                    .collect()
            };
            let prompts = rows("prompts")?;
            let keys = if v.get("keys").is_some() { rows("keys")?.into_iter().map(Some).collect() } else { vec![None; prompts.len()] };
            (prompts, keys)
        }
        None => {
            let prompt = ids(take_flag(args, "--prompt").ok_or("--items <spec.json> or --prompt <ids> is required")?, "--prompt")?;
            let key = take_flag(args, "--key").map(|v| ids(v, "--key")).transpose()?;
            (vec![prompt], vec![key])
        }
    };
    let mut results = Vec::new();
    for (i, prompt) in prompts.iter().enumerate() {
        let (kind, mode, params, reference_ids) = match &reference {
            Some(r) => (
                PalwScoringKindV1::RefLogLik,
                PalwEvalModeV1::TeacherForced { reference_commitment: Hash64::default() },
                PalwEvalStageParamsV1::RefLogLik { logit_scale_q24: 1 << 20 },
                r.clone(),
            ),
            None => (
                PalwScoringKindV1::ExactMatch,
                PalwEvalModeV1::Generate { seed: Hash64::default(), max_new, stop_ids: stop.clone() },
                PalwEvalStageParamsV1::ExactMatch { open, close },
                Vec::new(),
            ),
        };
        let task = PalwImproveEvalTaskV1 {
            line_id: Hash64::default(),
            epoch: 0,
            item: i as u32,
            subject: PalwEvalSubjectV1::Parent,
            subject_class: held.class_id,
            kind,
            mode: mode.clone(),
            job_id: Hash64::default(),
            prompt_ids: prompt.clone(),
            reference_ids,
            params,
        };
        let work = palw_eval_run_v1(&held, &task).map_err(|e| format!("item {i}: {e}"))?;
        let generated = work.generated().to_vec();
        let mut row = serde_json::json!({
            "item": i,
            "prompt": prompt,
            "generated": generated,
            "positions": task.positions_of(generated.len()),
            "step_leaves": work.execution.space.leaf_count(),
        });
        if kind == PalwScoringKindV1::ExactMatch {
            let span = palw_improve_answer_span_v1(&generated, open, close).map(<[u32]>::to_vec);
            row["answer_span"] = serde_json::json!(span);
            row["answer"] = serde_json::json!(palw_improve_answer_of_v1(&generated, open, close).map(|h| h.to_string()));
            if let Some(Some(key)) = keys.get(i) {
                row["key"] = serde_json::json!(key);
                row["pass"] = serde_json::json!(palw_improve_answer_of_v1(&generated, open, close) == Some(palw_improve_answer_span_hash_v1(key)));
            }
        } else {
            row["score"] = serde_json::json!(work.tail.score);
        }
        results.push(row);
    }
    let passes = results.iter().filter(|r| r["pass"] == serde_json::json!(true)).count();
    let keyed = results.iter().filter(|r| r.get("pass").is_some()).count();
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "class": entry.class_id().to_string(),
            "artifact_root": entry.artifact_root.to_string(),
            "composite": entry.artifact.composite_ref().is_some(),
            "items": results.len(),
            "keyed": keyed,
            "passes": passes,
            "results": results,
        }))
        .map_err(|e| e.to_string())?
    );
    Ok(())
}

/// `composite`: a LoRA candidate's container checked against its parent's, its sections rooted, and
/// (with `out`) written again with the record in its provenance.
/// **`improve <kind>`** (RFC-0004, A10): build and sign one of the Model Improvement Protocol's objects.
fn improve(args: &mut Vec<String>, network: Option<&str>) -> Result<(), String> {
    use kaspa_consensus_core::palw_improve_artifact_v1::PalwTirArtifactRefV1;
    use kaspa_consensus_core::palw_improve_state_v1::PalwLineageRollbackV1;
    use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
    use misaka_palw_sdk::improve_objects::{self as obj, PalwImproveSignerV1, spec};
    let kind = if args.is_empty() { return Err(USAGE.to_string()) } else { args.remove(0) };
    let network_id: NetworkId = network.ok_or(USAGE)?.parse().map_err(|e| format!("--network: {e}"))?;
    let drill = match take_flag(args, "--drill-salt") {
        Some(hex) => Some(kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(&hex).map_err(|e| format!("--drill-salt: {e}"))?),
        None => None,
    };
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network_id, drill.as_ref())?;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(params.net.to_string().as_bytes(), Some(params.genesis.hash));
    let key_file = take_flag(args, "--key-file").ok_or("--key-file <ml-dsa-87 seed> is required")?;
    let seed = kaspa_pq_validator_core::load_validator_seed(&key_file)?;
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
    let signer = PalwImproveSignerV1 { key: &key, network_domain: domain };
    let out = PathBuf::from(take_flag(args, "--out").ok_or("--out <path> is required")?);
    let bond = match take_flag(args, "--bond") {
        Some(b) => Some(spec::bond(&b)?),
        None => None,
    };
    let need_bond = || bond.ok_or_else(|| "--bond <txid:index> is required for this object".to_string());
    let spec_json = |args: &mut Vec<String>| -> Result<serde_json::Value, String> {
        let path = take_flag(args, "--spec").ok_or("--spec <json> is required")?;
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| format!("{path}: {e}"))
    };
    let write = |path: &std::path::Path, object: &PalwConsensusObjectV2, what: &str| -> Result<(), String> {
        let bytes = borsh::to_vec(object).map_err(|e| e.to_string())?;
        std::fs::write(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {} ({} bytes): {what}", path.display(), bytes.len());
        Ok(())
    };
    let side = |suffix: &str| -> PathBuf {
        let mut name = out.clone().into_os_string();
        name.push(suffix);
        PathBuf::from(name)
    };
    match kind.as_str() {
        "policy" => {
            let v = spec_json(args)?;
            let line = spec::hash(v.get("line").and_then(|l| l.as_str()).ok_or("the spec has no `line`")?)?;
            let sequence = v.get("sequence").and_then(|s| s.as_u64()).ok_or("the spec has no `sequence`")?;
            let opt_out = v.get("opt_out").and_then(|o| o.as_bool()).unwrap_or(false);
            let policy = if opt_out { None } else { Some(spec::policy(v.get("policy").unwrap_or(&serde_json::Value::Null))?) };
            if let Some(p) = &policy {
                let ceilings = match params.palw_improvement_v1 {
                    Some(f) => f.ceilings,
                    None => kaspa_consensus_core::palw_improve_v1::PALW_DRILL_IMPROVE_CEILINGS_V1,
                };
                // Λ, the ruleset's fast honest claim path (spec 17 §17.4.3 rows 19-20): the anchor slot, the licence allowance and the
                // challenge window in force where the policy is applied (here: at the improvement fence's height, else DAA 0) — the
                // bound the epoch's windows are held to.
                let lifecycle = match &params.palw_consensus_mode {
                    kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                        let at = params.palw_improvement_v1.map_or(0, |f| f.activation.daa_score());
                        Some(kaspa_consensus_core::palw_improve_v1::palw_improvement_claim_lifecycle_v1(
                            kaspa_consensus_core::palw_improve_v1::palw_improvement_claim_lifecycle_base_v1(bundle),
                            bundle.state.window_challenge_at(at),
                        ))
                    }
                    _ => None,
                };
                kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_policy_check_v1(p, &ceilings, lifecycle)
                    .map_err(|why| format!("the policy would be refused: {why}"))?;
                println!("policy check      ok under {} ceilings", if params.palw_improvement_v1.is_some() { "the network's" } else { "the drill's" });
                match lifecycle {
                    Some(l) => println!("claim lifecycle   {l} DAA (court_margin must be at least this)"),
                    None => println!("claim lifecycle   not asked (this network has no V2 bundle)"),
                }
                println!("policy digest     {}", kaspa_consensus_core::palw_improve_state_v1::palw_improvement_policy_digest_v1(p));
                println!("epoch length L_e  {} DAA", kaspa_consensus_core::palw_improve_policy_v1::palw_improvement_epoch_length_v1(&p.windows));
            }
            write(&out, &signer.policy(line, sequence, policy), "ModelLineImprovementPolicySet (tag 70)")
        }
        "hard-case" => {
            let v = spec_json(args)?;
            let (case, opened) = spec::hard_case(&v)?;
            println!("case id           {}", case.case_id);
            if let Some((key, salt)) = opened {
                std::fs::write(side(".key.json"), serde_json::json!({ "case_id": case.case_id.to_string(), "key": key, "salt": salt.to_string() }).to_string())
                    .map_err(|e| e.to_string())?;
                println!("key and salt      {}", side(".key.json").display());
            }
            write(&out, &signer.hard_case(need_bond()?, case), "HardCaseSubmitted (tag 71)")
        }
        "opt-in" => {
            let tx_path = take_flag(args, "--commitment-tx").ok_or("--commitment-tx <fp-commitment-tx.borsh> is required")?;
            let tx: kaspa_consensus_core::tx::Transaction =
                borsh::from_slice(&std::fs::read(&tx_path).map_err(|e| format!("{tx_path}: {e}"))?).map_err(|e| format!("{tx_path}: {e}"))?;
            let payload: kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3 =
                borsh::from_slice(&tx.payload).map_err(|e| format!("{tx_path}: the payload is not a free-prompt commitment: {e}"))?;
            let facts = kaspa_consensus_core::palw_improve_material_v1::PalwFpJobFactsV1::of_commitment(&payload.commitment);
            let claim = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&payload.commitment);
            let committer = PalwBondKeyV2(payload.commitment.job.executor_bond);
            println!("claim             {claim}\njob pin           {}", facts.pin());
            let optin = kaspa_consensus_core::palw_improve_material_v1::PalwDataUseOptInV1 { job_pin: facts.pin(), claim, job: facts };
            write(&out, &signer.data_use_opt_in(committer, optin), "DataUseOptIn (tag 72)")
        }
        "setter-set" => {
            let v = spec_json(args)?;
            let (set, salt) = spec::setter_set(&v)?;
            println!("set id            {}\nitems             {}\nsalt              {salt}", set.commitment.set_id, set.commitment.items);
            write(&side(".prompts"), &obj::improve_setter_prompts_object_v1(set.prompts), "SetterSetRevealed (tag 74): the prompts")?;
            write(&side(".keys"), &obj::improve_setter_keys_object_v1(set.keys), "SetterKeysRevealed (tag 75): the keys")?;
            write(&out, &signer.setter_set(need_bond()?, set.commitment), "SetterSetCommitted (tag 73)")
        }
        "dataset" => {
            let dataset = spec::dataset(&spec_json(args)?)?;
            println!("dataset id        {}", dataset.dataset_id);
            write(&out, &signer.dataset(need_bond()?, dataset), "DatasetRegistered (tag 76)")
        }
        "artifact" => {
            let artifact = spec::teaching_artifact(&spec_json(args)?)?;
            let commit = obj::improve_teaching_commit_v1(&artifact);
            println!("commitment        {}\nsalt              {}", commit.commit, artifact.salt);
            write(&side(".reveal"), &obj::improve_artifact_reveal_object_v1(artifact), "TeachingArtifactRevealed (tag 78)")?;
            write(&out, &signer.teaching_artifact(need_bond()?, commit), "TeachingArtifactCommitted (tag 77)")
        }
        "licence" => {
            let v = spec_json(args)?;
            let domains: Vec<u16> = match v.get("domains") {
                Some(d) => d
                    .as_array()
                    .ok_or("`domains` is not an array")?
                    .iter()
                    .map(|x| x.as_u64().and_then(|x| u16::try_from(x).ok()).ok_or("a domain is not a u16"))
                    .collect::<Result<_, _>>()?,
                None => Vec::new(),
            };
            let licence = obj::improve_licence_v1(
                key.public_key().to_vec(),
                spec::hash(v.get("model_family").and_then(|m| m.as_str()).ok_or("the spec has no `model_family`")?)?,
                domains,
                v.get("uses").and_then(|u| u.as_u64()).unwrap_or(1) as u8,
                v.get("per_use_fee").and_then(|f| f.as_u64()).unwrap_or(0),
                v.get("expiry_daa").and_then(|e| e.as_u64()).ok_or("the spec has no `expiry_daa`")?,
            );
            println!("licence id        {}", licence.licence_id);
            write(&out, &signer.licence(licence)?, "TeacherLicenceRegistered (tag 79)")
        }
        "candidate" => {
            let v = spec_json(args)?;
            let line = spec::hash(v.get("line").and_then(|l| l.as_str()).ok_or("the spec has no `line`")?)?;
            let epoch = v.get("epoch").and_then(|e| e.as_u64()).ok_or("the spec has no `epoch`")?;
            let declarations = spec::declarations(v.get("declarations").unwrap_or(&serde_json::Value::Null))?;
            let parent = take_flag(args, "--parent");
            let section = take_flag(args, "--section");
            let full = take_flag(args, "--artifact");
            let (class_id, artifact, layout) = match (parent, section, full) {
                (Some(parent), Some(section), None) => {
                    let lineage = misaka_palw_sdk::lineages::tir::TirLineageV1::new();
                    use misaka_palw_sdk::PalwModelLineageV1;
                    lineage
                        .load(std::path::Path::new(&parent), misaka_palw_sdk::PalwWeightResidencyV1::PageCache)
                        .map_err(|e| format!("{parent}: {e}"))?;
                    let entry = lineage.open_composite_entry(std::path::Path::new(&section))?;
                    let r = *entry.artifact.composite_ref().ok_or("the section opened as a single artifact")?;
                    (
                        entry.class_id(),
                        PalwTirArtifactRefV1::Composite { parent_class: r.parent_class, parent_root: r.parent_root, adapter_root: r.adapter_root, p: r.p },
                        entry.class.layout.clone(),
                    )
                }
                (None, None, Some(full)) => {
                    let entry = misaka_palw_sdk::lineages::tir::TirLineageV1::open_entry(std::path::Path::new(&full))?;
                    (entry.class_id(), PalwTirArtifactRefV1::Single { root: entry.artifact_root }, entry.class.layout.clone())
                }
                _ => return Err("a candidate is --parent <parent.palwtir> --section <candidate.palwtirs> (a composite) or --artifact <candidate.palwtir> (full weights)".into()),
            };
            println!("candidate class   {class_id}");
            let payload = obj::improve_candidate_v1(line, epoch, class_id, artifact, layout, declarations);
            write(&out, &signer.candidate(need_bond()?, payload), "CandidateSubmitted (tag 80)")
        }
        "adapter-list" => {
            // RFC-0001 §2.10: list a composite (parent + adapter section) as an adapter class — no spec, the artifacts say it all.
            let parent = take_flag(args, "--parent").ok_or("--parent <parent.palwtir> is required")?;
            let section = take_flag(args, "--section").ok_or("--section <candidate.palwtirs> is required")?;
            let lineage = misaka_palw_sdk::lineages::tir::TirLineageV1::new();
            use misaka_palw_sdk::PalwModelLineageV1;
            lineage.load(std::path::Path::new(&parent), misaka_palw_sdk::PalwWeightResidencyV1::PageCache).map_err(|e| format!("{parent}: {e}"))?;
            let entry = lineage.open_composite_entry(std::path::Path::new(&section))?;
            let r = *entry.artifact.composite_ref().ok_or("the section opened as a single artifact")?;
            let class_id = entry.class_id();
            println!("adapter class     {class_id}");
            let payload = kaspa_consensus_core::palw_adapter_class_v1::PalwAdapterClassListingV1 {
                class_id,
                artifact: kaspa_consensus_core::palw_improve_composite_v1::PalwTirCompositeRefV1 {
                    parent_class: r.parent_class,
                    parent_root: r.parent_root,
                    adapter_root: r.adapter_root,
                    p: r.p,
                },
                layout: entry.class.layout.clone(),
            };
            write(&out, &signer.adapter_class_listed(need_bond()?, payload), "AdapterClassListed (tag 94)")
        }
        "rollback" => {
            let v = spec_json(args)?;
            let payload = PalwLineageRollbackV1 {
                line_id: spec::hash(v.get("line").and_then(|l| l.as_str()).ok_or("the spec has no `line`")?)?,
                epoch: v.get("epoch").and_then(|e| e.as_u64()).ok_or("the spec has no `epoch`")?,
                to_class: spec::hash(v.get("to_class").and_then(|c| c.as_str()).ok_or("the spec has no `to_class`")?)?,
                cause: spec::rollback_cause(v.get("cause").ok_or("the spec has no `cause`")?)?,
            };
            write(&out, &signer.rollback(need_bond()?, payload), "LineageHeadRolledBack (tag 81)")
        }
        other => Err(format!("unknown improve kind {other:?}\n\n{USAGE}")),
    }
}

fn composite(
    parent: &std::path::Path,
    parent_class: Option<Hash64>,
    candidate: &std::path::Path,
    out: Option<&std::path::Path>,
    section_out: Option<&std::path::Path>,
) -> Result<(), String> {
    use misaka_palw_sdk::tir_composite::{tir_composite_derive_v1, tir_composite_section_write_v1, tir_composite_write_v1};
    use misaka_palw_tir_artifact::PalwTirContainerV1;
    let pc = PalwTirContainerV1::open(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    let cc = PalwTirContainerV1::open(candidate).map_err(|e| format!("{}: {e}", candidate.display()))?;
    let c = tir_composite_derive_v1(&pc, &cc, parent_class)?;
    println!("composite of {} over {}", candidate.display(), parent.display());
    println!("  P               {} (the parent's params), then {} of the adapter's", c.p, cc.program.params.len() as u32 - c.p);
    println!("  parent root     {} ({} leaves: the candidate's params 0..P, byte for byte)", c.parent_root, c.parent_leaves);
    println!("  adapter root    {} ({} leaves)", c.adapter_root, c.adapter_leaves);
    match (c.parent_class, c.artifact_root()) {
        (Some(pcid), Some(root)) => {
            println!("  parent class    {pcid}");
            println!("  artifact root   {root} (what the candidate's class id commits to)");
        }
        _ => println!("  parent class    not given (--parent-class <hex> derives the composite artifact root)"),
    }
    if let Some(out) = out {
        let digest = tir_composite_write_v1(&cc, &c, out)?;
        println!("wrote {} (file digest {})", out.display(), Hash64::from_bytes(digest));
    }
    if let Some(section) = section_out {
        let digest = tir_composite_section_write_v1(&cc, &c, section)?;
        println!(
            "wrote {} (the adapter section: params {}.. alone, file digest {}) — a seat holding the parent loads it after the parent",
            section.display(),
            c.p,
            Hash64::from_bytes(digest)
        );
    }
    Ok(())
}

/// **ADR-0096 Decision 10: bind a tokenizer into a converted artifact, and MEASURE that the
/// class's registered root did not move.**
///
/// The binding itself is `misaka_palw_base0::artifact::bind_tokenizer_file_v1` (one field set,
/// read back bound). What this command adds is the measurement a person needs before they trust
/// the output: the SDK's own pairing is run on the input and on the output, and the root each
/// pairs to is printed side by side.
///
/// **Two kinds of row answer differently, and both answers are printed** (measured 2026-09-10 on
/// the published dense file): a row that registers the INVENTORY root — the tiled-map rows,
/// `graph-v2`, `graph-v3`, `graph-v5@512` — keeps its root, because the tokenizer commitment is not
/// an input to it; a row that registers the artifact DIGEST — the one-byte-map rows — moves,
/// because the commitment is inside the digest, and for that row the output is a new artifact.
/// The gate is the class the person is binding FOR: `--model-id` names it, and the command fails
/// (and deletes the output) if that row's root moved; with no `--model-id`, it fails only if
/// every row moved. A file that registers somewhere else is not the file the person asked for.
///
/// Written to `<out>.tmp` and renamed, so a crash leaves no half file under the name a runtime
/// looks for. Holds the artifact in memory twice (about 3.6 GB for the 1.7 GB dense file).
fn bind_tokenizer(
    view: &NetworkView,
    input: &std::path::Path,
    tokenizer: &std::path::Path,
    out: &std::path::Path,
    wanted: Option<&str>,
) -> Result<(), String> {
    use misaka_palw_base0::artifact::{TokenizerBindOutcomeV1, bind_tokenizer_file_v1};
    if out == input {
        return Err("--out must not be the input: the input is the evidence the output is compared against".to_string());
    }
    let artifact_bytes = std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let tokenizer_bytes = std::fs::read(tokenizer).map_err(|e| format!("{}: {e}", tokenizer.display()))?;
    let outcome = bind_tokenizer_file_v1(&artifact_bytes, &tokenizer_bytes)?;
    drop(artifact_bytes);
    let (bytes, commitment, before, after) = match outcome {
        TokenizerBindOutcomeV1::AlreadyBound { commitment, artifact_digest } => {
            println!(
                "already bound: {} declares tokenizer {commitment} (artifact digest {artifact_digest}); nothing written",
                input.display()
            );
            return Ok(());
        }
        TokenizerBindOutcomeV1::Bound { bytes, commitment, digest_before, digest_after } => {
            (bytes, commitment, digest_before, digest_after)
        }
    };
    let tmp = out.with_extension("tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", tmp.display()))?;
    drop(bytes);
    std::fs::rename(&tmp, out).map_err(|e| format!("{} -> {}: {e}", tmp.display(), out.display()))?;

    let sdk = sdk_for(view);
    // One row per registered model: its id and the root the artifact registers it under.
    type ModelRoots = Vec<(String, Result<Hash64, String>)>;
    let roots = |path: &std::path::Path| -> Result<ModelRoots, String> {
        let loaded = sdk.load_artifact(path)?;
        Ok(sdk.pairings(&loaded).into_iter().map(|(entry, root)| (entry.model_id.to_string(), root)).collect())
    };
    let (input_roots, output_roots) = (roots(input)?, roots(out)?);
    let (mut kept, mut moved) = (Vec::new(), Vec::new());
    for ((model, a), (_, b)) in input_roots.iter().zip(output_roots.iter()) {
        if let (Ok(a), Ok(b)) = (a, b) {
            if a == b {
                println!("  {model}: registered root {a}  (unchanged — this row registers the inventory root)");
                kept.push(model.clone());
            } else {
                println!(
                    "  {model}: registered root {a} → {b}  (MOVED — this row registers the artifact digest; for it this is a new artifact)"
                );
                moved.push(model.clone());
            }
        }
    }
    let refusal = match wanted {
        Some(model) if moved.iter().any(|m| m == model) => {
            Some(format!("binding moved the registered root of {model}, the class named by --model-id"))
        }
        Some(model) if !kept.iter().any(|m| m == model) => {
            Some(format!("{model} does not pair with this artifact — run `palw-class inspect`"))
        }
        None if kept.is_empty() => Some("binding moved the registered root of every class this artifact pairs with".to_string()),
        _ => None,
    };
    if let Some(why) = refusal {
        let _ = std::fs::remove_file(out);
        return Err(format!("{why} — the output was deleted"));
    }
    println!("tokenizer commitment {commitment}");
    println!("artifact digest      {before} → {after}");
    println!("wrote                {}", out.display());
    Ok(())
}

fn ledger(view: &NetworkView) {
    let sdk = sdk_for(view);
    println!("classes this build can supply, against {}:", view.network_id);
    for entry in sdk.ledger() {
        let class_id = entry.class_id();
        let registered =
            if view.genesis_classes.iter().any(|(id, _)| *id == class_id) { "genesis-registered" } else { "unregistered" };
        let file = if entry.needs_artifact_file { "needs artifact file" } else { "derived, no file" };
        println!("  {}  [{}]", entry.model_id, entry.lineage_id);
        println!("    class id   {class_id}");
        println!(
            "    canonical  prefill {} / decode {}   {file}   {registered} (genesis view)",
            entry.canonical_job.0, entry.canonical_job.1
        );
    }
}

/// **Write — or re-check — the sidecar that keeps an inventory root out of human hands.**
///
/// Every row comes from `PalwClassManifestFileV1::derive_from_artifact`, which is `sdk.pairings` ->
/// `CanonicalClassV1::artifact_root`: the expression a producer evaluates when it asks whether it can
/// serve a registered class. A manifest built any other way would be the second mapping this is
/// replacing rather than a record of the first.
fn manifest(view: &NetworkView, path: &std::path::Path, out: Option<PathBuf>, check: bool) -> Result<(), String> {
    if misaka_palw_sdk::tir_manifest::PalwTirManifestV1::sniff(path) {
        return tir_manifest(path, out, check);
    }
    let sdk = sdk_for(view);
    let artifact = sdk.load_artifact(path)?;
    let bytes = std::fs::metadata(path).map(|m| m.len()).map_err(|e| format!("{}: {e}", path.display()))?;
    let derived = misaka_palw_sdk::PalwClassManifestFileV1::derive_from_artifact(&sdk, &artifact, bytes).map_err(|e| e.to_string())?;
    let target = out.unwrap_or_else(|| misaka_palw_sdk::PalwClassManifestFileV1::path_beside(path));

    if check {
        let text = std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let on_disk = misaka_palw_sdk::PalwClassManifestFileV1::from_json(&text).map_err(|e| e.to_string())?;
        on_disk.verify_against_the_artifact(&sdk, &artifact).map_err(|e| e.to_string())?;
        println!("{}: agrees with {} — {} class(es) re-derived", target.display(), path.display(), on_disk.rows.len());
        return Ok(());
    }

    println!("{}", artifact.summary);
    println!("artifact digest  {}", derived.artifact_digest);
    if derived.rows.is_empty() {
        return Err(format!(
            "this artifact pairs with no class this build knows, so there is no root to record — \
             run `palw-class inspect --network {} {}` for the reasons",
            view.network_id,
            path.display()
        ));
    }
    for r in &derived.rows {
        let registered = view.genesis_classes.iter().find(|(id, _)| *id == r.class_id.into_hash64());
        let note = match registered {
            Some((_, on_chain)) if *on_chain == r.inventory_root.into_hash64() => "  (matches this network's registration)",
            Some(_) => "  (THIS NETWORK REGISTERED A DIFFERENT ROOT FOR THIS CLASS)",
            None => "",
        };
        println!("  {}  class {}  root {}{note}", r.model_id, r.class_id, r.inventory_root);
    }
    std::fs::write(&target, derived.to_json()).map_err(|e| format!("{}: {e}", target.display()))?;
    println!("wrote {}", target.display());
    Ok(())
}

/// A `PALWTIR1` artifact (RFC-0002 Phase F, F3): ONE inventory root over the program's params,
/// whatever layouts its registrations declare, so the sidecar records it once.
fn tir_manifest(path: &std::path::Path, out: Option<PathBuf>, check: bool) -> Result<(), String> {
    use misaka_palw_sdk::tir_manifest::PalwTirManifestV1;
    let target = out.unwrap_or_else(|| misaka_palw_sdk::PalwClassManifestFileV1::path_beside(path));
    if check {
        let text = std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let on_disk = PalwTirManifestV1::from_json(&text)?;
        on_disk.check(path)?;
        println!(
            "{}: agrees with {} — inventory root re-derived over {} leaves",
            target.display(),
            path.display(),
            on_disk.leaf_count
        );
        return Ok(());
    }
    let m = PalwTirManifestV1::derive(path)?;
    println!("PALWTIR1 artifact {} ({} bytes, program {} bytes)", path.display(), m.artifact_bytes, m.program_bytes);
    println!("  graph_ir_root   {}", m.graph_ir_root);
    println!("  inventory root  {} over {} leaves", m.inventory_root, m.leaf_count);
    match m.class_id {
        Some(id) => println!("  class id        {id} (under the container's declared layout)"),
        None => println!("  class id        — the container declares no layout (a layout makes one class per layout)"),
    }
    std::fs::write(&target, m.to_json()).map_err(|e| format!("{}: {e}", target.display()))?;
    println!("wrote {}", target.display());
    Ok(())
}

/// **An IR artifact's class** (RFC-0002 Phase F): the class it declares, under the root its bytes
/// derive, at the formula's canonical job — what a `--palw-register-class` run registers.
fn inspect_tir(view: &NetworkView, entry: &misaka_palw_sdk::PalwTirClassEntryV1) {
    let taken = view.genesis_classes.iter().any(|(id, _)| *id == entry.class_id());
    println!("  IR CLASS  {}  class {}  root {}", entry.model_id, entry.class_id(), entry.artifact_root);
    println!(
        "            canonical job {} + {}, max_context {}, checkpoint interval {}, h_tile {}{}",
        entry.canonical_job.0,
        entry.canonical_job.1,
        entry.class.layout.max_context,
        entry.class.layout.checkpoint_interval,
        entry.class.layout.h_tile,
        if taken { " (class already in genesis)" } else { "" }
    );
}

fn inspect(view: &NetworkView, path: &std::path::Path) -> Result<(), String> {
    let sdk = sdk_for(view);
    let artifact = sdk.load_artifact(path)?;
    println!("{}", artifact.summary);
    println!("lineage: {}", artifact.lineage_id);
    for entry in misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&artifact)) {
        inspect_tir(view, &entry);
    }
    for (entry, paired) in sdk.pairings(&artifact) {
        match paired {
            Ok(root) => {
                let taken = view.genesis_classes.iter().any(|(id, _)| *id == entry.class_id());
                let status = if taken { " (class already in genesis)" } else { "" };
                println!("  PAIRS   {}  root {root}{status}", entry.model_id);
            }
            Err(why) => println!("  no      {}  — {why}", entry.model_id),
        }
    }
    Ok(())
}

/// `close-sizes`: every terminal close the node builds for the class's job, as carried, against the cap.
fn close_sizes(view: &NetworkView, path: &std::path::Path, anchor: Hash64, json: bool) -> Result<bool, String> {
    use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
    let entry = misaka_palw_sdk::lineages::tir::TirLineageV1::open_entry(path)?;
    let court = view.bundle.court;
    // Dense up to 1 GiB, so every close is built from the capture's own leaves (never a replay).
    let backend = misaka_palw_sdk::lineages::tir::TirLineageV1::backend(&entry, &court, view.params.palw_prompt_ids_form_v1())?
        .with_dense_capture_bytes(1 << 30);
    let (job, prompt) = backend.job_for_anchor(anchor)?;
    let started = std::time::Instant::now();
    let run = backend.execute(&job, &prompt)?;
    eprintln!(
        "{}: job {} + {} ran in {:.1} s ({} capture bytes)",
        entry.model_id,
        job.declared_prefill_tokens,
        job.exact_decode_tokens,
        started.elapsed().as_secs_f64(),
        run.material.len()
    );
    let rules = backend.court_rules(&court);
    let sizes = misaka_palw_sdk::lineages::tir::tir_terminal_close_sizes_v1(&backend, &run.material, &rules)?;
    let cap = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_carriable_close_bytes_v1(&court);
    let over: Vec<_> = sizes.iter().filter(|s| s.carried_bytes > cap).collect();
    let worst = sizes.iter().max_by_key(|s| s.carried_bytes);
    if json {
        let rows: Vec<_> = sizes
            .iter()
            .map(|s| {
                serde_json::json!({
                    "leaf": s.leaf, "call": format!("{:?}", s.call), "kind": format!("{:?}", s.kind),
                    "door": s.door, "carried_bytes": s.carried_bytes,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::json!({
                "schema": "misaka.palw-class.close-sizes.v1", "class_id": entry.class_id().to_string(), "model_id": entry.model_id,
                "cap": cap, "fits": over.is_empty(), "closes": rows,
            })
        );
    } else {
        for s in &sizes {
            println!("{:>10} {:<12} {:>9} {:?} {:?}", s.carried_bytes, s.door, s.leaf, s.call, s.kind);
        }
        match worst {
            Some(w) => println!(
                "{} closes; worst {} bytes ({} at leaf {}); cap {cap} (palw_tir_carriable_close_bytes_v1): {}",
                sizes.len(),
                w.carried_bytes,
                w.door,
                w.leaf,
                if over.is_empty() { "every close fits".to_string() } else { format!("{} OVER", over.len()) }
            ),
            None => println!("no close measured"),
        }
    }
    Ok(over.is_empty())
}

/// `drill-leaves`: the first leaf of every commit-point kind of the class's attempt job.
fn drill_leaves(view: &NetworkView, path: &std::path::Path, wanted: Option<&str>, decode: u32) -> Result<(), String> {
    let entry = misaka_palw_sdk::lineages::tir::TirLineageV1::open_entry(path)?;
    if wanted.is_some_and(|w| w != entry.model_id) {
        return Err(format!("{} declares {}, not the --model-id asked for", path.display(), entry.model_id));
    }
    let mut job = entry.canonical_context();
    job.exact_decode_tokens = decode.max(1);
    let space = kaspa_consensus_core::palw_tir_step_v1::PalwTirStepSpaceV1::new(&entry.class).map_err(|e| e.to_string())?;
    let count = space.leaf_count_capped(&job, view.bundle.court.max_step_leaf_count()).map_err(|e| e.to_string())?;
    let leaves = misaka_palw_sdk::lineages::tir::tir_drill_covering_leaves_v1(&space, &job, count)?;
    eprintln!(
        "{}: class {}, job {} + {} ({count} step leaves), {} commit-point kinds",
        entry.model_id,
        entry.class_id(),
        job.declared_prefill_tokens,
        job.exact_decode_tokens,
        leaves.len()
    );
    for ((kind, call), leaf) in leaves {
        println!("{leaf} {call:?} {kind:?}");
    }
    Ok(())
}

/// `declare-layout`: a lowered artifact written again under a declared layout, judged by the gate.
fn declare_layout(
    view: &NetworkView,
    input: &std::path::Path,
    output: &std::path::Path,
    choice: &misaka_palw_sdk::tir_layout::TirLayoutChoiceV1,
    model_id: Option<&str>,
    parent: Option<&std::path::Path>,
) -> Result<(), String> {
    let d = misaka_palw_sdk::tir_layout::tir_declare_layout_with_parent_v1(
        &view.params,
        &view.bundle,
        input,
        output,
        choice,
        model_id,
        parent,
    )?;
    let l = &d.layout;
    println!("wrote {} (file digest {})", output.display(), Hash64::from_bytes(d.file_digest));
    println!(
        "  layout          max_context {}, checkpoint interval {}, h_tile {}, {} commit tiles ({:?} distinct), {} state tiles",
        l.max_context,
        l.checkpoint_interval,
        l.h_tile,
        l.commit_tiles.len(),
        l.commit_tiles.iter().collect::<std::collections::BTreeSet<_>>(),
        l.state_tiles.len()
    );
    println!("  class id        {}", d.class_id);
    match &d.composite {
        Some(c) => {
            println!("  artifact root   {} (composite: parent class {}, P {})", d.artifact_root, c.parent_class, c.p);
            println!("  parent root     {}", c.parent_root);
            println!("  adapter root    {}", c.adapter_root);
        }
        None => println!("  inventory root  {}", d.artifact_root),
    }
    let at = if d.composite.is_some() { d.admission_at.clone() } else { format!("admission v10 {}", d.admission_at) };
    match &d.admission {
        Err(why) if why.starts_with("not judged") => {
            println!("  UNJUDGED        {why}");
            Err("the declared composite was not judged — nothing should be signed or funded for it yet".to_string())
        }
        Ok(()) => {
            println!("  ADMISSIBLE      {at}");
            Ok(())
        }
        Err(why) => {
            println!("  REFUSED         {at}: {why}");
            Err("the declared class would be refused — nothing should be signed or funded for it".to_string())
        }
    }
}

/// `certify`: the IR class's drill as the chain's `FamilyCertified { TirAttempt }` object, graded
/// here first, and the lane object that seats the class once the family is certified.
fn certify(
    view: &NetworkView,
    path: &std::path::Path,
    wanted: Option<&str>,
    family_id: Option<Hash64>,
    out: &str,
) -> Result<(), String> {
    use kaspa_consensus_core::palw_state_v2::{PALW_OBJECT_CHUNK_MAX_BYTES, palw_object_chunks_v1};
    let sdk = sdk_for(view);
    let artifact = sdk.load_artifact(path)?;
    let entries: Vec<_> = misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&artifact))
        .into_iter()
        .filter(|e| wanted.is_none_or(|w| w == e.model_id))
        .collect();
    let [entry] = entries.as_slice() else {
        return Err(match entries.len() {
            0 => "this artifact declares no IR class (by that --model-id) — `certify` drills PALWTIR1 classes".to_string(),
            n => format!("this artifact declares {n} IR classes — name one with --model-id"),
        });
    };
    let write = |path: &str, object: &PalwConsensusObjectV2| -> Result<usize, String> {
        let bytes = borsh::to_vec(object).map_err(|e| format!("the object does not serialize: {e}"))?;
        std::fs::write(path, &bytes).map_err(|e| format!("{path}: {e}"))?;
        Ok(bytes.len())
    };
    let (object, family) = misaka_palw_sdk::tir_certification::tir_family_certification_v1(
        entry,
        &view.bundle.court,
        view.params.palw_prompt_ids_form_v1(),
        family_id,
    )?;
    let vectors = match &object {
        PalwConsensusObjectV2::FamilyCertified { evidence } => evidence.vector_count(),
        _ => 0,
    };
    let bytes = write(out, &object)?;
    println!(
        "wrote {out}: FamilyCertified (TirAttempt), family {} (digest {}), class {} ({}), {vectors} fault vectors, {} kernels, {bytes} bytes",
        family.family_id,
        family.digest(),
        entry.model_id,
        family.drilled_class_id,
        family.kernel_ids.len()
    );
    match palw_object_chunks_v1(&object) {
        Ok(None) => println!("fits one carrier: submit {out} as it is"),
        Ok(Some(chunks)) => {
            let mut names = Vec::with_capacity(chunks.len());
            for chunk in &chunks {
                let PalwConsensusObjectV2::ObjectChunk { index, count, group, .. } = chunk else {
                    return Err("the chunker returned something other than a chunk".to_string());
                };
                let name = format!("{out}.chunk{index}");
                let n = write(&name, chunk)?;
                println!("wrote {name}: ObjectChunk {index}/{count} of group {group}, {n} bytes");
                names.push(name);
            }
            println!(
                "too large for one carrier ({bytes} > {PALW_OBJECT_CHUNK_MAX_BYTES}): submit the chunks in order — misaka palw \
                 submit-object {} --yes",
                names.iter().map(|n| format!("--object {n}")).collect::<Vec<_>>().join(" ")
            );
        }
        Err(e) => return Err(format!("the drill cannot be chunked: {e}")),
    }
    let lane = format!("{out}.lane");
    let n = write(&lane, &misaka_palw_sdk::tir_certification::tir_lane_certification_v1(entry))?;
    println!(
        "wrote {lane}: ClassLaneCertifiedTirV1 for {} ({n} bytes) — submit it once the family is certified and the class is Active",
        entry.class_id()
    );
    Ok(())
}

fn preflight(view: &NetworkView, path: &std::path::Path, wanted: Option<&str>) -> Result<(), String> {
    let sdk = sdk_for(view);
    let artifact = sdk.load_artifact(path)?;
    println!("{}", artifact.summary);
    // RFC-0002 Phase F: an IR artifact is its own class — gated as a `--palw-register-class` run
    // gates it, at the point the network's `palw_tir_v1` fence arms (or refused: not armed).
    let tir = misaka_palw_sdk::tir_registration::tir_entries_of_v1(std::slice::from_ref(&artifact));
    if !tir.is_empty() {
        let mut refused = false;
        // The gate the registration will meet: at the fence's height where `palw_tir_v1` is armed,
        // else (testnet-12, whose IR fence is a flag day not yet scheduled) as if it were.
        let gate = misaka_palw_sdk::tir_layout::TirOfflineGateV1::of(&view.params);
        for entry in tir.iter().filter(|e| wanted.is_none_or(|w| w == e.model_id)) {
            inspect_tir(view, entry);
            // The registration a `--palw-register-class` run would build, judged by admission v10 at
            // the point the fence arms (the genesis terms: the base class's pricing, weightless).
            let at = gate.daa;
            let terms = kaspa_consensus_core::palw_state_v2::PalwRegistrationTermsV2 {
                min_grantable_share_permille: 0,
                slash_value_per_pwu: 1,
                initial_target: u128::MAX,
                registered_class_ids: view.genesis_classes.iter().map(|(id, _)| *id).collect(),
                registered_artifact_roots: view.genesis_classes.iter().map(|(_, root)| *root).collect(),
                chain_certified_families: Vec::new(),
            };
            let bond = kaspa_consensus_core::palw_state_v2::PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(
                kaspa_consensus_core::tx::TransactionId::from_bytes([0; 64]),
                0,
            ));
            match misaka_palw_sdk::tir_registration::build_tir_registration_v1(
                &gate.params,
                &view.bundle,
                entry,
                &terms,
                0,
                bond,
                Vec::new(),
                at,
            ) {
                Ok(_) => println!(
                    "  ADMISSIBLE (admission v10 {}; the live chain's terms decide the rest)  {}  root {}",
                    gate.note(),
                    entry.model_id,
                    entry.artifact_root
                ),
                Err(why) => {
                    refused = true;
                    println!("  REFUSED   {}  — {why}", entry.model_id);
                }
            }
        }
        return if refused {
            Err("the IR class would be refused — nothing should be signed or funded for it".to_string())
        } else {
            Ok(())
        };
    }
    let pairings: Vec<_> = sdk
        .pairings(&artifact)
        .into_iter()
        .filter_map(|(entry, paired)| paired.ok().map(|root| (entry, root)))
        .filter(|(entry, _)| wanted.is_none_or(|w| w == entry.model_id))
        .collect();
    if pairings.is_empty() {
        return Err(match wanted {
            Some(w) => format!("this artifact pairs with no class named {w} — run `palw-class inspect` for the reasons"),
            None => "this artifact pairs with no class this build knows — run `palw-class inspect` for the reasons".to_string(),
        });
    }
    let mut refused = false;
    for (entry, root) in pairings {
        if let Some((_, genesis_root)) = view.genesis_classes.iter().find(|(id, _)| *id == entry.class_id()) {
            if *genesis_root == root {
                println!("  ALREADY REGISTERED  {}  — this exact (class, root) is in {}'s genesis", entry.model_id, view.network_id);
            } else {
                println!(
                    "  REFUSED   {}  — the class is in {}'s genesis under root {genesis_root}, and this artifact roots to {root}: \
                     different weights",
                    entry.model_id, view.network_id
                );
                refused = true;
            }
            continue;
        }
        // The gate's shape at genesis (DAA 0): the fences a shipped preset arms are `always()`,
        // so the genesis point answers for the chain's whole life. A live chain judges at the
        // virtual DAA, which is the panel's reading, not this offline one.
        let shape = match kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
            &view.params,
            &view.bundle,
            &entry.profile,
            0,
        ) {
            Ok(shape) => shape,
            Err(why) => {
                refused = true;
                println!("  REFUSED   {}  — {why}", entry.model_id);
                continue;
            }
        };
        match sdk.preflight_admission(&view.bundle, &entry, root, &shape) {
            Ok(catalog) => println!(
                "  ADMISSIBLE  {}  root {root}  pwu/inference {}  (gate verdict on {}; a live chain may hold more classes than genesis)",
                entry.model_id, catalog.canonical_step_leaf_count, view.network_id
            ),
            Err(why) => {
                refused = true;
                println!("  REFUSED   {}  — {why}", entry.model_id);
            }
        }
    }
    if refused {
        Err("at least one pairing would be refused — nothing should be signed or funded for it".to_string())
    } else {
        Ok(())
    }
}

// =================================================================================================
// ADR-0100 — `measure` and `verify`: the Measured Model Artifact from a HELD artifact
// =================================================================================================

use kaspa_consensus_core::palw_measured_model_v1::{
    PALW_MEASURED_MODEL_MLDSA87_CONTEXT, PalwHeldArtifactV1, PalwMeasureInputsV1, PalwMeasuredCheckV1, PalwMeasuredModelV1,
    PalwModelManifestV1, palw_measure_model_v1, palw_measured_model_id_v1, palw_verify_measured_model_v1,
};
use kaspa_consensus_core::palw_shard_plan_v1::{PalwArtifactBytesV1, PalwInventoryRowMetaV1, palw_artifact_bytes_from_inventory_v1};

const MEASURE_CONTEXTS: [u32; 4] = [512, 32_768, 131_072, 1_048_576];
const MEASURE_SEAT_GIBS: [u64; 4] = [24, 64, 128, 512];
/// The point of judgement the fences are read at: every scheduled fence armed, `never()` dormant.
const MEASURE_EVER: u64 = u64::MAX - 1;

/// What a held artifact yields before any ruleset is consulted: its manifest (the geometry read
/// off the file), its inventory-measured bytes, and the root the court-capable row registers.
struct HeldMeasurementV1 {
    manifest: PalwModelManifestV1,
    bytes: PalwArtifactBytesV1,
    inventory_root: Hash64,
    lineage: &'static str,
    layers: u16,
}

fn held_measurement(loaded: &misaka_palw_sdk::PalwLoadedArtifactV1, name: Option<String>) -> Result<HeldMeasurementV1, String> {
    if let Some(artifact) = misaka_palw_sdk::lineages::dense::artifact_of(loaded) {
        use kaspa_consensus_core::palw_qwen25_profile::{PalwQwen25GeometryV1, QWEN25_1_5B, QWEN25_A16_GRAPH_V5_N_CTX};
        let sh = &artifact.shape;
        let g = PalwQwen25GeometryV1 {
            layer_count: sh.n_layers as u16,
            hidden_dim: sh.d_model() as u32,
            ffn_dim: sh.d_ff as u32,
            attn_heads: sh.n_heads as u16,
            attn_kv_heads: sh.n_kv_heads as u16,
            attn_head_dim: sh.d_head as u32,
            vocab_size: sh.vocab as u32,
            rms_eps_q: sh.eps_q,
            ..QWEN25_1_5B
        };
        let manifest = PalwModelManifestV1::from_dense(name.unwrap_or_else(|| loaded.summary.clone()), &g);
        let profile =
            manifest.profile(QWEN25_A16_GRAPH_V5_N_CTX).map_err(|e| format!("the dense manifest builds no profile: {e:?}"))?;
        let inventory = misaka_palw_base0::inventory::a16_inventory_v1(&artifact, &profile)
            .map_err(|e| format!("the artifact yields no inventory under its own profile: {e:?}"))?;
        let rows: Vec<PalwInventoryRowMetaV1> = inventory.operands().iter().map(PalwInventoryRowMetaV1::from).collect();
        let bytes = palw_artifact_bytes_from_inventory_v1(&profile, &rows).map_err(|e| e.to_string())?;
        return Ok(HeldMeasurementV1 {
            manifest,
            bytes,
            inventory_root: inventory.root(),
            lineage: loaded.lineage_id,
            layers: g.layer_count,
        });
    }
    if let Some((_root, artifact)) = misaka_palw_sdk::lineages::qwen36::parts_of(loaded) {
        use kaspa_consensus_core::palw_qwen36_profile::{PalwQwen36GeometryV1, QWEN36_35B_A3B};
        use misaka_palw_base0::qwen36::Qwen36LayerKind;
        let sh = &artifact.shape;
        let interval = sh
            .layer_types
            .iter()
            .position(|k| matches!(k, Qwen36LayerKind::FullAttention))
            .map(|i| i as u16 + 1)
            .ok_or_else(|| "a hybrid artifact with no full-attention layer has no graph in this tree".to_string())?;
        let g = PalwQwen36GeometryV1 {
            layer_count: sh.n_layers() as u16,
            full_attention_interval: interval,
            hidden_dim: sh.d_model as u32,
            attn_heads: sh.n_heads as u16,
            attn_kv_heads: sh.n_kv_heads as u16,
            attn_head_dim: sh.head_dim as u32,
            rope_dims: sh.rotary_dim as u16,
            gdn_k_heads: sh.linear_k_heads as u16,
            gdn_v_heads: sh.linear_v_heads as u16,
            gdn_head_dim: sh.linear_head_dim as u32,
            gdn_conv_kernel: sh.conv_kernel as u16,
            n_experts: sh.n_experts as u32,
            experts_per_token: sh.experts_per_token as u32,
            moe_dim: sh.moe_dim as u32,
            shared_dim: sh.shared_dim as u32,
            attn_output_gate: u8::from(sh.attn_output_gate()),
            vocab_size: sh.vocab as u32,
            rms_eps_q: sh.eps_q,
            ..QWEN36_35B_A3B
        };
        // **ADR-0102: a held hybrid artifact is measured under graph-v6**, the graph that reads its
        // embedding lift the way the engine executes it (per token; a one-row store lifting every
        // token alike). Under graph-v5 the converter's calibrated store — one triple per
        // vocabulary row — has no inventory at all, and this measurement refused the Qwen3.5-2B
        // artifact by name on exactly that.
        let manifest = PalwModelManifestV1::from_hybrid_token_lift(name.unwrap_or_else(|| loaded.summary.clone()), &g, None);
        let profile = manifest.profile(MEASURE_CONTEXTS[0]).map_err(|e| format!("the hybrid manifest builds no profile: {e:?}"))?;
        // **ADR-0106: streamed, not materialized.** Each row is checked, hashed into the root and
        // dropped where it is read; what is kept is one entry per tensor (its rows' bytes summed —
        // placement reads a row's tensor, layer and length only). So measuring a 33 GiB artifact
        // holds one read block, not the artifact: the same root and bytes as the materializing
        // builder, pinned on every fixture graph and on this Mac's 2B artifact (document id equal).
        let (summary, tensors) = misaka_palw_base0::inventory::qwen36_inventory_measure_v1(&artifact, &profile)
            .map_err(|e| format!("the artifact yields no inventory under its own profile: {e:?}"))?;
        let bytes = palw_artifact_bytes_from_inventory_v1(&profile, &tensors).map_err(|e| e.to_string())?;
        return Ok(HeldMeasurementV1 {
            manifest,
            bytes,
            inventory_root: summary.root,
            lineage: loaded.lineage_id,
            layers: g.layer_count,
        });
    }
    Err(format!("the {} lineage has no measurement path in this build", loaded.lineage_id))
}

fn hex_bytes(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn measure_inputs<'a>(
    view: &'a NetworkView,
    name: &'a str,
    fingerprint: &'a str,
    budgets: &'a [u64],
    held: Option<PalwHeldArtifactV1<'a>>,
) -> PalwMeasureInputsV1<'a> {
    PalwMeasureInputsV1 {
        ruleset: name,
        ruleset_fingerprint_hex: fingerprint,
        bundle: &view.bundle,
        contexts: &MEASURE_CONTEXTS,
        seat_budgets: budgets,
        max_shards: 1_024,
        held,
    }
}

fn court_for_view(
    view: &NetworkView,
) -> impl Fn(
    &PalwShapeProfileV3,
) -> (
    Option<kaspa_consensus_core::palw_class_admission_v2::PalwKaryCourtV1>,
    kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) + '_ {
    move |profile: &PalwShapeProfileV3| {
        let shape = kaspa_consensus_core::palw_class_admission_v2::palw_admission_shape_at_v1(
            &view.params,
            &view.bundle,
            profile,
            MEASURE_EVER,
        )
        .expect("a network with a V2 bundle has an admission shape");
        (shape.court, view.params.palw_prompt_ids_form_at(MEASURE_EVER))
    }
}

fn measure(
    view: &NetworkView,
    path: &std::path::Path,
    name: Option<String>,
    replay_ms: Option<u64>,
    measured_on: &str,
    key_file: Option<&str>,
    out: Option<&str>,
) -> Result<(), String> {
    let sdk = sdk_for(view);
    let loaded = sdk.load_artifact(path)?;
    let held = held_measurement(&loaded, name)?;
    let file_bytes = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?.len();
    let pairings = sdk.pairings(&loaded);
    let ruleset_name = view.network_id.to_string();
    let fingerprint = format!("{}", view.params.consensus_params_id());
    let budgets: Vec<u64> = MEASURE_SEAT_GIBS.iter().map(|g| g << 30).collect();
    let court_for = court_for_view(view);
    let inputs = measure_inputs(
        view,
        &ruleset_name,
        &fingerprint,
        &budgets,
        Some(PalwHeldArtifactV1 { bytes: &held.bytes, root: held.inventory_root, file_bytes }),
    );
    let mut doc = palw_measure_model_v1(&held.manifest, inputs, &court_for, replay_ms, measured_on);
    if let Some(key_file) = key_file {
        let seed = kaspa_pq_validator_core::load_validator_seed(key_file)?;
        let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
        doc.adder_pubkey_hex = hex_bytes(key.public_key());
        doc.signature_hex.clear();
        let id = palw_measured_model_id_v1(&doc);
        doc.signature_hex = hex_bytes(&key.sign_with_context(id.as_byte_slice(), PALW_MEASURED_MODEL_MLDSA87_CONTEXT));
    }
    let id = palw_measured_model_id_v1(&doc);
    let out_path = out.map(std::path::PathBuf::from).unwrap_or_else(|| {
        let mut p = path.to_path_buf();
        p.set_extension("measured.json");
        p
    });
    let text = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&out_path, text).map_err(|e| format!("{}: {e}", out_path.display()))?;

    println!("# Measured Model Artifact — {} on {ruleset_name}\n", held.manifest.name);
    println!("- artifact `{}` ({} bytes on disk), lineage `{}`, {} layers", path.display(), file_bytes, held.lineage, held.layers);
    println!(
        "- bytes measured from the inventory: **{}** ({}); the family formula says {}",
        held.bytes.total(),
        held.bytes.basis,
        held.manifest.artifact_bytes().total()
    );
    println!("- inventory root `{}`", held.inventory_root);
    for (entry, root) in &pairings {
        match root {
            Ok(r) => println!(
                "- registers as `{}` (class `{}`) with root `{r}`{}",
                entry.model_id,
                entry.profile.shape_profile_id(),
                if *r == held.inventory_root { " — the inventory root" } else { "" }
            ),
            Err(e) => println!("- `{}`: no root ({e})", entry.model_id),
        }
    }
    println!();
    println!("| n_ctx | class id | fit | refused by | cache | fewest shards per seat budget |");
    println!("|---|---|---|---|---|---|");
    for row in &doc.deterministic.rows {
        let plans: Vec<String> = row
            .plans
            .iter()
            .map(|p| {
                format!(
                    "{} GiB → {}",
                    p.seat_budget_bytes >> 30,
                    p.shard_count.map(|c| c.to_string()).unwrap_or_else(|| "none".into())
                )
            })
            .collect();
        println!(
            "| {} | `{}` | {} | {} | {:.1} GiB | {} |",
            row.n_ctx,
            row.shape_profile_id_hex.as_deref().map(|h| format!("{}…", &h[..12])).unwrap_or_else(|| "— (no profile)".into()),
            if row.fit_admitted { "admitted" } else { "**refused**" },
            row.refusing_walls.join(", "),
            row.kv_cache_bytes as f64 / (1u64 << 30) as f64,
            plans.join("; ")
        );
    }
    println!();
    match &doc.self_reported.replay_ms_per_position {
        Some(ms) => println!(
            "- self-reported: replay {ms} ms/position on \"{}\" → {} positions within window_receipt; verified by {}",
            doc.self_reported.measured_on,
            doc.self_reported.positions_within_window_receipt.map(|p| p.to_string()).unwrap_or_else(|| "—".into()),
            doc.self_reported.verified_by
        ),
        None => println!("- self-reported: no replay rate given (--replay-ms); the certification drill is what measures one"),
    }
    println!(
        "- id `{id}`; {}",
        if doc.signature_hex.is_empty() {
            "unsigned (give --key-file to sign under the bond key)".to_string()
        } else {
            format!("signed by `{}…`", &doc.adder_pubkey_hex[..16])
        }
    );
    println!("- written: `{}`", out_path.display());
    Ok(())
}

fn verify(view: &NetworkView, path: &std::path::Path, artifact: Option<&std::path::Path>) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let doc: PalwMeasuredModelV1 =
        serde_json::from_str(&text).map_err(|e| format!("{} is not a PalwMeasuredModelV1: {e}", path.display()))?;
    let ruleset_name = view.network_id.to_string();
    let fingerprint = format!("{}", view.params.consensus_params_id());
    let budgets: Vec<u64> =
        doc.deterministic.rows.first().map(|r| r.plans.iter().map(|p| p.seat_budget_bytes).collect()).unwrap_or_default();
    let held_owned = match artifact {
        Some(p) => {
            let sdk = sdk_for(view);
            let loaded = sdk.load_artifact(p)?;
            let held = held_measurement(&loaded, Some(doc.manifest.name.clone()))?;
            let file_bytes = std::fs::metadata(p).map_err(|e| format!("{}: {e}", p.display()))?.len();
            Some((held, file_bytes))
        }
        None => None,
    };
    let held = held_owned.as_ref().map(|(h, file_bytes)| PalwHeldArtifactV1 {
        bytes: &h.bytes,
        root: h.inventory_root,
        file_bytes: *file_bytes,
    });
    let court_for = court_for_view(view);
    let verdict = palw_verify_measured_model_v1(&doc, measure_inputs(view, &ruleset_name, &fingerprint, &budgets, held), &court_for);
    println!("# Measured Model Artifact — verification on {ruleset_name}\n");
    println!("- model **{}**, id `{}`\n", doc.manifest.name, palw_measured_model_id_v1(&doc));
    println!("| field | verdict |");
    println!("|---|---|");
    for check in &verdict.checks {
        match check {
            PalwMeasuredCheckV1::Recomputed { field } => println!("| {field} | recomputed, equal |"),
            PalwMeasuredCheckV1::Mismatch { field, expected, got } => {
                println!("| {field} | **MISMATCH** — recomputed `{expected}`, the document says `{got}` |")
            }
            PalwMeasuredCheckV1::SelfReported { field, verified_by } => {
                println!("| {field} | self-reported; verified by {verified_by} |")
            }
            PalwMeasuredCheckV1::NeedsTheArtifact { field } => println!(
                "| {field} | needs the artifact: measured from its inventory, recomputable only by a holder (give --artifact) |"
            ),
        }
    }
    let signature = if doc.signature_hex.is_empty() {
        "unsigned".to_string()
    } else {
        let pk = decode_hex(&doc.adder_pubkey_hex)?;
        let sig = decode_hex(&doc.signature_hex)?;
        let id = palw_measured_model_id_v1(&doc);
        match kaspa_txscript::verify_mldsa87_with_context(&pk, id.as_byte_slice(), &sig, PALW_MEASURED_MODEL_MLDSA87_CONTEXT) {
            Ok(true) => format!("verifies under `{}…`", &doc.adder_pubkey_hex[..16]),
            Ok(false) | Err(_) => "**DOES NOT VERIFY**".to_string(),
        }
    };
    println!();
    println!("- signature: {signature}");
    let needs = verdict.needs_the_artifact();
    if !needs.is_empty() {
        println!("- {} field(s) need the artifact and were not compared: {}", needs.len(), needs.join(", "));
    }
    println!("- deterministic half: **{}**", if verdict.deterministic_ok() { "agrees" } else { "REFUSED" });
    if !verdict.deterministic_ok() || signature.contains("DOES NOT VERIFY") {
        std::process::exit(1);
    }
    Ok(())
}

fn decode_hex(h: &str) -> Result<Vec<u8>, String> {
    if !h.len().is_multiple_of(2) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("not hex".into());
    }
    (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).map_err(|e| e.to_string())).collect()
}

//! **RFC-0009 stage D, client side: `getPalwStateProof` and what is done with it.**
//!
//! A node's answer about a bond, a class or a claim is `UNVERIFIED_REMOTE_STATE` however many nodes give it. A *proof* is different: the
//! header of a block the client PINNED commits a state root, and the node ships the preimage and every row of the collection, so the client
//! recomputes the header's hash, takes the root from it and checks the rows (`misaka_palw_remote::proof`). What the client then trusts is the
//! pin — a block hash it got from a signed checkpoint or its own node — and nothing the serving node said.
//!
//! Not done here and recorded as `CODE_GAP`: proving that the pinned block is on the heaviest chain (header-chain PoW verification), and a
//! cheap proof (the state commitment is flat, so a proof is O(rows)).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::header::Header;
use kaspa_consensus_core::palw_state_proof_v1::PalwFactProofV1;
use kaspa_consensus_core::palw_state_v2::PalwBondKeyV2;
use kaspa_rpc_core::GetPalwStateProofRequest;
use kaspa_rpc_core::api::rpc::RpcApi;
use misaka_palw_remote::bundle::{SignedRegistrationV1, UNVERIFIED_REMOTE_STATE};
use misaka_palw_remote::proof::{RegistrationAtPinV1, StateProofV1, proof_from_parts_v1, registration_at_pin_v1};

use crate::CliResult;
use crate::exit;
use crate::operator::finding::{Finding, Severity, paint};
use crate::operator::snapshot::{self, NodeRead};
use crate::operator::tty::{Flow, Halt, Step};
use std::time::Duration;

/// Ask `node` for the proof of `collection` against the header of `block`. A node that predates op 202 drops the WebSocket on it, which is
/// an error here like any other: no proof is no proof.
pub(crate) async fn fetch_state_proof(node: &NodeRead, block: Hash64, collection: &str) -> Result<(Header, PalwFactProofV1), String> {
    let r = node
        .client()
        .get_palw_state_proof(GetPalwStateProofRequest { block_hash: block.to_string(), collection: collection.to_string() })
        .await
        .map_err(|e| format!("getPalwStateProof: {e} (a node built before op 202 cannot prove)"))?;
    if !r.available {
        return Err(format!("{} cannot prove: {}", node.url, r.reason));
    }
    if r.block_hash != block.to_string() || r.collection != collection {
        return Err(format!("{} answered for another block or collection than the one asked", node.url));
    }
    let rpc_header = r.header.ok_or_else(|| format!("{} returned no header", node.url))?;
    let header = Header::try_from(&rpc_header).map_err(|e| format!("the header does not convert: {e}"))?;
    let rows = r.rows.into_iter().map(|row| (row.key, row.value)).collect();
    Ok((header, proof_from_parts_v1(r.state_preimage, collection, rows)))
}

/// The proof in the form a bundle carries.
pub(crate) fn embeddable(block: Hash64, header: &Header, proof: &PalwFactProofV1) -> StateProofV1 {
    StateProofV1::new(block, header, proof)
}

pub(crate) fn parse_pin(text: &str) -> Result<Hash64, Halt> {
    text.trim().parse::<Hash64>().map_err(|_| {
        Halt::Blocked(
            Finding::error("E-PIN", exit::GENERIC, "--pin is not a 128-hex block hash")
                .current(text.to_string())
                .reason("a pin is the block hash you trust before you talk to any node (a signed checkpoint, or your own node's)"),
        )
    })
}

/// **Print where a registration stands against the pinned block**, from the class table's proof. `Ok(true)`: proven registered.
pub(crate) fn print_registration_standing(
    flow: &Flow,
    header: &Header,
    pin: Hash64,
    classes: &PalwFactProofV1,
    class_id: &Hash64,
    artifact_root: Hash64,
    owner: &PalwBondKeyV2,
) -> Result<bool, String> {
    match registration_at_pin_v1(header, pin, classes, class_id, artifact_root, owner).map_err(|e| e.to_string())? {
        RegistrationAtPinV1::Registered(p) => {
            flow.ui.mark(
                Severity::Ok,
                "proven",
                &format!("class {}… is registered under this root and this bond — {}", &class_id.to_string()[..16], p.label()),
            );
            Ok(true)
        }
        RegistrationAtPinV1::NotYetRegistered(p) => {
            flow.ui.mark(
                Severity::Info,
                "not in pin",
                &format!("class {}… is not in the pinned state — {}", &class_id.to_string()[..16], p.label()),
            );
            flow.ui.sub(&paint::dim("a registration mined after the pinned block is not covered: pin a newer block to prove it"));
            Ok(false)
        }
        RegistrationAtPinV1::Misattributed { why, provenance } => {
            flow.ui.mark(Severity::Error, "MISATTRIBUTED", &format!("{why} — {}", provenance.label()));
            Ok(false)
        }
    }
}

fn bond_of(text: &str) -> Result<PalwBondKeyV2, Halt> {
    crate::bond::parse_outpoint(text)
        .map(PalwBondKeyV2)
        .map_err(|e| Halt::Blocked(Finding::error("E-EXPECT", exit::GENERIC, "the owner bond is not <txid>:<index>").current(e.msg)))
}

/// `misaka model verify`: is this registration in the state the PINNED block commits?
pub(crate) struct VerifyArgs {
    pub(crate) signed: Option<std::path::PathBuf>,
    pub(crate) class: Option<String>,
    pub(crate) root: Option<String>,
    pub(crate) owner: Option<String>,
    pub(crate) pin: String,
}

pub(crate) async fn verify(ctx: &crate::node::Ctx, args: VerifyArgs) -> CliResult {
    let mut flow = Flow::new(ctx.output, true);
    let doc = serde_json::Map::new();
    let result = verify_flow(ctx, &args, &mut flow).await;
    flow.finish(result, "misaka.model.verify.v1", "verified against your pin", "misaka model verify", doc)
}

async fn verify_flow(ctx: &crate::node::Ctx, args: &VerifyArgs, flow: &mut Flow) -> Step {
    let pin = parse_pin(&args.pin)?;
    let (class, root, owner) = match (&args.signed, &args.class, &args.root, &args.owner) {
        (Some(path), _, _, _) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| Halt::Blocked(Finding::error("E-READ", exit::HOST, format!("{}: {e}", path.display()))))?;
            let s = SignedRegistrationV1::from_json(&text).map_err(|e| {
                Halt::Blocked(Finding::error("E-BUNDLE-REFUSED", exit::GENERIC, "not a signed registration").current(e.to_string()))
            })?;
            (s.class_id, s.artifact_root, bond_of(&s.owner_bond)?)
        }
        (None, Some(c), Some(r), Some(o)) => (
            c.parse::<Hash64>()
                .map_err(|_| Halt::Blocked(Finding::error("E-EXPECT", exit::GENERIC, "--class is not a 128-hex hash")))?,
            r.parse::<Hash64>()
                .map_err(|_| Halt::Blocked(Finding::error("E-EXPECT", exit::GENERIC, "--root is not a 128-hex hash")))?,
            bond_of(o)?,
        ),
        _ => {
            return Err(Halt::Blocked(Finding::error(
                "E-EXPECT",
                exit::GENERIC,
                "name the registration: a signed file, or --class and --root and --owner",
            )));
        }
    };
    let node = snapshot::connect_to(&ctx.network, ctx.rpc.as_deref(), Duration::from_secs(ctx.timeout_secs.clamp(2, 15)))
        .await
        .map_err(|(u, e)| {
            Halt::Blocked(
                Finding::error("E-NODE-RPC-UNREACHABLE", exit::COMPONENT_DOWN, "The node's RPC does not answer")
                    .current(format!("{u}: {e}")),
            )
        })?;
    flow.ui.say(&paint::bold("MISAKA model verify"));
    flow.ui.sub(&format!(
        "trust root: the block you pinned, {}…; nothing the node says is believed until it opens against it",
        &pin.to_string()[..16]
    ));
    let (header, classes) = fetch_state_proof(&node, pin, "classes").await.map_err(|e| {
        Halt::Blocked(
            Finding::error("E-PROOF-UNAVAILABLE", exit::NOT_READY, "The node could not prove the class table at your pin")
                .current(e)
                .reason(format!("without a proof the registry's row stays {UNVERIFIED_REMOTE_STATE}")),
        )
    })?;
    let ok = print_registration_standing(flow, &header, pin, &classes, &class, root, &owner).map_err(|e| {
        Halt::Blocked(Finding::error("E-PROOF-REFUSED", exit::NOT_READY, "The proof does not hold against your pin").current(e))
    })?;
    if ok { Ok(()) } else { Err(Halt::Declined("the registration is not proven present at your pin".into())) }
}

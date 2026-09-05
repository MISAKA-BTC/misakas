//! **`misaka palw da-accuse`** — the accuser's half of ADR-0062's data-availability court
//! (private-prompts design, 2026-09-05).
//!
//! An accusation is an OPERATOR's act, never a node's reflex: a seat that did not receive a
//! claim's material cannot tell transport loss from withholding, and ADR-0065 D4 (armed on every
//! shipped V2 network) already turns "I never got it" into a slash-free abstention. What the court
//! adds is a way to make a specific executor open a specific trace event on chain, at the price
//! of the accuser's own charge if the executor answers — so the command builds the object, signs
//! it under the accuser's bond key, and writes it to a file. Filing it is `palw submit-object`,
//! the same carriage every other lifecycle object rides.
//!
//! The chain's rules are the chain's (`palw_state_v2`'s `DefaultAccused` arm): the accuser must be
//! an Active bond above the floor and either a seat of the claim's panel or a bonded challenger,
//! never the executor; the event index must be inside `trace_chunk_count × 256` rows. This
//! command re-derives none of that — it refuses only what it can see locally, and says so.

use kaspa_rpc_core::api::rpc::RpcApi;

use kaspa_consensus_core::palw_state_v2::{
    PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT, PalwBondKeyV2, PalwConsensusObjectV2, palw_da_accusation_message_v2, palw_da_event_index_v1,
};

use crate::bond::{network_domain, parse_outpoint};
use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, exit};

pub(crate) struct DaAccuseArgs<'a> {
    /// 128-hex claim id.
    pub claim: &'a str,
    /// The logits row (decode position) and the tile inside it — `(row << 8) | tile` on chain.
    pub row: u32,
    pub tile: u8,
    /// The accuser's own bond outpoint, `txid:index`.
    pub bond: &'a str,
    /// Where the signed `DefaultAccused` is written.
    pub out: &'a std::path::Path,
}

pub(crate) async fn accuse(ctx: &Ctx, ks: &KeySource, args: DaAccuseArgs<'_>) -> Result<(), CliError> {
    let claim = args
        .claim
        .parse::<kaspa_consensus_core::Hash64>()
        .map_err(|e| CliError::new(exit::GENERIC, format!("--claim '{}' is not a 128-hex claim id: {e}", args.claim)))?;
    let accuser = PalwBondKeyV2(parse_outpoint(args.bond)?);
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;

    // **The key must be the bond's registered key**, or the chain refuses the signature and the
    // carrier's fee is gone — the same reason `court-close` refuses to default `--side`.
    let facts = nv
        .client
        .get_palw_producer_facts(String::new(), accuser.0.transaction_id.to_string(), accuser.0.index, true)
        .await
        .map_err(|e| CliError::connection(format!("cannot read the bond's facts from the node: {e}")))?;
    if !facts.bond_known {
        return Err(CliError::new(exit::GENERIC, format!("the chain knows no bond at {}", args.bond)));
    }
    let registered = facts.bond_registered_pubkey.to_ascii_lowercase();
    let ours: String = key.public_key().iter().map(|b| format!("{b:02x}")).collect();
    if registered != ours {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the bond at {} registered a different key than the one loaded — an accusation signed with this key is refused on chain",
                args.bond
            ),
        ));
    }

    let missing_event_index = palw_da_event_index_v1(args.row, args.tile);
    let message = palw_da_accusation_message_v2(network_domain(&nv), &claim, missing_event_index, &accuser);
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_DA_ACCUSATION_V2_MLDSA87_CONTEXT).to_vec();
    let object = PalwConsensusObjectV2::DefaultAccused { claim, missing_event_index, accuser, signature };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
        .map_err(|why| CliError::new(exit::GENERIC, format!("the accusation cannot ride a carrier: {why}")))?;
    let bytes = borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, format!("cannot serialize the accusation: {e}")))?;
    std::fs::write(args.out, &bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", args.out.display())))?;
    println!(
        "accusation written: claim {claim}, event {missing_event_index} (row {}, tile {}), accuser {}",
        args.row, args.tile, args.bond
    );
    println!("file: `misaka palw submit-object --object {} …` under the same key.", args.out.display());
    println!(
        "note: if the executor discloses the event inside the window, the accuser's charge is the price of the question \
         (ADR-0062); a disclosure is public on chain, ids included."
    );
    Ok(())
}

//! **`misaka palw shard-accuse`** — sign a one-move accusation (ADR-0099 Decision 5, built by
//! ADR-0100) under the accuser's bond key.
//!
//! A seat that found a leaf of a claim's own capture that does not recompute files the accusation
//! itself (`kaspad`'s panel service, when `Params::palw_shard_court` is in force). This command is
//! the operator's path for an accusation built elsewhere — a seat that recorded the fault while
//! the fence was dormant, or a tool that produced the refutation and the openings — and it does
//! one thing: reads an UNSIGNED `PalwShardCourtAccusationV1` (borsh), sets the accuser's bond,
//! signs the session id under the accusation's own context, wraps it as
//! `PalwConsensusObjectV2::ShardCourtAccused`, and writes the object for `palw submit-object`.
//!
//! The chain's rules are the chain's (`palw_state_v2`'s `ShardCourtAccused` arm, the processor's
//! acceptance arm): the accuser must be an Active bond above the floor and not the claim's own;
//! the leaf must be under the ladder; the refutation must name the leaf and bind the claim's
//! execution root; the object must fit the close ceiling. This command re-derives only the shape
//! (which needs no chain) and refuses what it can see locally, and says so.

use kaspa_rpc_core::api::rpc::RpcApi;

use kaspa_consensus_core::palw_shard_court_v1::{
    PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT, PalwShardCourtAccusationV1, palw_shard_court_accusation_bytes_v1,
    palw_shard_court_session_id_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};

use crate::bond::{network_domain, parse_outpoint};
use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, exit};

pub(crate) struct ShardAccuseArgs<'a> {
    /// The unsigned accusation, borsh.
    pub accusation: &'a std::path::Path,
    /// The accuser's own bond outpoint, `txid:index`.
    pub bond: &'a str,
    /// The ladder to check the leaf against locally (the ruleset's; the chain applies its own).
    pub ladder: u64,
    /// Where the signed `ShardCourtAccused` is written.
    pub out: &'a std::path::Path,
}

pub(crate) async fn accuse(ctx: &Ctx, ks: &KeySource, args: ShardAccuseArgs<'_>) -> Result<(), CliError> {
    let bytes =
        std::fs::read(args.accusation).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", args.accusation.display())))?;
    let mut accusation: PalwShardCourtAccusationV1 = borsh::from_slice(&bytes).map_err(|e| {
        CliError::new(exit::GENERIC, format!("{} is not a borsh PalwShardCourtAccusationV1: {e}", args.accusation.display()))
    })?;
    let accuser = PalwBondKeyV2(parse_outpoint(args.bond)?);
    accusation.accuser_bond = accuser;
    accusation.signature.clear();
    accusation
        .validate_shape(args.ladder)
        .map_err(|e| CliError::new(exit::GENERIC, format!("the accusation's shape is refused: {e}")))?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;

    // **The key must be the bond's registered key**, or the chain refuses the signature and the
    // carrier's fee is gone — the DA accusation's precheck, for its reason.
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

    let session_id = palw_shard_court_session_id_v1(network_domain(&nv).as_byte_slice(), &accusation);
    accusation.signature = key.sign_with_context(session_id.as_byte_slice(), PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT).to_vec();
    let payload_bytes = palw_shard_court_accusation_bytes_v1(&accusation);
    let (claim, leaf) = (accusation.claim, accusation.leaf_index);
    let object = PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) };
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
        .map_err(|why| CliError::new(exit::GENERIC, format!("the accusation cannot ride a carrier: {why}")))?;
    let out = borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, format!("cannot serialize the accusation: {e}")))?;
    std::fs::write(args.out, &out).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", args.out.display())))?;
    println!(
        "accusation signed: claim {claim}, leaf {leaf}, accuser {}, session {session_id}, {payload_bytes} refutation bytes",
        args.bond
    );
    println!("file: `misaka palw submit-object --object {} …` under the same key.", args.out.display());
    println!(
        "note: the chain derives the verdict itself — a leaf that recomputes charges the accuser the claim's reservation up to the \
         registry floor (ADR-0100); a fused-attention leaf is refused, not tried."
    );
    Ok(())
}

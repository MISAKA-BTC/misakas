//! **`misaka palw shard-plan` / `misaka palw bond-shards`** — sign ADR-0100 Decision 4's two
//! declarations under the right bond key, and write each object for `palw submit-object`.
//!
//! * `shard-plan`: a class's registrant declares the class's shard plan, once. The key must be the
//!   registrant bond's registered key; the chain refuses a genesis class (it has no registrant),
//!   a second plan, and a count outside the range it accepts.
//! * `bond-shards`: a bond declares which shards of a class it holds — a refinement of the class
//!   it declared in `capable_classes` (declare the class there first). An empty list withdraws.
//!
//! The chain's rules are the chain's (`palw_state_v2`'s arms, the processor's acceptance arms).
//! This command refuses only what it can see locally — the key against the bond's registered key,
//! the list's shape — and says so.

use kaspa_rpc_core::api::rpc::RpcApi;

use kaspa_consensus_core::palw_shard_licensing_v1::{
    PALW_BOND_SHARDS_MLDSA87_CONTEXT, PALW_SHARD_PLAN_MLDSA87_CONTEXT, palw_bond_shards_message_v1, palw_bond_shards_shape_v1,
    palw_class_shard_plan_message_v1, palw_shard_count_in_range_v1,
};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};

use crate::bond::{network_domain, parse_outpoint};
use crate::keys::KeySource;
use crate::node::Ctx;
use crate::wallet::connect;
use crate::{CliError, exit};

fn parse_class(raw: &str) -> Result<kaspa_consensus_core::Hash64, CliError> {
    raw.parse::<kaspa_consensus_core::Hash64>()
        .map_err(|e| CliError::new(exit::GENERIC, format!("--class '{raw}' is not a 128-hex class id: {e}")))
}

fn parse_shards(raw: &str) -> Result<Vec<u32>, CliError> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    raw.split(',')
        .map(|part| part.trim().parse::<u32>().map_err(|e| CliError::new(exit::GENERIC, format!("--shards '{raw}': {e}"))))
        .collect()
}

/// The loaded key must be the bond's registered key, or the chain refuses the signature and the
/// carrier's fee is gone — the DA accusation's precheck, for its reason.
async fn key_is_the_bonds(
    nv: &crate::wallet::NodeView,
    key: &kaspa_pq_validator_core::ValidatorKey,
    bond: &PalwBondKeyV2,
    raw: &str,
) -> Result<(), CliError> {
    let facts = nv
        .client
        .get_palw_producer_facts(String::new(), bond.0.transaction_id.to_string(), bond.0.index, true)
        .await
        .map_err(|e| CliError::connection(format!("cannot read the bond's facts from the node: {e}")))?;
    if !facts.bond_known {
        return Err(CliError::new(exit::GENERIC, format!("the chain knows no bond at {raw}")));
    }
    let ours: String = key.public_key().iter().map(|b| format!("{b:02x}")).collect();
    if facts.bond_registered_pubkey.to_ascii_lowercase() != ours {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the bond at {raw} registered a different key than the one loaded — a declaration signed with this key is refused on chain"
            ),
        ));
    }
    Ok(())
}

fn write_object(object: PalwConsensusObjectV2, out: &std::path::Path, what: &str) -> Result<(), CliError> {
    kaspa_consensus_core::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object)
        .map_err(|why| CliError::new(exit::GENERIC, format!("the {what} cannot ride a carrier: {why}")))?;
    let bytes = borsh::to_vec(&object).map_err(|e| CliError::new(exit::GENERIC, format!("cannot serialize the {what}: {e}")))?;
    std::fs::write(out, &bytes).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", out.display())))?;
    println!("file: `misaka palw submit-object --object {} …` under the same key.", out.display());
    Ok(())
}

pub(crate) async fn shard_plan(
    ctx: &Ctx,
    ks: &KeySource,
    class: &str,
    count: u32,
    bond: &str,
    out: &std::path::Path,
) -> Result<(), CliError> {
    let class_id = parse_class(class)?;
    if !palw_shard_count_in_range_v1(count) {
        return Err(CliError::new(exit::GENERIC, format!("--count {count} is outside the plan range the chain accepts (2..=1024)")));
    }
    let registrant = PalwBondKeyV2(parse_outpoint(bond)?);
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    key_is_the_bonds(&nv, &key, &registrant, bond).await?;
    let message = palw_class_shard_plan_message_v1(network_domain(&nv), &class_id, count);
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_SHARD_PLAN_MLDSA87_CONTEXT).to_vec();
    println!("shard plan signed: class {class_id}, {count} shards, registrant {bond}");
    println!("note: a plan is declared once and cannot be changed; claims bound after it draw a panel per shard.");
    write_object(PalwConsensusObjectV2::ClassShardPlanDeclared { class_id, shard_count: count, signature }, out, "shard plan")
}

pub(crate) async fn bond_shards(
    ctx: &Ctx,
    ks: &KeySource,
    bond: &str,
    class: &str,
    count: u32,
    shards: &str,
    out: &std::path::Path,
) -> Result<(), CliError> {
    let class_id = parse_class(class)?;
    let shards = parse_shards(shards)?;
    palw_bond_shards_shape_v1(count, &shards)
        .map_err(|why| CliError::new(exit::GENERIC, format!("the shard list is refused: {why}")))?;
    let bond_key = PalwBondKeyV2(parse_outpoint(bond)?);
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    key_is_the_bonds(&nv, &key, &bond_key, bond).await?;
    let message = palw_bond_shards_message_v1(network_domain(&nv), &bond_key, &class_id, count, &shards);
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_BOND_SHARDS_MLDSA87_CONTEXT).to_vec();
    if shards.is_empty() {
        println!("shard list signed: bond {bond} WITHDRAWS its shards of class {class_id}");
    } else {
        println!("shard list signed: bond {bond} holds shards {shards:?} of class {class_id}'s {count}-shard plan");
    }
    println!("note: the bond must also declare the class itself in capable_classes, or the chain refuses the list.");
    write_object(
        PalwConsensusObjectV2::BondShardsDeclared { bond: bond_key, class_id, shard_count: count, shards, signature },
        out,
        "shard list",
    )
}

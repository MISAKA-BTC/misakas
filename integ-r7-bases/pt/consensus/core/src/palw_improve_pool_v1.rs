//! **RFC-0004 §8.5 / spec 17 §17.11.1: the improvement sink** — how a sponsor's deposit
//! (`ImprovementPoolFunded`, tag 82) enters a governed line's pool: the carrier pays exactly `amount`
//! to `OP_RETURN OP_DATA8 "MSKIMP01" OP_DATA64 <line id>` at the output the object names, bound as a
//! model sink is bound to its `ModelBuy` (PALW-MK-11) and an activation sink to its top-up.
//!
//! Two doors, as the activation sink has (`palw_activation_pool_v1`):
//! * **isolation** (height-free, on a ruleset that declares `palw_improvement_v1`): the sink is a legal
//!   output class, and every one must be bound — [`palw_improvement_sink_binding_refusal_v1`];
//! * **the header context**: below the fence's height the sink is refused, so a build that schedules
//!   the fence and one that does not agree on every transaction before it.
//!
//! A deposit the fold refuses (the line not governed) is paid back to the carrier's first
//! P2PKH-ML-DSA-87 output by P-B1's refund route (`palw_model_carrier_refund_v1`), which is why the
//! binding refuses a carrier without one.

use crate::Hash64;
use crate::tx::ScriptPublicKey;

/// The improvement sink's tag.
pub const PALW_IMPROVEMENT_SINK_TAG_V1: &[u8; 8] = b"MSKIMP01";
const OP_RETURN: u8 = 0x6a;
const OP_DATA8: u8 = 0x08;
const OP_DATA64: u8 = 0x40;

/// The sink a deposit pays into (75 bytes, script version 0). Unspendable by construction; its value
/// leaves circulation when the carrier is accepted and is credited to the line's pool by the fold.
pub fn palw_improvement_sink_spk_v1(line_id: &Hash64) -> ScriptPublicKey {
    let mut script = Vec::with_capacity(75);
    script.push(OP_RETURN);
    script.push(OP_DATA8);
    script.extend_from_slice(PALW_IMPROVEMENT_SINK_TAG_V1);
    script.push(OP_DATA64);
    script.extend_from_slice(line_id.as_byte_slice());
    ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&script))
}

/// The line an improvement sink names, if the script is one — by its EXACT form, never its shape.
pub fn palw_improvement_sink_line_v1(spk: &ScriptPublicKey) -> Option<Hash64> {
    if spk.version() != 0 {
        return None;
    }
    let script = spk.script();
    if script.len() != 75
        || script[0] != OP_RETURN
        || script[1] != OP_DATA8
        || &script[2..10] != PALW_IMPROVEMENT_SINK_TAG_V1
        || script[10] != OP_DATA64
    {
        return None;
    }
    let mut id = [0u8; 64];
    id.copy_from_slice(&script[11..75]);
    Some(Hash64::from_bytes(id))
}

/// **Why an improvement sink output of `tx` is not bound**, as `(output index, reason)`, or `None`
/// when every one is. A sink is bound iff `tx` is a lifecycle carrier, pays a P2PKH-ML-DSA-87 output
/// (a refusal's refund payee), and decodes to an `ImprovementPoolFunded` naming that output's index,
/// its value and the line its script names. One carrier carries one object, so a second sink is
/// unbound by construction.
pub fn palw_improvement_sink_binding_refusal_v1(tx: &crate::tx::Transaction) -> Option<(usize, &'static str)> {
    use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    let mut sinks = tx
        .outputs
        .iter()
        .enumerate()
        .filter_map(|(i, output)| palw_improvement_sink_line_v1(&output.script_public_key).map(|l| (i, l)));
    let first = sinks.next()?;
    if tx.subnetwork_id != crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
        return Some((first.0, "an improvement sink rides only a lifecycle carrier"));
    }
    if !tx.outputs.iter().any(|output| crate::mldsa87_primitives::p2pkh_mldsa87_payload(&output.script_public_key).is_some()) {
        return Some((first.0, "an improvement sink's carrier pays no P2PKH-ML-DSA-87 output a refusal could be paid back to"));
    }
    let bound = match borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) {
        Ok(payload) if payload.version == PALW_LIFECYCLE_TX_VERSION_V2 => match payload.object {
            crate::palw_state_v2::PalwConsensusObjectV2::ImprovementPoolFunded { payload } => {
                Some((payload.sink_index as usize, payload.line_id, payload.amount))
            }
            _ => None,
        },
        _ => None,
    };
    for (index, line) in std::iter::once(first).chain(sinks) {
        match bound {
            None => return Some((index, "an improvement sink's carrier carries no ImprovementPoolFunded")),
            Some((named, _, _)) if named != index => {
                return Some((index, "an improvement sink is not the output its carrier's ImprovementPoolFunded names"));
            }
            Some((_, named_line, _)) if named_line != line => {
                return Some((index, "an improvement sink names another line than its carrier's ImprovementPoolFunded"));
            }
            Some((_, _, amount)) if amount != tx.outputs[index].value => {
                return Some((index, "an improvement sink holds another amount than its carrier's ImprovementPoolFunded declares"));
            }
            Some(_) => {}
        }
    }
    None
}

/// **The other direction**: a deposit's carrier pays `amount` to the line's improvement sink at the
/// output the object names, or the object does not ride (the extraction walk skips it).
pub fn palw_improvement_pool_binds_its_carrier_v1(
    tx: &crate::tx::Transaction,
    object: &crate::palw_state_v2::PalwConsensusObjectV2,
) -> Result<(), &'static str> {
    let crate::palw_state_v2::PalwConsensusObjectV2::ImprovementPoolFunded { payload } = object else {
        return Ok(());
    };
    let Some(output) = tx.outputs.get(payload.sink_index as usize) else {
        return Err("a pool deposit names a sink output its carrier does not have");
    };
    if output.value != payload.amount || payload.amount == 0 {
        return Err("a pool deposit declares an amount its sink output does not hold");
    }
    if palw_improvement_sink_line_v1(&output.script_public_key) != Some(payload.line_id) {
        return Err("a pool deposit's sink output must be the line's own improvement sink");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sink_names_its_line_and_nothing_else_does() {
        let line = Hash64::from_bytes([7; 64]);
        let spk = palw_improvement_sink_spk_v1(&line);
        assert_eq!(palw_improvement_sink_line_v1(&spk), Some(line));
        assert_eq!(spk.script().len(), 75);
        let activation = crate::palw_activation_pool_v1::palw_activation_sink_spk_v1(&line);
        assert_eq!(palw_improvement_sink_line_v1(&activation), None, "another sink's tag");
        assert_eq!(crate::palw_activation_pool_v1::palw_activation_sink_class_v1(&spk), None, "and the other way");
        let mut forged = spk.script().to_vec();
        forged.push(0);
        assert_eq!(palw_improvement_sink_line_v1(&ScriptPublicKey::new(0, crate::tx::ScriptVec::from_slice(&forged))), None);
        assert_eq!(palw_improvement_sink_line_v1(&ScriptPublicKey::new(1, crate::tx::ScriptVec::from_slice(spk.script()))), None);
    }

    /// An improvement sink is bound, or refused by name; the carrier binding holds the other way.
    #[test]
    fn an_improvement_sink_is_bound_or_refused() {
        use crate::palw_improve_state_v1::PalwImprovementPoolFundingV1;
        use crate::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
        use crate::palw_state_v2::PalwConsensusObjectV2;
        use crate::subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_LIFECYCLE};
        use crate::tx::{Transaction, TransactionOutput};
        let line = Hash64::from_u64_word(0x11E);
        let payee = crate::mldsa87_primitives::p2pkh_mldsa87_spk(&[0x11; 64]);
        let carrier = |object: Option<PalwConsensusObjectV2>, outputs: Vec<TransactionOutput>, subnet| {
            let payload = object
                .map(|object| borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap())
                .unwrap_or_default();
            Transaction::new(0, vec![], outputs, 0, subnet, 0, payload)
        };
        let funded = |amount, sink_index, line_id| {
            Some(PalwConsensusObjectV2::ImprovementPoolFunded {
                payload: Box::new(PalwImprovementPoolFundingV1 { line_id, amount, sink_index }),
            })
        };
        let outs =
            |value| vec![TransactionOutput::new(5, payee.clone()), TransactionOutput::new(value, palw_improvement_sink_spk_v1(&line))];
        let bound = carrier(funded(700, 1, line), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE);
        assert_eq!(palw_improvement_sink_binding_refusal_v1(&bound), None, "bound");
        let object = funded(700, 1, line).unwrap();
        assert_eq!(palw_improvement_pool_binds_its_carrier_v1(&bound, &object), Ok(()));
        assert!(palw_improvement_pool_binds_its_carrier_v1(&bound, &funded(701, 1, line).unwrap()).is_err(), "another amount");
        assert!(palw_improvement_pool_binds_its_carrier_v1(&bound, &funded(700, 0, line).unwrap()).is_err(), "not the sink");
        assert!(palw_improvement_pool_binds_its_carrier_v1(&bound, &funded(700, 5, line).unwrap()).is_err(), "no such output");
        for (tx, why) in [
            (carrier(None, outs(700), SUBNETWORK_ID_NATIVE), "rides only a lifecycle carrier"),
            (carrier(None, outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "carries no ImprovementPoolFunded"),
            (carrier(funded(700, 0, line), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "is not the output"),
            (carrier(funded(701, 1, line), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "another amount"),
            (carrier(funded(700, 1, Hash64::from_u64_word(9)), outs(700), SUBNETWORK_ID_PALW_LIFECYCLE), "another line"),
        ] {
            let refusal = palw_improvement_sink_binding_refusal_v1(&tx);
            assert!(refusal.is_some_and(|(index, reason)| index == 1 && reason.contains(why)), "{why}: {refusal:?}");
        }
        let no_change = carrier(
            funded(700, 0, line),
            vec![TransactionOutput::new(700, palw_improvement_sink_spk_v1(&line))],
            SUBNETWORK_ID_PALW_LIFECYCLE,
        );
        assert!(palw_improvement_sink_binding_refusal_v1(&no_change).is_some_and(|(_, why)| why.contains("P2PKH-ML-DSA-87")));
        // A refused deposit is paid back to the carrier's first P2PKH-ML-DSA-87 output (P-B1).
        let refund = crate::palw_lifecycle_objects_v2::palw_model_carrier_refund_v1(&bound, &object).expect("a refund route");
        assert_eq!((refund.line_id, refund.amount), (line, 700));
    }
}

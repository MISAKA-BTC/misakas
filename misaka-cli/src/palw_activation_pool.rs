//! **`misaka palw model-sponsor` / `model-pool` — a class's Activation Pool** (ADR-0152-adjacent:
//! Activation Pool, user decision 2026-09-25).
//!
//! `pool` reads one class's pool at the tip (`getPalwActivationPool`, op 200): its preparation and
//! bonus budgets, who was paid, the operators credited toward the activation bonus, the terms, and
//! the sink a top-up pays into. `sponsor` files an `ActivationPoolFunded` in a carrier whose output 1
//! pays the class's activation sink (`OP_RETURN "MSKACT01" <class>`) — the shape of `model-seed`. A
//! preview is printed before anything is sent and nothing is sent without `--yes`. Anyone may
//! sponsor any listing, its registrant included; a top-up is a donation and nothing refunds it once
//! the chain has folded it (a carrier the fold refuses is paid back, P-B1).

use crate::node::Ctx;
use crate::palw_model::{PALW_CARRIER_MAX_INPUTS, build_move_carrier_multi, msk, parse_msk_amount, refusal_line, submit_move};
use crate::wallet::connect;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_activation_pool_v1::{palw_activation_inflow_split_v1, palw_activation_sink_spk_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::tx::TransactionOutput;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwActivationPoolRequest, GetPalwActivationPoolResponse};

fn parse_class(raw: &str) -> Result<kaspa_consensus_core::Hash64, CliError> {
    kaspa_consensus_core::palw_panel_view_v1::palw_parse_class_alias_v1(raw).map_err(|why| CliError::new(exit::GENERIC, why))
}

/// Op 200, on a connection of its own: a node built before it drops the WebSocket on the op.
async fn read(nv: &crate::wallet::NodeView, class: &kaspa_consensus_core::Hash64) -> Result<GetPalwActivationPoolResponse, CliError> {
    nv.client.get_palw_activation_pool(GetPalwActivationPoolRequest { class_id: class.to_string() }).await.map_err(|e| {
        CliError::new(
            exit::CONNECTION,
            format!("getPalwActivationPool (op 200): {e} — a node built before the Activation Pool drops the connection on it"),
        )
    })
}

/// **Why the chain would refuse a top-up of `amount` now**, as the fold asks it
/// (`palw_activation_pool_admits_v1`): the pool not armed, the class unknown or Frozen, the amount
/// under the least top-up. `None` when it would fold.
pub(crate) fn sponsor_refusal(r: &GetPalwActivationPoolResponse, amount: u64) -> Option<String> {
    if !r.available {
        return Some("the node answers from no V2 state (not a ConsensusV2 network, or no state yet)".to_string());
    }
    if !r.pool_armed {
        return Some("this chain does not arm the Activation Pool (Params::palw_activation_pool)".to_string());
    }
    if !r.class_found {
        return Some(format!("this chain holds no class {}", r.class_id));
    }
    if r.class_is_floor {
        return Some(format!("class {} is the floor: no rule pays an Activation Pool of it", r.class_id));
    }
    if r.class_status == "frozen" {
        return Some(format!("class {} is Frozen: a proven-bad class takes no sponsor", r.class_id));
    }
    if amount < r.min_topup_sompi {
        return Some(format!("a top-up of {} is under the least top-up of {}", msk(amount), msk(r.min_topup_sompi)));
    }
    None
}

/// `misaka palw model-pool <class>` — one class's pool at the tip.
pub async fn pool(ctx: &Ctx, class_text: &str) -> CliResult {
    let class = parse_class(class_text)?;
    let nv = connect(ctx).await?;
    let r = read(&nv, &class).await;
    let _ = nv.client.disconnect().await;
    let r = r?;
    if ctx.output == OutputFormat::Json {
        println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
        return Ok(());
    }
    if !r.available {
        println!("the node answers from no V2 state");
        return Ok(());
    }
    println!("class {}", r.class_id);
    if !r.class_found {
        println!("  not on this chain");
        return Ok(());
    }
    println!(
        "  status         {}{}",
        r.class_status,
        if r.lifecycle.is_empty() { String::new() } else { format!(" — registry {}", r.lifecycle) }
    );
    if !r.pool_armed {
        println!("  pool           not armed on this chain");
        return Ok(());
    }
    if !r.has_pool {
        println!("  pool           none yet — the first top-up opens it (`misaka palw model-sponsor {} <MSK>`)", r.class_id);
    } else {
        println!("  preparation    {} — (a): paid at this Candidate's audit to each prepared juror, once", msk(r.prep_sompi));
        println!(
            "  activation     {} — (b): paid at Probation → ActiveLimited to the operators credited on its probes",
            msk(r.bonus_sompi)
        );
        println!(
            "  funded         {} (paid {}, owed and queued next {}, withheld {})",
            msk(r.funded_sompi),
            msk(r.paid_sompi),
            msk(r.scheduled_sompi),
            msk(r.withheld_sompi)
        );
        println!("  opened         DAA {}", r.opened_daa);
        if r.prep_reward_now_sompi > 0 {
            println!("  (a) now        {} per prepared juror (cap {})", msk(r.prep_reward_now_sompi), msk(r.prep_cap_now_sompi));
        }
        if r.next_audit_span > 0 {
            println!("  next audit     span {} (DAA {})", r.next_audit_span, r.next_audit_span.saturating_mul(r.span_daa.max(1)));
        }
        println!("  paid (a)       {} operator(s)", r.prep_paid.len());
        println!("  paid (b)       {} operator(s)", r.bonus_paid.len());
        if !r.probe_credited.is_empty() {
            println!("  credited       {} operator(s) toward (b)", r.probe_credited.len());
        }
    }
    if !r.registrant_operator.is_empty() {
        println!("  registrant     operator {} (never paid from its own pool)", r.registrant_operator);
    }
    println!(
        "  terms          A0 {}, α {}‰, β {}‰, ramp {} DAA, caps {}/{}, least top-up {}",
        msk(r.prep_base_sompi),
        r.prep_share_permille,
        r.bonus_share_permille,
        r.ramp_daa,
        r.prep_payee_cap,
        r.bonus_payee_cap,
        msk(r.min_topup_sompi)
    );
    println!("  sink           {}", r.sink_script);
    Ok(())
}

/// `misaka palw model-sponsor <class> <MSK> --key … [--yes]` — top a class's Activation Pool up.
pub async fn sponsor(ctx: &Ctx, ks: &crate::keys::KeySource, class_text: &str, msk_text: &str, yes: bool) -> CliResult {
    let class = parse_class(class_text)?;
    let amount = parse_msk_amount(msk_text)?;
    let key = ks.load_key()?;
    let nv = connect(ctx).await?;
    let r = read(&nv, &class).await?;
    if let Some(why) = sponsor_refusal(&r, amount) {
        let _ = nv.client.disconnect().await;
        return Err(CliError::new(exit::GENERIC, why));
    }
    let object = PalwConsensusObjectV2::ActivationPoolFunded { class_id: class, amount, sink_index: 1 };
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(&nv, &addr).await?;
    let mut sorted: Vec<_> = candidates.into_iter().collect();
    sorted.sort_by(|a, b| b.1.amount.cmp(&a.1.amount));
    let want = amount.saturating_add(kaspa_pq_validator_core::ATTESTATION_TX_FEE_FLOOR_SOMPI);
    let mut funding: Vec<_> = Vec::new();
    let mut have: u64 = 0;
    for (outpoint, entry) in sorted.into_iter().take(PALW_CARRIER_MAX_INPUTS) {
        have = have.saturating_add(entry.amount);
        funding.push((outpoint, entry));
        if have > want {
            break;
        }
    }
    if funding.is_empty() || have <= want {
        let _ = nv.client.disconnect().await;
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "the {} mature utxo(s) this carrier can spend at {addr} hold {}, which does not cover {} plus a fee (one carrier \
                 spends at most {PALW_CARRIER_MAX_INPUTS} inputs): sponsor in smaller top-ups",
                funding.len(),
                msk(have),
                msk(amount)
            ),
        ));
    }
    let sink = TransactionOutput::new(amount, palw_activation_sink_spk_v1(&class));
    let (tx, fee) = build_move_carrier_multi(&key, &nv, &object, &funding, vec![sink])?;
    if ctx.output != OutputFormat::Json {
        let candidate = r.lifecycle == "Candidate";
        let (prep, bonus) = palw_activation_inflow_split_v1(amount, r.prep_share_permille.min(1_000) as u16, candidate);
        println!("sponsor {} into class {}'s Activation Pool", msk(amount), r.class_id);
        println!(
            "  split          {} to preparation (a), {} to activation (b){}",
            msk(prep),
            msk(bonus),
            if candidate { "" } else { " — past Candidate every sompi is (b)" }
        );
        println!("  a DONATION: no object refunds a top-up once the chain has folded it, and it confers no right");
        println!("{}", refusal_line(&nv, amount, &addr));
    }
    let what = format!("ActivationPoolFunded {} into class {}", msk(amount), class);
    let out = submit_move(ctx, &nv, tx, fee, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CLI refuses locally what the fold would refuse, in the fold's order.
    #[test]
    fn a_sponsor_is_refused_locally_where_the_fold_would_refuse_it() {
        let live = GetPalwActivationPoolResponse {
            available: true,
            pool_armed: true,
            class_found: true,
            class_id: "c".into(),
            class_status: "active".into(),
            min_topup_sompi: 100_000_000,
            ..Default::default()
        };
        assert_eq!(sponsor_refusal(&live, 100_000_000), None);
        assert!(sponsor_refusal(&live, 99_999_999).is_some_and(|why| why.contains("least top-up")));
        assert!(
            sponsor_refusal(&GetPalwActivationPoolResponse { pool_armed: false, ..live.clone() }, 1 << 40)
                .is_some_and(|why| why.contains("does not arm"))
        );
        assert!(
            sponsor_refusal(&GetPalwActivationPoolResponse { class_is_floor: true, ..live.clone() }, 1 << 40)
                .is_some_and(|why| why.contains("floor"))
        );
        assert!(
            sponsor_refusal(&GetPalwActivationPoolResponse { class_found: false, ..live.clone() }, 1 << 40)
                .is_some_and(|why| why.contains("no class"))
        );
        assert!(
            sponsor_refusal(&GetPalwActivationPoolResponse { class_status: "frozen".into(), ..live.clone() }, 1 << 40)
                .is_some_and(|why| why.contains("Frozen"))
        );
        assert!(sponsor_refusal(&GetPalwActivationPoolResponse::default(), 1 << 40).is_some());
    }
}

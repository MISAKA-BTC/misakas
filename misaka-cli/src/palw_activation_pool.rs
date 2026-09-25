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
//!
//! **A registration sponsors its own listing by default** (user decision 2026-09-25, the pool's
//! P4): `misaka model add` and `misaka palw extension submit` file one `ActivationPoolFunded` of
//! [`PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI`] for the class they registered, once the chain has
//! folded the registration ([`sponsor_listing`]); `--sponsor <MSK>` changes the amount and
//! `--no-sponsor` files none. The chain's own seed stays 0 — this is the tool's default, not a rule.
//! Every figure shown as what a listing needs is the NON-BINDING recommended pool (`16 · A_MAX / α`,
//! op 200's `recommendedPoolSompi`), never the registry's derived registration bond, which nothing
//! charges.

use crate::node::Ctx;
use crate::palw_model::{PALW_CARRIER_MAX_INPUTS, build_move_carrier_multi, msk, parse_msk_amount, refusal_line, submit_move};
use crate::wallet::connect;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::palw_activation_pool_v1::{palw_activation_inflow_split_v1, palw_activation_sink_spk_v1};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;
use kaspa_consensus_core::tx::TransactionOutput;
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwActivationPoolRequest, GetPalwActivationPoolResponse};
use std::time::{Duration, Instant};

/// **The sponsor a registration files by default: 500 MSK** (user decision 2026-09-25) — `10·A0/α`
/// at the pool's terms of 2026-09-25 (`A0` = 20 MSK, `α` = 400 ‰). The least top-up whose
/// preparation share (`α` of it: 200 MSK = `10·A0`) keeps `a = min(A_MAX, ⌊prep/10⌋)` at the full
/// `A0` at the pool's opening, so the listing's first jury is paid the full base reward each. A CLI
/// default and nothing more: the chain's seed is 0, and any amount from the least top-up up is a
/// sponsor. **The terms are illustrative and may change with the mainnet-values decision — re-derive
/// this with them** (`the_registration_sponsor_default_is_ten_a0_over_alpha` fails until it is).
pub(crate) const PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI: u64 = 500 * kaspa_consensus_core::constants::SOMPI_PER_KASPA;

/// `--sponsor <MSK>` / `--no-sponsor`, on every command that registers a class.
#[derive(clap::Args, Clone, Debug, Default)]
pub(crate) struct ListingSponsorArgs {
    /// MSK to sponsor into the new class's Activation Pool once the registration folds (default
    /// 500: 10·A0/α at the pool's terms). A donation to the class's preparers, never refunded once
    /// folded; `0` is `--no-sponsor`.
    #[arg(long, value_name = "MSK", conflicts_with = "no_sponsor")]
    pub(crate) sponsor: Option<String>,
    /// File no sponsor with the registration.
    #[arg(long)]
    pub(crate) no_sponsor: bool,
}

impl ListingSponsorArgs {
    /// The sompi to sponsor, or `None` for no sponsor.
    pub(crate) fn resolve(&self) -> Result<Option<u64>, CliError> {
        if self.no_sponsor {
            return Ok(None);
        }
        let amount = match &self.sponsor {
            Some(text) => parse_msk_amount(text)?,
            None => PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI,
        };
        Ok(Some(amount).filter(|amount| *amount > 0))
    }
}

/// `sompi` as the plain MSK number `model-sponsor` takes (`500`, `12.5`).
fn msk_arg(sompi: u64) -> String {
    let text = msk(sompi);
    let number = text.trim_end_matches(" MSK");
    if number.contains('.') { number.trim_end_matches('0').trim_end_matches('.').to_string() } else { number.to_string() }
}

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

/// **What a top-up of this class buys, where it is not a Candidate's** (P4): a Candidate's top-up
/// splits `α` into (a); every other class's is wholly (b), and (b) pays only at a
/// `Probation → ActiveLimited` formation — the next one for a class still forming, a RE-formation
/// (`Held → … → ActiveLimited`, operators not paid before) for one that has formed. `None` for a
/// Candidate and wherever `sponsor_refusal` already refuses.
pub(crate) fn sponsor_warning(r: &GetPalwActivationPoolResponse) -> Option<String> {
    if !r.pool_armed || !r.class_found || r.class_is_floor || r.class_status == "frozen" {
        return None;
    }
    if r.class_status == "dormant" {
        return Some(format!(
            "class {} is Dormant: its pool waits for a re-registration of the class id, and pays nothing before",
            r.class_id
        ));
    }
    let state = r.lifecycle.split([' ', '{', '(']).next().unwrap_or("");
    if state == "Candidate" {
        return None;
    }
    Some(match state {
        "Prefetching" | "Probation" => format!(
            "class {} is {state}, not a Candidate: every sompi goes to the activation bonus (b), paid when it reaches \
             ActiveLimited to the operators its probes credit — none to preparation (a)",
            r.class_id
        ),
        "" => format!(
            "class {} has no registry row: every sompi goes to the activation bonus (b), which pays only at a \
             Probation → ActiveLimited formation",
            r.class_id
        ),
        _ => format!(
            "class {} is {state}, not a Candidate: every sompi goes to the activation bonus (b), which pays only through a \
             RE-formation (Held → … → ActiveLimited), to operators not paid before",
            r.class_id
        ),
    })
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
    if r.recommended_pool_sompi > 0 {
        println!(
            "  recommended    {} — NON-BINDING: pays a listing's preparers in full (16·A_MAX/α); nothing enforces it",
            msk(r.recommended_pool_sompi)
        );
    }
    if let Some(warning) = sponsor_warning(&r) {
        println!("  ! {warning}");
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
        "  terms          A0 {}, α {}‰, β {}‰ (at most {} an operator), ramp {} DAA, caps {}/{}, least top-up {}",
        msk(r.prep_base_sompi),
        r.prep_share_permille,
        r.bonus_share_permille,
        msk(r.bonus_cap_sompi),
        r.ramp_daa,
        r.prep_payee_cap,
        r.bonus_payee_cap,
        msk(r.min_topup_sompi)
    );
    println!("  sink           {}", r.sink_script);
    Ok(())
}

/// **One top-up carrier**: `ActivationPoolFunded` in the payload, the change back to the key's own
/// P2PKH-ML-DSA-87 address at output 0 (the payee P-B1 pays a refusal back to), the class's
/// activation sink at output 1, funded from the key's largest spendable outputs.
async fn top_up_carrier(
    key: &kaspa_pq_validator_core::ValidatorKey,
    nv: &crate::wallet::NodeView,
    class: &kaspa_consensus_core::Hash64,
    amount: u64,
) -> Result<(kaspa_consensus_core::tx::Transaction, u64, String), CliError> {
    let object = PalwConsensusObjectV2::ActivationPoolFunded { class_id: *class, amount, sink_index: 1 };
    let addr = key.funding_address(nv.params.prefix());
    let candidates = crate::palw_fp::spendable_candidates_v1(nv, &addr).await?;
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
    let sink = TransactionOutput::new(amount, palw_activation_sink_spk_v1(class));
    let (tx, fee) = build_move_carrier_multi(key, nv, &object, &funding, vec![sink])?;
    Ok((tx, fee, addr.to_string()))
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
    let (tx, fee, addr) = match top_up_carrier(&key, &nv, &class, amount).await {
        Ok(built) => built,
        Err(e) => {
            let _ = nv.client.disconnect().await;
            return Err(e);
        }
    };
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
        if r.recommended_pool_sompi > 0 {
            println!(
                "  recommended    {} a listing (NON-BINDING: pays its preparers in full; the pool holds {} now)",
                msk(r.recommended_pool_sompi),
                msk(r.prep_sompi.saturating_add(r.bonus_sompi))
            );
        }
        if let Some(warning) = sponsor_warning(&r) {
            println!("  ! {warning}");
        }
        println!("  a DONATION: no object refunds a top-up once the chain has folded it, and it confers no right");
        println!("{}", refusal_line(&nv, amount, &addr));
    }
    let what = format!("ActivationPoolFunded {} into class {}", msk(amount), class);
    let out = submit_move(ctx, &nv, tx, fee, &what, yes).await;
    let _ = nv.client.disconnect().await;
    out
}

/// **The sponsor a registration files, once the chain has folded the registration** (P4). Returns
/// the carrier's id, or why none was filed (the registration stands either way; the caller prints
/// `misaka palw model-sponsor` for a retry).
///
/// **A second carrier, after the fold — not the registration's own carrier, not its block:**
/// * a lifecycle carrier carries ONE object (`PalwLifecycleTxPayloadV2 { version, object }`), so a
///   top-up cannot ride in the registration's transaction;
/// * a node refuses at its mempool and its template what its fold would refuse at the tip
///   (`palw_model_market_carrier_refusal_v1` asks `palw_activation_pool_admits_v1`), and until the
///   registration folds the tip holds no such class: a top-up sent beside its registration is
///   refused `MissingClass`. Were one mined anyway, the fold applies a block's objects in order
///   against the running state — credited behind its registration, refused and paid back through
///   P-B1 ahead of it, never lost (`activation_pool_r1_v1::p4_*`).
///
/// So this polls op 200 until the tip holds the class (at most `patience`), asks the fold's own
/// questions (`sponsor_refusal`), and submits one `ActivationPoolFunded`. No second confirmation:
/// the caller's preview named the amount before its one yes.
pub(crate) async fn sponsor_listing(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    class: kaspa_consensus_core::Hash64,
    amount: u64,
    patience: Duration,
) -> Result<String, String> {
    let key = ks.load_key().map_err(|e| e.msg)?;
    let nv = connect(ctx).await.map_err(|e| e.msg)?;
    let deadline = Instant::now() + patience;
    let outcome = async {
        let r = loop {
            let r = read(&nv, &class).await.map_err(|e| e.msg)?;
            if r.class_found || !r.available || !r.pool_armed {
                break r;
            }
            if Instant::now() >= deadline {
                return Err(format!("the tip does not hold class {class} yet ({}s waited)", patience.as_secs()));
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        };
        if let Some(why) = sponsor_refusal(&r, amount) {
            return Err(why);
        }
        let (tx, _fee, _addr) = top_up_carrier(&key, &nv, &class, amount).await.map_err(|e| e.msg)?;
        let txid = tx.id();
        nv.client.submit_transaction((&tx).into(), false).await.map_err(|e| format!("submit the carrier {txid}: {e}"))?;
        Ok(txid.to_string())
    }
    .await;
    let _ = nv.client.disconnect().await;
    outcome
}

/// The command that files a sponsor by hand, for a registration whose own sponsor was not filed.
pub(crate) fn sponsor_retry_hint(class: &kaspa_consensus_core::Hash64, amount: u64) -> String {
    format!("misaka palw model-sponsor {class} {} --key <seed> --yes", msk_arg(amount))
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

    /// **P4: the default sponsor is `10·A0/α` at the pool's terms** — and `α` of it keeps the first
    /// jury's reward at the full `A0`. Fails when the terms move, until the constant is re-derived.
    #[test]
    fn the_registration_sponsor_default_is_ten_a0_over_alpha() {
        use kaspa_consensus_core::palw_activation_pool_v1::{PALW_ACTIVATION_POOL_TERMS_V1, palw_activation_prep_reward_v1};
        let t = PALW_ACTIVATION_POOL_TERMS_V1;
        let derived = 10 * u128::from(t.prep_base_sompi) * 1_000 / u128::from(t.prep_share_permille);
        assert_eq!(u128::from(PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI), derived, "10·A0/α — re-derive the default with the terms");
        assert_eq!(PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI, 500 * 100_000_000);
        let (prep, bonus) = palw_activation_inflow_split_v1(PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI, t.prep_share_permille, true);
        assert_eq!((prep, bonus), (200 * 100_000_000, 300 * 100_000_000));
        assert_eq!(palw_activation_prep_reward_v1(&t, prep, 0), t.prep_base_sompi, "the first jury is paid the full A0");
        assert!(PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI >= t.min_topup_sompi);
    }

    /// `--sponsor` / `--no-sponsor`: the default, a stated amount, zero and the opt-out.
    #[test]
    fn a_registration_sponsors_500_msk_unless_told_otherwise() {
        let args = |sponsor: Option<&str>, no_sponsor: bool| ListingSponsorArgs { sponsor: sponsor.map(str::to_string), no_sponsor };
        assert_eq!(args(None, false).resolve().unwrap(), Some(PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI));
        assert_eq!(args(Some("12.5"), false).resolve().unwrap(), Some(1_250_000_000));
        assert_eq!(args(Some("0"), false).resolve().unwrap(), None);
        assert_eq!(args(None, true).resolve().unwrap(), None);
        assert!(args(Some("five"), false).resolve().is_err());
        assert_eq!(msk_arg(PALW_REGISTRATION_SPONSOR_DEFAULT_SOMPI), "500");
        assert_eq!(msk_arg(1_250_000_000), "12.5");
        assert!(sponsor_retry_hint(&kaspa_consensus_core::Hash64::from_u64_word(7), 50_000_000_000).contains(" 500 --key"));
    }

    /// P4: a top-up of a class that is not a Candidate is warned about — and what (b) waits for is
    /// named by the state it is in.
    #[test]
    fn a_top_up_of_a_class_past_candidate_is_warned_it_pays_only_at_a_formation() {
        let live = GetPalwActivationPoolResponse {
            available: true,
            pool_armed: true,
            class_found: true,
            class_id: "c".into(),
            class_status: "active".into(),
            lifecycle: "Candidate".into(),
            min_topup_sompi: 100_000_000,
            ..Default::default()
        };
        assert_eq!(sponsor_warning(&live), None, "a Candidate's top-up is split into (a) and (b)");
        let at = |lifecycle: &str| sponsor_warning(&GetPalwActivationPoolResponse { lifecycle: lifecycle.into(), ..live.clone() });
        for formed in ["Active", "ActiveLimited", "Held", "Registered"] {
            assert!(at(formed).is_some_and(|w| w.contains("RE-formation") && w.contains(formed)), "{formed}: {:?}", at(formed));
        }
        assert!(at("Probation { probes_passed: 3 }").is_some_and(|w| w.contains("Probation") && w.contains("reaches ActiveLimited")));
        assert!(at("Prefetching").is_some_and(|w| w.contains("reaches ActiveLimited")));
        assert!(at("").is_some_and(|w| w.contains("no registry row")));
        assert!(
            sponsor_warning(&GetPalwActivationPoolResponse { class_status: "dormant".into(), ..live.clone() })
                .is_some_and(|w| w.contains("re-registration"))
        );
        assert_eq!(sponsor_warning(&GetPalwActivationPoolResponse { class_is_floor: true, lifecycle: "Active".into(), ..live }), None);
    }
}

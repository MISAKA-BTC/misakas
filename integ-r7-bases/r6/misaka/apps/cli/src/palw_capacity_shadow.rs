//! **`misaka palw capacity-shadow` — ADR-0160's shadow accounting, read from a node** (lane shadow).
//!
//! What the capacity formulas would reserve, weigh and allow on this chain, per ramp step, next to
//! today's values: the fork weight today vs under J-1's per-bond cap, what a fresh 13,000 BILI bond
//! holds, the seats' capacity per DAA, the licence queue, and the attribution counters Stage 0's gate
//! reads. Read-only (`getPalwCapacityShadow`, op 201: a node built before it drops the connection,
//! so the read goes on a connection of its own). Nothing here is a rule: no capacity fence is armed.
//!
//! **Which steps, priced how.** With none named the node prices the schedule its F-L fence arms,
//! else the UNCREDITED ramp (ρ 10 … 1000 at q 0, `m_c = E`). Every step is priced as the fold would
//! (lane escrow's `m_c`: nothing is credited below `q_seat` = 250‰, and past it `m*` is priced on the
//! conviction floor — 4 floor claims per 13k at 250‰, 10 at 500‰, at every ρ). `--reference` names
//! ADR-0160 v1's reference ramp (q 143‰): it prices as the uncredited one, and v1's figures for it
//! (E-T3's 20 / 50 / 101 / 203 / 2,030 at 13k) are printed only in the column marked superseded.
//!
//! **The adversary rows.** `--adversary TXID:INDEX[:STRATEGY]` names an O-3 bond and its strategy
//! (naive, garbage, borrowed); `q` is printed per (class, strategy) and counts a claim caught only
//! when its producer is convicted by a route the credit prices — a lower bound, never a credit's input
//! on its own.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwCapacityShadowRequest, GetPalwCapacityShadowResponse, RpcPalwCapacityStep};
use kaspa_wrpc_client::error::rpc_error_is_connection_loss;

const SOMPI_PER_MSK: u128 = 100_000_000;

/// `misaka palw capacity-shadow`'s arguments (their own `Args`, so the command tree's parse stays small).
#[derive(clap::Args, Clone, Debug, Default)]
pub(crate) struct CapacityShadowArgs {
    /// A step `rho[:q_permille]` (repeatable; q defaults to 0, no attribution credited). None: the
    /// node's default — the armed F-L schedule, else ρ 10 / 25 / 50 / 100 / 1000 at q 0.
    #[arg(long = "step", value_name = "RHO[:Q]")]
    pub(crate) step: Vec<String>,
    /// Price ADR-0160 v1's reference ramp, ρ 10 / 25 / 50 / 100 / 1000 at q 143‰ — below q_seat, so it
    /// prices as the uncredited ramp; v1's own figures appear in the superseded column.
    #[arg(long, conflicts_with = "step")]
    pub(crate) reference: bool,
    /// Only this bond's row (and claims), `<txid>:<index>`.
    #[arg(long, value_name = "TXID:INDEX")]
    pub(crate) bond: Option<String>,
    /// A bond whose claims are adversarial (an O-3 run; repeatable), with its strategy (naive,
    /// garbage or borrowed): measures q per (class, strategy).
    #[arg(long, value_name = "TXID:INDEX[:STRATEGY]")]
    pub(crate) adversary: Vec<String>,
    /// Also list the claim rows.
    #[arg(long)]
    pub(crate) claims: bool,
    /// At most this many bond and claim rows (0: the node's cap, 500).
    #[arg(long, default_value_t = 0)]
    pub(crate) limit: u32,
    /// JSON output (`--output json` does the same).
    #[arg(long)]
    pub(crate) json: bool,
}
const FCW: u128 = 604_250_611;

fn read_error(e: &kaspa_rpc_core::RpcError) -> CliError {
    if rpc_error_is_connection_loss(e) {
        CliError::new(
            exit::COMPONENT_DOWN,
            format!("getPalwCapacityShadow: {e} — the node closed the connection on op 201, so it predates ADR-0160's shadow"),
        )
    } else {
        CliError::new(exit::GENERIC, format!("getPalwCapacityShadow: {e}"))
    }
}

/// `rho[:q]` → a step (`q` in permille, default 0: a credit is named, never assumed).
pub(crate) fn parse_step(text: &str) -> Result<RpcPalwCapacityStep, CliError> {
    let (rho, q) = match text.split_once(':') {
        Some((rho, q)) => (rho, q),
        None => (text, "0"),
    };
    let bad = || CliError::new(exit::GENERIC, format!("step '{text}' is not rho[:q_permille]"));
    Ok(RpcPalwCapacityStep {
        from_daa: 0,
        rho: rho.trim().parse().map_err(|_| bad())?,
        q_credit_permille: q.trim().parse().map_err(|_| bad())?,
    })
}

/// ADR-0160's reference ramp (`PALW_CAPACITY_REFERENCE_STEPS_V1`), on the wire.
pub(crate) fn reference_steps() -> Vec<RpcPalwCapacityStep> {
    kaspa_consensus_core::palw_capacity_formulas_v1::PALW_CAPACITY_REFERENCE_STEPS_V1
        .iter()
        .map(|s| RpcPalwCapacityStep { from_daa: s.from_daa, rho: s.rho, q_credit_permille: u32::from(s.q_credit_permille) })
        .collect()
}

fn msk(text: &str) -> String {
    let sompi: u128 = text.parse().unwrap_or(0);
    format!("{}.{:02}", sompi / SOMPI_PER_MSK, (sompi % SOMPI_PER_MSK) / 1_000_000)
}

fn fcw(text: &str) -> String {
    let w: u128 = text.parse().unwrap_or(0);
    format!("{}.{:02}", w / FCW, (w % FCW) * 100 / FCW)
}

fn per_daa(milli: u64) -> String {
    format!("{}.{:02}", milli / 1_000, (milli % 1_000) / 10)
}

/// The human view of one answer.
pub(crate) fn render(r: &GetPalwCapacityShadowResponse) -> String {
    if !r.available {
        return "capacity shadow: the node answers from no V2 state (not a ConsensusV2 network, or no state yet)\n".to_string();
    }
    let mut out = String::new();
    out.push_str(&format!("{}\n", r.summary));
    out.push_str(&format!(
        "fork weight (FCW): today {} → under J-1 {} (Σ W_cap {}); safe {}\n",
        fcw(&r.bounded_immature_today),
        fcw(&r.bounded_immature_new),
        fcw(&r.w_cap_total),
        fcw(&r.safe_weight)
    ));
    out.push_str(&format!(
        "reference floor claim: E {} BILI, w {} BILI, conviction tier {} BILI (v1's L = 3G {} BILI); {} seats, duty {} / lock {} BILI\n",
        msk(&r.reference_escrow_sompi),
        msk(&r.reference_w_floor_sompi),
        if r.reference_conviction_tier_sompi.is_empty() { "whole bond".to_string() } else { msk(&r.reference_conviction_tier_sompi) },
        msk(&r.reference_l_sompi),
        r.reference_seats,
        msk(&r.reference_duty_sompi),
        msk(&r.reference_lock_sompi)
    ));
    out.push_str(&format!(
        "seats {} (usable {} BILI): duty {} / lock {} BILI today → {}/DAA; licence queue {} ({} per block, {} blocks)\n",
        r.seats,
        msk(&r.seat_usable_capital_sompi),
        msk(&r.seat_duty_today_sompi),
        msk(&r.seat_lock_today_sompi),
        per_daa(r.seat_capacity_today_milli_per_daa),
        r.licence_queue,
        r.carriers_per_block,
        r.carriage_blocks_to_drain
    ));
    out.push_str(
        "  ρ     q‰  m_floor BILI  q_needed‰  13k holds  seats/DAA (if D-5)  claims commit BILI  A8 bar‰  alarm  | superseded v1: m BILI / 13k\n",
    );
    for s in &r.steps {
        out.push_str(&format!(
            "  {:<5} {:<3} {:>12} {:>9} {:>10} {:>9} ({:>7}) {:>18} {:>8}  {:<7} | {} / {}\n",
            s.step.rho,
            s.step.q_credit_permille,
            msk(&s.m_floor_sompi),
            s.q_needed_permille,
            s.n_instant_13k,
            per_daa(s.seat_capacity_milli_per_daa),
            per_daa(s.seat_capacity_if_d5_milli_per_daa),
            msk(&s.claims_commitment_sompi),
            s.q_required_permille,
            if s.q_alarm_unmeasured {
                "UNMEAS"
            } else if s.q_alarm {
                "q-ALARM"
            } else {
                "-"
            },
            msk(&s.m_floor_v1_superseded_sompi),
            s.n_instant_13k_v1_superseded
        ));
    }
    out.push_str(
        "  (every column left of '|' is priced as the fold would: a credit applies only from q_seat = 250‰ — lane escrow's \
         gate, lane liab's AS-2 — and m_c is priced on the conviction tier; q_needed‰ is the credit that makes ⌈E/ρ⌉ bind. \
         The column right of '|' is ADR-0160 v1's pricing (L = 3G, no gate), SUPERSEDED. The parenthesised seats/DAA \
         column is §10 D-5's, an open decision. A8 bar = 2 × max(q needed at L = 3G, q_seat, q‰); UNMEAS: a credited \
         step with a named strategy nobody measured)\n",
    );
    out.push_str(&format!("bonds ({} of {}):\n", r.bonds.len(), r.bonds_total));
    for b in &r.bonds {
        out.push_str(&format!(
            "  {} {} BILI{}: {} live ({} unlicensed), weight {} → {} FCW, holds {} today → {:?}{}\n",
            b.bond,
            msk(&b.collateral_sompi.to_string()),
            if b.seat { " seat" } else { "" },
            b.live_claims,
            b.unlicensed_claims,
            fcw(&b.x_b),
            fcw(&b.capped),
            b.n_instant_today,
            b.n_instant_new,
            if b.frozen_would_be { if b.freeze_final { " FROZEN (final)" } else { " frozen" } } else { "" }
        ));
    }
    if !r.claims.is_empty() {
        out.push_str(&format!("claims ({} of {}):\n", r.claims.len(), r.claims_total));
        for c in &r.claims {
            out.push_str(&format!(
                "  {}… {} {} commit {} → {:?} BILI\n",
                &c.claim_id[..c.claim_id.len().min(12)],
                c.phase,
                c.stage,
                msk(&c.commitment_today_sompi),
                c.commitment_new_sompi.iter().map(|x| msk(x)).collect::<Vec<_>>()
            ));
        }
    }
    for a in &r.attribution {
        out.push_str(&format!(
            "class {}: live {} final {} voided {} (conviction voids {}), convictions {:?}, DA non-seat open {} / ever {}\n",
            if a.class_id.is_empty() { "(none)" } else { &a.class_id[..a.class_id.len().min(12)] },
            a.claims_live,
            a.claims_final,
            a.claims_voided,
            a.voids_attributed,
            a.convictions_by_kind,
            a.da_open_non_seat,
            a.da_opened_non_seat_total,
        ));
    }
    for a in &r.adversary {
        out.push_str(&format!(
            "O-3 class {} {}{}: {} claims — caught {}, late {}, unpriced {} {:?}, undetected {}, censored {}, in flight {}, \
             seat-only {}; q {} (a lower bound: only convictions the credit prices)\n",
            &a.class_id[..a.class_id.len().min(12)],
            a.strategy,
            if a.c7 { " (C7: never credited)" } else { "" },
            a.claims,
            a.caught,
            a.caught_late,
            a.caught_unpriced,
            a.unpriced_by_route,
            a.undetected,
            a.censored,
            a.in_flight,
            a.seat_only,
            a.q_measured_permille.map(|q| format!("{q}‰")).unwrap_or_else(|| "-".to_string())
        ));
    }
    out
}

pub(crate) async fn run(ctx: &Ctx, args: CapacityShadowArgs) -> CliResult {
    let CapacityShadowArgs { step, reference, bond, adversary, claims, limit, json } = args;
    let request = GetPalwCapacityShadowRequest {
        steps: if reference { reference_steps() } else { step.iter().map(|s| parse_step(s)).collect::<Result<Vec<_>, _>>()? },
        bond: bond.unwrap_or_default(),
        adversary_bonds: adversary,
        include_claims: claims,
        limit,
    };
    let reader = crate::palw_derived::connect(ctx).await?;
    let answer = reader.client.get_palw_capacity_shadow(request).await;
    let _ = reader.client.disconnect().await;
    let response = answer.map_err(|e| read_error(&e))?;
    if json || ctx.output == OutputFormat::Json {
        let mut doc = serde_json::to_value(&response).expect("a response serializes");
        doc["schema"] = "misaka.palw.capacity-shadow.v2".into();
        println!("{}", serde_json::to_string_pretty(&doc).expect("serializable"));
    } else {
        print!("{}", render(&response));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_rpc_core::RpcPalwCapacityStepRow;

    #[test]
    fn the_subcommand_parses_its_steps_and_bonds() {
        use clap::Parser;
        let bond = format!("{}:0", "ab".repeat(64));
        let cli = crate::Cli::try_parse_from([
            "misaka",
            "palw",
            "capacity-shadow",
            "--step",
            "100",
            "--step",
            "10:250",
            "--adversary",
            &format!("{bond}:garbage"),
            "--claims",
            "--limit",
            "7",
        ])
        .expect("parses");
        let crate::Command::Palw(crate::PalwCmd::CapacityShadow(args)) = cli.command else { panic!("capacity-shadow") };
        assert_eq!(
            (args.step, args.adversary, args.claims, args.limit, args.bond, args.reference),
            (vec!["100".into(), "10:250".into()], vec![format!("{bond}:garbage")], true, 7, None, false)
        );
        let cli = crate::Cli::try_parse_from(["misaka", "palw", "capacity-shadow", "--reference"]).expect("parses");
        let crate::Command::Palw(crate::PalwCmd::CapacityShadow(args)) = cli.command else { panic!("capacity-shadow") };
        assert!(args.reference && args.step.is_empty());
        assert!(
            crate::Cli::try_parse_from(["misaka", "palw", "capacity-shadow", "--reference", "--step", "10"]).is_err(),
            "the reference ramp and named steps are exclusive"
        );
        assert_eq!(
            reference_steps().iter().map(|s| (s.rho, s.q_credit_permille)).collect::<Vec<_>>(),
            vec![(10, 143), (25, 143), (50, 143), (100, 143), (1000, 143)]
        );
    }

    #[test]
    fn steps_parse_as_rho_and_q() {
        assert_eq!(
            parse_step("100").unwrap(),
            RpcPalwCapacityStep { from_daa: 0, rho: 100, q_credit_permille: 0 },
            "no credit assumed"
        );
        assert_eq!(parse_step("10:250").unwrap(), RpcPalwCapacityStep { from_daa: 0, rho: 10, q_credit_permille: 250 });
        assert!(parse_step("x").is_err());
    }

    #[test]
    fn the_view_names_the_weight_and_the_ramp() {
        let r = GetPalwCapacityShadowResponse {
            available: true,
            summary: "capacity-shadow: daa=9".to_string(),
            bounded_immature_today: (13 * FCW).to_string(),
            bounded_immature_new: (2 * FCW).to_string(),
            steps: vec![RpcPalwCapacityStepRow {
                step: RpcPalwCapacityStep { from_daa: 0, rho: 100, q_credit_permille: 143 },
                m_floor_sompi: "320084650080".to_string(),
                n_instant_13k: 2,
                m_floor_v1_superseded_sompi: "3200846501".to_string(),
                n_instant_13k_v1_superseded: 203,
                q_required_permille: 500,
                q_alarm: true,
                q_alarm_unmeasured: true,
                ..Default::default()
            }],
            adversary: vec![kaspa_rpc_core::RpcPalwCapacityAdversaryRow {
                class_id: "e0".repeat(64),
                strategy: "garbage".to_string(),
                claims: 6,
                caught_unpriced: 3,
                unpriced_by_route: vec!["refuted-untagged=1".to_string()],
                undetected: 3,
                q_measured_permille: Some(0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = render(&r);
        assert!(text.contains("today 13.00 → under J-1 2.00"), "{text}");
        assert!(text.contains("3200.84") && text.contains("| 32.00 / 203"), "the fold's price, and v1's only as superseded: {text}");
        assert!(text.contains("SUPERSEDED") && text.contains("UNMEAS"), "{text}");
        assert!(text.contains("O-3 class e0e0e0e0e0e0 garbage: 6 claims") && text.contains("q 0‰"), "{text}");
        assert!(render(&GetPalwCapacityShadowResponse::default()).contains("no V2 state"));
    }
}

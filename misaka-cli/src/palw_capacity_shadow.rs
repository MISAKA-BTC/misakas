//! **`misaka palw capacity-shadow` — ADR-0160's shadow accounting, read from a node** (lane shadow).
//!
//! What the capacity formulas would reserve, weigh and allow on this chain, per ramp step, next to
//! today's values: the fork weight today vs under J-1's per-bond cap, what a fresh 13,000 MSK bond
//! holds, the seats' capacity per DAA, the licence queue, and the attribution counters Stage 0's gate
//! reads. Read-only (`getPalwCapacityShadow`, op 201: a node built before it drops the connection,
//! so the read goes on a connection of its own). Nothing here is a rule: no capacity fence is armed.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwCapacityShadowRequest, GetPalwCapacityShadowResponse, RpcPalwCapacityStep};
use kaspa_wrpc_client::error::rpc_error_is_connection_loss;

const SOMPI_PER_MSK: u128 = 100_000_000;

/// `misaka palw capacity-shadow`'s arguments (their own `Args`, so the command tree's parse stays small).
#[derive(clap::Args, Clone, Debug, Default)]
pub(crate) struct CapacityShadowArgs {
    /// A step `rho[:q_permille]` (repeatable; none: ρ 10 / 25 / 50 / 100 / 1000 at q 143‰).
    #[arg(long = "step", value_name = "RHO[:Q]")]
    pub(crate) step: Vec<String>,
    /// Only this bond's row (and claims), `<txid>:<index>`.
    #[arg(long, value_name = "TXID:INDEX")]
    pub(crate) bond: Option<String>,
    /// A bond whose claims are adversarial (an O-3 run; repeatable): measures q per class.
    #[arg(long, value_name = "TXID:INDEX")]
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

/// `rho[:q]` → a step (`q` in permille, default 143: the credit at which the ramp term binds).
pub(crate) fn parse_step(text: &str) -> Result<RpcPalwCapacityStep, CliError> {
    let (rho, q) = match text.split_once(':') {
        Some((rho, q)) => (rho, q),
        None => (text, "143"),
    };
    let bad = || CliError::new(exit::GENERIC, format!("step '{text}' is not rho[:q_permille]"));
    Ok(RpcPalwCapacityStep {
        from_daa: 0,
        rho: rho.trim().parse().map_err(|_| bad())?,
        q_credit_permille: q.trim().parse().map_err(|_| bad())?,
    })
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
        "reference floor claim: E {} MSK, w {} MSK, L = 3G {} MSK; {} seats, duty {} / lock {} MSK\n",
        msk(&r.reference_escrow_sompi),
        msk(&r.reference_w_floor_sompi),
        msk(&r.reference_l_sompi),
        r.reference_seats,
        msk(&r.reference_duty_sompi),
        msk(&r.reference_lock_sompi)
    ));
    out.push_str(&format!(
        "seats {} (usable {} MSK): duty {} / lock {} MSK today → {}/DAA; licence queue {} ({} per block, {} blocks)\n",
        r.seats,
        msk(&r.seat_usable_capital_sompi),
        msk(&r.seat_duty_today_sompi),
        msk(&r.seat_lock_today_sompi),
        per_daa(r.seat_capacity_today_milli_per_daa),
        r.licence_queue,
        r.carriers_per_block,
        r.carriage_blocks_to_drain
    ));
    out.push_str("  ρ     q‰  m_floor MSK  q_needed‰  13k holds  seats/DAA  claims commit MSK  alarm\n");
    for s in &r.steps {
        out.push_str(&format!(
            "  {:<5} {:<3} {:>12} {:>9} {:>10} {:>10} {:>18}  {}\n",
            s.step.rho,
            s.step.q_credit_permille,
            msk(&s.m_floor_sompi),
            s.q_needed_permille,
            s.n_instant_13k,
            per_daa(s.seat_capacity_milli_per_daa),
            msk(&s.claims_commitment_sompi),
            if s.q_alarm { "q-ALARM" } else { "-" }
        ));
    }
    out.push_str(&format!("bonds ({} of {}):\n", r.bonds.len(), r.bonds_total));
    for b in &r.bonds {
        out.push_str(&format!(
            "  {} {} MSK{}: {} live ({} unlicensed), weight {} → {} FCW, holds {} today → {:?}{}\n",
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
                "  {}… {} {} commit {} → {:?} MSK\n",
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
            "class {}: live {} final {} voided {} (attributed {}), convictions {:?}, DA non-seat open {} / ever {}, q {}\n",
            if a.class_id.is_empty() { "(none)" } else { &a.class_id[..a.class_id.len().min(12)] },
            a.claims_live,
            a.claims_final,
            a.claims_voided,
            a.voids_attributed,
            a.convictions_by_kind,
            a.da_open_non_seat,
            a.da_opened_non_seat_total,
            a.q_measured_permille.map(|q| format!("{q}‰")).unwrap_or_else(|| "-".to_string())
        ));
    }
    out
}

pub(crate) async fn run(ctx: &Ctx, args: CapacityShadowArgs) -> CliResult {
    let CapacityShadowArgs { step, bond, adversary, claims, limit, json } = args;
    let request = GetPalwCapacityShadowRequest {
        steps: step.iter().map(|s| parse_step(s)).collect::<Result<Vec<_>, _>>()?,
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
        doc["schema"] = "misaka.palw.capacity-shadow.v1".into();
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
            &bond,
            "--claims",
            "--limit",
            "7",
        ])
        .expect("parses");
        let crate::Command::Palw(crate::PalwCmd::CapacityShadow(args)) = cli.command else { panic!("capacity-shadow") };
        assert_eq!(
            (args.step, args.adversary, args.claims, args.limit, args.bond),
            (vec!["100".into(), "10:250".into()], vec![bond], true, 7, None)
        );
    }

    #[test]
    fn steps_parse_as_rho_and_q() {
        assert_eq!(parse_step("100").unwrap(), RpcPalwCapacityStep { from_daa: 0, rho: 100, q_credit_permille: 143 });
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
                m_floor_sompi: "3200846501".to_string(),
                n_instant_13k: 203,
                ..Default::default()
            }],
            ..Default::default()
        };
        let text = render(&r);
        assert!(text.contains("today 13.00 → under J-1 2.00"), "{text}");
        assert!(text.contains("32.00") && text.contains("203"), "{text}");
        assert!(render(&GetPalwCapacityShadowResponse::default()).contains("no V2 state"));
    }
}

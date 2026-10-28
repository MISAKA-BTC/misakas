//! **PALW participation reads for wallet users** (lane UX-B, 2026-10-27): the bond (collateral, locks, held rewards), the claim and
//! the fence schedule, read from the connected node's public views (ops 226–228). Read-only: nothing is signed or broadcast here.
//! Building and filing PALW objects (bond registration, kernel job posting, court opening, dispute reservations) is the `misaka`
//! CLI's job (`misaka bond …`, `misaka palw build …`), which this command points to.
use crate::imports::*;
use kaspa_rpc_core::{GetPalwBondDetailRequest, GetPalwClaimDetailRequest, GetPalwFenceScheduleRequest};

#[derive(Default, Handler)]
#[help("PALW participation: bond (collateral, locks, held rewards), claim and fence reads")]
pub struct Palw;

/// What a `palw` line asks for, parsed before the node is contacted.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PalwRead {
    Bond(String),
    Claim(String),
    Fences,
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn parse_palw_argv(argv: &[String]) -> std::result::Result<PalwRead, String> {
    match argv.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["bond", outpoint] => {
            let (txid, index) = outpoint.split_once(':').ok_or("a bond is <txid>:<index>")?;
            if !is_hex(txid, 128) || index.parse::<u32>().is_err() {
                return Err("a bond is <128-hex txid>:<index>".into());
            }
            Ok(PalwRead::Bond(outpoint.to_string()))
        }
        ["claim", id] => {
            if !is_hex(id, 128) {
                return Err("a claim id is 128 hex digits".into());
            }
            Ok(PalwRead::Claim(id.to_string()))
        }
        ["fences"] => Ok(PalwRead::Fences),
        _ => Err("usage: palw bond <txid:index> | palw claim <128-hex> | palw fences".into()),
    }
}

impl Palw {
    async fn main(self: Arc<Self>, ctx: &Arc<dyn Context>, argv: Vec<String>, _cmd: &str) -> Result<()> {
        let ctx = ctx.clone().downcast_arc::<KaspaCli>()?;
        let read = match parse_palw_argv(&argv) {
            Ok(r) => r,
            Err(usage) => {
                tprintln!(ctx, "{usage}");
                tprintln!(ctx, "balance: `details`; register a bond: `misaka bond register`; post a kernel job, open a court or reserve");
                tprintln!(ctx, "a dispute: `misaka palw build --help` (builders write a file; `misaka palw submit-object` dry-runs it)");
                return Ok(());
            }
        };
        let rpc = ctx.wallet().rpc_api().clone();
        let (available, found, now_daa, json) = match read {
            PalwRead::Bond(bond) => {
                let r = rpc.get_palw_bond_detail(GetPalwBondDetailRequest { bond }).await?;
                (r.available, r.found, r.now_daa, r.view_json)
            }
            PalwRead::Claim(claim_hex) => {
                let r = rpc.get_palw_claim_detail(GetPalwClaimDetailRequest { claim_hex }).await?;
                (r.available, r.found, r.now_daa, r.view_json)
            }
            PalwRead::Fences => {
                let r = rpc.get_palw_fence_schedule(GetPalwFenceScheduleRequest::default()).await?;
                (r.available, r.found, r.now_daa, r.view_json)
            }
        };
        if !available {
            tprintln!(ctx, "the node serves no PALW view (not a ConsensusV2 network, or an older node)");
        } else if !found {
            tprintln!(ctx, "the chain holds no such row at DAA {now_daa}");
        } else {
            let pretty = serde_json::from_str::<serde_json::Value>(&json)
                .ok()
                .and_then(|v| serde_json::to_string_pretty(&v).ok())
                .unwrap_or(json);
            tprintln!(ctx, "at DAA {now_daa}:");
            ctx.term().writeln(pretty.crlf());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn palw_reads_parse_and_refuse_malformed_ids() {
        let bond = format!("{}:0", "ab".repeat(64));
        assert_eq!(parse_palw_argv(&v(&["bond", &bond])), Ok(PalwRead::Bond(bond.clone())));
        assert_eq!(parse_palw_argv(&v(&["claim", &"cd".repeat(64)])), Ok(PalwRead::Claim("cd".repeat(64))));
        assert_eq!(parse_palw_argv(&v(&["fences"])), Ok(PalwRead::Fences));
        assert!(parse_palw_argv(&v(&["bond", "xyz"])).is_err());
        assert!(parse_palw_argv(&v(&["claim", "ab"])).is_err());
        assert!(parse_palw_argv(&v(&[])).is_err());
    }
}

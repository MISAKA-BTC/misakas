//! Offline signed all-target enrollment path, using the same typed SDK builders.
//! Snapshot roots are supplied separately by the operator; node acceptance repeats
//! chain authority, consent, collateral, capacity and source/receipt verification.
use super::*;
use std::collections::BTreeMap;
use std::io::{Read, Write};
const USAGE: &str = "palw-class claim-v2 CLASS|JOB|COMMIT|FUND|LOCK|VERIFY|MESSAGE|ROUND-FUND|ROUND-LOCK|ROUND-VERIFY|SIGN|INSPECT|FINAL|COMPACT-COVERAGE|COMPACT-SIGN
All commands: --snapshot FILE --ledger-root HEX --aux-root HEX --out FILE
CLASS: --claim-policy POLICY_BORSH --source-policy SOURCE_BORSH --beacon-policy CHALLENGE_BORSH --base-class HEX --actor BOND_HEX [--rounds N --max-material-bytes N]
JOB: --class HEX --base-job HEX --actor BOND_HEX --nonce HEX --last-commit DAA --max-consumers N
COMMIT: --class HEX --job HEX --commit SALTED_COMMIT_BORSH --salt HEX --material-consent HEX --height DAA
FUND: --enrollment HEX --actor BOND_HEX --fee-policy POLICY_BORSH --fee SOMPI --max-sources N
LOCK: --enrollment HEX
VERIFY: --context HEX --material MATERIAL_BORSH --actor BOND_HEX --height DAA
MESSAGE: --enrollment HEX --round N --material MATERIAL_BORSH
ROUND-FUND: --enrollment HEX --round N --actor HEX --fee-policy POLICY_BORSH --fee SOMPI --max-sources N
ROUND-LOCK: --enrollment HEX --round N
ROUND-VERIFY: --enrollment HEX --round N --material MATERIAL_BORSH --actor HEX --height DAA
SIGN: --draft KERNEL_OBJECT --network ID [--drill-salt HEX] --bond TXID:INDEX --key-file SEED
INSPECT: --context HEX
FINAL: --enrollment HEX
COMPACT-COVERAGE: --base-class HEX --compact-arithmetic true|false --arithmetic-repetitions N --fs-query-bits N
COMPACT-SIGN: --draft KERNEL_OBJECT --bond TXID:INDEX --key-file SEED (exact authenticated snapshot domain)
COMPACT-COVERAGE is a read-only inventory of directly checkable relations. Compact V4 registration, proof generation, proof
binding and compact verification are retired (PRINCIPLES §8): no compact proof record is generated, bound or credited.
Snapshot is borsh((header, Vec<(table,key,row)>)); roots must be authenticated from your selected chain.
SIGN writes an outer native object for misaka palw submit-object; the other commands write unsigned drafts.
An unapproved mechanics policy, economic estimate or signed seat receipt does not grant public security approval.";
fn read(path: &str, max: usize) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    std::fs::File::open(path).map_err(|e| e.to_string())?.take((max + 1) as u64).read_to_end(&mut b).map_err(|e| e.to_string())?;
    if b.len() > max {
        return Err("input exceeds bound".into());
    }
    Ok(b)
}
fn typed<T: borsh::BorshDeserialize>(path: &str, max: usize) -> Result<T, String> {
    borsh::from_slice(&read(path, max)?).map_err(|e| e.to_string())
}
fn take(f: &mut BTreeMap<String, String>, name: &str) -> Result<String, String> {
    f.remove(name).ok_or_else(|| format!("{name} required\n{USAGE}"))
}
fn hash(f: &mut BTreeMap<String, String>, name: &str) -> Result<Digest, String> {
    Ok(crate::improve_objects::spec::hash(&take(f, name)?)?.as_bytes())
}
fn number<T: std::str::FromStr>(f: &mut BTreeMap<String, String>, name: &str) -> Result<T, String> {
    take(f, name)?.parse().map_err(|_| format!("{name} is not an integer"))
}
pub fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let command = args[0].to_ascii_uppercase();
    let mut f = BTreeMap::new();
    if (args.len() - 1) % 2 != 0 {
        return Err(USAGE.into());
    }
    for pair in args[1..].chunks_exact(2) {
        if !pair[0].starts_with("--") || f.insert(pair[0].clone(), pair[1].clone()).is_some() {
            return Err("unknown positional or duplicate option".into());
        }
    }
    let out = take(&mut f, "--out")?;
    let lr = Hash64::from_bytes(hash(&mut f, "--ledger-root")?);
    let ar = Hash64::from_bytes(hash(&mut f, "--aux-root")?);
    let snapshot = take(&mut f, "--snapshot")?;
    let (header, rows): (Vec<u8>, Vec<(u8, Vec<u8>, Vec<u8>)>) = typed(&snapshot, 256 << 20)?;
    let route = PalwKernelRouteStateV1::from_served_rows_v1(&header, rows.clone(), &lr, &ar).map_err(|e| e.to_string())?;
    let object = match command.as_str() {
        "COMPACT-COVERAGE" => {
            let class = hash(&mut f, "--base-class")?;
            let params = misaka_palw_kernel::claim_compact_v1::CompactCompileParamsV1 {
                compact_exact_arithmetic: number(&mut f, "--compact-arithmetic")?,
                arithmetic_repetitions: number(&mut f, "--arithmetic-repetitions")?,
                fs_query_bits: number(&mut f, "--fs-query-bits")?,
            };
            let coverage = kaspa_consensus_core::palw_claim_compact_v1::palw_native_compact_coverage_v2(&route, class, params)?;
            if !f.is_empty() {
                return Err(format!("unknown options: {:?}", f.keys()));
            }
            return write(&out, &borsh::to_vec(&coverage).map_err(|e| e.to_string())?);
        }
        "CLASS" => {
            let p = typed(&take(&mut f, "--claim-policy")?, 64 << 10)?;
            let s = typed(&take(&mut f, "--source-policy")?, 64 << 10)?;
            let beacon = typed(&take(&mut f, "--beacon-policy")?, 64 << 10)?;
            let actor = hash(&mut f, "--actor")?;
            let base = hash(&mut f, "--base-class")?;
            if let Some(rounds) = f.remove("--rounds") {
                paid_claim_interactive_class_object_v3(
                    &route,
                    &p,
                    actor,
                    base,
                    beacon,
                    &s,
                    rounds.parse().map_err(|_| "invalid round count")?,
                    number(&mut f, "--max-material-bytes")?,
                )
                .map_err(|e| e.to_string())?
            } else {
                paid_claim_class_object_v2(&route, &p, actor, base, beacon, &s).map_err(|e| e.to_string())?
            }
        }
        "JOB" => paid_claim_job_object_v2(
            &route,
            hash(&mut f, "--class")?,
            hash(&mut f, "--base-job")?,
            hash(&mut f, "--actor")?,
            hash(&mut f, "--nonce")?,
            number(&mut f, "--last-commit")?,
            number(&mut f, "--max-consumers")?,
        )
        .map_err(|e| e.to_string())?,
        "COMMIT" => {
            let commit = typed(&take(&mut f, "--commit")?, 128 << 20)?;
            paid_claim_commit_object_derived_v2(
                &route,
                hash(&mut f, "--class")?,
                hash(&mut f, "--job")?,
                commit,
                hash(&mut f, "--salt")?,
                hash(&mut f, "--material-consent")?,
                number(&mut f, "--height")?,
            )
            .map_err(|e| e.to_string())?
        }
        "FUND" => {
            let p = typed(&take(&mut f, "--fee-policy")?, 64 << 10)?;
            paid_claim_funding_object_v2(
                &route,
                hash(&mut f, "--enrollment")?,
                hash(&mut f, "--actor")?,
                number(&mut f, "--fee")?,
                number(&mut f, "--max-sources")?,
                &p,
            )
            .map_err(|e| e.to_string())?
        }
        "LOCK" => KernelRouteObjectV1::PaidClaimV2 { object: PaidClaimObjectV2::Lock { enrollment: hash(&mut f, "--enrollment")? } },
        "VERIFY" => {
            let context = hash(&mut f, "--context")?;
            let material: PaidClaimMaterialWireV2 = typed(&take(&mut f, "--material")?, 256 << 20)?;
            let verified =
                paid_claim_sources_from_kernel_rows_v2(&header, rows.clone(), &lr, &ar, &context).map_err(|e| e.to_string())?;
            let checked = verified.verify_material(&material).map_err(|e| e.to_string())?;
            let complete =
                checked.complete.ok_or_else(|| format!("claim did not pass complete verification: {:?}", checked.finding))?;
            verified.receipt(&complete, hash(&mut f, "--actor")?, number(&mut f, "--height")?).map_err(|e| e.to_string())?
        }
        "MESSAGE" => paid_claim_round_message_object_v3(
            &route,
            hash(&mut f, "--enrollment")?,
            number(&mut f, "--round")?,
            &typed::<PaidClaimMaterialWireV2>(&take(&mut f, "--material")?, 64 << 20)?,
        )
        .map_err(|e| e.to_string())?,
        "ROUND-FUND" => paid_claim_round_funding_object_v3(
            &route,
            hash(&mut f, "--enrollment")?,
            number(&mut f, "--round")?,
            hash(&mut f, "--actor")?,
            number(&mut f, "--fee")?,
            number(&mut f, "--max-sources")?,
            &typed(&take(&mut f, "--fee-policy")?, 64 << 10)?,
        )
        .map_err(|e| e.to_string())?,
        "ROUND-LOCK" => KernelRouteObjectV1::PaidClaimV2 {
            object: PaidClaimObjectV2::LockRound { enrollment: hash(&mut f, "--enrollment")?, round: number(&mut f, "--round")? },
        },
        "ROUND-VERIFY" => {
            let enrollment = hash(&mut f, "--enrollment")?;
            let round = number(&mut f, "--round")?;
            let material: PaidClaimMaterialWireV2 = typed(&take(&mut f, "--material")?, 64 << 20)?;
            let verified = paid_claim_round_sources_from_kernel_rows_v3(&header, rows.clone(), &lr, &ar, enrollment, round)
                .map_err(|e| e.to_string())?;
            let checked = verified.verify_material(&material).map_err(|e| e.to_string())?;
            let complete = checked.complete.ok_or_else(|| format!("full proof opening did not pass: {:?}", checked.finding))?;
            verified.receipt(&complete, hash(&mut f, "--actor")?, number(&mut f, "--height")?).map_err(|e| e.to_string())?
        }
        "COMPACT-SIGN" => {
            let draft = KernelRouteObjectV1::decode(&read(&take(&mut f, "--draft")?, 64 << 20)?).map_err(|e| e.to_string())?;
            let KernelRouteObjectV1::PaidClaimV2 { object: inner } = &draft else {
                return Err("compact signing requires paid claim native object".into());
            };
            if !matches!(
                inner,
                PaidClaimObjectV2::RegisterClass { .. }
                    | PaidClaimObjectV2::EnrollJob { .. }
                    | PaidClaimObjectV2::Commit { .. }
                    | PaidClaimObjectV2::StageMessage { .. }
                    | PaidClaimObjectV2::Fund { .. }
                    | PaidClaimObjectV2::FundRound { .. }
                    | PaidClaimObjectV2::Lock { .. }
                    | PaidClaimObjectV2::LockRound { .. }
                    | PaidClaimObjectV2::RoundReceipt { .. }
            ) {
                return Err("not a compact lifecycle signing operation".into());
            }
            let bond = crate::improve_objects::spec::bond(&take(&mut f, "--bond")?)?;
            let seed = kaspa_pq_validator_core::load_validator_seed(&take(&mut f, "--key-file")?)?;
            let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
            // Selected-chain LR/AR supplied above authenticate this exact domain,
            // including a new private V4 genesis. Do not reconstruct old V3 params.
            let signed = sign_paid_claim_object_v2(&key, Hash64::from_bytes(route.header.policy.network_domain), bond, &draft)?;
            if !f.is_empty() {
                return Err(format!("unknown options: {:?}", f.keys()));
            }
            return write(&out, &borsh::to_vec(&signed).map_err(|e| e.to_string())?);
        }
        "SIGN" => {
            let draft = KernelRouteObjectV1::decode(&read(&take(&mut f, "--draft")?, 256 << 20)?).map_err(|e| e.to_string())?;
            let network: kaspa_consensus_core::network::NetworkId =
                take(&mut f, "--network")?.parse().map_err(|e| format!("network: {e}"))?;
            let salt = f
                .remove("--drill-salt")
                .map(|s| kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(&s))
                .transpose()
                .map_err(|e| e.to_string())?;
            let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network, salt.as_ref())?;
            let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                params.net.to_string().as_bytes(),
                Some(params.genesis.hash),
            );
            if route.header.policy.network_domain != domain.as_bytes() {
                return Err("signed draft chain differs from snapshot".into());
            }
            let bond = crate::improve_objects::spec::bond(&take(&mut f, "--bond")?)?;
            let seed = kaspa_pq_validator_core::load_validator_seed(&take(&mut f, "--key-file")?)?;
            let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
            let signed = sign_paid_claim_object_v2(&key, domain, bond, &draft)?;
            if !f.is_empty() {
                return Err(format!("unknown options: {:?}", f.keys()));
            }
            return write(&out, &borsh::to_vec(&signed).map_err(|e| e.to_string())?);
        }
        "FINAL" => {
            let enrollment = hash(&mut f, "--enrollment")?;
            let final_row =
                paid_claim_final_from_kernel_rows_v2(&header, rows.clone(), &lr, &ar, enrollment).map_err(|e| e.to_string())?;
            if !f.is_empty() {
                return Err(format!("unknown options: {:?}", f.keys()));
            }
            return write(&out, &borsh::to_vec(&final_row).map_err(|e| e.to_string())?);
        }
        "INSPECT" => {
            let context = hash(&mut f, "--context")?;
            let verified = paid_claim_sources_from_kernel_rows_v2(&header, rows, &lr, &ar, &context).map_err(|e| e.to_string())?;
            if !f.is_empty() {
                return Err(format!("unknown options: {:?}", f.keys()));
            }
            return write(&out, &borsh::to_vec(verified.authority()).map_err(|e| e.to_string())?);
        }
        _ => return Err(USAGE.into()),
    };
    if !f.is_empty() {
        return Err(format!("unknown options: {:?}", f.keys()));
    }
    write(&out, &object.encode())
}
fn write(path: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path).map_err(|e| format!("{path}: {e}"))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    println!("wrote {path} ({} bytes); native acceptance and approval remain separate", bytes.len());
    Ok(())
}

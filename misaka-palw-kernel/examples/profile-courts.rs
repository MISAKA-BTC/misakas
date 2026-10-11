//! Offline diagnosis of the segmented decoder's derived court prices, without weights.
//! These are abstract work units, not measured CPU time or proof of public admission.
use std::io::Read;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: profile-courts <program.tir> <positive positions>".into());
    }
    let positions: u32 = args[1].parse()?;
    if positions == 0 {
        return Err("positions must be positive".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&args[0])?.take((misaka_palw_tir::program::MAX_PROGRAM_BYTES + 1) as u64).read_to_end(&mut bytes)?;
    let p = misaka_palw_tir::TirProgramV1::decode_canonical(&bytes)?;
    let d = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor();
    let plan =
        misaka_palw_kernel::plan::plan_for_tir_program_v1(&d, &p, misaka_palw_kernel::public::program_root_v1(&bytes), positions)
            .map_err(|e| format!("plan: {e:?}"))?;
    let occurrences = p.occurrences();
    let node_count = occurrences.iter().map(|(b, _)| p.blocks[*b as usize].nodes.len() as u64).sum();
    let mask = misaka_palw_kernel::seg_scope::seg_withheld_mask_v1(&p);
    let mut rows = Vec::new();
    for (s, (b, _)) in occurrences.iter().enumerate() {
        for r in plan.relations.iter().filter(|r| r.block == *b) {
            if r.court != misaka_palw_kernel::CourtIdV1::ElementRecompute {
                continue;
            }
            let whole = misaka_palw_kernel::seg_scope::whole_value_court_cost_v1(&p, s, r.node as usize, node_count);
            let priced = misaka_palw_kernel::element::element_court_cost_masked_v1(
                &p,
                *b as usize,
                r.node as usize,
                node_count,
                positions,
                None,
                &mask,
            );
            let n = &p.blocks[*b as usize].nodes[r.node as usize];
            rows.push((priced.1, s, *b, r.node, format!("{:?}", n.prim), mask[s][r.node as usize], whole.0, whole.1, priced.0));
        }
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0));
    let hex = |digest: &[u8; 64]| digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
    println!(
        "descriptor_digest\t{}\nprogram_root\t{}\nplan_root\t{}\npositions\t{positions}",
        hex(&d.digest()),
        hex(&plan.program_root),
        hex(&plan.root())
    );
    println!("plan_worst_court_bytes\t{}\nplan_worst_court_work\t{}", plan.budgets.worst_court_bytes, plan.budgets.worst_court_work);
    println!("priced_work\toccurrence\tblock\tnode\tprimitive\twithheld\twhole_bytes\twhole_work\tpriced_bytes");
    for (work, s, b, n, prim, hidden, whole_bytes, whole_work, priced_bytes) in rows.into_iter().take(20) {
        println!("{work}\t{s}\t{b}\t{n}\t{prim}\t{hidden}\t{whole_bytes}\t{whole_work}\t{priced_bytes}");
    }
    Ok(())
}

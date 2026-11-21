//! Read-only comparison of two replay schedules on previously recorded Originals.
use kaspa_consensus_core::Hash64;
use misaka_palw_sdk::qf1_local::residual_tiles::Candidate;
use misaka_palw_tir_exec::node::{TirArtifactV1, qf1::cost_review::ArchivedOriginal};
use serde::Deserialize;
use serde_json::json;
use std::{error::Error, fs, path::Path, time::Instant};

#[derive(Deserialize)]
struct Record {
    attempt: String,
    context: Hash64,
    input_ids: Vec<u32>,
    output_ids: Vec<u32>,
    z: Hash64,
    roots: [Hash64; 4],
    ticket: String,
}

fn bytes32(hex: &str) -> Result<[u8; 32], Box<dyn Error>> {
    if !hex.is_ascii() || hex.len() != 64 {
        return Err("expected a 32-byte hex field".into());
    }
    let mut bytes = [0; 32];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16)?;
    }
    Ok(bytes)
}

impl Record {
    fn original(self) -> Result<ArchivedOriginal, Box<dyn Error>> {
        Ok(ArchivedOriginal {
            attempt: bytes32(&self.attempt)?,
            context: self.context,
            prompt: self.input_ids,
            answer: self.output_ids,
            z: self.z.as_bytes(),
            roots: self.roots.map(|r| r.as_bytes()),
            ticket: bytes32(&self.ticket)?,
        })
    }
}

#[derive(Deserialize)]
struct Plan {
    class_id: Hash64,
    inventory: Hash64,
    descriptor: Hash64,
    decode_limit: u32,
    max_leaves: u64,
    trials: Vec<Record>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [parent, section, plan_path] = args.as_slice() else {
        return Err("usage: qf1-paired-cost <parent> <tile-section> <plan.json>".into());
    };
    let plan: Plan = serde_json::from_slice(&fs::read(plan_path)?)?;
    if plan.trials.len() < 2 || plan.trials.len() % 2 != 0 {
        return Err("expected a nonempty series of pairs".into());
    }
    let trials: Vec<_> = plan.trials.into_iter().map(Record::original).collect::<Result<_, _>>()?;
    let start = Instant::now();
    let parent = TirArtifactV1::open(Path::new(parent))?;
    let candidate = Candidate::over(&parent)?;
    let artifact = candidate.open(&parent, Path::new(section))?;
    let instance = candidate.bind_full(&artifact)?;
    let id = instance.identity();
    if (id.class, id.inventory, id.descriptor) != (plan.class_id, plan.inventory, plan.descriptor) {
        return Err("model differs from archived instance".into());
    }
    println!(
        "{}",
        json!({"discovered_answer_length":true,"setup_ms":start.elapsed().as_millis(),"class_id":id.class,"descriptor":id.descriptor,
        "inventory":id.inventory,"pairs":trials.len()/2,"decode_limit":plan.decode_limit,"max_leaves":plan.max_leaves})
    );
    for (index, chunk) in trials.chunks_exact(2).enumerate() {
        let pair = [&chunk[0], &chunk[1]];
        for shared in [index % 2 != 0, index % 2 == 0] {
            let start = Instant::now();
            let common = if shared {
                instance.verify_shared_original_pair(pair, plan.decode_limit, plan.max_leaves)?
            } else {
                for o in pair {
                    instance.verify_independent_original(o, plan.decode_limit, plan.max_leaves)?;
                }
                0
            };
            println!(
                "{}",
                json!({"pair":index,"shared":shared,"common_positions_executed_once":common,
                "elapsed_us":start.elapsed().as_micros(),"verified_tickets":pair.map(|o| o.ticket)})
            );
        }
    }
    Ok(())
}

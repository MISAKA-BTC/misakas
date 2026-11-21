//! Non-rewarding native replay of the full fixed-scale RFC-0066 experiment.
//! Feed the authenticated tensor stream produced by check_integer_weights.py.
use misaka_palw_tir::DType;
use misaka_palw_tir_exec::{Buf, NodeValue, ParamData, Slice, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_lower::qf1::{INVENTORY, fixed_scale_program, visit_fixed_scale_weights};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    error::Error,
    fs,
    io::{self, Write},
    path::Path,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
struct Capture {
    decoder: u8,
    post: u8,
    hidden: u16,
    norm: u16,
    prediction: bool,
    prompt: bool,
    prefix: Sha256,
    vectors: Vec<u8>,
    next: usize,
}

impl StepSink for Capture {
    fn node(&mut self, n: &NodeValue<'_>) {
        let role = if n.block == self.decoder && n.node == self.hidden {
            usize::from(n.layer.unwrap())
        } else if n.block == self.post && n.node == self.norm {
            36
        } else {
            return;
        };
        let Slice::I16(codes) = n.data else { panic!("wrong readout type") };
        assert_eq!(codes.len(), 4096);
        let bytes: Vec<_> = codes.iter().flat_map(|x| x.to_le_bytes()).collect();
        if self.prompt && role == 0 {
            self.prefix.update(&bytes);
        }
        if self.prediction {
            assert_eq!(role, self.next);
            self.vectors.extend_from_slice(&bytes);
            self.next += 1;
        }
    }
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("usage: replay-qf1-8b <inventory.json> <token IDs.json> <max new tokens> <new readout file>".into());
    }
    let bytes = fs::read(&args[0])?;
    if format!("{:x}", Sha256::digest(&bytes)) != INVENTORY {
        return Err("wrong integer inventory".into());
    }
    let mut tokens: Vec<u32> = serde_json::from_slice(&fs::read(&args[1])?)?;
    let length = tokens.len();
    let limit: usize = args[2].parse()?;
    if length == 0
        || limit == 0
        || limit > 1024
        || length > 8193 - limit
        || tokens.iter().any(|&x| x >= 151936)
        || Path::new(&args[3]).exists()
    {
        return Err("invalid context, tokens, decode limit or destination".into());
    }
    let mut scheme = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/tiled-logits/scheme/v1").to_state();
    let domain = b"tiled-logits-v1";
    scheme.update(&(domain.len() as u64).to_le_bytes()).update(domain);
    let instance = fixed_scale_program(*scheme.finalize().as_array())?;
    let program_hash = format!("{:x}", Sha256::digest(instance.program.encode()));
    let plan = TirPlan::compile(&instance.program)?;
    let decoder = plan.program.schedule.layers[0];
    let mut cap = Capture {
        decoder,
        post: plan.program.schedule.post,
        hidden: plan.program.blocks[decoder as usize].carry_out[0],
        norm: instance.final_norm,
        prediction: false,
        prompt: true,
        prefix: Sha256::new(),
        vectors: vec![],
        next: 0,
    };
    eprintln!("full QF1 program sha256={program_hash}; reading converted parameters");
    let mut params = TirParams::new(&plan);
    let indices: BTreeMap<_, _> = plan.program.params.iter().enumerate().map(|(j, p)| (p.name.as_str(), j as u16)).collect();
    visit_fixed_scale_weights(&mut io::stdin().lock(), &bytes, |name, layer, raw| {
        let j = *indices.get(name).ok_or("unknown parameter")?;
        let dtype = plan.program.params[usize::from(j)].dtype;
        let buf = if dtype == DType::I8 {
            Buf::I8(raw.into_iter().map(|b| b as i8).collect())
        } else {
            Buf::from_le_bytes(dtype, &raw).ok_or("invalid parameter bytes")?
        };
        params.insert(&plan, j, layer, ParamData::from_buf(buf)?)?;
        Ok(())
    })?;
    let mut exec = TirExecutor::new(&plan, &params)?;
    exec.set_fused(true);
    let mut logits_hashes = Vec::new();
    let mut answers = Vec::new();
    let mut pos = 0;
    loop {
        cap.prompt = pos < length;
        cap.prediction = pos + 1 >= length;
        cap.next = 0;
        let run_post = cap.prediction;
        exec.step_opt(tokens[pos], &mut cap, run_post)?;
        if cap.prediction {
            if cap.next != 37 {
                return Err("incomplete prediction state map".into());
            }
            let (_, Slice::I32(logits)) = exec.logits() else { return Err("invalid output head".into()) };
            if logits.len() != 151936 {
                return Err("incorrect vocabulary".into());
            }
            let largest = logits.iter().max().unwrap();
            let y = logits.iter().position(|x| x == largest).unwrap() as u32;
            logits_hashes.push(format!("{:x}", Sha256::digest(logits.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<_>>())));
            answers.push(y);
            if matches!(y, 151643 | 151645) || answers.len() == limit {
                break;
            }
            tokens.push(y);
        }
        pos += 1;
        if pos % 16 == 0 {
            eprintln!("completed {pos} positions");
        }
    }
    let mut out = fs::OpenOptions::new().create_new(true).write(true).open(&args[3])?;
    out.write_all(&cap.vectors)?;
    out.sync_all()?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"program_sha256":program_hash,"integer_inventory_sha256":INVENTORY,
        "input_positions":length,"processed_positions":pos+1,"output_ids":answers,"decode_limit":limit,
        "stop":if matches!(answers.last(),Some(151643|151645)) {"Eos"} else {"Limit"},
        "prefix_layer0_sha256":format!("{:x}",cap.prefix.finalize()),"logits_sha256":logits_hashes,
        "readout_bytes":cap.vectors.len(),"readout_i16le_sha256":format!("{:x}",Sha256::digest(&cap.vectors))}))?
    );
    Ok(())
}

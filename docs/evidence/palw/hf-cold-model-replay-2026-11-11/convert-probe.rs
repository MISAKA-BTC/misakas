use misaka_palw_tir_lower::convert::{CalibInput, ConvertRequest, convert_model};
use std::path::Path;
fn main() {
    let a: Vec<_> = std::env::args().collect();
    let mut r = ConvertRequest::new(&a[1], &a[2]);
    r.stats_in = Some(a[3].clone().into());
    r.calib = Some(CalibInput::from_file(Path::new(&a[4])).unwrap());
    r.context = Some(8192);
    r.max_window = Some(8192);
    r.policy.headroom16 = 2.0;
    r.policy.headroom32 = 4.0;
    r.policy.headroom_resid = 4.0;
    r.linear_weight_bits = 8;
    r.logits_bits = 32;
    r.attention_weight_bits = 8;
    r.mlp_down_weight_bits = 8;
    r.math = misaka_palw_tir_lower::detmath::MathMode::LibmV1;
    r.block_mib = 16;
    r.defer_min_mib = 16;
    r.keep_chunks = false;
    let started = std::time::Instant::now();
    convert_model(&r, &|s| println!("{s}")).unwrap();
    println!("conversion_elapsed_seconds={:.3}", started.elapsed().as_secs_f64());
}

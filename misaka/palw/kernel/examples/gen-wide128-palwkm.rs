//! **Lane ROLES: the testnet-12 Panel=1 slice class `wide128_v1(7)`** — writes, into `<out-dir>`:
//! - `wide128-v1-7.palwkm`: the PALWKM01 model file the kernel verifier / Panel=1 seat loads (`kaspad/src/palw_kernel_verifier.rs`
//!   `palw_kernel_model_write_v1`'s format: magic `PALWKM01` + borsh `Vec<(u16, Option<u16>, TensorWireV1)>`);
//! - `wide128-v1-7.register-class.bin`: the encoded `KernelRouteObjectV1::RegisterClass` (k2_tir_v2 descriptor, plan at 64 positions,
//!   `ParamCommitmentsV1::of`) — the exact object the kernel-route E2E registers (card 1);
//! and prints the parameter commitment root and the class id. Deterministic: the fixture is a pure function of the seed.
//!
//! `cargo run -p misaka-palw-kernel --example gen-wide128-palwkm -- <out-dir>`
//!
//! Hashes and commitments only identify data; nothing here proves any computation (PRINCIPLES §8).
use misaka_palw_kernel::descriptor::k2_tir_v2_descriptor;
use misaka_palw_kernel::ledger::single_class_binding_v1;
use misaka_palw_kernel::plan::plan_for_tir_program_v1;
use misaka_palw_kernel::public::{TensorWireV1, program_root_v1};
use misaka_palw_kernel::route::KernelRouteObjectV1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;

const SEED: u64 = 7;
const MAX_POSITIONS: u32 = 64;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: gen-wide128-palwkm <out-dir>"));
    std::fs::create_dir_all(&out).expect("out dir");
    let fx = misaka_palw_tir_sketch::fixture::wide128_v1(SEED);
    let d = k2_tir_v2_descriptor();
    let program_bytes = fx.program.encode();
    let plan = plan_for_tir_program_v1(&d, &fx.program, program_root_v1(&program_bytes), MAX_POSITIONS).expect("a K2-TIR-v2 plan");
    let pc = ParamCommitmentsV1::of(&fx.params);

    let rows: Vec<(u16, Option<u16>, TensorWireV1)> =
        fx.params.tensors.iter().map(|((j, l), t)| (*j, *l, TensorWireV1::of(t))).collect();
    let mut model = b"PALWKM01".to_vec();
    model.extend(borsh::to_vec(&rows).expect("borsh"));
    let model_path = out.join("wide128-v1-7.palwkm");
    std::fs::write(&model_path, &model).expect("write model");

    let binding = single_class_binding_v1(d.digest(), &program_bytes, &plan, &pc);
    let register = KernelRouteObjectV1::RegisterClass { descriptor: d.digest(), program_bytes, plan, param_commitments: pc.clone() };
    let reg_path = out.join("wide128-v1-7.register-class.bin");
    std::fs::write(&reg_path, register.encode()).expect("write register-class");

    println!("model            {} ({} tensors, {} bytes)", model_path.display(), rows.len(), model.len());
    println!("register-class   {}", reg_path.display());
    println!("param_root       {}", hex(&pc.root()));
    println!("param_root_v3    {}", hex(&ParamCommitmentsV1::of_v3(&fx.params).root()));
    println!("class_id         {}  (PanelLicensed = the binding id)", hex(&binding.class_binding_id()));
}

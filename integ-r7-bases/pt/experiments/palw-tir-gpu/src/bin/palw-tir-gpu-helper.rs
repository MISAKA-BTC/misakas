//! **`palw-tir-gpu-helper` — the GPU device as a helper process for the node's cell steps (RFC-0006).**
//!
//! kaspad (`--palw-tir-shard-gpu`) spawns this binary and speaks the versioned cell wire of `misaka_palw_tir_exec::cellproc` to it over
//! its stdin and stdout: `Hello`, `Open{program, occ, params}`, `Step{..}`, state reads, `Close`. It is built in this workspace
//! (its own lock: wgpu 30), so the node's lock never sees the graphics stack. With no usable device it says so on stderr and exits, and
//! the node runs its cells on the CPU.

fn main() {
    let device = match misaka_palw_tir_gpu::GpuDeviceV1::open() {
        Ok(d) => d,
        Err(why) => {
            eprintln!("palw-tir-gpu-helper: no device: {why}");
            std::process::exit(3);
        }
    };
    eprintln!("palw-tir-gpu-helper: serving cell steps on {}", misaka_palw_tir_exec::TirDeviceV1::name(&device));
    let (stdin, stdout) = (std::io::stdin(), std::io::stdout());
    let (mut r, mut w) = (stdin.lock(), stdout.lock());
    if let Err(e) = misaka_palw_tir_exec::serve_cell_requests_v1(&mut r, &mut w, &device) {
        eprintln!("palw-tir-gpu-helper: {e}");
        std::process::exit(1);
    }
}

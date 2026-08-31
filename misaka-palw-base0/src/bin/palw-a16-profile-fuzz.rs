//! **ADR-0067 Decision 5's saturation runner.** `--seed N --iters N`; the tally prints, and a
//! non-zero finding column exits non-zero — a CI or an operator can gate on the exit code. The
//! seed is the repro: the same seed replays the same corpus bit for bit.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str, default: u64| -> u64 {
        args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.strip_prefix("0x").map_or_else(|| v.parse().ok(), |h| u64::from_str_radix(h, 16).ok())).unwrap_or(default)
    };
    let seed = flag("--seed", 0x0067);
    let iters = flag("--iters", 10_000);
    let started = std::time::Instant::now();
    let tally = misaka_palw_base0::fuzz_a16::fuzz_a16_profiles_v1(seed, iters);
    println!("seed {seed:#x}, {iters} iterations in {:?}", started.elapsed());
    println!("{tally:?}");
    if tally.panics != 0 || tally.nondeterminism != 0 {
        eprintln!("FINDINGS — the ADR-0067 fence must stay down; replay with --seed {seed:#x}");
        std::process::exit(1);
    }
}

//! **ADR-0067 Decision 5's saturation runner, for the mmap (Qwen3.6) container.** `--seed N
//! --iters N`; the tally prints, and a non-zero finding column exits non-zero — a CI or an
//! operator can gate on the exit code. The seed is the repro: the same seed replays the same
//! corpus bit for bit.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str, default: u64| -> u64 {
        let Some(i) = args.iter().position(|a| a == name) else { return default };
        // Underscores spell a number the way the source does (`0x0067_2026_09_01`), and a value
        // that does not parse is a WRONG repro, never silently the default — the doc line above
        // says the seed is the repro, so the parser is not allowed to lie about which one ran.
        let v = args.get(i + 1).map(|v| v.replace('_', ""));
        let parsed = v.as_deref().and_then(|v| match v.strip_prefix("0x") {
            Some(h) => u64::from_str_radix(h, 16).ok(),
            None => v.parse().ok(),
        });
        parsed.unwrap_or_else(|| {
            eprintln!("{name} {} is not a number this runner can replay", args.get(i + 1).map(String::as_str).unwrap_or("(missing)"));
            std::process::exit(2);
        })
    };
    let seed = flag("--seed", 0x0067);
    let iters = flag("--iters", 10_000);
    let started = std::time::Instant::now();
    let tally = misaka_palw_base0::fuzz_qwen36::fuzz_qwen36_profiles_v1(seed, iters);
    println!("seed {seed:#x}, {iters} iterations in {:?}", started.elapsed());
    println!("{tally:?}");
    if tally.panics != 0 || tally.nondeterminism != 0 {
        eprintln!("FINDINGS — the ADR-0067 fence must stay down; replay with --seed {seed:#x}");
        std::process::exit(1);
    }
}

#!/usr/bin/env python3
"""PALW v2 determinism-class comparator — decides whether N fleet hosts are ONE class.

    scripts/misaka-palw-v2-class-compare.py host-a.jsonl host-b.jsonl [host-c.jsonl ...]

Each input is a class line from `misaka-palw-v2-class-probe.sh`.

WHY THIS EXISTS
---------------
The boot self-test proves a host reproduces its own goldens. That passes on a host
that agrees with nobody, so it cannot establish a class. The PALW-Ollama flavor was
deployed on a campaign that looked like cross-architecture agreement because the
quantity being compared had collapsed to a constant; agreement on a constant does
not move with reduction order, so the campaign certified nothing. This comparator
separates the two questions that got conflated:

  Q1  Are these hosts running the SAME RUNTIME?      -> identity fields
  Q2  Given the same runtime, do they compute the    -> per-job trace roots
      SAME ARITHMETIC?

Both must be answered, in that order, and a mismatch in Q1 makes Q2 vacuous: two
hosts with different builds that disagree tell you nothing about determinism, and
two hosts with different builds that AGREE tell you nothing either.

VERDICTS
  ONE-CLASS        identity matches and every trace root matches. The claim holds
                   for these hosts, on this corpus.
  BUILD-MISMATCH   identity differs. Align the builds and re-run; determinism is
                   UNTESTED, not disproven. The differing fields are named.
  DETERMINISM-FAIL identity matches but trace roots differ. The class claim is
                   FALSE for these hosts. This is the finding the Ollama campaign
                   was structurally unable to produce.
  SELFTEST-FAIL    a host does not reproduce its own goldens. Fix before anything
                   else; cross-host comparison is meaningless until it passes.
"""
import json
import sys

# Identity fields, in the order a human should read them when they disagree:
# build inputs first (actionable), derived ids after (consequences).
IDENTITY = [
    ("cmake_cache_sha256", "llama.cpp CMake configuration (flags/toolchain)"),
    ("llama_static_library_sha256", "the linked llama.cpp/ggml archives"),
    ("worker_binary_sha256", "the palw-worker binary"),
    ("model_profile_id", "the GGUF / model profile"),
    ("tokenizer_id_v2", "the tokenizer"),
    ("shape_profile_id_v2", "the shape profile (context/batch geometry)"),
    ("trace_scheme_id_v2", "the trace scheme"),
    ("runtime_class_id", "the derived runtime class id"),
]

RED, GRN, YLW, DIM, RST = "\033[31m", "\033[32m", "\033[33m", "\033[2m", "\033[0m"
if not sys.stdout.isatty():
    RED = GRN = YLW = DIM = RST = ""


def _prefill_of(job_name):
    """Corpus job names encode their prefill length as `-p<N>-`; 0 when absent."""
    import re
    m = re.search(r"-p(\d+)-", job_name)
    return int(m.group(1)) if m else 0


def load(path):
    with open(path) as f:
        text = f.read().strip()
    # Accept a bare JSON object or a JSONL file whose last non-empty line is the class line.
    for chunk in reversed([c for c in text.splitlines() if c.strip()]):
        try:
            d = json.loads(chunk)
        except json.JSONDecodeError:
            continue
        if d.get("schema") == "misaka.palw.v2-class-line.v1":
            return d
    sys.exit(f"{path}: no misaka.palw.v2-class-line.v1 object found")


def main(paths):
    if len(paths) < 2:
        sys.exit("need at least two class lines — a class is a claim about pairs of hosts")
    hosts = [load(p) for p in paths]
    labels = [h["label"] for h in hosts]
    if len(set(labels)) != len(labels):
        sys.exit(f"duplicate labels {labels} — pass distinct hosts, or the comparison is trivial")

    print(f"PALW v2 determinism-class comparison over {len(hosts)} hosts: {', '.join(labels)}")
    for h in hosts:
        arch = f"{h['host']['machine']}/{h['host']['system']}"
        fp = "canonical" if h.get("fp_environment_canonical") else f"{YLW}NON-CANONICAL{RST}"
        print(f"  {DIM}·{RST} {h['label']:<16} {arch:<16} fp-env {fp}  openmp={h['ggml_flags'].get('openmp')}")

    problems = []

    # ── Corpus agreement (random-corpus mode only) ──────────────────────────────
    # Comparing trace roots across hosts is only meaningful if the hosts ran the SAME jobs.
    # The fixed probe guarantees that by compiling the corpus into the worker; the random
    # corpus has to prove it, so refuse the comparison outright when the inputs differ.
    modes = {h["label"]: h.get("mode", "fixed-golden") for h in hosts}
    digests = {h["label"]: h.get("corpus_digest") for h in hosts if h.get("corpus_digest")}
    if len(set(modes.values())) > 1:
        print(f"\n{YLW}MODE MISMATCH{RST}: cannot compare a fixed-corpus line against a random-corpus one.")
        for label, m in modes.items():
            print(f"    {label:<16} {m}")
        return 1
    if digests:
        if len(digests) != len(hosts):
            print(f"\n{YLW}corpus_digest missing on some hosts{RST} — cannot prove they ran the same jobs.")
            return 1
        if len(set(digests.values())) > 1:
            print(f"\n{RED}CORPUS MISMATCH{RST}: hosts did not run the same jobs, so any agreement or")
            print("  disagreement below is meaningless. Re-run with the same --master-seed and --jobs.")
            for label, d in digests.items():
                print(f"    {label:<16} {d[:48]}")
            return 1
        seeds = {h["label"]: h.get("corpus_master_seed") for h in hosts}
        n = next(iter({h.get("corpus_jobs") for h in hosts}))
        print(f"  {DIM}corpus{RST} {n} jobs, master seed {next(iter(set(seeds.values())))!r}, "
              f"inputs agree ({next(iter(set(digests.values())))[:16]}…)")
        stuck = {h["label"]: h.get("corpus_failures") or {} for h in hosts}
        if any(stuck.values()):
            print(f"\n{YLW}some jobs failed to execute{RST} — the class is untested for those:")
            for label, f in stuck.items():
                for name, err in list(f.items())[:4]:
                    print(f"    {label:<16} {name}: {str(err)[:90]}")
            problems.append("corpus-failures")

    # ── SELFTEST floor ──────────────────────────────────────────────────────────
    failed = [h["label"] for h in hosts if h.get("selftest") != "pass"]
    if failed:
        verb = "does" if len(failed) == 1 else "do"
        print(f"\n{RED}SELFTEST-FAIL{RST}: {', '.join(failed)} {verb} not reproduce its own goldens."
              if len(failed) == 1 else
              f"\n{RED}SELFTEST-FAIL{RST}: {', '.join(failed)} do not reproduce their own goldens.")
        print("  A runtime that is not deterministic against itself cannot be in any class.")
        print("  Fix that first; every comparison below is meaningless until it passes.")
        return 2

    # ── Q1: same runtime? ───────────────────────────────────────────────────────
    ref = hosts[0]
    identity_diffs = []
    for field, human in IDENTITY:
        vals = {h["label"]: h.get(field) for h in hosts}
        if len(set(vals.values())) > 1:
            identity_diffs.append((field, human, vals))

    flag_diffs = []
    all_flags = sorted({k for h in hosts for k in h["ggml_flags"]})
    for k in all_flags:
        vals = {h["label"]: h["ggml_flags"].get(k) for h in hosts}
        if len(set(vals.values())) > 1:
            flag_diffs.append((k, vals))

    if identity_diffs or flag_diffs:
        print(f"\n{YLW}BUILD-MISMATCH{RST}: these hosts are NOT running the same runtime.")
        print("  Determinism is UNTESTED — not disproven. Align the builds and re-run.")
        for field, human, vals in identity_diffs:
            print(f"\n  {field}  ({human})")
            for label, v in vals.items():
                print(f"    {label:<16} {v}")
        if flag_diffs:
            print(f"\n  ggml build flags that differ:")
            for k, vals in flag_diffs:
                rendered = "  ".join(f"{lab}={v}" for lab, v in vals.items())
                extra = ""
                if k == "openmp" and any(vals.values()):
                    extra = f"  {RED}<- OpenMP makes matmul reduction order an external runtime's scheduling decision{RST}"
                if k == "native" and any(vals.values()):
                    extra = f"  {RED}<- GGML_NATIVE compiles for the build host's ISA; it cannot be a portable class{RST}"
                print(f"    {k:<18} {rendered}{extra}")
        problems.append("build")

    # ── Q2: same arithmetic? ────────────────────────────────────────────────────
    job_names = sorted({n for h in hosts for n in h["jobs"]})
    missing = [(h["label"], n) for h in hosts for n in job_names if n not in h["jobs"]]
    if missing:
        print(f"\n{YLW}corpus mismatch{RST}: some hosts lack jobs the others have: {missing}")
        print("  Different worker builds carry different probe corpora — align first.")
        problems.append("corpus")

    root_diffs = []
    for n in job_names:
        vals = {h["label"]: h["jobs"].get(n, {}).get("expected_root") for h in hosts}
        cus = {h["label"]: h["jobs"].get(n, {}).get("expected_cu") for h in hosts}
        if len(set(vals.values())) > 1:
            root_diffs.append((n, vals, cus))

    golden_roots = {h["label"]: h["golden_root"] for h in hosts}
    golden_agree = len(set(golden_roots.values())) == 1

    if root_diffs:
        verdict = "DETERMINISM-FAIL" if not (identity_diffs or flag_diffs) else "BUILD-MISMATCH (arithmetic also differs)"
        print(f"\n{RED}{verdict}{RST}: {len(root_diffs)}/{len(job_names)} probe jobs computed different traces.")
        for n, vals, cus in root_diffs:
            print(f"\n  job {n}")
            for label in labels:
                print(f"    {label:<16} root={vals[label]}  cu={cus[label]}")
        if not (identity_diffs or flag_diffs):
            print(f"\n  Identity matched on every field, so this is not a build difference:")
            print(f"  the same source, flags, archives and model produce different arithmetic on")
            print(f"  these machines. The class does not exist as specified. Options: narrow the")
            print(f"  class (pin the microarchitecture), or make the trace tolerant by construction")
            print(f"  (quantize before committing) and re-measure the margin.")
        problems.append("determinism")
    elif not golden_agree:
        print(f"\n{RED}DETERMINISM-FAIL{RST}: per-job roots agree but golden_root differs — "
              f"the set is not canonicalized identically:")
        for label, r in golden_roots.items():
            print(f"    {label:<16} {r}")
        problems.append("golden-root")

    if not problems:
        corpus_mode = bool(digests)
        print(f"\n{GRN}ONE-CLASS{RST}: identity matches on all {len(IDENTITY)} fields and all "
              f"{len(job_names)} probe jobs agree byte for byte.")
        print(f"  aggregate    {ref['golden_root']}")
        print(f"  class id     {ref['runtime_class_id']}")
        print(f"\n  Scope of this result: {len(job_names)} jobs on {len(hosts)} hosts.")
        if corpus_mode:
            # The reachable prefill range is 1..512: the shape profile pins prefill-single-batch
            # and the worker refuses anything longer, so ">512" is out of profile by DESIGN, not a
            # coverage gap. The gap that matters is the fixed corpus's 96-token ceiling.
            prefills = [_prefill_of(n) for n in job_names if _prefill_of(n)]
            beyond = sum(1 for p in prefills if p > 96)
            print(f"  Random corpus, inputs proven identical across hosts. "
                  f"{beyond}/{len(job_names)} jobs exceed the fixed corpus's 96-token prefill ceiling"
                  + (f", longest {max(prefills)}." if prefills else "."))
            if prefills and max(prefills) >= 512:
                print(f"  That reaches the 512-token single-batch limit, so the whole legal prefill range")
                print(f"  is exercised — anything longer is refused by the worker (prefill-single-batch),")
                print(f"  which is also why the multi-batch GEMM that once split arm64 from x86 is not a")
                print(f"  path v2 can take.")
            elif beyond:
                print(f"  {YLW}The 512-token boundary itself is not reached{RST} — raise --jobs so the")
                print(f"  longest prefill lengths are included.")
            else:
                print(f"  {YLW}This adds no prefill coverage over the fixed corpus{RST} — raise --jobs.")
            print(f"  Still evidence rather than proof: one seed, one shape profile, one decode-budget")
            print(f"  set, and agreement on a corpus cannot speak for inputs outside it.")
        else:
            print(f"  It is evidence for the class, not proof of it — this corpus is fixed and small.")
            print(f"  Its longest prefill is 96 tokens while the profile allows 512, so four fifths of")
            print(f"  the legal prefill range is unmeasured here; misaka-palw-v2-class-corpus.py covers")
            print(f"  it. Before relying on this, run the decisive cross-check below, which the JSON")
            print(f"  comparison cannot replace:")
            for h in hosts:
                others = [o["label"] for o in hosts if o["label"] != h["label"]]
                print(f"    MISAKA_PALW_GOLDEN={h['golden_file']} palw-worker --mode v2-selftest   "
                      f"# on {', '.join(others)}")
            print(f"\n  Each host must PASS every other host's set. That exercises the real verifier")
            print(f"  path, including the set's own identity gate, rather than comparing two JSON docs.")
        return 0

    print(f"\nverdict: {', '.join(problems)}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

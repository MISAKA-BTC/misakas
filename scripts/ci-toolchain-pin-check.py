#!/usr/bin/env python3
"""**The compiler CI uses is a pin or it is a rumour.**

Two invariants, both checkable from the repository alone, both of which this tree violated on the
day this file was written.

**PIN** — `rust-toolchain.toml` exists at the root and names an exact version, and no job in
`.github/workflows/` installs a toolchain other than that one. The action every workflow uses,
`dtolnay/rust-toolchain`, is pinned by commit sha; its `toolchain` INPUT is not pinned by that,
and its own `action.yml` gives that input `default: stable`. So a job that writes

    uses: dtolnay/rust-toolchain@29eef336...   # stable

installs whatever `stable` was on the morning the job ran, and the sha in the line buys nothing
but the action's source. That is the skew behind this repository's own record of six consecutive
CI reds reported as green, and behind release binaries built by a compiler nobody chose.

**PARITY** — every build gate `scripts/ci-gates.sh` offers is spelled, character for character,
in `.github/workflows/ci.yaml`, AND every judgment ci.yaml makes about the tree is a gate the
script offers. A gate the script runs that CI does not is a lie to whoever runs it; a `run:`
line in CI that the script does not offer is a gate nobody can run before pushing, which is the
whole reason the first job was red for an unknown length of time. The second half of that
sentence was prose only until the deliberate breakage tried it: adding `cargo audit` to ci.yaml
and deleting `gate_doc` from the script both left PARITY green. A ci.yaml judgment that cannot
be a local gate is named in `CI_ONLY` with its reason, one entry per command and no wildcards.

Both print their coverage: how many workflow files, how many toolchain steps, how many gates.
A verdict with no coverage cannot be falsified, and this repository keeps writing that down as
the defect.

    python3 scripts/ci-toolchain-pin-check.py            # PIN
    python3 scripts/ci-toolchain-pin-check.py --parity   # PARITY

Exit 0 = the invariant holds, and the last line is `PIN OK` / `PARITY OK`. Exit 1 = it does not,
and every violation is named with its file and line.
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOWS = os.path.join(ROOT, ".github", "workflows")
TOOLCHAIN_FILE = os.path.join(ROOT, "rust-toolchain.toml")

# The one expression a job is allowed to pass instead of the literal. Each job reads
# `rust-toolchain.toml` in a step of its own and exports the channel, so the expression IS the
# pinned value -- but only if that reading step is actually in the job, which is checked below.
# A `${{ env.RUST_CHANNEL }}` with nothing setting RUST_CHANNEL expands to the EMPTY STRING, and
# `dtolnay/rust-toolchain` treats an empty `toolchain` input as "not given" and falls back to its
# `stable` default. That failure would look exactly like success in the diff and exactly like
# nothing in the log, so it is a violation here.
PIN_EXPR = "env.RUST_CHANNEL"


def is_reading_step(line):
    """Does this line EXPORT the channel? Not "does it mention the file".

    The first version of this asked whether `rust-toolchain.toml` appeared anywhere earlier in
    the job, and the comment above the reading step says the words `rust-toolchain.toml` -- so
    deleting the step outright still passed. A checker satisfied by prose about itself is the
    exact defect this file was written to catch, one level up.
    """
    stripped = line.lstrip()
    return "RUST_CHANNEL=" in line and "GITHUB_ENV" in line and not stripped.startswith("#")


def pinned_channel():
    """The `channel` out of `rust-toolchain.toml`. Parsed, not assumed: a second spelling of the
    version is a second version."""
    if not os.path.exists(TOOLCHAIN_FILE):
        return None, "rust-toolchain.toml does not exist: nothing pins the compiler"
    with open(TOOLCHAIN_FILE, encoding="utf-8") as fh:
        text = fh.read()
    m = re.search(r'^\s*channel\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        return None, "rust-toolchain.toml has no `channel = \"...\"`"
    chan = m.group(1)
    if not re.fullmatch(r"\d+\.\d+(\.\d+)?", chan):
        return None, f'channel "{chan}" is not an exact version -- "stable"/"nightly" float by design'
    return chan, None


def workflow_files():
    if not os.path.isdir(WORKFLOWS):
        return []
    return sorted(
        os.path.join(WORKFLOWS, f) for f in os.listdir(WORKFLOWS) if f.endswith((".yaml", ".yml"))
    )


def check_pin():
    chan, err = pinned_channel()
    print("== PIN: the compiler every job installs ==")
    if err:
        print(f"  VIOLATION  {err}", file=sys.stderr)
        return 1
    print(f"  rust-toolchain.toml pins channel {chan}")

    bad = []
    steps = 0
    files = workflow_files()
    for path in files:
        with open(path, encoding="utf-8") as fh:
            lines = fh.read().splitlines()
        rel = os.path.relpath(path, ROOT)
        for i, line in enumerate(lines):
            if "dtolnay/rust-toolchain@" not in line:
                continue
            steps += 1
            # The step's own block. A step is `      - name: ...` / `        uses: ...` /
            # `        with:` / `          key: ...`, so the block ends at the next line indented
            # at or left of the `- ` bullet -- which is the `uses:` indent MINUS 2. Breaking at
            # the `uses:` indent itself (the first version of this loop) stopped at the step's own
            # `with:` line and made every job look unpinned: a checker whose scan is one column
            # off reports a tree-wide catastrophe, which is its own kind of unfalsifiable.
            indent = len(line) - len(line.lstrip())
            stop_at = indent - 2
            block = []
            for j in range(i + 1, len(lines)):
                nxt = lines[j]
                if nxt.strip() and (len(nxt) - len(nxt.lstrip())) <= stop_at:
                    break
                block.append((j, nxt))
            got = None
            for j, b in block:
                m = re.match(r"\s*toolchain:\s*(\S.*?)\s*$", b)
                if m:
                    got = (j, m.group(1))
                    break
            # The job this step belongs to: back up to the last two-space-indented job key.
            job_start = 0
            for k in range(i, -1, -1):
                if re.match(r"^  [A-Za-z0-9_-]+:\s*$", lines[k]):
                    job_start = k
                    break
            reads_pin = any(is_reading_step(lines[k]) for k in range(job_start, i))
            if got is None:
                bad.append(
                    f"{rel}:{i + 1}: installs the action's default toolchain (`stable`, per its "
                    f"action.yml) -- pass `toolchain: {chan}` or ${{{{ {PIN_EXPR} }}}}"
                )
            else:
                j, value = got
                literal = value.strip('"\'') == chan
                expr = PIN_EXPR in value
                ok = literal or (expr and reads_pin)
                why = "OK" if ok else ("MISMATCH" if not expr else "UNSET -- no step in this job exports RUST_CHANNEL")
                print(f"  {rel}:{j + 1}: toolchain: {value}  {why}")
                if not ok:
                    if expr:
                        bad.append(
                            f"{rel}:{j + 1}: passes ${{{{ {PIN_EXPR} }}}} but no earlier step in this job "
                            f"writes RUST_CHANNEL to $GITHUB_ENV; the input expands to EMPTY and the action "
                            f"falls back to its `stable` default"
                        )
                    else:
                        bad.append(f"{rel}:{j + 1}: installs `{value}`, but the tree pins {chan}")

    print(f"  checked {len(files)} workflow file(s), {steps} toolchain step(s)")
    if not steps and files:
        # A pin nothing consumes is not a pin; if every action reference disappeared, say so
        # rather than printing a green line about zero things.
        print("  VIOLATION  no workflow installs a Rust toolchain at all -- did a job lose its setup step?", file=sys.stderr)
        return 1
    for b in bad:
        print(f"  VIOLATION  {b}", file=sys.stderr)
    if bad:
        print(f"PIN FAILED: {len(bad)} job(s) do not honour rust-toolchain.toml", file=sys.stderr)
        return 1
    report_container_bases(chan)
    print(f"PIN OK: {steps} toolchain step(s) in {len(files)} workflow file(s) all install {chan}")
    return 0


def report_container_bases(chan):
    """**What this check does NOT cover, said out loud.**

    `docker/Dockerfile.*` choose a compiler with `FROM rust:X.Y-alpine@sha256:...`. That is a
    different lever from the workflows' and this function does not fail on it, for a reason worth
    stating: `rust-toolchain.toml` arrives in the build context with `COPY . .`, so cargo inside
    the container installs the pinned toolchain anyway and the base tag becomes a bootstrap
    rather than the compiler. It is reported rather than ignored because a silent gap is how a
    pin stops meaning anything, and because a mismatch has a real cost here: `cargo chef cook`
    runs BEFORE `COPY . .`, so the dependency layer is compiled by the base image's rustc and
    then thrown away when the app layer compiles with a different one.
    """
    import glob

    docker = sorted(glob.glob(os.path.join(ROOT, "docker", "Dockerfile*")))
    rows = []
    for path in docker:
        with open(path, encoding="utf-8") as fh:
            for i, line in enumerate(fh):
                m = re.match(r"\s*FROM\s+rust:(\d+\.\d+(?:\.\d+)?)", line)
                if m:
                    rows.append((os.path.relpath(path, ROOT), i + 1, m.group(1)))
    if not rows:
        print("  container images: no `FROM rust:` base found under docker/")
        return
    agree = [r for r in rows if chan.startswith(r[2] + ".") or r[2] == chan]
    print(f"  container images: {len(rows)} `FROM rust:` base(s) under docker/, {len(agree)} on {chan}")
    for rel, ln, ver in rows:
        note = "matches the pin" if (rel, ln, ver) in agree else (
            f"base is {ver}; the toml still forces {chan} inside the container, but `cargo chef cook` "
            f"runs before the toml is copied, so the cached dependency layer is built by {ver} and rebuilt"
        )
        print(f"    {rel}:{ln}  rust:{ver}  -- {note}")
    print("  (not a violation: this check governs .github/workflows only)")


# The build gates in `ci-gates.sh` are one-liners of the form
#   gate_fmt()           { cargo fmt --all -- --check; }
# so the command is extractable without executing anything.
GATE_LINE = re.compile(r"^gate_([a-z0-9_]+)\(\)\s*\{\s*(.+?);\s*\}\s*$", re.M)

def ci_run_commands(lines):
    """Every command ci.yaml actually runs, as WHOLE strings.

    Two shapes: `run: <cmd>` on one line, and a `run: |` block scalar whose more-indented lines
    are each a command. Continuations (a line ending in `\\`) are joined, so a `pip install`
    wrapped over two lines is one command and not two fragments.
    """
    cmds = set()
    i = 0
    while i < len(lines):
        m = re.match(r"^(\s*)-?\s*run:\s*(.*)$", lines[i])
        if not m:
            i += 1
            continue
        indent, rest = len(m.group(1)), m.group(2).strip()
        if rest and rest not in ("|", ">", "|-", ">-", "|+", ">+"):
            cmds.add(rest)
            i += 1
            continue
        i += 1
        buf = ""
        while i < len(lines):
            nxt = lines[i]
            if nxt.strip() and (len(nxt) - len(nxt.lstrip())) <= indent:
                break
            body = nxt.strip()
            if body:
                if buf:
                    buf += " " + body
                else:
                    buf = body
                if buf.endswith("\\"):
                    buf = buf[:-1].rstrip()
                else:
                    cmds.add(buf)
                    buf = ""
            i += 1
        if buf:
            cmds.add(buf)
    return cmds


# Gates that deliberately have no ci.yaml counterpart, and why. An entry here is a claim a
# reviewer can check, not a silence.
NOT_IN_CI = {
    # `bash scripts/pq-ci-guard.sh` IS in ci.yaml, but the Lints job spells it without `bash`.
}


# **The other direction, which the docstring above promised and the first version did not do.**
#
# `check_parity` walked the SCRIPT's gates and looked each one up in ci.yaml. Nothing walked
# ci.yaml. So CI could grow a whole new `cargo` gate -- or the script could drop one CI still
# runs -- and PARITY stayed green, which is the same defect as the pin's: a green nobody can
# tell apart from not looking. Both were confirmed by breaking them: adding
# `run: cargo audit --deny warnings` to ci.yaml, and deleting `gate_doc` from the script.
#
# A ci.yaml command JUDGES the tree if it starts with `cargo ` or `bash scripts/`. Every such
# command must be a gate the script offers, or be named here with the reason it cannot be. A
# WILDCARD is deliberately not accepted: `cargo .* --target ` would excuse a whole class in one
# line and hide the next addition to it, which is exactly how a table stops being a claim.
CI_ONLY = {
    "bash scripts/ci-gates.sh --group fast":
        "this IS the script; a gate that runs the runner would recurse",
    "bash scripts/ci-gates-selftest.sh":
        "it rewrites tracked files and restores them with `git checkout -- .` -- safe on an "
        "ephemeral CI checkout, a trap on a laptop, so it refuses on a dirty tree and stays out "
        "of the pre-push command. Run it deliberately: `bash scripts/ci-gates-selftest.sh`",
    # Cross-target work. `scripts/ci-gates.sh --list` says the same thing in its footer; this is
    # the machine-checked half of that sentence.
    "cargo check --locked --tests --benches --target x86_64-pc-windows-msvc":
        "Check (x86_64-pc-windows-msvc) -- needs windows-latest, not a runner a contributor has",
    "cargo build --locked --target x86_64-pc-windows-msvc -p kaspad --bin kaspad -p misaka-cli --bin misaka":
        "Check (x86_64-pc-windows-msvc) -- same runner, and it builds the exes the --help smoke opens",
    "cargo check -p kaspa-addresses --no-default-features --target thumbv7em-none-eabi":
        "Check no_std -- the job installs the target with `rustup target add`; this script does "
        "not silently add targets to a contributor's toolchain",
    "cargo clippy -p kaspa-wrpc-wasm --target wasm32-unknown-unknown":
        "Check WASM32 -- needs `rustup target add wasm32-unknown-unknown`",
    "cargo clippy -p kaspa-wallet-cli-wasm --target wasm32-unknown-unknown":
        "Check WASM32 -- needs `rustup target add wasm32-unknown-unknown`",
    "cargo clippy -p kaspa-wasm --target wasm32-unknown-unknown":
        "Check WASM32 -- needs `rustup target add wasm32-unknown-unknown`",
    "cargo build --bin kaspad --bin rothschild --bin kaspa-wallet --bin stratum-bridge --release --target x86_64-unknown-linux-musl":
        "Build Linux Release -- runs after `source musl-toolchain/build.sh`, a cross toolchain "
        "this script does not build",
    "cargo build -p misaka-cli --bin misaka --release --target x86_64-unknown-linux-musl":
        "Build Linux Release -- same musl cross toolchain",
    "cargo build --locked --release -p misaka-cli --bin misaka":
        "Check (x86_64-pc-windows-msvc)/release smoke -- a --release artifact for a `--help` "
        "run, not a verdict; `check-evm-send` compiles the same crate in debug",
}

# What counts as a judgment ci.yaml makes about the tree, as opposed to runner housekeeping
# (`sudo apt`, `df -h`, `rustup target add`, PowerShell) or a shell fragment out of a `run: |`
# block. Narrow on purpose: a prefix list that is too wide fills the table above with excuses
# and stops being readable, and it is also how `# Alias cc to clang` ends up being called a gate.
JUDGMENT_PREFIXES = ("cargo ", "bash scripts/")


def check_parity():
    print("== PARITY: every build gate is spelled the same way in ci.yaml, both directions ==")
    gates_path = os.path.join(ROOT, "scripts", "ci-gates.sh")
    ci_path = os.path.join(WORKFLOWS, "ci.yaml")
    for p in (gates_path, ci_path):
        if not os.path.exists(p):
            print(f"  VIOLATION  {os.path.relpath(p, ROOT)} does not exist", file=sys.stderr)
            return 1
    with open(gates_path, encoding="utf-8") as fh:
        gates_src = fh.read()
    with open(ci_path, encoding="utf-8") as fh:
        ci_lines = fh.read().splitlines()
    ci_cmds = ci_run_commands(ci_lines)
    if not ci_cmds:
        print("  VIOLATION  ci.yaml has no `run:` commands -- this checker went blind", file=sys.stderr)
        return 1

    pairs = GATE_LINE.findall(gates_src)
    if not pairs:
        print("  VIOLATION  no `gate_x() { ...; }` one-liners found -- this checker went blind", file=sys.stderr)
        return 1

    def both_spellings(cmd):
        # Both spellings of a script gate (`bash x.sh` and `x.sh`) count as the same one.
        return {cmd, cmd[len("bash ") :] if cmd.startswith("bash ") else "bash " + cmd}

    # ---- direction 1: script -> ci.yaml. A gate the script runs that CI does not is a lie. ----
    print("  -- gates the script offers, looked up in ci.yaml --")
    missing = []
    gate_cmds = set()
    for name, cmd in pairs:
        gate_cmds |= both_spellings(cmd)
        if name in NOT_IN_CI:
            print(f"  {name:<16} not in CI by design: {NOT_IN_CI[name]}")
            continue
        # WHOLE-command comparison, not `in`. `cargo doc --no-deps` is a SUBSTRING of
        # `cargo doc --no-deps --document-private-items`, so a substring test said "found" while
        # CI ran a different command -- a parity check that cannot see a drift is not a parity
        # check.
        hit = both_spellings(cmd) & ci_cmds
        if hit:
            print(f"  {name:<16} `{cmd}`  found in ci.yaml")
        else:
            missing.append((name, cmd))

    # ---- direction 2: ci.yaml -> script. A judgment CI makes that nobody can run before
    # pushing is the whole reason the first job was red for an unknown length of time. ----
    print("  -- judgments ci.yaml makes, looked up in the script --")
    judgments = sorted(c for c in ci_cmds if c.startswith(JUDGMENT_PREFIXES))
    unrunnable, excused = [], 0
    for cmd in judgments:
        if both_spellings(cmd) & gate_cmds:
            continue
        if cmd in CI_ONLY:
            excused += 1
            print(f"    excused  `{cmd}`\n               {CI_ONLY[cmd]}")
        else:
            unrunnable.append(cmd)
    print(
        f"    {len(judgments)} judgment(s) in ci.yaml: "
        f"{len(judgments) - excused - len(unrunnable)} offered by the script, "
        f"{excused} excused by name, {len(unrunnable)} unrunnable"
    )

    for name, cmd in missing:
        print(f"  VIOLATION  gate_{name} runs `{cmd}`, which does not appear in ci.yaml", file=sys.stderr)
    for cmd in unrunnable:
        print(
            f"  VIOLATION  ci.yaml runs `{cmd}`, which no gate in scripts/ci-gates.sh offers -- "
            f"add a gate, or add it to CI_ONLY with the reason it cannot be one",
            file=sys.stderr,
        )
    print(
        f"  checked {len(pairs)} build gate(s) and {len(judgments)} ci.yaml judgment(s) against "
        f"{os.path.relpath(ci_path, ROOT)}"
    )
    if missing or unrunnable:
        print(
            f"PARITY FAILED: {len(missing)} gate(s) are not what CI runs, "
            f"{len(unrunnable)} CI judgment(s) cannot be run before pushing",
            file=sys.stderr,
        )
        return 1
    print(
        f"PARITY OK: {len(pairs)} build gate(s) spelled identically in ci.yaml, and all "
        f"{len(judgments)} judgment(s) ci.yaml makes are either offered by the script or excused by name"
    )
    return 0


def main(argv):
    if "--parity" in argv:
        return check_parity()
    return check_pin()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

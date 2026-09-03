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
in `.github/workflows/ci.yaml`. A gate the script runs that CI does not is a lie to whoever runs
it; a `run:` line in CI that the script does not offer is a gate nobody can run before pushing,
which is the whole reason the first job was red for an unknown length of time.

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
    print(f"PIN OK: {steps} toolchain step(s) in {len(files)} workflow file(s) all install {chan}")
    return 0


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


def check_parity():
    print("== PARITY: every build gate is spelled the same way in ci.yaml ==")
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

    missing = []
    for name, cmd in pairs:
        if name in NOT_IN_CI:
            print(f"  {name:<16} not in CI by design: {NOT_IN_CI[name]}")
            continue
        # WHOLE-command comparison, not `in`. `cargo doc --no-deps` is a SUBSTRING of
        # `cargo doc --no-deps --document-private-items`, so a substring test said "found" while
        # CI ran a different command -- a parity check that cannot see a drift is not a parity
        # check. Both spellings of a script gate (`bash x.sh` and `x.sh`) count as the same one.
        forms = {cmd, cmd[len("bash ") :] if cmd.startswith("bash ") else "bash " + cmd}
        hit = forms & ci_cmds
        if hit:
            print(f"  {name:<16} `{cmd}`  found in ci.yaml")
        else:
            missing.append((name, cmd))
    for name, cmd in missing:
        print(f"  VIOLATION  gate_{name} runs `{cmd}`, which does not appear in ci.yaml", file=sys.stderr)
    print(f"  checked {len(pairs)} build gate(s) against {os.path.relpath(ci_path, ROOT)}")
    if missing:
        print(f"PARITY FAILED: {len(missing)} gate(s) are not what CI runs", file=sys.stderr)
        return 1
    print(f"PARITY OK: {len(pairs)} build gate(s) are spelled identically in ci.yaml")
    return 0


def main(argv):
    if "--parity" in argv:
        return check_parity()
    return check_pin()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

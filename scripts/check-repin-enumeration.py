#!/usr/bin/env python3
"""**Which frozen literals must a re-genesis re-pin?** — the enumeration, derived from the tree.

The card asserts "these pins move at a re-genesis, and no others". Nothing checked that, so a
fifth pinned constant added tomorrow would be missed by the freeze and discovered by whoever ran
the wrong build. This makes that violation loud.

**It does not prove "and no others", and no static check can.** It proves something narrower and
still useful: the SET OF FILES on the MISAKA surface carrying a frozen digest literal has not
changed since this baseline was taken. A new such file fails here, on the day it is added, with
the person who added it standing over it. That is the whole claim. A gate that claimed
completeness would be the thing this repository has spent the day finding.

**The reason it exists at all is what the survey turned up: a re-pin is spelled FOUR ways in this
tree, and a scanner keyed on one misses the pins that matter most.**

    "<64 hex>" / "<128 hex>"   transformer ids, params fingerprints
    "<32 hex>"                 the free-prompt goldens, truncated with [..32]
    [u8; 64] = [ 0x.., .. ]    PALW_RC_COURT_E2E_ROOT_BYTES
    Hash64::from_bytes([ .. ]) the genesis hash, merkle root and utxo_commitment

The first pass of this scan read only 64/128-hex strings. It found 2 of the 5 re-pin-bearing
files and reported a clean classification over them — blind to `genesis.rs` (19 `from_bytes`),
`palw_e2e_adjudicability.rs` (3 byte arrays) and the free-prompt goldens' 32-char form. **The two
most consequential pins in the cut were in the two spellings it could not see.** Anyone grepping
the tree by hand before a re-genesis will make the same mistake, which is the practical reason to
have this rather than a checklist.

**Scope.** Only the MISAKA surface: `consensus/core/src/palw_*`, `consensus/core/src/config/`,
`misaka-*`, `misaka-cli/`, `kaspad/src/`. Upstream Kaspa carries 168 further occurrences — hash
KATs, sighash vectors, wallet test vectors — that predate PALW and cannot be a function of a
genesis set. Excluding them is a judgement, stated here rather than hidden in a regex.

**How the baseline was classified.** The five MOVES entries are measured, not assumed: each one
went red in this cut's own run when the derive tree and the class set moved. The rest are
baselined FROZEN on the same evidence — they carry frozen literals and did NOT go red in that
run. That is real evidence and it is not proof: a pin with no test wired to it would sit in the
frozen list looking innocent. Which is the same defect one layer up, and is why this file says so
instead of implying otherwise.

    usage:  python3 scripts/check-repin-enumeration.py            # check
            python3 scripts/check-repin-enumeration.py --list     # print the derived re-pin set
    exit:   0 clean, 2 an unclassified file appeared, 3 a baselined file vanished
"""

import os
import re
import sys

SKIP = {"target", ".git", "node_modules"}

# The four spellings. Keyed by name so the report can say WHICH form it found, because "a digest
# is here" is much less useful to a re-pinner than "a 64-byte array literal is here".
PATTERNS = {
    "hex-string": re.compile(r'"[0-9a-fA-F]{32}"|"[0-9a-fA-F]{64}"|"[0-9a-fA-F]{128}"'),
    "byte-array": re.compile(r"\[u8;\s*(?:16|32|64)\]\s*=\s*\["),
    "from_bytes": re.compile(r"from_bytes\(\s*\["),
}

SCOPE = (
    "consensus/core/src/palw_",
    "consensus/core/src/config/",
    "misaka-",
    "misaka-cli/",
    "kaspad/src/",
)

# The files whose literals MOVE at a re-genesis. Each went red in this cut's own run.
MOVES = {
    "consensus/core/src/config/genesis.rs":
        "the genesis header: hash, hash_merkle_root and the utxo_commitment the premine builds",
    "consensus/core/src/config/params.rs":
        "consensus_params_id per shipped preset, and the genesis bundle the presets carry",
    "consensus/core/src/palw_e2e_adjudicability.rs":
        "PALW_RC_COURT_E2E_ROOT_BYTES, which is itself inside consensus_params_id",
    "consensus/core/src/palw_freeprompt_v3.rs":
        "the free-prompt golden vector ids (job/claim/spend/ticket)",
    "misaka-palw-derive/tests/transformer_id_pin.rs":
        "source_tree_sha256 and the eight transformer_ids, a function of every byte under misaka-palw-derive/src/",
}

# Carries frozen literals; did NOT move when the derive tree and the class set moved.
FROZEN = [
    "consensus/core/src/config/premine.rs",
    "consensus/core/src/config/trusted_checkpoint.rs",
    "consensus/core/src/palw_adversarial.rs",
    "consensus/core/src/palw_attempt_v2.rs",
    "consensus/core/src/palw_attn_court_v1.rs",
    "consensus/core/src/palw_base0_a16.rs",
    "consensus/core/src/palw_base0_profile.rs",
    "consensus/core/src/palw_bisect.rs",
    "consensus/core/src/palw_carriage.rs",
    "consensus/core/src/palw_context_ladder.rs",
    "consensus/core/src/palw_credit.rs",
    "consensus/core/src/palw_credit_batch.rs",
    "consensus/core/src/palw_decode_select_v2.rs",
    "consensus/core/src/palw_derived_v1.rs",
    "consensus/core/src/palw_exposure.rs",
    "consensus/core/src/palw_facts.rs",
    "consensus/core/src/palw_job_panel.rs",
    "consensus/core/src/palw_legs.rs",
    "consensus/core/src/palw_prompt_ids_v1.rs",
    "consensus/core/src/palw_qwen25_profile.rs",
    "consensus/core/src/palw_qwen36_profile.rs",
    "consensus/core/src/palw_registry.rs",
    "consensus/core/src/palw_routing.rs",
    "consensus/core/src/palw_schedule.rs",
    "consensus/core/src/palw_slash.rs",
    "consensus/core/src/palw_state_v2.rs",
    "consensus/core/src/palw_step.rs",
    "consensus/core/src/palw_step_leg.rs",
    "consensus/core/src/palw_step_refute.rs",
    "consensus/core/src/palw_v2.rs",
    "kaspad/src/compute.rs",
    "kaspad/src/palw_fp_seat.rs",
    "kaspad/src/validator_service.rs",
    "misaka-cli/src/key_roles.rs",
    "misaka-cli/src/keys.rs",
    "misaka-cli/src/main.rs",
    "misaka-cli/src/palw_court.rs",
    "misaka-cli/src/palw_derived.rs",
    "misaka-cli/src/prea.rs",
    "misaka-palw-base0-ref2/src/gemmlowp.rs",
    "misaka-palw-base0/src/artifact.rs",
    "misaka-palw-base0/src/classes.rs",
    "misaka-palw-base0/src/engine.rs",
    "misaka-palw-base0/src/fuzz_a16.rs",
    "misaka-palw-base0/src/fuzz_qwen36.rs",
    "misaka-palw-base0/src/kat.rs",
    "misaka-palw-base0/tests/a16_root_probe.rs",
    "misaka-palw-derive/src/derive.rs",
    "misaka-palw-derive/src/kinds/cad.rs",
    "misaka-palw-derive/src/kinds/code.rs",
    "misaka-palw-derive/src/kinds/image.rs",
    "misaka-palw-derive/src/kinds/map.rs",
    "misaka-palw-derive/src/kinds/music.rs",
    "misaka-palw-derive/src/kinds/scene.rs",
    "misaka-palw-derive/src/kinds/simulation.rs",
    "misaka-palw-derive/src/registry.rs",
    "misaka-palw-derive/src/source_tree.rs",
    "misaka-palw-derive/tests/evm_runner_gate.rs",
    "misaka-palw-pow-driver/tests/palw_agent_concurrency.rs",
    "misaka-palw-pow-driver/tests/palw_agent_equivalence.rs",
    "misaka-palw-pow-driver/tests/palw_agent_fallback.rs",
    "misaka-palw-pow-driver/tests/palw_agent_recovery.rs",
    "misaka-palw-reexecutor/src/lib.rs",
    "misaka-palw-shadow/src/main.rs",
    "misaka-palw/src/agent_client.rs",
    "misaka-palw/src/lib.rs",
]


def in_scope(path):
    return path.startswith(SCOPE)


def scan():
    """Every in-scope file carrying a frozen literal, with the spellings found in it."""
    found = {}
    for root, dirs, files in os.walk("."):
        dirs[:] = [d for d in dirs if d not in SKIP and not d.startswith(".")]
        for f in files:
            if not f.endswith(".rs"):
                continue
            path = os.path.join(root, f)[2:]
            if not in_scope(path):
                continue
            try:
                src = open(path, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            spellings = {name: len(rx.findall(src)) for name, rx in PATTERNS.items()}
            spellings = {k: v for k, v in spellings.items() if v}
            if spellings:
                found[path] = spellings
    return found


def main(argv):
    found = scan()
    baseline = set(MOVES) | set(FROZEN)

    if "--list" in argv:
        print("The re-pin set this tree declares — every one of these must be re-pinned at a")
        print("re-genesis, and the reason is why it moves rather than that it is a digest:\n")
        for path in sorted(MOVES):
            forms = found.get(path, {})
            forms = ", ".join(f"{n}x {k}" for k, n in sorted(forms.items())) or "NOT FOUND IN TREE"
            print(f"  {path}\n      {MOVES[path]}\n      [{forms}]")
        return 0

    print(f"scanned: {len(found)} in-scope files carrying frozen literals "
          f"({sum(sum(v.values()) for v in found.values())} occurrences, three spellings)")
    print(f"declared: {len(MOVES)} MOVES + {len(FROZEN)} FROZEN = {len(baseline)}")

    # An UNCLASSIFIED file is the failure this exists for: a frozen literal appeared somewhere
    # nobody decided about, so the re-pin procedure does not know whether to touch it.
    unclassified = sorted(set(found) - baseline)
    # A VANISHED file is not a safety failure, but a baseline that quietly stops matching the tree
    # is how a checker drifts into agreeing with everything.
    vanished = sorted(baseline - set(found))

    for path in unclassified:
        forms = ", ".join(f"{n}x {k}" for k, n in sorted(found[path].items()))
        print(f"  UNCLASSIFIED  {path}  [{forms}]", file=sys.stderr)
    for path in vanished:
        print(f"  VANISHED      {path}  (baselined, no frozen literal found now)", file=sys.stderr)

    if unclassified:
        print(f"\n{len(unclassified)} file(s) carry a frozen literal that nobody classified. Decide "
              f"whether each moves at a re-genesis and add it to MOVES or FROZEN — the point is "
              f"that the decision is made by whoever added the constant, not by whoever runs the "
              f"re-genesis at 3am.", file=sys.stderr)
        return 2
    if vanished:
        print(f"\n{len(vanished)} baselined file(s) no longer carry a frozen literal. Remove them "
              f"from the baseline so this keeps measuring the tree rather than a memory of it.",
              file=sys.stderr)
        return 3
    print(f"\nok: every in-scope frozen literal sits in a file someone classified. "
          f"{len(MOVES)} files must be re-pinned at a re-genesis (--list names them).")
    print("This does NOT prove the re-pin set is complete; it proves it has not silently grown.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

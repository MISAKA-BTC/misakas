#!/usr/bin/env python3
"""**The testnet-12 re-pin, mechanical** — the engine behind `scripts/t12-repin.sh` (run that).

Every value the shipping commit may move is pinned somewhere in the tree as a literal a human once
pasted: the genesis header in `genesis.rs`, the preset fingerprints in `params.rs`, the "only this
fence moved it" twins, the v22 goldens, the t11 parity dump, the deploy kit's and the explorer's copies,
the launch checklist's §3. This reads every one of them (the REGISTRY below: file, anchor, spelling),
reads the value this build COMPUTES for it (the REPIN lines `consensus/core/tests/t12_repin_values.rs`
prints, and the lines the pin tests themselves print before they assert), and sets them side by side.

    t12-repin.sh [--shipping]                  dry run: "pinned vs computed" for every pin
    t12-repin.sh --apply [--shipping] --reason "<why>"
                                               rewrite the drifted t12 / mainnet / layout pins in place,
                                               one reason comment each, then run every pin test
    t12-repin.sh --selftest                    the decision rules on the last harvest logs (no build)

Exit: 0 clean; 1 (dry run) only drift a rewrite fixes; 3 anything a rewrite cannot fix — a REFUSE, a pin
NOT COMPUTED or NOT FOUND, a CHECK, an UNREGISTERED literal — whatever else drifted (--apply stops there,
in any round); 4 the harvest build failed; 5 applied, but a pin test failed or the rounds did not converge.

`--shipping` declares the shipping tree: the pin files and gate tests a step-1 merge brings must be there
(otherwise they are "absent", not an error), and `--apply --shipping` relabels the documents' copy blocks
as the shipping values (the dry run with --shipping reports a still-provisional label as a CHECK).

**--apply runs in rounds, inputs first.** A genesis constant is not only a pin, it is an INPUT: the params
ids hash `genesis.hash`, so a fingerprint, a twin or a copy computed while the constant is stale is
computed over the wrong genesis. So a round re-pins only the lowest stage that drifted (stage 0: the
genesis constants), rebuilds, and harvests again; the next round re-pins the rest; the last round is a
harvest with no drift (the confirming dry run). A value that moves every time it is re-pinned stops it
after MAX_ROUNDS.

**What it will not move.** A testnet-11, testnet-10, devnet or simnet pin that drifts is a build that
changed a network it must not change: the tool refuses the whole --apply and names the test that would
fail. Two classes sit between:

* **t11-layout** — testnet-11/dormant goldens that hash a v22 state root (the t11 dormant-parity dump,
  the dormant ring root, the t11 verdict roots). They move when the v22 LAYOUT moves and never
  otherwise. The tool re-pins one only when (a) a v22 layout golden moved in the same run, (b) every
  non-root value beside it held — the dump with its roots masked, its length and line count; the lock
  and collaterals beside the verdict roots — and (c) `--allow-t11-layout` was given, after the audit
  signed off what the run lists (the ring and verdict guards cannot tell a layout move from a t11 fold
  change the way the masked dump can). Otherwise it refuses.
* **verdict** — ADR-0150's stored-corpus verdict digest is network-independent (a stateless gate): it
  moves only with a raised ruleset revision, so it is re-pinned only with `--allow-verdict` AND a rule
  manifest digest that moved.

The values are never typed: each comes from a build of THIS tree. Pins in files a step-1 merge brings
(EXPECTED_BY_MERGE: the Activation Pool's and the readiness horizon's `*_is_t12_only.rs`) are reported
absent before that merge; any other missing pin file, or one HEAD's history once had, is NOT FOUND. A
quoted 64/128-hex literal in any `consensus/core/tests/*.rs` that no registry entry covers is
UNREGISTERED and blocks — add an entry (and, if the value is built inside a test, a print before the
assertion) rather than editing by hand. GATES are tests with no literal (the A-held line's v22 root-block
and tail order) that must pass on the shipping tree; they run in the harvest.
"""

from __future__ import annotations

import argparse
import datetime
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
from dataclasses import dataclass

REPO = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
CORE = "kaspa-consensus-core"

MAX_ROUNDS = 4

# Scopes a drift may move; every other scope but `t11-layout` and `verdict` (their own rules, `decide`) refuses.
MOVABLE = {"t12", "mainnet", "layout", "copy"}

# ---------------------------------------------------------------------------------------------------
# The harvest: which cargo runs print the computed values, and how each value is read from them.
# ---------------------------------------------------------------------------------------------------

# Integration targets whose tests print a harvested value, and the tests that print it. A filter runs
# every test of every listed target whose name contains it; the extra ones only add lines.
HARVEST_TARGETS = [
    "t12_repin_values",
    "palw_offence_attribution_is_t12_only",
    "evm_bridge_ledger_is_t12_only",
    "rcore_m5_v22_golden",
    "panel_room_t11_dormant_parity",
    "t12_two_clock_ring",
    "palw_offence_attribution_t11_verdicts",
    "palw_same_fingerprint_same_verdict",
    "palw_readiness_horizon_is_t12_only",
    "palw_exec_maturity_is_t12_only",
    "palw_clock_lead_cap_is_t12_only",
    "t12_mainnet_values_moved_only_these",
]
HARVEST_FILTERS = [
    "print_every_value_the_pins_hold",
    "the_fence_moves_testnet12s_fingerprint_and_nothing_else_did",
    "the_ledger_is_the_only_thing_that_moved_testnet12",
    "t41_the_v22_golden_vectors_empty_and_inhabited",
    "t41_the_v22_record_encodings_are_pinned",
    "below_the_audit_fence_the_panel_room_folds_as_the_parent_did",
    "a_dormant_network_ticks_at_final_and_never_touches_the_ring",
    "t11_the_v1_fold_is_the_fold_it_was",
    "the_stored_corpus_gets_the_verdicts_this_build_pinned",
    "the_horizon_moves_testnet12s_fingerprint_and_nothing_else_did",
    "the_maturity_moves_testnet12s_fingerprint_and_nothing_else_did",
    "the_cap_moves_testnet12s_fingerprint_and_a_never_collapses",
    "the_three_mainnet_values_are_the_only_thing_that_moved_testnet12",
]
# The one lib test whose value is only visible in its own failure message (or, when it passes, is
# the pin itself): run alone, so its result line is unambiguous.
LIB_GOLDEN_TEST = "palw_state_v2::tests::the_version_22_state_root_golden_vectors"

HEX = r"([0-9a-f]+)"
# (regex, keys): each group of a match is stored under the key in the same position.
HARVEST_LINES = [
    (r"testnet-12 without the fence: params (\w+) identity (\w+) schedule (\w+)",
     ["twin.no_attribution.params_id", "twin.no_attribution.identity_id", "twin.no_attribution.schedule_id"]),
    (r"testnet-12 without the fence or the deadline fence: params (\w+) identity (\w+) schedule (\w+)",
     ["twin.no_attribution_no_deadline.params_id", "twin.no_attribution_no_deadline.identity_id",
      "twin.no_attribution_no_deadline.schedule_id"]),
    (r"testnet-12 at the parent without the attribution: params (\w+) identity (\w+) schedule (\w+)",
     ["twin.evm_parent.params_id", "twin.evm_parent.identity_id", "twin.evm_parent.schedule_id"]),
    (r"T41 empty root: " + HEX, ["v22.t41.empty_root"]),
    (r"T41 empty carriage: " + HEX, ["v22.t41.empty_carriage"]),
    (r"T41 inhabited root: " + HEX, ["v22.t41.inhabited_root"]),
    (r"T41 inhabited carriage: " + HEX, ["v22.t41.inhabited_carriage"]),
    (r"t11 dump: (\d+) bytes, (\d+) lines, BLAKE2b-256 " + HEX, ["t11.parity.bytes", "t11.parity.lines", "t11.parity.digest"]),
    (r"t11 dump roots masked: BLAKE2b-256 " + HEX, ["t11.parity.masked"]),
    (r"dormant root = " + HEX, ["dormant.ring_root"]),
    (r"licensed root (\w+) lock (\d+) collateral (\d+)",
     ["t11.verdicts.root_licensed", "t11.verdicts.lock", "t11.verdicts.collateral_before"]),
    (r"after the V1 conviction root (\w+) collateral (\d+)", ["t11.verdicts.root_after", "t11.verdicts.collateral_after"]),
    (r"the next block empty root (\w+)", ["t11.verdicts.root_empty_next"]),
    (r"corpus verdict digest (\w+) \(", ["corpus.verdict_digest"]),
    (r"a version-22 root moved: empty (\w+) \(want \w+\); inhabited (\w+) \(want", ["v22.state.empty", "v22.state.full"]),
    # `palw_readiness_horizon_is_t12_only` prints both id triples (`{:?}` of a String tuple) before its asserts.
    (r'testnet-12 with the horizon \("(\w+)", "(\w+)", "(\w+)"\) / without \("(\w+)", "(\w+)", "(\w+)"\)',
     ["twin.with_horizon.params_id", "twin.with_horizon.identity_id", "twin.with_horizon.schedule_id",
      "twin.no_horizon.params_id", "twin.no_horizon.identity_id", "twin.no_horizon.schedule_id"]),
    # `palw_exec_maturity_is_t12_only` likewise (the execution-quantum maturity, user decision 2026-09-25).
    (r'testnet-12 with the maturity \("(\w+)", "(\w+)", "(\w+)"\) / without the maturity \("(\w+)", "(\w+)", "(\w+)"\)',
     ["twin.with_maturity.params_id", "twin.with_maturity.identity_id", "twin.with_maturity.schedule_id",
      "twin.no_maturity.params_id", "twin.no_maturity.identity_id", "twin.no_maturity.schedule_id"]),    # `palw_clock_lead_cap_is_t12_only` (the beat lead cap, 2026-09-25) prints each triple on its own line.
    (r'testnet-12 without it:\s+\("(\w+)", "(\w+)", "(\w+)"\)',
     ["twin.no_cap.params_id", "twin.no_cap.identity_id", "twin.no_cap.schedule_id"]),
    # `t12_mainnet_values_moved_only_these` (the mainnet values, 2026-09-25): testnet-12 with the three values set back.
    (r"testnet-12 mainnet-values parent twin: params (\w+) identity (\w+) schedule (\w+)",
     ["twin.mnv_parent.params_id", "twin.mnv_parent.identity_id", "twin.mnv_parent.schedule_id"]),
]
T41_RECORD = re.compile(r"T41 record (\w+)[^:\n]*: ([0-9a-f]{64})")
REPIN_LINE = re.compile(r"REPIN (\S+) (\S+)")


def harvest_text(text: str) -> dict[str, str]:
    got: dict[str, str] = {}
    for m in REPIN_LINE.finditer(text):
        got[m.group(1)] = m.group(2)
    for pattern, keys in HARVEST_LINES:
        for m in re.finditer(pattern, text):
            for key, value in zip(keys, m.groups()):
                got[key] = value
    for m in T41_RECORD.finditer(text):
        got[f"v22.t41.record.{m.group(1)}"] = m.group(2)
    return got


def file_facts() -> dict[str, str]:
    """Facts read from committed files, NOT from the build: the class manifests (so the transcription
    tests in `class_manifest_const_v1` keep a second route to the value) and the sidecar's sha256."""
    got = {}
    for tag, name in [("8k", "qwen25-1.5b-a16-8k"), ("2m", "qwen25-1.5b-a16-2m")]:
        path = os.path.join(REPO, "consensus/core/src/config/class-manifests", name + ".palwmanifest")
        raw = open(path, "rb").read()
        doc = json.loads(raw)
        got[f"manifest.{tag}.artifact_digest"] = doc["artifact_digest"]
        got[f"manifest.{tag}.artifact_bytes"] = str(doc["artifact_bytes"])
        got[f"manifest.{tag}.class_id"] = doc["classes"][0]["class_id"]
        got[f"manifest.{tag}.inventory_root"] = doc["classes"][0]["inventory_root"]
        got[f"manifest.{tag}.sha256"] = hashlib.sha256(raw).hexdigest()
    return got


# ---------------------------------------------------------------------------------------------------
# The registry.
# ---------------------------------------------------------------------------------------------------

@dataclass
class Pin:
    key: str                 # unique name, printed in the report
    scope: str               # t12 | mainnet | layout | copy | t11-layout | verdict | testnet-11 | ... | history
    file: str                # repo-relative
    anchors: list            # regexes, applied in order; the first must match exactly once in the file
    value: object            # computed key (str), or a function of the computed dict, or None (history)
    nth: int = 0             # which literal after the last anchor
    fmt: str = "hex"         # hex | bytes | token | int | abbr
    length: int = 128        # hex length (hex / token)
    until: str | None = None # the literal must precede the first match of this after the first anchor
    tests: tuple = ()        # "target::test" named in a refusal, and run after a rewrite
    guards: tuple = ()       # t11-layout: pin keys that must all hold
    comment: str | None = "auto"  # comment syntax for the reason line; None = no comment (tables, prose)
    group: str | None = None # pins sharing a group share one reason comment (default: the line of the last anchor)
    stage: int = 1           # 0 = an INPUT other computed values hash (a genesis constant): re-pinned, rebuilt, re-read first
    note: str = ""


def _rows(prefix, scope_of, file, const, value_of, tests, until=r"^\];"):
    """A `&[(network, params, identity, schedule)]` table: one pin per id per row."""
    out = []
    for net in ["testnet-11", "devnet", "mainnet"]:
        scope = scope_of(net)
        for i, what in enumerate(["params_id", "identity_id", "schedule_id"]):
            out.append(Pin(
                key=f"{prefix}.{net}.{what}", scope=scope, file=file,
                anchors=[re.escape(const), rf'"{re.escape(net)}",'], nth=i, length=64, until=until,
                value=value_of(net, what) if scope != "history" else None, tests=tests))
    return out


def _net_scope(net):
    return {"testnet-11": "testnet-11", "devnet": "devnet", "mainnet": "mainnet"}[net]


def _shipped(net, what):
    return f"shipped.{net}.{what}"


def _triple(prefix, scope, file, const, value_prefix, tests, until=r"^\);"):
    return [Pin(key=f"{prefix}.{what}", scope=scope, file=file, anchors=[re.escape(const)], nth=i, length=64, until=until,
                value=f"{value_prefix}.{what}", tests=tests)
            for i, what in enumerate(["params_id", "identity_id", "schedule_id"])]


def _genesis(const, net, scope):
    return [Pin(key=f"genesis.{net}.{field_}", scope=scope, file="consensus/core/src/config/genesis.rs",
                anchors=[rf"^pub const {const}: GenesisBlock = GenesisBlock \{{", rf"\b{field_}: Hash64::from_bytes\("],
                until=r"^\};", fmt="bytes", value=f"genesis.{net}.{field_}", group=f"genesis.{net}", stage=0,
                tests=("lib::config::genesis::tests::test_genesis_hashes",
                       "lib::config::genesis::tests::every_genesis_commits_to_the_premine_this_build_mints"))
            for field_ in ["hash", "hash_merkle_root", "utxo_commitment"]]


RCORE = "consensus/core/tests/palw_rcore_plus_is_t12_only.rs"
FLOOR = "consensus/core/tests/palw_clock_floor_is_t12_only.rs"
ATTR = "consensus/core/tests/palw_offence_attribution_is_t12_only.rs"
DEADLINE = "consensus/core/tests/palw_class_verify_deadline_is_t12_only.rs"
POOL = "consensus/core/tests/palw_activation_pool_is_t12_only.rs"
HORIZON = "consensus/core/tests/palw_readiness_horizon_is_t12_only.rs"
MATURITY = "consensus/core/tests/palw_exec_maturity_is_t12_only.rs"
CAP = "consensus/core/tests/palw_clock_lead_cap_is_t12_only.rs"
MNV = "consensus/core/tests/t12_mainnet_values_moved_only_these.rs"
EVM = "consensus/core/tests/evm_bridge_ledger_is_t12_only.rs"
RELEASE = "consensus/core/tests/palw_the_release_did_not_move.rs"
PARAMS = "consensus/core/src/config/params.rs"
GOLDEN = "consensus/core/tests/rcore_m5_v22_golden.rs"
STATE = "consensus/core/src/palw_state_v2.rs"
PARITY = "consensus/core/tests/panel_room_t11_dormant_parity.rs"
RING = "consensus/core/tests/t12_two_clock_ring.rs"
VERDICTS = "consensus/core/tests/palw_offence_attribution_t11_verdicts.rs"
CORPUS = "consensus/core/tests/palw_same_fingerprint_same_verdict.rs"
REGEN = "consensus/core/tests/t12_regenesis.rs"
MANIFEST = "consensus/core/src/config/class_manifest_const_v1.rs"
FLEET = "contrib/t12-deploy-kit/fleet.env.example"
KITLIB = "contrib/t12-deploy-kit/lib.sh"
PLAN = "contrib/t12-deploy-kit/PLAN.md"
APP = "contrib/misakascan-t12/app.js"
SCAN_DEPLOY = "contrib/misakascan-t12/DEPLOY.md"
CHECKLIST = "docs/t12-rcore-launch-checklist.md"
REGEN_DOC = "docs/testnet-12-regenesis-2026-09-23.md"
JOIN_DOC = "docs/testnet12-join-mining.md"

# Pin files a step-1 merge brings (checklist §4 steps 1–3). Before that merge their rows are "absent"
# (reported, not an error); once this tree's history has had the file, or with --shipping, a missing
# file is NOT FOUND — a pin file that was renamed or deleted must not read as "no drift".
EXPECTED_BY_MERGE = {
    POOL: "feat/t12-activation-pool (with feat/t12-readiness-horizon)",
    HORIZON: "feat/t12-readiness-horizon",
}

# Tests with NO literal that must pass on the shipping tree (checklist §5, item 4): the A-held line's
# pins of the v22 root-block order (`rcore_plus/v1` < `activation_pool/v1` < `held_forfeits/v1`) and
# carriage-tail order (0xB4 < 0xB5 < 0xB6), read off the source, and of the delta entries 76–79 it
# reserves for the Activation Pool (which the pool merge must reconcile). They run in the harvest's lib
# run once the source has them, and a failure is a CHECK (the dry run exits 3, --apply refuses). Absent
# before the A-held merge; with --shipping, absent is a CHECK too.
GATES = [
    ("palw_state_v2::tests::held_forfeits_v1::the_held_forfeits_block_and_tail_come_after_r_core_plus_and_the_activation_pool",
     STATE, "feat/t12-aheld-node"),
    ("palw_state_v2::tests::held_forfeits_v1::the_held_forfeit_entry_applies_reverts_and_the_placeholders_are_refused",
     STATE, "feat/t12-aheld-node"),
]

# The one phrase that says whose values a document's copy block holds, in either form. `--apply` rewrites
# the commit in it (files it re-pinned); `--apply --shipping` turns every one into the shipping form.
LABEL = re.compile(r"(?:([0-9a-f]{8,12}) での暫定値（出荷値ではない）|出荷 commit（([0-9a-f]{8,12}) ＋ 再 pin）の値)")
LABEL_ANCHOR = r"\*\*(?=" + LABEL.pattern + r"\*\*)"


def label_text(head: str, shipping: bool) -> str:
    return f"出荷 commit（{head[:8]} ＋ 再 pin）の値" if shipping else f"{head[:8]} での暫定値（出荷値ではない）"


def t41_record_names() -> list[str]:
    """The records `t41_the_v22_record_encodings_are_pinned` pins, in its `want` order, by each literal's
    trailing `// <Record>` comment (a literal without one stays UNREGISTERED)."""
    path = os.path.join(REPO, GOLDEN)
    if not os.path.exists(path):
        return []
    text = open(path, encoding="utf-8").read()
    fn = re.search(r"fn t41_the_v22_record_encodings_are_pinned\(\)", text)
    want = re.compile(r"let want = \[", re.M).search(text, fn.end()) if fn else None
    end = re.compile(r"^\s*\];", re.M).search(text, want.end()) if want else None
    if not end:
        return []
    return re.findall(r'"[0-9a-f]{64}",\s*//\s*(\w+)\s*$', text[want.end():end.start()], re.M)


def t41_want_count() -> int:
    path = os.path.join(REPO, GOLDEN)
    if not os.path.exists(path):
        return 0
    text = open(path, encoding="utf-8").read()
    fn = re.search(r"fn t41_the_v22_record_encodings_are_pinned\(\)", text)
    want = re.compile(r"let want = \[", re.M).search(text, fn.end()) if fn else None
    end = re.compile(r"^\s*\];", re.M).search(text, want.end()) if want else None
    return len(HEXLIT[64].findall(text, want.end(), end.start())) if end else 0


def _single(key):
    """A computed value that must be exactly one 128-hex id (a one-element list printed joined)."""
    def of(c):
        v = c.get(key)
        return v if v and re.fullmatch(r"[0-9a-f]{128}", v) else None
    return of


def registry() -> list[Pin]:
    pins: list[Pin] = []
    # ---- t11 / devnet / mainnet: the presets that do not arm the t12 fences --------------------------
    rcore_tests = (f"{RCORE}::every_other_preset_moves_only_by_the_v22_version_re_pin",)
    pins += _rows("rcore.AT_V22", _net_scope, RCORE, "const AT_V22: &[(&str, &str, &str, &str)] = &[", _shipped, rcore_tests)
    # AT_V21 is history (f1192685's v21 ids) — except its mainnet row, which the same test asserts
    # EQUAL to AT_V22's (mainnet carries no V2 bundle): it follows mainnet when mainnet moves.
    pins += _rows("rcore.AT_V21", lambda n: "mainnet" if n == "mainnet" else "history", RCORE,
                  "const AT_V21: &[(&str, &str, &str, &str)] = &[", _shipped, rcore_tests)
    pins += _rows("floor.BEFORE_THE_FLOOR", _net_scope, FLOOR, "const BEFORE_THE_FLOOR: &[(&str, &str, &str, &str)] = &[", _shipped,
                  (f"{FLOOR}::testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_floor",))
    pins += _rows("attribution.BEFORE_THE_ATTRIBUTION", _net_scope, ATTR,
                  "const BEFORE_THE_ATTRIBUTION: &[(&str, &str, &str, &str)] = &[", _shipped,
                  (f"{ATTR}::testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_attribution_fence",))
    pins += _rows("deadline.BEFORE_THE_DEADLINE", _net_scope, DEADLINE, "const BEFORE_THE_DEADLINE: &[(&str, &str, &str, &str)] = &[",
                  _shipped,
                  (f"{DEADLINE}::testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_fence",))
    pins += _rows("pool.BEFORE_THE_POOL", _net_scope, POOL, "const BEFORE_THE_POOL: &[(&str, &str, &str, &str)] = &[", _shipped,
                  (f"{POOL}::testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_pool",))
    pins += _rows("horizon.BEFORE_THE_HORIZON", _net_scope, HORIZON, "const BEFORE_THE_HORIZON: &[(&str, &str, &str, &str)] = &[",
                  _shipped, (f"{HORIZON}::testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_horizon",))
    pins += _rows("maturity.UNMOVED", _net_scope, MATURITY, "const UNMOVED: &[(&str, &str, &str, &str)] = &[",
                  _shipped, (f"{MATURITY}::testnet11_devnet_and_mainnet_fingerprint_as_they_did_before_the_maturity",))
    rel = (f"{RELEASE}::the_shipped_release_fingerprint_did_not_move",)
    for const, what in [("T11_CONSENSUS_PARAMS_ID", "params_id"), ("T11_CONSENSUS_IDENTITY_ID", "identity_id"),
                        ("T11_CONSENSUS_SCHEDULE_ID", "schedule_id")]:
        pins.append(Pin(f"release.{const}", "testnet-11", RELEASE, [rf"^const {const}: &str = "], f"shipped.testnet-11.{what}",
                        length=64, tests=rel))
    pins.append(Pin("release.MAINNET_CONSENSUS_PARAMS_ID", "mainnet", RELEASE, [r"^const MAINNET_CONSENSUS_PARAMS_ID: &str = "],
                    "const.mainnet.params_id", length=64, tests=rel))
    # `shipped_presets_have_pinned_fingerprints`: each const materialized through its own `net`.
    fp_test = ("lib::config::params::consensus_params_id_tests::shipped_presets_have_pinned_fingerprints",)
    fp_anchor = r"fn shipped_presets_have_pinned_fingerprints\(\)"
    for name, scope, anchor in [("mainnet", "mainnet", r'\("mainnet", MAINNET_PARAMS, '),
                                ("testnet", "testnet-10", r'\("testnet", TESTNET_PARAMS, '),
                                ("testnet-11", "testnet-11", r'"testnet-11",\s*TESTNET11_PARAMS,'),
                                ("simnet", "simnet", r'\("simnet", SIMNET_PARAMS, '),
                                ("devnet", "devnet", r'\("devnet", DEVNET_PARAMS, ')]:
        pins.append(Pin(f"params.shipped_presets.{name}", scope, PARAMS, [fp_anchor, anchor], f"preset_net.{name}.params_id",
                        length=64, until=r"\.into_iter\(\)", tests=fp_test))

    # ---- every genesis the build mints --------------------------------------------------------------
    pins += _genesis("PALW_T12_GENESIS", "testnet-12", "t12")
    pins += _genesis("GENESIS", "mainnet", "mainnet")
    pins += _genesis("TESTNET_GENESIS", "testnet-10", "testnet-10")
    pins += _genesis("PALW_RC_GENESIS", "testnet-11", "testnet-11")
    pins += _genesis("DEVNET_GENESIS", "devnet", "devnet")
    pins += _genesis("SIMNET_GENESIS", "simnet", "simnet")

    # ---- testnet-12's relational twins: "this fence alone moved testnet-12" -------------------------
    pins += _triple("attribution.T12_BEFORE_THE_ATTRIBUTION", "t12", ATTR, "const T12_BEFORE_THE_ATTRIBUTION: (&str, &str, &str) = (",
                    "twin.no_attribution", (f"{ATTR}::the_fence_moves_testnet12s_fingerprint_and_nothing_else_did",))
    pins += _triple("attribution.T12_BEFORE_THE_ATTRIBUTION_AND_THE_DEADLINE", "t12", ATTR,
                    "const T12_BEFORE_THE_ATTRIBUTION_AND_THE_DEADLINE: (&str, &str, &str) = (",
                    "twin.no_attribution_no_deadline", (f"{ATTR}::the_fence_moves_testnet12s_fingerprint_and_nothing_else_did",))
    pins += _triple("evm.T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION", "t12", EVM,
                    "const T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION: (&str, &str, &str) = (",
                    "twin.evm_parent", (f"{EVM}::the_ledger_is_the_only_thing_that_moved_testnet12",))
    # The readiness-V2 horizon (arrives with feat/t12-readiness-horizon): its twin, and testnet-12's
    # FULL ids — the one code pin of the fingerprint a node announces (the shipped preset's).
    horizon_twin = (f"{HORIZON}::the_horizon_moves_testnet12s_fingerprint_and_nothing_else_did",)
    pins += _triple("horizon.T12_BEFORE_THE_HORIZON", "t12", HORIZON, "const T12_BEFORE_THE_HORIZON: (&str, &str, &str) = (",
                    "twin.no_horizon", horizon_twin)
    pins += _triple("horizon.T12_WITH_THE_HORIZON", "t12", HORIZON, "const T12_WITH_THE_HORIZON: (&str, &str, &str) = (",
                    "shipped.testnet-12", horizon_twin)
    # The execution-quantum maturity (user decision 2026-09-25): its twin, and testnet-12's full ids again.
    maturity_twin = (f"{MATURITY}::the_maturity_moves_testnet12s_fingerprint_and_nothing_else_did",)
    pins += _triple("maturity.T12_WITHOUT_THE_MATURITY", "t12", MATURITY, "const T12_WITHOUT_THE_MATURITY: (&str, &str, &str) = (",
                    "twin.no_maturity", maturity_twin)
    pins += _triple("maturity.T12_WITH_THE_MATURITY", "t12", MATURITY, "const T12_WITH_THE_MATURITY: (&str, &str, &str) = (",
                    "shipped.testnet-12", maturity_twin)
    # The beat lead cap (2026-09-25): the unmoved presets, testnet-10 / simnet as history, and testnet-12's twin.
    cap_tests = (f"{CAP}::every_other_preset_fingerprints_as_it_did_before_the_cap",)
    cap_twin = (f"{CAP}::the_cap_moves_testnet12s_fingerprint_and_a_never_collapses",)
    pins += _rows("cap.BEFORE_THE_CAP", _net_scope, CAP, "const BEFORE_THE_CAP: &[(&str, &str, &str, &str)] = &[", _shipped, cap_tests)
    for const_ in ("TESTNET_10_IDS", "SIMNET_IDS"):
        pins += [Pin(key=f"cap.{const_}.{what}", scope="history", file=CAP, anchors=[re.escape(f"const {const_}: (&str, &str, &str) = (")],
                     nth=i_, length=64, until=r"^\);", value=None, tests=cap_tests)
                 for i_, what in enumerate(["params_id", "identity_id", "schedule_id"])]
    pins += _triple("cap.T12_BEFORE_THE_CAP", "t12", CAP, "const T12_BEFORE_THE_CAP: (&str, &str, &str) = (", "twin.no_cap", cap_twin)
    pins += _triple("cap.T12_WITH_THE_CAP", "t12", CAP, "const T12_WITH_THE_CAP: (&str, &str, &str) = (", "shipped.testnet-12", cap_twin)
    # The mainnet values (2026-09-25): testnet-12 with the three values set back (and the later fences taken away).
    pins += _triple("mnv.PARENT_WITHOUT_THE_ATTRIBUTION", "t12", MNV, "const PARENT_WITHOUT_THE_ATTRIBUTION: (&str, &str, &str) = (",
                    "twin.mnv_parent", (f"{MNV}::the_three_mainnet_values_are_the_only_thing_that_moved_testnet12",))

    # ---- testnet-12's classes ------------------------------------------------------------------------
    held = (f"{REGEN}::the_held_rows_are_the_fleets_classes",)
    pins.append(Pin("regenesis.held_row_2m", "t12", REGEN, [r"fn the_held_rows_are_the_fleets_classes\(\)",
                                                             r"hex\(dense\.shape_profile_id\(\)\),"], "testnet-12.class.2m", tests=held))
    pins.append(Pin("regenesis.held_row_8k", "t12", REGEN, [r"fn the_held_rows_are_the_fleets_classes\(\)",
                                                             r"hex\(narrow\.shape_profile_id\(\)\),"], "testnet-12.class.8k", tests=held))
    # ADR-0152 C7: the const preset's literal of the 2M row's class id. An INPUT of testnet-12's params id
    # (hashed in `consensus_params_id`, checked against the genesis held rows by `validate_palw_v2`), so
    # stage 0: re-pinned and rebuilt before the fingerprint and its copies are read.
    c7 = ("lib::config::params::consensus_params_id_tests::the_t12_c7_list_is_the_2m_row",)
    pins.append(Pin("params.PALW_T12_2M_CLASS_ID_BYTES", "t12", PARAMS, [r"^const PALW_T12_2M_CLASS_ID_BYTES: \[u8; 64\] = "],
                    _single("testnet-12.class.c7"), fmt="bytes", stage=0, tests=c7,
                    note="compared with palw_t12_rcore_conservative_classes_v1(), the derivation"))
    pins.append(Pin("params.c7_doc", "copy", PARAMS, [r"^/// `(?=[0-9a-f]{4,}…[0-9a-f]*`, the 2M row's class id)"],
                    _single("testnet-12.class.c7"), fmt="abbr", comment=None))
    for tag, big, fn in [("2m", "2M", "the_committed_manifest_parses_to_what_it_says"),
                         ("8k", "8K", "the_committed_8k_manifest_parses_to_what_it_says")]:
        t = (f"lib::config::class_manifest_const_v1::tests::{fn}",)
        for what, call in [("inventory_root", rf"inventory_root_of_class\(QWEN25_A16_{big}_MANIFEST_V1, 1\)"),
                           ("class_id", rf"class_id_of_class\(QWEN25_A16_{big}_MANIFEST_V1, 1\)"),
                           ("artifact_digest", rf"artifact_digest_of\(QWEN25_A16_{big}_MANIFEST_V1\)")]:
            pins.append(Pin(f"manifest_const.{tag}.{what}", "t12", MANIFEST, [rf"fn {fn}\(\)", call], f"manifest.{tag}.{what}",
                            tests=t, note="read from the committed sidecar JSON (a second route, not the const fn)"))

    # ---- the v22 goldens: layout (encoding) and the t12 fold ----------------------------------------
    t41 = (f"{GOLDEN}::t41_the_v22_golden_vectors_empty_and_inhabited",)
    anchor = [r"fn t41_the_v22_golden_vectors_empty_and_inhabited\(\)", r"let want = \["]
    pins.append(Pin("t41.empty_root", "layout", GOLDEN, anchor, "v22.t41.empty_root", nth=0, until=r"^\}", tests=t41))
    pins.append(Pin("t41.empty_carriage", "layout", GOLDEN, anchor, "v22.t41.empty_carriage", nth=0, length=64, until=r"^\}",
                    tests=t41))
    pins.append(Pin("t41.inhabited_root", "t12", GOLDEN, anchor, "v22.t41.inhabited_root", nth=1, until=r"^\}", tests=t41,
                    note="a real testnet-12 fold: moves with a rule as well as a layout"))
    pins.append(Pin("t41.inhabited_carriage", "t12", GOLDEN, anchor, "v22.t41.inhabited_carriage", nth=1, length=64,
                    until=r"^\}", tests=t41))
    for record in t41_record_names():
        pins.append(Pin(f"t41.record.{record}", "layout", GOLDEN,
                        [r"fn t41_the_v22_record_encodings_are_pinned\(\)", r"let want = \[",
                         rf'^\s*(?="[0-9a-f]{{64}}",\s*//\s*{re.escape(record)}\s*$)'], f"v22.t41.record.{record}",
                        length=64, until=r"^\}", tests=(f"{GOLDEN}::t41_the_v22_record_encodings_are_pinned",)))
    v22 = (f"lib::{LIB_GOLDEN_TEST}",)
    pins.append(Pin("state_v2.want_empty", "layout", STATE, [r"fn the_version_22_state_root_golden_vectors\(\)", r"let want_empty = "],
                    "v22.state.empty", tests=v22))
    pins.append(Pin("state_v2.want_full", "layout", STATE, [r"fn the_version_22_state_root_golden_vectors\(\)", r"let want_full = "],
                    "v22.state.full", tests=v22))

    # ---- t11 / dormant goldens that hash a v22 root -------------------------------------------------
    parity = (f"{PARITY}::below_the_audit_fence_the_panel_room_folds_as_the_parent_did",)
    pins.append(Pin("parity.PARENT_DUMP_BYTES", "testnet-11", PARITY, [r"^const PARENT_DUMP_BYTES: usize = "], "t11.parity.bytes",
                    fmt="int", tests=parity))
    pins.append(Pin("parity.PARENT_DUMP_LINES", "testnet-11", PARITY, [r"^const PARENT_DUMP_LINES: usize = "], "t11.parity.lines",
                    fmt="int", tests=parity))
    pins.append(Pin("parity.PARENT_DUMP_ROOTS_MASKED_BLAKE2B_256", "testnet-11", PARITY,
                    [r"^const PARENT_DUMP_ROOTS_MASKED_BLAKE2B_256: &str = "], "t11.parity.masked", length=64, tests=parity))
    pins.append(Pin("parity.PARENT_DUMP_BLAKE2B_256", "t11-layout", PARITY, [r"^const PARENT_DUMP_BLAKE2B_256: &str = "],
                    "t11.parity.digest", length=64, tests=parity,
                    guards=("parity.PARENT_DUMP_BYTES", "parity.PARENT_DUMP_LINES", "parity.PARENT_DUMP_ROOTS_MASKED_BLAKE2B_256")))
    pins.append(Pin("ring.DORMANT_GOLDEN_ROOT", "t11-layout", RING, [r"^const DORMANT_GOLDEN_ROOT: &str ="], "dormant.ring_root",
                    tests=(f"{RING}::a_dormant_network_ticks_at_final_and_never_touches_the_ring",),
                    note="its premises (an empty ring, no ring delta) are asserted before it prints"))
    verdict_fold = (f"{VERDICTS}::t11_the_v1_fold_is_the_fold_it_was",)
    for const, key in [("SEAT_LOCK_SOMPI: u128", "t11.verdicts.lock"), ("SEAT_COLLATERAL_BEFORE: u64", "t11.verdicts.collateral_before"),
                       ("SEAT_COLLATERAL_AFTER: u64", "t11.verdicts.collateral_after")]:
        pins.append(Pin(f"t11_verdicts.{const.split(':')[0]}", "testnet-11", VERDICTS, [rf"^const {re.escape(const)} = "], key,
                        fmt="int", tests=verdict_fold))
    guards = ("t11_verdicts.SEAT_LOCK_SOMPI", "t11_verdicts.SEAT_COLLATERAL_BEFORE", "t11_verdicts.SEAT_COLLATERAL_AFTER")
    for const, key in [("ROOT_LICENSED", "t11.verdicts.root_licensed"), ("ROOT_AFTER_V1_CONVICTION", "t11.verdicts.root_after"),
                       ("ROOT_EMPTY_NEXT", "t11.verdicts.root_empty_next")]:
        pins.append(Pin(f"t11_verdicts.{const}", "t11-layout", VERDICTS, [rf"^const {const}: &str ="], key, tests=verdict_fold,
                        guards=guards))

    # ---- ADR-0150: the stored corpus's verdicts (every network) ------------------------------------
    pins.append(Pin("corpus.CORPUS_VERDICT_DIGEST", "verdict", CORPUS, [r"^const CORPUS_VERDICT_DIGEST: &str = "],
                    "corpus.verdict_digest", length=64,
                    tests=(f"{CORPUS}::the_stored_corpus_gets_the_verdicts_this_build_pinned",)))

    # ---- the kit's, the explorer's and the documents' copies of testnet-12 facts -------------------
    kit = (f"consensus/core/tests/t12_deploy_kit_constants.rs::"
           f"the_deploy_kit_and_the_explorer_name_this_builds_classes_and_premine_layout",)
    pins.append(Pin("fleet.CLASS_8K", "copy", FLEET, [r"^CLASS_8K="], "testnet-12.class.8k", fmt="token", length=128, tests=kit))
    pins.append(Pin("fleet.FEE_FLOAT_BASE", "copy", FLEET, [r"^FEE_FLOAT_BASE="], "kit.fee_float_base", fmt="int", tests=kit))
    pins.append(Pin("fleet.ART_8K_BYTES", "copy", FLEET, [r"^ART_8K_BYTES="], "manifest.8k.artifact_bytes", fmt="int", tests=kit))
    pins.append(Pin("fleet.MANIFEST_8K_SHA256", "copy", FLEET, [r"^MANIFEST_8K_SHA256="], "manifest.8k.sha256", fmt="token", length=64,
                    note="probe-identity-local.sh --layout checks it"))
    pins.append(Pin("fleet.HB_ADDR", "copy", FLEET, [r"^HB_ADDR="], "kit.hb_addr", fmt="addr",
                    note="card 0's payout address, the operator's default — confirm before stage"))
    pins.append(Pin("kitlib.CLASS_2M_PREFIX", "copy", KITLIB, [r"^CLASS_2M_PREFIX="], lambda c: c["testnet-12.class.2m"][:8],
                    fmt="token", length=8, tests=kit))
    pins.append(Pin("app.PANEL_BOND_TX", "copy", APP, [r'^const PANEL_BOND_TX = '], "testnet-12.premine_txid", tests=kit))
    for i, what in enumerate(["floor", "8k", "2m"]):
        pins.append(Pin(f"app.LLM_CLASSES.{what}", "copy", APP, [r"^const LLM_CLASSES = \["], f"testnet-12.class.{what}", nth=i,
                        until=r"^\];", tests=kit))
    # The launch checklist §3 (the identity the kit's fleet.env is filled against).
    sec3 = [r"^## 3\. "]
    for name, key, length in [("EXPECT_FP", "from.testnet-12.params_id", 64), ("EXPECT_GENESIS", "genesis.testnet-12.hash", 128),
                              ("PREMINE_TXID", "testnet-12.premine_txid", 128)]:
        pins.append(Pin(f"checklist.{name}", "copy", CHECKLIST, sec3 + [rf"^{name}="], key, fmt="token", length=length,
                        until=r"^## 4\. ", group="checklist.sec3"))
    pins.append(Pin("checklist.schedule_id", "copy", CHECKLIST, sec3 + [r"^# schedule id "], "from.testnet-12.schedule_id", fmt="token",
                    length=64, until=r"^## 4\. ", group="checklist.sec3"))
    pins.append(Pin("checklist.rule_manifest_digest", "copy", CHECKLIST, sec3 + [r"^# rule manifest digest "], "rule_manifest.digest",
                    fmt="token", length=128, until=r"^## 4\. ", group="checklist.sec3"))
    pins.append(Pin("checklist.drill_genesis_salt53", "copy", CHECKLIST, [r"の drill genesis は `"], "drill.testnet-12.salt53.genesis",
                    fmt="abbr", comment=None))
    # The label on each document's copy block ("<commit> での暫定値（出荷値ではない）" / the shipping form).
    for name, file in [("checklist", CHECKLIST), ("plan", PLAN), ("scan_deploy", SCAN_DEPLOY)]:
        pins.append(Pin(f"label.{name}", "label", file, [LABEL_ANCHOR], None, fmt="label", comment=None))
    # PLAN.md §4's provisional line.
    pins.append(Pin("plan.EXPECT_FP", "copy", PLAN, [r"^`EXPECT_FP="], "from.testnet-12.params_id", fmt="token", length=64, comment=None))
    pins.append(Pin("plan.EXPECT_GENESIS", "copy", PLAN, [r"^`EXPECT_FP=", r"`EXPECT_GENESIS="], "genesis.testnet-12.hash", fmt="abbr", comment=None))
    pins.append(Pin("plan.PREMINE_TXID", "copy", PLAN, [r"^`EXPECT_FP=", r"`PREMINE_TXID="], "testnet-12.premine_txid", fmt="abbr", comment=None))
    pins.append(Pin("plan.schedule_id", "copy", PLAN, [r"^`EXPECT_FP=", r"schedule id `"], "from.testnet-12.schedule_id", fmt="abbr",
                    comment=None))
    pins.append(Pin("plan.rule_manifest_digest", "copy", PLAN, [r"^`EXPECT_FP=", r"rule manifest digest `"], "rule_manifest.digest",
                    fmt="abbr", comment=None))
    # The regenesis record's table and the join guide (prose: no reason comment, the commit carries it).
    new_genesis = [r"^## 新 genesis"]
    for row, key in [("genesis hash", "genesis.testnet-12.hash"), ("hash_merkle_root", "genesis.testnet-12.hash_merkle_root"),
                     ("utxo commitment", "genesis.testnet-12.utxo_commitment"), ("premine txid", "testnet-12.premine_txid"),
                     ("community txid", "testnet-12.community_txid")]:
        pins.append(Pin(f"regen_doc.{row.replace(' ', '_')}", "copy", REGEN_DOC, new_genesis + [rf"^\| {re.escape(row)} \| `"], key,
                        fmt="token", length=128, comment=None, until=r"^## "))
    pins.append(Pin("join_doc.genesis", "copy", JOIN_DOC, [r"The genesis is `"], "genesis.testnet-12.hash", fmt="abbr", comment=None))
    pins.append(Pin("join_doc.premine", "copy", JOIN_DOC, [r"The premine sits on `"], "testnet-12.premine_txid", fmt="abbr",
                    comment=None))
    pins.append(Pin("scan_deploy.genesis", "copy", SCAN_DEPLOY, [r"\*\*: genesis `"], "genesis.testnet-12.hash", fmt="abbr",
                    comment=None))
    pins.append(Pin("scan_deploy.fp", "copy", SCAN_DEPLOY, [r"\*\*: genesis `", r"、fp `"], "from.testnet-12.params_id", fmt="abbr",
                    comment=None))
    pins.append(Pin("scan_deploy.PANEL_BOND_TX", "copy", SCAN_DEPLOY, [r"固有の premine txid `"], "testnet-12.premine_txid", fmt="abbr",
                    comment=None))
    return pins


def scanned_files() -> list[str]:
    """Quoted 64/128-hex literals in these files are pins; any not located by a registry entry is
    UNREGISTERED (the dry run exits 3, --apply refuses). Every consensus-core integration test file —
    a glob, so a pin file a merge adds cannot be invisible — plus the class-manifest transcription."""
    tests = glob.glob(os.path.join(REPO, "consensus/core/tests/*.rs"))
    return sorted({os.path.relpath(t, REPO) for t in tests} | {MANIFEST})


_HISTORY: dict[str, bool] = {}


def in_history(file: str) -> bool:
    """Whether HEAD's history ever had `file` (a pin file that is missing now was renamed or deleted)."""
    if file not in _HISTORY:
        p = subprocess.run(["git", "log", "-1", "--format=%h", "HEAD", "--", file], cwd=REPO, capture_output=True, text=True)
        _HISTORY[file] = bool(p.stdout.strip())
    return _HISTORY[file]

# ---------------------------------------------------------------------------------------------------
# Locating and reading a pin.
# ---------------------------------------------------------------------------------------------------

HEXLIT = {n: re.compile(r'"([0-9a-f]{%d})"' % n) for n in (8, 16, 32, 64, 128)}
BYTES = re.compile(r"\[((?:\s*0x[0-9a-fA-F]{2}\s*,){63}\s*0x[0-9a-fA-F]{2}\s*,?\s*)\]")
BYTE = re.compile(r"0x([0-9a-fA-F]{2})")
TOKEN = {n: re.compile(r"([0-9a-f]{%d})(?![0-9a-f])" % n) for n in (8, 16, 32, 64, 128)}
ADDR = re.compile(r"(misaka(?:test|dev|sim)?:[a-z0-9]+)")
INT = re.compile(r"([0-9][0-9_]*)")
ABBR = re.compile(r"([0-9a-f]{4,})…([0-9a-f]*)")


class LocateError(Exception):
    pass


def locate(text: str, pin: Pin):
    """(start, end) of the pin's literal VALUE in `text`, the pinned value (normalized), and the offset
    of the line holding the last anchor (where the reason comment goes)."""
    first = list(re.finditer(pin.anchors[0], text, re.M))
    if len(first) != 1:
        raise LocateError(f"anchor {pin.anchors[0]!r} matches {len(first)} times")
    pos = first[0].end()
    anchor_at = first[0].start()
    limit = len(text)
    if pin.until:
        m = re.compile(pin.until, re.M).search(text, pos)
        if m:
            limit = m.start()
    for a in pin.anchors[1:]:
        m = re.compile(a, re.M).search(text, pos, limit)
        if not m:
            raise LocateError(f"anchor {a!r} not found after {pin.anchors[0]!r}")
        pos, anchor_at = m.end(), m.start()
    line_at = text.rfind("\n", 0, anchor_at) + 1
    if pin.fmt == "hex":
        found = list(HEXLIT[pin.length].finditer(text, pos, limit))
        if len(found) <= pin.nth:
            raise LocateError(f"no {pin.length}-hex literal #{pin.nth} after the anchor")
        m = found[pin.nth]
        return m.start(1), m.end(1), m.group(1), line_at
    if pin.fmt == "label":
        m = LABEL.match(text, pos)
        if not m or m.end() > limit:
            raise LocateError("no copy-block label right after the anchor")
        return m.start(0), m.end(0), m.group(0), line_at
    if pin.fmt == "bytes":
        m = BYTES.match(text, pos)
        if not m:
            raise LocateError("no 64-byte array right after the anchor")
        return m.start(1), m.end(1), "".join(b.lower() for b in BYTE.findall(m.group(1))), line_at
    regex = {"token": TOKEN.get(pin.length), "addr": ADDR, "int": INT, "abbr": ABBR}.get(pin.fmt)
    if regex is None:
        raise LocateError(f"unknown format {pin.fmt}/{pin.length}")
    m = regex.match(text, pos)
    if not m or m.end() > limit:
        raise LocateError(f"no {pin.fmt} value right after the anchor")
    if pin.fmt == "int":
        return m.start(1), m.end(1), m.group(1).replace("_", ""), line_at
    if pin.fmt == "abbr":
        return m.start(0), m.end(0), m.group(0), line_at
    return m.start(1), m.end(1), m.group(1), line_at


def same(pin: Pin, pinned: str, computed: str) -> bool:
    if pin.fmt == "abbr":
        head, tail = pinned.split("…", 1)
        return computed.startswith(head) and computed.endswith(tail)
    return pinned == computed


def render(pin: Pin, old_literal: str, computed: str) -> str:
    """The text that replaces the old literal span (the old layout kept: a byte array's line breaks,
    an integer's digit grouping, an abbreviation's head and tail lengths)."""
    if pin.fmt == "bytes":
        values = iter(computed[i:i + 2] for i in range(0, 128, 2))
        return BYTE.sub(lambda m: "0x" + next(values), old_literal)
    if pin.fmt == "int":
        return f"{int(computed):_}" if "_" in old_literal else computed
    if pin.fmt == "abbr":
        head, tail = old_literal.split("…", 1)
        return computed[:len(head)] + "…" + (computed[-len(tail):] if tail else "")
    return computed


# ---------------------------------------------------------------------------------------------------
# The run.
# ---------------------------------------------------------------------------------------------------

def sh(cmd: list[str], log_path: str) -> tuple[int, str]:
    print(f"  $ {' '.join(cmd)}   > {log_path}", flush=True)
    env = dict(os.environ)
    env.setdefault("CARGO_INCREMENTAL", "0")
    with open(log_path, "w") as log:
        p = subprocess.run(cmd, cwd=REPO, stdout=log, stderr=subprocess.STDOUT, env=env)
    return p.returncode, open(log_path, errors="replace").read()


def compiled(text: str) -> bool:
    return "test result:" in text and "could not compile" not in text


def harvest(log_dir: str, from_logs: list[str] | None) -> tuple[dict[str, str], dict[str, str]]:
    """Every computed value, and notes: `*` a failed build, else test name -> its first panic."""
    notes: dict[str, str] = {}
    if from_logs:
        texts = [open(path, errors="replace").read() for path in from_logs]
    else:
        base = ["cargo", "test", "--locked", "-p", CORE]
        targets = [a for t in HARVEST_TARGETS if os.path.exists(os.path.join(REPO, "consensus/core/tests", t + ".rs"))
                   for a in ("--test", t)]
        _, tests_out = sh(base + ["--no-fail-fast"] + targets + ["--", "--nocapture", "--test-threads=1"] + HARVEST_FILTERS,
                          os.path.join(log_dir, "harvest-tests.log"))
        _, lib_out = sh(base + ["--lib", "--", "--exact", LIB_GOLDEN_TEST, *present_gates(), "--nocapture"],
                        os.path.join(log_dir, "harvest-lib.log"))
        texts = [tests_out, lib_out]
        for what, text in [("the pin tests", tests_out), ("the lib golden", lib_out)]:
            if not compiled(text):
                notes["*"] = f"{what} did not build or run — see {log_dir}"
    computed, panics = parse_harvest(texts)
    notes.update(panics)
    return computed, notes


def present_gates() -> list[str]:
    out = []
    for name, file, _ in GATES:
        path = os.path.join(REPO, file)
        if os.path.exists(path) and f"fn {name.split('::')[-1]}(" in open(path, encoding="utf-8").read():
            out.append(name)
    return out


def parse_harvest(texts: list[str]) -> tuple[dict[str, str], dict[str, str]]:
    text = "\n".join(texts)
    computed = harvest_text(text)
    computed.update(file_facts())
    failed = set(re.findall(r"^    (\S+)$", "\n".join(re.findall(r"^failures:\n((?:    \S+\n)+)", text, re.M)), re.M))
    for name, _, _ in GATES:
        m = re.search(rf"^test {re.escape(name)} \.\.\. (ok|FAILED)", text, re.M)
        if m or name in failed:
            computed[f"gate.{name}"] = "FAILED" if name in failed or (m and m.group(1) == "FAILED") else "ok"
    # The lib golden prints its values only in its failure message; when it passes, its pins hold.
    if "v22.state.empty" not in computed and re.search(re.escape(f"test {LIB_GOLDEN_TEST} ... ok"), text):
        computed["v22.state.empty"] = computed["v22.state.full"] = "=pinned"
    notes = {}
    for m in re.finditer(r"thread '([^']+)' panicked at ([^\n]+)\n([^\n]*)", text):
        notes.setdefault(m.group(1).split("::")[-1], f"{m.group(2)} {m.group(3)}".strip())
    return computed, notes


@dataclass
class Row:
    pin: Pin
    status: str                   # ok | drift | absent | nocompute | locate-error | history | label
    pinned: str = ""
    computed: str = ""
    span: tuple = ()
    line_at: int = -1
    detail: str = ""
    decision: str = ""            # move | refuse | keep
    t11_layout_pending: bool = False  # refused only for want of --allow-t11-layout


def compare(pins: list[Pin], computed: dict[str, str], notes: dict[str, str], shipping: bool = False) -> list[Row]:
    rows = []
    cache: dict[str, str] = {}
    for pin in pins:
        path = os.path.join(REPO, pin.file)
        if not os.path.exists(path):
            arrives = EXPECTED_BY_MERGE.get(pin.file)
            if arrives and not shipping and not in_history(pin.file):
                rows.append(Row(pin, "absent", detail=f"{pin.file} arrives with {arrives} (not merged yet)"))
            else:
                why = ("this tree's history had it: renamed or deleted?" if in_history(pin.file) else
                       f"--shipping: {arrives} must have landed" if arrives else "no such file")
                rows.append(Row(pin, "locate-error", detail=f"{pin.file} does not exist ({why})"))
            continue
        text = cache.setdefault(pin.file, open(path, encoding="utf-8").read())
        try:
            start, end, pinned, line_at = locate(text, pin)
        except LocateError as e:
            rows.append(Row(pin, "locate-error", detail=f"{pin.file}: {e}"))
            continue
        row = Row(pin, "label" if pin.scope == "label" else "history", pinned=pinned, span=(start, end), line_at=line_at)
        rows.append(row)
        if pin.scope in ("history", "label"):
            continue
        try:
            value = pin.value(computed) if callable(pin.value) else computed.get(pin.value)
        except KeyError:
            value = None
        if value == "=pinned":
            value = pinned
        if value is None:
            why = [notes[t.split("::")[-1]] for t in pin.tests if t.split("::")[-1] in notes]
            row.status = "nocompute"
            row.detail = (f"no computed value ({pin.value if isinstance(pin.value, str) else 'derived'})"
                          + (f" — its test panicked first: {why[0]}" if why else ""))
            continue
        row.computed = value
        row.status = "ok" if same(pin, pinned, value) else "drift"
    return rows


def decide(rows: list[Row], allow_verdict: bool, allow_t11_layout: bool = False) -> None:
    by_key = {r.pin.key: r for r in rows}
    layout_moved = [r.pin.key for r in rows if r.pin.scope == "layout" and r.status == "drift"]
    for r in rows:
        if r.status != "drift":
            r.decision = "keep"
            continue
        s = r.pin.scope
        if s in MOVABLE:
            r.decision = "move"
        elif s == "t11-layout":
            broken = [g for g in r.pin.guards if g not in by_key or by_key[g].status != "ok"]
            if broken:
                r.decision = "refuse"
                r.detail = ("a non-root value beside it moved or was not computed (" + ", ".join(broken) +
                            "): what testnet-11 folds changed, not only the v22 layout")
            elif not layout_moved:
                r.decision = "refuse"
                r.detail = "only its roots moved, and no v22 LAYOUT golden moved in this run: a testnet-11 state change"
            elif not allow_t11_layout:
                r.decision = "refuse"
                r.t11_layout_pending = True
                r.detail = ("a v22 layout move (" + ", ".join(layout_moved[:2]) + (", …" if len(layout_moved) > 2 else "") +
                            ") with every non-root value beside it held — it moves only with --allow-t11-layout, after the audit's "
                            "sign-off (AUDIT SIGN-OFF below; checklist §5, item 5)")
            else:
                r.decision = "move"
                r.detail = ("a v22 layout move (" + ", ".join(layout_moved[:2]) + (", …" if len(layout_moved) > 2 else "") +
                            "); every non-root value beside it held; --allow-t11-layout (the audit signed off, checklist §5, item 5)")
        elif s == "verdict":
            manifest = by_key.get("checklist.rule_manifest_digest")
            manifest_moved = manifest is not None and manifest.status == "drift"
            if allow_verdict and manifest_moved:
                r.decision = "move"
                r.detail = "ADR-0150: the rule manifest moved in the same build"
            else:
                r.decision = "refuse"
                r.detail = ("a stateless verdict moved on EVERY network (testnet-11 included). ADR-0150: raise the ruleset's revision in "
                            "PALW_CONSENSUS_RULE_MANIFEST_V1 in the same commit, then re-run with --allow-verdict"
                            + ("" if manifest_moved else " (the rule manifest digest has NOT moved)"))
        else:
            r.decision = "refuse"
            r.detail = f"a {s} pin: this build moved a network the regenesis must not move"


def line_no(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def unregistered(rows: list[Row]) -> list[str]:
    covered: dict[str, set] = {}
    for r in rows:
        if r.span:
            covered.setdefault(r.pin.file, set()).add(r.span[0])
    out = []
    for f in scanned_files():
        path = os.path.join(REPO, f)
        if not os.path.exists(path):
            continue
        text = open(path, encoding="utf-8").read()
        for n in (64, 128):
            for m in HEXLIT[n].finditer(text):
                if m.start(1) not in covered.get(f, set()):
                    out.append(f"{f}:{line_no(text, m.start())}: \"{m.group(1)[:16]}…\"")
    return out


def stale_mentions(moved: list[Row], rows: list[Row]) -> list[str]:
    """Where an old value of a `moved` pin is still written, outside the pins themselves (as `rows`
    locates them now), the tool's own reason comments and the kit's forbidden-genesis list."""
    own: set[tuple[str, int]] = set()
    texts: dict[str, str] = {}
    for r in rows:
        if r.span:
            text = texts.setdefault(r.pin.file, open(os.path.join(REPO, r.pin.file), encoding="utf-8").read())
            own.add((r.pin.file, line_no(text, r.span[0])))
    olds = {}
    for r in moved:
        if r.pin.fmt in ("hex", "bytes", "token") and len(r.pinned) >= 16:
            olds.setdefault(r.pinned[:8], r.pin.key)
    out, seen = [], set()
    for prefix, key in sorted(olds.items()):
        p = subprocess.run(["git", "grep", "-n", "-I", prefix, "--", "docs", "contrib", "consensus", "kaspad", "scripts",
                            ":!scripts/t12_repin.py"], cwd=REPO, capture_output=True, text=True)
        for line in p.stdout.splitlines():
            file, num, body = (line.split(":", 2) + ["", ""])[:3]
            if "/legacy-" in file or (file, int(num or 0)) in own or (file, num) in seen or "re-pin " in body \
                    or "for superseded in" in body or re.fullmatch(r'(FORBIDDEN_GENESIS=)?"?[0-9a-f]{128}"?', body.strip()):
                continue
            seen.add((file, num))
            out.append(f"{file}:{num}: {body.strip()[:150]}   [old {key}]")
    return sorted(out)


def reason_line(pin: Pin, file: str, body: str, inside_code_block: bool) -> str | None:
    if pin.comment is None:
        return None
    if file.endswith((".rs", ".js")):
        return "// " + body
    if file.endswith((".sh", ".example", ".env")):
        return "# " + body
    if file.endswith(".md") and inside_code_block:
        return "# " + body
    return None


REPIN_COMMENT = re.compile(r"[ \t]*(?://|#) re-pin \d{4}-\d{2}-\d{2} @[0-9a-f]+: [^\n]*\n")


def apply(rows: list[Row], reason: str, head: str) -> list[str]:
    """Rewrite each moving pin's literal in place, and put one reason comment above the line of each
    group's last anchor (a table row, a tuple const, a genesis field, a `want` array)."""
    today = datetime.date.today().isoformat()
    by_file: dict[str, list[Row]] = {}
    for r in rows:
        if r.decision == "move":
            by_file.setdefault(r.pin.file, []).append(r)
    for file, rs in sorted(by_file.items()):
        path = os.path.join(REPO, file)
        text = open(path, encoding="utf-8").read()
        groups: dict[object, list[Row]] = {}
        for r in rs:
            groups.setdefault(r.pin.group or r.line_at, []).append(r)
        edits = [(r.span[0], r.span[1], render(r.pin, text[r.span[0]:r.span[1]], r.computed)) for r in rs]
        for grp in groups.values():
            line_at = min(r.line_at for r in grp)
            in_code = file.endswith(".md") and text.count("```", 0, line_at) % 2 == 1
            olds = ", ".join(r.pinned[:8] + "…" for r in sorted(grp, key=lambda r: r.span[0]))
            line = reason_line(grp[0].pin, file, f"re-pin {today} @{head}: {reason} (was {olds})", in_code)
            above_at = text.rfind("\n", 0, max(line_at - 1, 0)) + 1
            above = text[above_at:line_at]
            if line and f"re-pin {today} @{head}: {reason}" in above:
                line = None  # this run already explained this group (an earlier round)
            if line:
                indent = re.match(r"[ \t]*", text[line_at:]).group(0)
                if REPIN_COMMENT.fullmatch(above):
                    # An earlier run's reason for this same group: the new one replaces it (git keeps the old).
                    edits.append((above_at, line_at, indent + line + "\n"))
                else:
                    edits.append((line_at, line_at, indent + line + "\n"))
        # Apply from the end so every offset stays valid; an insert at a line start sorts after the
        # literal edits that begin at the same offset (none do: a literal never starts a line here).
        for s, e, rep in sorted(edits, key=lambda x: (x[0], x[1]), reverse=True):
            text = text[:s] + rep + text[e:]
        open(path, "w", encoding="utf-8").write(text)
    return sorted(by_file)


def superseded_genesis(rows: list[Row], reason: str, head: str) -> list[str]:
    """testnet-12's genesis moved: builds of the old one exist, so the kit forbids it and
    t12_regenesis names it superseded (checklist §4 step 7)."""
    r = next((r for r in rows if r.pin.key == "genesis.testnet-12.hash" and r.decision == "move"), None)
    if r is None:
        return []
    old, touched = r.pinned, []
    path = os.path.join(REPO, FLEET)
    text = open(path, encoding="utf-8").read()
    m = re.search(r'^FORBIDDEN_GENESIS="([0-9a-f\s]*)"', text, re.M)
    if m and old not in m.group(1).split():
        text = (text[:m.start()] + f"#   {old[:8]}… testnet-12's genesis until the re-pin @{head} ({reason}) — builds of it exist\n"
                + text[m.start():m.end(1)] + "\n" + old + text[m.end(1):])
        open(path, "w", encoding="utf-8").write(text)
        touched.append(FLEET)
    path = os.path.join(REPO, REGEN)
    text = open(path, encoding="utf-8").read()
    m = re.search(r"for superseded in \[([^\]]*)\]", text)
    if m and old[:16] not in m.group(1):
        text = text[:m.end(1)] + f', "{old[:16]}"' + text[m.end(1):]
        open(path, "w", encoding="utf-8").write(text)
        touched.append(REGEN)
    return touched


def run_tests(rows: list[Row], extra: set[str], log_dir: str) -> tuple[int, str]:
    """Every pin's test after a rewrite, not only the moved ones' (a merged pin file's other tests, a
    twin whose premise a rewrite changed): each integration target that holds a pin, the lib tests the
    registry names, the A-held gates' module once the source has it, plus `extra` targets."""
    targets, lib_filters = set(extra), set()
    for r in rows:
        for t in r.pin.tests:
            if t.startswith("lib::"):
                lib_filters.add(t[len("lib::"):])
            else:
                targets.add(os.path.splitext(os.path.basename(t.split("::")[0]))[0])
        if r.pin.file.startswith("consensus/core/tests/") and r.pin.file.endswith(".rs"):
            targets.add(os.path.splitext(os.path.basename(r.pin.file))[0])
    lib_filters.add(LIB_GOLDEN_TEST)
    state = os.path.join(REPO, STATE)
    if "mod held_forfeits_v1" in open(state, encoding="utf-8").read():
        lib_filters.add("palw_state_v2::tests::held_forfeits_v1")
    targets = sorted(t for t in targets if os.path.exists(os.path.join(REPO, "consensus/core/tests", t + ".rs")))
    rc, out = 0, ""
    rc1, o = sh(["cargo", "test", "--locked", "-p", CORE, "--lib", "--"] + sorted(lib_filters), os.path.join(log_dir, "verify-lib.log"))
    rc, out = rc or rc1, out + o
    if targets:
        cmd = ["cargo", "test", "--locked", "-p", CORE, "--no-fail-fast"] + [a for t in targets for a in ("--test", t)]
        rc2, o = sh(cmd, os.path.join(log_dir, "verify-tests.log"))
        rc, out = rc or rc2, out + "\n" + o
    return rc, out


def summarize_results(out: str) -> list[str]:
    res = []
    for m in re.finditer(r"Running (?:unittests )?(\S+) \([^)]*\)\n[\s\S]*?test result: (ok|FAILED)\. (\d+) passed; (\d+) failed", out):
        res.append(f"{m.group(2):6} {m.group(1)}  ({m.group(3)} passed, {m.group(4)} failed)")
    for block in re.findall(r"^failures:\n((?:    \S+\n)+)", out, re.M):
        res += [f"FAILED {name.strip()}" for name in block.splitlines()]
    return res


def structural_problems(rows: list[Row], computed: dict[str, str], shipping: bool, check_labels: bool) -> list[str]:
    """What a rewrite cannot fix (a CHECK: the dry run exits 3, --apply refuses). Judged as the tree will
    stand AFTER this run's pending moves, so a copy the rewrite fixes is not also a CHECK."""
    problems = []
    registered = computed.get("testnet-12.class.registered", "")
    if registered:
        on_chain = set(registered.split(",")) | {computed.get("testnet-12.class.floor", "")}
        app = open(os.path.join(REPO, APP), encoding="utf-8").read()
        start = app.find("const LLM_CLASSES = [")
        in_app = set(re.findall(r'id:"([0-9a-f]{128})"', app[start:app.find("];", start)]))
        for r in rows:
            if r.pin.key.startswith("app.LLM_CLASSES.") and r.status == "drift" and r.decision == "move":
                in_app.discard(r.pinned)
                in_app.add(r.computed)
        problems += [f"{APP} LLM_CLASSES names {x[:16]}…, not a genesis class of this build — edit by hand" for x in sorted(in_app - on_chain)]
        problems += [f"{APP} LLM_CLASSES lacks genesis class {x[:16]}… — add its row by hand (name, model, tag)"
                     for x in sorted(on_chain - in_app)]
        order = registered.split(",")
        if order != [computed.get("testnet-12.class.8k"), computed.get("testnet-12.class.2m")]:
            problems.append(f"testnet-12's genesis model classes are {[o[:8] for o in order]}, not [8k, 2M]: "
                            "t12_regenesis::the_held_rows_are_the_fleets_classes and the kit's node tables need a hand edit")
    c7 = computed.get("testnet-12.class.c7")
    if c7 is not None and _single("testnet-12.class.c7")(computed) is None:
        problems.append(f"palw_t12_rcore_conservative_classes_v1() derives {c7!r}, not one class id: PALW_T12_2M_CLASS_ID_BYTES "
                        "(a one-element C7 list) needs a hand edit")
    if computed.get("kit.cards") and computed["kit.cards"] != ",".join(str(i) for i in range(8)):
        problems.append(f"PALW_T12_GENESIS_BONDS declares premine indices {computed['kit.cards']}, not 0..7 in order: the kit's "
                        "`$PREMINE_TXID:N` / FEE_FLOAT_BASE+N naming breaks (t12_deploy_kit_constants)")
    gen = computed.get("genesis.testnet-12.hash")
    m = re.search(r'^FORBIDDEN_GENESIS="([0-9a-f\s]*)"', open(os.path.join(REPO, FLEET), encoding="utf-8").read(), re.M)
    if gen and m and gen in m.group(1).split():
        problems.append(f"{FLEET} FORBIDDEN_GENESIS lists this build's own genesis {gen[:16]}…")
    # T41's records: one printed digest per pinned literal, by name, and one pin per name.
    names = t41_record_names()
    printed = sorted(k[len("v22.t41.record."):] for k in computed if k.startswith("v22.t41.record."))
    want_count = t41_want_count()
    if len(names) != want_count:
        problems.append(f"{GOLDEN}: {want_count} record digests but {len(names)} carry a `// <Record>` comment — name every literal")
    if len(set(names)) != len(names):
        problems.append(f"{GOLDEN}: a record name is pinned twice ({sorted(n for n in set(names) if names.count(n) > 1)})")
    if printed and sorted(names) != printed:
        problems.append(f"{GOLDEN}: the test printed records {sorted(set(printed) - set(names))} with no pinned digest and pins "
                        f"{sorted(set(names) - set(printed))} it did not print — the want array and the records disagree")
    # The literal-free gates (the A-held line's order and placeholder pins).
    present = present_gates()
    for name, _, arrives in GATES:
        if name not in present:
            if shipping:
                problems.append(f"gate {name.split('::')[-1]} is not in this tree (--shipping: {arrives} must have landed, "
                                "or the test was renamed — update GATES)")
            continue
        result = computed.get(f"gate.{name}")
        if result != "ok":
            problems.append(f"gate {name} " + ("FAILED" if result else "did not run") + " (checklist §5, item 4)")
    if check_labels:
        for r in rows:
            if r.status == "label" and not r.pinned.startswith("出荷 commit"):
                problems.append(f"{r.pin.file}: its copy block is still labelled {r.pinned!r} (--shipping: run --apply --shipping)")
    return problems


def relabel(rows: list[Row], head: str, shipping: bool, files: set[str]) -> list[str]:
    """Rewrite each copy block's label: the commit these values were computed at (the files this run
    re-pinned), or with --shipping every label to the shipping form."""
    touched = []
    for r in rows:
        if r.status != "label" or not (shipping or r.pin.file in files):
            continue
        new = label_text(head, shipping)
        if r.pinned == new:
            continue
        path = os.path.join(REPO, r.pin.file)
        text = open(path, encoding="utf-8").read()
        start, end = r.span
        assert text[start:end] == r.pinned, f"{r.pin.file}: the label moved under the run"
        open(path, "w", encoding="utf-8").write(text[:start] + new + text[end:])
        touched.append(r.pin.file)
    return touched


def selftest(log_dir: str) -> int:
    """The decision rules on THIS tree's own harvest logs with one value changed at a time (no build):
    each scenario names the pins that must drift and what the tool must decide for each, and the CHECK
    lines it must raise. A scenario over a pin file this tree does not have yet is skipped."""
    base = ["\n".join(open(os.path.join(log_dir, name), errors="replace").read() for name in ("harvest-tests.log", "harvest-lib.log"))]
    base_computed, _ = parse_harvest(base)

    def flip(v: str) -> str:
        # The FIRST digit, so an abbreviated copy (`a27f8f44…`) sees the move too.
        return ("0" if v[0] != "0" else "1") + v[1:]

    def change(*labels):
        """Change the value printed after each label regex (every occurrence), keeping its length."""
        def mutate(text):
            for (label,) in labels:
                pattern = re.compile("(" + label + r")([0-9a-f]{16,}|\d+)")
                assert pattern.search(text), f"selftest: no {label!r} in the harvest log"
                text = pattern.sub(lambda m: m.group(1) + (str(int(m.group(2)) + 1) if len(m.group(2)) < 16 else flip(m.group(2))), text)
            return text
        return mutate

    def everywhere(key):
        """Change one computed value everywhere the harvest printed it (a moved class id is also in
        the registered-class list)."""
        old = base_computed[key]
        return lambda text: text.replace(old, flip(old))

    def drop(line_regex):
        return lambda text: re.sub(line_regex, "", text, flags=re.M)

    rep_line = lambda key: rf"REPIN {re.escape(key)} "
    layout = {"allow_t11_layout": True}
    gate = GATES[0][0]
    scenarios = [
        ("the tree as built", change(), {}, {}),
        ("t11 dump: only its roots moved, no layout golden moved", change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",)), layout,
         {"parity.PARENT_DUMP_BLAKE2B_256": "refuse"}),
        ("t11 dump roots moved WITH a v22 record encoding, --allow-t11-layout",
         change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",), (r"T41 record PalwVestingRowV1: ",)), layout,
         {"parity.PARENT_DUMP_BLAKE2B_256": "move", "t41.record.PalwVestingRowV1": "move"}),
        ("t11 dump roots moved WITH a v22 record encoding, no flag",
         change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",), (r"T41 record PalwVestingRowV1: ",)), {},
         {"parity.PARENT_DUMP_BLAKE2B_256": "refuse", "t41.record.PalwVestingRowV1": "move"}),
        ("t11 dump: a non-root line moved too (masked digest)", change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",),
                                                                       (r"t11 dump roots masked: BLAKE2b-256 ",),
                                                                       (r"T41 record PalwVestingRowV1: ",)), layout,
         {"parity.PARENT_DUMP_BLAKE2B_256": "refuse", "parity.PARENT_DUMP_ROOTS_MASKED_BLAKE2B_256": "refuse",
          "t41.record.PalwVestingRowV1": "move"}),
        ("t11 verdict roots moved with the lock, and a layout move", change((r"licensed root ",), (r"T41 empty carriage: ",),
                                                                            (r"licensed root \w+ lock ",)), layout,
         {"t11_verdicts.ROOT_LICENSED": "refuse", "t11_verdicts.SEAT_LOCK_SOMPI": "refuse", "t41.empty_carriage": "move"}),
        ("the dormant ring root moved with a layout move, no flag", change((r"dormant root = ",), (r"T41 empty carriage: ",)), {},
         {"ring.DORMANT_GOLDEN_ROOT": "refuse", "t41.empty_carriage": "move"}),
        ("the dormant ring root moved with a layout move, --allow-t11-layout", change((r"dormant root = ",), (r"T41 empty carriage: ",)),
         layout, {"ring.DORMANT_GOLDEN_ROOT": "move", "t41.empty_carriage": "move"}),
        ("the corpus verdicts moved (allowed, but the manifest did not move)", change((r"corpus verdict digest ",)),
         {"allow_verdict": True}, {"corpus.CORPUS_VERDICT_DIGEST": "refuse"}),
        ("testnet-11's params id moved", change((rep_line("shipped.testnet-11.params_id"),), (rep_line("preset_net.testnet-11.params_id"),)),
         {}, {k: "refuse" for k in ["rcore.AT_V22.testnet-11.params_id", "floor.BEFORE_THE_FLOOR.testnet-11.params_id",
                                    "attribution.BEFORE_THE_ATTRIBUTION.testnet-11.params_id",
                                    "deadline.BEFORE_THE_DEADLINE.testnet-11.params_id", "release.T11_CONSENSUS_PARAMS_ID",
                                    "params.shipped_presets.testnet-11", "pool.BEFORE_THE_POOL.testnet-11.params_id",
                                    "horizon.BEFORE_THE_HORIZON.testnet-11.params_id", "maturity.UNMOVED.testnet-11.params_id"]}),
        ("devnet's identity moved", change((rep_line("shipped.devnet.identity_id"),)), {},
         {k: "refuse" for k in ["rcore.AT_V22.devnet.identity_id", "floor.BEFORE_THE_FLOOR.devnet.identity_id",
                                "attribution.BEFORE_THE_ATTRIBUTION.devnet.identity_id", "deadline.BEFORE_THE_DEADLINE.devnet.identity_id",
                                "pool.BEFORE_THE_POOL.devnet.identity_id", "horizon.BEFORE_THE_HORIZON.devnet.identity_id",
                                "maturity.UNMOVED.devnet.identity_id"]}),
        ("mainnet's ruleset moved", change((rep_line("shipped.mainnet.params_id"),), (rep_line("const.mainnet.params_id"),),
                                           (rep_line("preset_net.mainnet.params_id"),)), {},
         {k: "move" for k in ["rcore.AT_V22.mainnet.params_id", "rcore.AT_V21.mainnet.params_id", "floor.BEFORE_THE_FLOOR.mainnet.params_id",
                              "attribution.BEFORE_THE_ATTRIBUTION.mainnet.params_id", "deadline.BEFORE_THE_DEADLINE.mainnet.params_id",
                              "release.MAINNET_CONSENSUS_PARAMS_ID", "params.shipped_presets.mainnet",
                              "pool.BEFORE_THE_POOL.mainnet.params_id", "horizon.BEFORE_THE_HORIZON.mainnet.params_id",
                              "maturity.UNMOVED.mainnet.params_id"]}),
        ("testnet-12's fingerprint and a twin moved", change((rep_line("from.testnet-12.params_id"),),
                                                             (r"testnet-12 without the fence: params ",)), {},
         {k: "move" for k in ["checklist.EXPECT_FP", "plan.EXPECT_FP", "scan_deploy.fp", "attribution.T12_BEFORE_THE_ATTRIBUTION.params_id"]}),
        ("testnet-12's shipped params moved (the horizon's full-ids pin)", change((rep_line("shipped.testnet-12.params_id"),)), {},
         {"horizon.T12_WITH_THE_HORIZON.params_id": "move", "maturity.T12_WITH_THE_MATURITY.params_id": "move"}),
        ("the horizon's twin moved", change((r'/ without \("',)), {}, {"horizon.T12_BEFORE_THE_HORIZON.params_id": "move"}),
        ("the maturity's twin moved", change((r'/ without the maturity \("',)), {},
         {"maturity.T12_WITHOUT_THE_MATURITY.params_id": "move"}),
        ("testnet-12's genesis moved", change((rep_line("genesis.testnet-12.hash"),), (rep_line("genesis.testnet-12.utxo_commitment"),)),
         {}, {k: "move" for k in ["genesis.testnet-12.hash", "genesis.testnet-12.utxo_commitment", "checklist.EXPECT_GENESIS",
                                  "plan.EXPECT_GENESIS", "regen_doc.genesis_hash", "regen_doc.utxo_commitment", "join_doc.genesis",
                                  "scan_deploy.genesis"]}),
        ("the 2M row's class id moved (C7, the held row, the kit and explorer copies; no CHECK)", everywhere("testnet-12.class.2m"), {},
         {k: "move" for k in ["params.PALW_T12_2M_CLASS_ID_BYTES", "params.c7_doc", "regenesis.held_row_2m", "kitlib.CLASS_2M_PREFIX",
                              "app.LLM_CLASSES.2m"]}),
        ("the example drill genesis moved", change((rep_line("drill.testnet-12.salt53.genesis"),)), {},
         {"checklist.drill_genesis_salt53": "move"}),
        ("a twin was never printed (its premise failed first)", lambda t: t.replace("testnet-12 at the parent without the attribution",
                                                                                   "(premise failed)"), {},
         {f"evm.T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION.{w}": "nocompute" for w in ["params_id", "identity_id", "schedule_id"]}),
        ("a T41 record was not printed (its digest pin cannot be read)", drop(r"^T41 record PalwConsumedOffenceV1:.*\n"), {},
         {"t41.record.PalwConsumedOffenceV1": "nocompute", "CHECK": "the want array and the records disagree"}),
        ("an A-held gate failed", lambda t: re.sub(rf"^test {re.escape(gate)} \.\.\. ok", f"test {gate} ... FAILED", t, flags=re.M), {},
         {"CHECK": "FAILED (checklist §5, item 4)"}),
    ]
    requires = {"testnet-12's shipped params moved (the horizon's full-ids pin)": HORIZON, "the horizon's twin moved": HORIZON,
                "the maturity's twin moved": MATURITY,
                "an A-held gate failed": ("gate", gate)}
    failures = skipped = 0
    for name, mutate, opts, want in scenarios:
        need = requires.get(name)
        if (isinstance(need, str) and not os.path.exists(os.path.join(REPO, need))) or \
                (isinstance(need, tuple) and need[1] not in present_gates()):
            skipped += 1
            print(f"SKIP  {name}: {need if isinstance(need, str) else 'the gate'} is not in this tree yet")
            continue
        want = dict(want)
        want_check = want.pop("CHECK", None)
        computed, notes = parse_harvest([mutate(t) for t in base])
        rows = compare(registry(), computed, notes)
        decide(rows, opts.get("allow_verdict", False), opts.get("allow_t11_layout", False))
        got = {r.pin.key: (r.decision if r.status == "drift" else r.status) for r in rows
               if r.status in ("drift", "nocompute", "locate-error")}
        # The pool / horizon rows exist only once their file does: expect them only then.
        want = {k: v for k, v in want.items()
                if not (k.startswith("pool.") and not os.path.exists(os.path.join(REPO, POOL)))
                and not (k.startswith("horizon.") and not os.path.exists(os.path.join(REPO, HORIZON)))}
        problems = structural_problems(rows, computed, False, False)
        ok = got == want and (any(want_check in p for p in problems) if want_check else not problems)
        failures += not ok
        print(f"{'PASS' if ok else 'FAIL'}  {name}: {len(want)} pin(s) " + (", ".join(sorted(set(want.values()))) if want else "none")
              + (f" + CHECK" if want_check else ""))
        if not ok or os.environ.get("REPIN_SELFTEST_VERBOSE"):
            for k in sorted(set(got) | set(want)):
                print(f"        {k:60} got {got.get(k, 'ok'):10} want {want.get(k, 'ok')}")
            for p in problems:
                print(f"        CHECK {p}")
    ran = len(scenarios) - skipped
    print(f"\nselftest: {ran - failures}/{ran} scenarios as expected" + (f" ({skipped} skipped)" if skipped else ""))
    return 1 if failures else 0


def short(v: str) -> str:
    return (v[:16] + "…") if len(v) > 17 else (v or "-")


def main() -> int:
    ap = argparse.ArgumentParser(description="testnet-12 re-pin: pinned vs computed, and --apply")
    ap.add_argument("--apply", action="store_true", help="rewrite the drifted movable pins, then run every pin test")
    ap.add_argument("--reason", help="the one line each rewrite's comment carries (required with --apply)")
    ap.add_argument("--allow-verdict", action="store_true", help="let the ADR-0150 corpus verdict digest move (with the manifest)")
    ap.add_argument("--allow-t11-layout", action="store_true",
                    help="let the testnet-11 goldens that hash a v22 root move with a v22 layout move (after the audit's sign-off)")
    ap.add_argument("--shipping", action="store_true",
                    help="this is the shipping tree: every merge-borne pin file and gate must be present, and (with --apply) "
                         "the copy blocks are labelled as the shipping values")
    ap.add_argument("--from-log", action="append", help="parse these harvest logs instead of running cargo (repeatable)")
    ap.add_argument("--log-dir", default=None, help="cargo logs (default: $CARGO_TARGET_DIR/t12-repin, else target/t12-repin)")
    ap.add_argument("--no-verify", action="store_true", help="with --apply: do not run the pin tests afterwards")
    ap.add_argument("--drift-only", action="store_true", help="print only the rows that are not ok")
    ap.add_argument("--selftest", action="store_true", help="check the decision rules on the last harvest logs (no build)")
    args = ap.parse_args()

    head = subprocess.run(["git", "rev-parse", "--short=12", "HEAD"], cwd=REPO, capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=REPO, capture_output=True, text=True).stdout
    log_dir = args.log_dir or os.path.join(os.environ.get("CARGO_TARGET_DIR", os.path.join(REPO, "target")), "t12-repin")
    os.makedirs(log_dir, exist_ok=True)
    if args.selftest:
        return selftest(log_dir)
    print(f"t12-repin: tree {head}{' + uncommitted changes' if dirty.strip() else ''}; logs in {log_dir}"
          + ("; --shipping" if args.shipping else ""))
    if args.apply and not (args.reason and args.reason.strip()):
        print("--apply needs --reason \"<one line: what moved these pins>\"", file=sys.stderr)
        return 2
    if args.reason and ("\n" in args.reason or "*/" in args.reason):
        print("--reason is one line of plain text", file=sys.stderr)
        return 2

    if args.apply and args.from_log:
        print("--apply rebuilds between rounds; it does not take --from-log", file=sys.stderr)
        return 2
    if not args.apply:
        computed, notes = harvest(log_dir, args.from_log)
        if "*" in notes:
            print(f"\nHARVEST FAILED: {notes['*']}")
            return 4
        rows = compare(registry(), computed, notes, args.shipping)
        decide(rows, args.allow_verdict, args.allow_t11_layout)
        drift, refused, moves, broken, problems, unreg = report(rows, computed, args.drift_only, args.shipping, True)
        if drift:
            print(f"\nDRY RUN: {len(drift)} pin(s) drifted — {len(moves)} movable, {len(refused)} refused.")
            if any(r.pin.stage == 0 for r in moves):
                print("NOTE: a stage-0 input drifted (a genesis constant, C7). Every value that hashes it (the params and identity "
                      "ids, the twins, their copies) is computed above over the constant AS IT STANDS; --apply re-pins it first, "
                      "rebuilds, and recomputes them.")
            stale = stale_mentions(moves, rows)
            if stale:
                print("Other mentions of the moving old values (not rewritten; review by hand):")
                for line in stale:
                    print("  " + line)
        blocking = [f"{len(refused)} REFUSE"] * bool(refused) + [f"{len(broken)} NOT COMPUTED / NOT FOUND"] * bool(broken) + \
                   [f"{len(problems)} CHECK"] * bool(problems) + [f"{len(unreg)} UNREGISTERED"] * bool(unreg)
        if blocking:
            print(f"\nDRY RUN: NOT CLEAN — {', '.join(blocking)} (above). Nothing may be applied until each is resolved.")
            return 3
        if drift:
            print("Re-run with --apply --reason \"…\" to rewrite them.")
            return 1
        print("\nDRY RUN: no drift — every checked pin is this build's value, every gate in this tree passed, no literal is unregistered.")
        return 0

    # ---- --apply: in rounds, inputs first; each round is a fresh build and harvest ----
    reason = args.reason.strip()
    moved: dict[str, Row] = {}
    written: set[str] = set()
    rows: list[Row] = []
    for round_ in range(1, MAX_ROUNDS + 1):
        print(f"\n==== --apply round {round_} ====")
        computed, notes = harvest(log_dir, None)
        if "*" in notes:
            print(f"\nHARVEST FAILED: {notes['*']}" + (f" (already rewritten: {sorted(written)})" if written else ""))
            return 4
        rows = compare(registry(), computed, notes, args.shipping)
        decide(rows, args.allow_verdict, args.allow_t11_layout)
        drift, refused, moves, broken, problems, unreg = report(rows, computed, True, args.shipping, False)
        if refused or broken or problems or unreg:
            print(f"\n--apply REFUSED in round {round_} (REFUSE / NOT COMPUTED / NOT FOUND / CHECK / UNREGISTERED above)." +
                  (f" Rewritten in earlier rounds (review or `git checkout -- <file>`): {sorted(written)}" if written else
                   " Nothing was written."))
            return 3
        if not moves:
            if round_ == 1:
                labels = relabel(rows, head, True, set()) if args.shipping else []
                print("\n--apply: nothing drifted; nothing re-pinned." +
                      (f" Labelled the copy blocks as the shipping values: {sorted(set(labels))}" if labels else ""))
                return 0
            print(f"\nround {round_}: no drift — the re-pin converged.")
            break
        stage = min(r.pin.stage for r in moves)
        batch = [r for r in moves if r.pin.stage == stage]
        for r in batch:
            moved.setdefault(r.pin.key, r)
        files = apply(batch, reason, head) + superseded_genesis(batch, reason, head)
        written |= set(files)
        wait = len(moves) - len(batch)
        print(f"\nround {round_}: rewrote {len(batch)} pin(s) in {len(set(files))} file(s)" +
              (f"; {wait} more are recomputed over them in the next round" if wait else "; confirming with one more build"))
        for f in sorted(set(files)):
            print("  " + f)
    else:
        print(f"\n--apply did not converge in {MAX_ROUNDS} rounds: a value moves every time it is re-pinned. Rewritten: {sorted(written)}")
        return 5
    labels = relabel(rows, head, args.shipping, written)
    written |= set(labels)
    print(f"\n--apply: {len(moved)} pin(s) re-pinned in {len(written)} file(s)." +
          (f" Copy-block labels rewritten ({'shipping' if args.shipping else head[:8]}): {sorted(set(labels))}" if labels else ""))
    stale = stale_mentions(list(moved.values()), rows)
    if stale:
        print("Other mentions of the old values (NOT rewritten; review by hand):")
        for line in stale:
            print("  " + line)
    if args.no_verify:
        print("(--no-verify: the pin tests were not run)")
        return 0
    rc, out = run_tests(rows, {"t12_deploy_kit_constants", "t12_regenesis", "t12_repin_values"}, log_dir)
    print("\npin tests after the rewrite:")
    for line in summarize_results(out):
        print("  " + line)
    print("\nThe last round was the confirming dry run. Review `git diff` and the `re-pin` comments, fix the prose above, commit.")
    return 0 if rc == 0 else 5


def report(rows: list[Row], computed: dict[str, str], drift_only: bool, shipping: bool, dry_run: bool):
    """Print the pinned-vs-computed table and what a rewrite cannot fix; return the row classes."""
    width = max(len(r.pin.key) for r in rows)
    label = {"drift": "DRIFT", "ok": "ok", "absent": "absent", "nocompute": "NOT COMPUTED", "locate-error": "NOT FOUND",
             "history": "history", "label": "label"}
    print(f"\n{'pin':{width}}  {'scope':10}  {'status':12}  {'pinned':18} {'computed':18}")
    for r in rows:
        if drift_only and r.status in ("ok", "history", "absent", "label"):
            continue
        tail = f"  -> {r.decision.upper()}" if r.status == "drift" else ""
        print(f"{r.pin.key:{width}}  {r.pin.scope:10}  {label[r.status]:12}  {short(r.pinned):18} {short(r.computed):18}{tail}")
        if r.detail and r.status not in ("ok", "absent"):
            print(f"{'':{width}}    {r.detail}")
    counts: dict[str, int] = {}
    for r in rows:
        counts[label[r.status]] = counts.get(label[r.status], 0) + 1
    print("\nsummary: " + ", ".join(f"{v} {k}" for k, v in sorted(counts.items())))
    for detail in sorted({r.detail for r in rows if r.status == "absent"}):
        print(f"  absent: {detail} — with --shipping this is NOT FOUND")
    for key in ["from.testnet-12.params_id", "genesis.testnet-12.hash", "testnet-12.premine_txid", "from.testnet-12.schedule_id",
                "rule_manifest.digest"]:
        print(f"  computed {key:30} {computed.get(key, '-')}")
    present = present_gates()
    for name, _, arrives in GATES:
        state = computed.get(f"gate.{name}", "did not run") if name in present else f"not in this tree (arrives with {arrives})"
        print(f"  gate {name.split('::')[-1]}: {state}")

    problems = structural_problems(rows, computed, shipping, shipping and dry_run)
    for p in problems:
        print("CHECK: " + p)
    unreg = unregistered(rows)
    if unreg:
        print("\nUNREGISTERED quoted hex literals in pin files (a pin the registry does not check — add an entry):")
        for u in unreg:
            print("  " + u)

    drift = [r for r in rows if r.status == "drift"]
    refused = [r for r in drift if r.decision == "refuse"]
    moves = [r for r in drift if r.decision == "move"]
    broken = [r for r in rows if r.status in ("nocompute", "locate-error")]
    if broken:
        print("\nNOT CHECKED (resolve before trusting the result):")
        for r in broken:
            print(f"  {r.pin.key}: {r.detail}")
    if refused:
        print("\nREFUSED — these must not move on the regenesis:")
        for r in refused:
            print(f"  {r.pin.key} [{r.pin.scope}] {short(r.pinned)} -> {short(r.computed)}: {r.detail}")
            for t in r.pin.tests:
                print(f"      would fail: {t}")
    pending = [r for r in refused if r.t11_layout_pending]
    if pending:
        layout_moved = [r.pin.key for r in drift if r.pin.scope == "layout"]
        print("\nAUDIT SIGN-OFF NEEDED before --allow-t11-layout (checklist §5, item 5):")
        print("  1. the v22 layout pins that moved in this run, and why each encoding changed: " + ", ".join(layout_moved))
        print("  2. the testnet-11 goldens that would move with them: " + ", ".join(r.pin.key for r in pending))
        print("  3. that testnet-11's fold changed only by that encoding: the parity dump's roots-masked digest, length and line "
              "count held; the verdict fixture's lock and collaterals held; the dormant ring's premises (empty ring, no ring "
              "delta) are asserted before its root prints — the audit reads the diff of the fold code, not only these guards.")
    return drift, refused, moves, broken, problems, unreg


if __name__ == "__main__":
    sys.exit(main())

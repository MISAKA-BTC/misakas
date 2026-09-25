#!/usr/bin/env python3
"""**The testnet-12 re-pin, mechanical** — the engine behind `scripts/t12-repin.sh` (run that).

Every value the shipping commit may move is pinned somewhere in the tree as a literal a human once
pasted: the genesis header in `genesis.rs`, the preset fingerprints in `params.rs`, the "only this
fence moved it" twins, the v22 goldens, the t11 parity dump, the deploy kit's and the explorer's copies,
the launch checklist's §3. This reads every one of them (the REGISTRY below: file, anchor, spelling),
reads the value this build COMPUTES for it (the REPIN lines `consensus/core/tests/t12_repin_values.rs`
prints, and the lines the pin tests themselves print before they assert), and sets them side by side.

    t12-repin.sh                               dry run: "pinned vs computed" for every pin, exit 1 on drift
    t12-repin.sh --apply --reason "<why>"      rewrite the drifted t12 / mainnet / layout pins in place,
                                               one reason comment each, then run the affected tests
    t12-repin.sh --selftest                    the decision rules on the last harvest logs (no build)

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
  otherwise. The tool re-pins one only when (a) a v22 layout golden moved in the same run and (b) every
  non-root value beside it held — the dump with its roots masked, its length and line count; the lock
  and collaterals beside the verdict roots. Otherwise it is a t11 behaviour change and it refuses.
* **verdict** — ADR-0150's stored-corpus verdict digest is network-independent (a stateless gate): it
  moves only with a raised ruleset revision, so it is re-pinned only with `--allow-verdict` AND a rule
  manifest digest that moved.

The values are never typed: each comes from a build of THIS tree. Pins in files that do not exist yet
(the Activation Pool's `palw_activation_pool_is_t12_only.rs` before its merge) are reported absent, and
a quoted hex literal in a pin file that no registry entry covers is reported UNREGISTERED — add an
entry (and, if the value is built inside a test, a print before the assertion) rather than editing by hand.
"""

from __future__ import annotations

import argparse
import datetime
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

T41_RECORDS = ["PalwClaimRcoreV1", "PalwVestingRowV1", "PalwVestingCountersV1", "PalwPendingRewardV1", "PalwReporterCommitV1",
               "PalwReporterCountersV1", "PalwDaSessionV1", "PalwDaClaimV1", "PalwSlashableLockV1",
               "PalwPanelLiabilityRecordV1", "PalwConsumedOffenceV1"]

LAYOUT_GUARDS = ("v22.state.empty", "v22.state.full", "v22.t41.empty_root", "v22.t41.empty_carriage",
                 *(f"v22.t41.record.{r}" for r in T41_RECORDS))


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

    # ---- testnet-12's classes ------------------------------------------------------------------------
    held = (f"{REGEN}::the_held_rows_are_the_fleets_classes",)
    pins.append(Pin("regenesis.held_row_2m", "t12", REGEN, [r"fn the_held_rows_are_the_fleets_classes\(\)",
                                                             r"hex\(dense\.shape_profile_id\(\)\),"], "testnet-12.class.2m", tests=held))
    pins.append(Pin("regenesis.held_row_8k", "t12", REGEN, [r"fn the_held_rows_are_the_fleets_classes\(\)",
                                                             r"hex\(narrow\.shape_profile_id\(\)\),"], "testnet-12.class.8k", tests=held))
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
    for i, record in enumerate(T41_RECORDS):
        pins.append(Pin(f"t41.record.{record}", "layout", GOLDEN,
                        [r"fn t41_the_v22_record_encodings_are_pinned\(\)", r"let want = \["], f"v22.t41.record.{record}",
                        nth=i, length=64, until=r"^\}", tests=(f"{GOLDEN}::t41_the_v22_record_encodings_are_pinned",)))
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
    pins.append(Pin("scan_deploy.genesis", "copy", SCAN_DEPLOY, [r"暫定値は genesis `"], "genesis.testnet-12.hash", fmt="abbr",
                    comment=None))
    pins.append(Pin("scan_deploy.fp", "copy", SCAN_DEPLOY, [r"暫定値は genesis `", r"fp `"], "from.testnet-12.params_id", fmt="abbr",
                    comment=None))
    pins.append(Pin("scan_deploy.PANEL_BOND_TX", "copy", SCAN_DEPLOY, [r"固有の premine txid `"], "testnet-12.premine_txid", fmt="abbr",
                    comment=None))
    return pins


# Quoted hex literals in these files are pins; any not located by an entry above is UNREGISTERED.
SCANNED = sorted({RCORE, FLOOR, ATTR, DEADLINE, POOL, EVM, RELEASE, GOLDEN, PARITY, RING, VERDICTS, CORPUS, REGEN, MANIFEST,
                  "consensus/core/tests/t12_deploy_kit_constants.rs"})

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
        _, lib_out = sh(base + ["--lib", "--", "--exact", LIB_GOLDEN_TEST, "--nocapture"], os.path.join(log_dir, "harvest-lib.log"))
        texts = [tests_out, lib_out]
        for what, text in [("the pin tests", tests_out), ("the lib golden", lib_out)]:
            if not compiled(text):
                notes["*"] = f"{what} did not build or run — see {log_dir}"
    computed, panics = parse_harvest(texts)
    notes.update(panics)
    return computed, notes


def parse_harvest(texts: list[str]) -> tuple[dict[str, str], dict[str, str]]:
    text = "\n".join(texts)
    computed = harvest_text(text)
    computed.update(file_facts())
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
    status: str                   # ok | drift | absent | nocompute | locate-error | history
    pinned: str = ""
    computed: str = ""
    span: tuple = ()
    line_at: int = -1
    detail: str = ""
    decision: str = ""            # move | refuse | keep


def compare(pins: list[Pin], computed: dict[str, str], notes: dict[str, str]) -> list[Row]:
    rows = []
    cache: dict[str, str] = {}
    for pin in pins:
        path = os.path.join(REPO, pin.file)
        if not os.path.exists(path):
            rows.append(Row(pin, "absent", detail=f"{pin.file} does not exist (yet)"))
            continue
        text = cache.setdefault(pin.file, open(path, encoding="utf-8").read())
        try:
            start, end, pinned, line_at = locate(text, pin)
        except LocateError as e:
            rows.append(Row(pin, "locate-error", detail=f"{pin.file}: {e}"))
            continue
        row = Row(pin, "history", pinned=pinned, span=(start, end), line_at=line_at)
        rows.append(row)
        if pin.scope == "history":
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


def decide(rows: list[Row], allow_verdict: bool) -> None:
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
            else:
                r.decision = "move"
                r.detail = ("a v22 layout move (" + ", ".join(layout_moved[:2]) + (", …" if len(layout_moved) > 2 else "") +
                            "); every non-root value beside it held — the audit confirms (checklist §5-5)")
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
    for f in SCANNED:
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
            above = text[text.rfind("\n", 0, max(line_at - 1, 0)) + 1:line_at]
            if line and f"re-pin {today} @{head}: {reason}" in above:
                line = None  # this run already explained this group (an earlier round)
            if line:
                indent = re.match(r"[ \t]*", text[line_at:]).group(0)
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
    """The tests the moved pins sit in (whole integration targets; the named lib tests), plus `extra`."""
    targets, lib_filters = set(extra), set()
    for r in rows:
        if r.decision != "move":
            continue
        for t in r.pin.tests:
            if t.startswith("lib::"):
                lib_filters.add(t[len("lib::"):])
            else:
                targets.add(os.path.splitext(os.path.basename(t.split("::")[0]))[0])
    targets = sorted(t for t in targets if os.path.exists(os.path.join(REPO, "consensus/core/tests", t + ".rs")))
    rc, out = 0, ""
    if lib_filters:
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


def selftest(log_dir: str) -> int:
    """The decision rules on THIS tree's own harvest logs with one value changed at a time (no build):
    each scenario names the pins that must drift and what the tool must decide for each."""
    base = ["\n".join(open(os.path.join(log_dir, name), errors="replace").read() for name in ("harvest-tests.log", "harvest-lib.log"))]

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

    rep_line = lambda key: rf"REPIN {re.escape(key)} "
    scenarios = [
        ("the tree as built", change(), False, {}),
        ("t11 dump: only its roots moved, no layout golden moved", change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",)), False,
         {"parity.PARENT_DUMP_BLAKE2B_256": "refuse"}),
        ("t11 dump roots moved WITH a v22 record encoding", change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",),
                                                                   (r"T41 record PalwVestingRowV1: ",)), False,
         {"parity.PARENT_DUMP_BLAKE2B_256": "move", "t41.record.PalwVestingRowV1": "move"}),
        ("t11 dump: a non-root line moved too (masked digest)", change((r"t11 dump: \d+ bytes, \d+ lines, BLAKE2b-256 ",),
                                                                       (r"t11 dump roots masked: BLAKE2b-256 ",),
                                                                       (r"T41 record PalwVestingRowV1: ",)), False,
         {"parity.PARENT_DUMP_BLAKE2B_256": "refuse", "parity.PARENT_DUMP_ROOTS_MASKED_BLAKE2B_256": "refuse",
          "t41.record.PalwVestingRowV1": "move"}),
        ("t11 verdict roots moved with the lock, and a layout move", change((r"licensed root ",), (r"T41 empty carriage: ",),
                                                                            (r"licensed root \w+ lock ",)), False,
         {"t11_verdicts.ROOT_LICENSED": "refuse", "t11_verdicts.SEAT_LOCK_SOMPI": "refuse", "t41.empty_carriage": "move"}),
        ("the dormant ring root moved with a layout move", change((r"dormant root = ",), (r"T41 empty carriage: ",)), False,
         {"ring.DORMANT_GOLDEN_ROOT": "move", "t41.empty_carriage": "move"}),
        ("the corpus verdicts moved (allowed, but the manifest did not move)", change((r"corpus verdict digest ",)), True,
         {"corpus.CORPUS_VERDICT_DIGEST": "refuse"}),
        ("testnet-11's params id moved", change((rep_line("shipped.testnet-11.params_id"),), (rep_line("preset_net.testnet-11.params_id"),)),
         False, {k: "refuse" for k in ["rcore.AT_V22.testnet-11.params_id", "floor.BEFORE_THE_FLOOR.testnet-11.params_id",
                                       "attribution.BEFORE_THE_ATTRIBUTION.testnet-11.params_id",
                                       "deadline.BEFORE_THE_DEADLINE.testnet-11.params_id", "release.T11_CONSENSUS_PARAMS_ID",
                                       "params.shipped_presets.testnet-11"]}),
        ("devnet's identity moved", change((rep_line("shipped.devnet.identity_id"),)), False,
         {k: "refuse" for k in ["rcore.AT_V22.devnet.identity_id", "floor.BEFORE_THE_FLOOR.devnet.identity_id",
                                "attribution.BEFORE_THE_ATTRIBUTION.devnet.identity_id", "deadline.BEFORE_THE_DEADLINE.devnet.identity_id"]}),
        ("mainnet's ruleset moved", change((rep_line("shipped.mainnet.params_id"),), (rep_line("const.mainnet.params_id"),),
                                           (rep_line("preset_net.mainnet.params_id"),)), False,
         {k: "move" for k in ["rcore.AT_V22.mainnet.params_id", "rcore.AT_V21.mainnet.params_id", "floor.BEFORE_THE_FLOOR.mainnet.params_id",
                              "attribution.BEFORE_THE_ATTRIBUTION.mainnet.params_id", "deadline.BEFORE_THE_DEADLINE.mainnet.params_id",
                              "release.MAINNET_CONSENSUS_PARAMS_ID", "params.shipped_presets.mainnet"]}),
        ("testnet-12's fingerprint and a twin moved", change((rep_line("from.testnet-12.params_id"),),
                                                             (r"testnet-12 without the fence: params ",)), False,
         {k: "move" for k in ["checklist.EXPECT_FP", "plan.EXPECT_FP", "scan_deploy.fp", "attribution.T12_BEFORE_THE_ATTRIBUTION.params_id"]}),
        ("testnet-12's genesis moved", change((rep_line("genesis.testnet-12.hash"),), (rep_line("genesis.testnet-12.utxo_commitment"),)),
         False, {k: "move" for k in ["genesis.testnet-12.hash", "genesis.testnet-12.utxo_commitment", "checklist.EXPECT_GENESIS",
                                     "plan.EXPECT_GENESIS", "regen_doc.genesis_hash", "regen_doc.utxo_commitment", "join_doc.genesis",
                                     "scan_deploy.genesis"]}),
        ("a twin was never printed (its premise failed first)", lambda t: t.replace("testnet-12 at the parent without the attribution",
                                                                                   "(premise failed)"), False,
         {f"evm.T12_AT_THE_PARENT_WITHOUT_THE_ATTRIBUTION.{w}": "nocompute" for w in ["params_id", "identity_id", "schedule_id"]}),
    ]
    failures = 0
    for name, mutate, allow_verdict, want in scenarios:
        computed, notes = parse_harvest([mutate(t) for t in base])
        rows = compare(registry(), computed, notes)
        decide(rows, allow_verdict)
        got = {r.pin.key: (r.decision if r.status == "drift" else r.status) for r in rows
               if r.status in ("drift", "nocompute", "locate-error")}
        ok = got == want
        failures += not ok
        print(f"{'PASS' if ok else 'FAIL'}  {name}: {len(want)} pin(s) " + (", ".join(sorted(set(want.values()))) if want else "none"))
        if not ok or os.environ.get("REPIN_SELFTEST_VERBOSE"):
            for k in sorted(set(got) | set(want)):
                print(f"        {k:60} got {got.get(k, 'ok'):10} want {want.get(k, 'ok')}")
    print(f"\nselftest: {len(scenarios) - failures}/{len(scenarios)} scenarios as expected")
    return 1 if failures else 0


def short(v: str) -> str:
    return (v[:16] + "…") if len(v) > 17 else (v or "-")


def main() -> int:
    ap = argparse.ArgumentParser(description="testnet-12 re-pin: pinned vs computed, and --apply")
    ap.add_argument("--apply", action="store_true", help="rewrite the drifted movable pins, then run the affected tests")
    ap.add_argument("--reason", help="the one line each rewrite's comment carries (required with --apply)")
    ap.add_argument("--allow-verdict", action="store_true", help="let the ADR-0150 corpus verdict digest move (with the manifest)")
    ap.add_argument("--from-log", action="append", help="parse these harvest logs instead of running cargo (repeatable)")
    ap.add_argument("--log-dir", default=None, help="cargo logs (default: $CARGO_TARGET_DIR/t12-repin, else target/t12-repin)")
    ap.add_argument("--no-verify", action="store_true", help="with --apply: do not run the affected tests afterwards")
    ap.add_argument("--drift-only", action="store_true", help="print only the rows that are not ok")
    ap.add_argument("--selftest", action="store_true", help="check the decision rules on the last harvest logs (no build)")
    args = ap.parse_args()

    head = subprocess.run(["git", "rev-parse", "--short=12", "HEAD"], cwd=REPO, capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "status", "--porcelain", "--untracked-files=no"], cwd=REPO, capture_output=True, text=True).stdout
    log_dir = args.log_dir or os.path.join(os.environ.get("CARGO_TARGET_DIR", os.path.join(REPO, "target")), "t12-repin")
    os.makedirs(log_dir, exist_ok=True)
    if args.selftest:
        return selftest(log_dir)
    print(f"t12-repin: tree {head}{' + uncommitted changes' if dirty.strip() else ''}; logs in {log_dir}")
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
        rows = compare(registry(), computed, notes)
        decide(rows, args.allow_verdict)
        drift, refused, moves, broken, problems, unreg = report(rows, computed, args.drift_only)
        if drift:
            print(f"\nDRY RUN: {len(drift)} pin(s) drifted — {len(moves)} movable, {len(refused)} refused.")
            if any(r.pin.stage == 0 for r in moves):
                print("NOTE: a genesis constant drifted. Every value that hashes the genesis (the params and identity ids, the twins, "
                      "their copies) is computed above over the constant AS IT STANDS; --apply re-pins the genesis first, rebuilds, "
                      "and recomputes them.")
            stale = stale_mentions(moves, rows)
            if stale:
                print("Other mentions of the moving old values (not rewritten; review by hand):")
                for line in stale:
                    print("  " + line)
            print("Nothing may be applied until the refused pins are explained." if refused else
                  "Re-run with --apply --reason \"…\" to rewrite them.")
            return 1
        print("\nDRY RUN: no drift — every checked pin is this build's value." +
              (" But see NOT CHECKED / CHECK above." if broken or problems or unreg else ""))
        return 3 if broken or problems else 0

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
        rows = compare(registry(), computed, notes)
        decide(rows, args.allow_verdict)
        drift, refused, moves, broken, problems, unreg = report(rows, computed, True)
        if refused or broken:
            print(f"\n--apply REFUSED in round {round_} (above)." +
                  (f" Rewritten in earlier rounds (review or `git checkout -- <file>`): {sorted(written)}" if written else
                   " Nothing was written."))
            return 3
        if not moves:
            if round_ == 1:
                print("\n--apply: nothing drifted; nothing written.")
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
    print(f"\n--apply: {len(moved)} pin(s) re-pinned in {len(written)} file(s).")
    stale = stale_mentions(list(moved.values()), rows)
    if stale:
        print("Other mentions of the old values (NOT rewritten; review by hand):")
        for line in stale:
            print("  " + line)
    if args.no_verify:
        print("(--no-verify: the affected tests were not run)")
        return 0
    rc, out = run_tests(list(moved.values()), {"t12_deploy_kit_constants", "t12_regenesis", "t12_repin_values"}, log_dir)
    print("\naffected tests:")
    for line in summarize_results(out):
        print("  " + line)
    print("\nThe last round was the confirming dry run. Review `git diff` and the `re-pin` comments, fix the prose above, commit.")
    return 0 if rc == 0 else 5


def report(rows: list[Row], computed: dict[str, str], drift_only: bool):
    """Print the pinned-vs-computed table and what a rewrite cannot fix; return the row classes."""
    width = max(len(r.pin.key) for r in rows)
    label = {"drift": "DRIFT", "ok": "ok", "absent": "absent", "nocompute": "NOT COMPUTED", "locate-error": "NOT FOUND",
             "history": "history"}
    print(f"\n{'pin':{width}}  {'scope':10}  {'status':12}  {'pinned':18} {'computed':18}")
    for r in rows:
        if drift_only and r.status in ("ok", "history", "absent"):
            continue
        tail = f"  -> {r.decision.upper()}" if r.status == "drift" else ""
        print(f"{r.pin.key:{width}}  {r.pin.scope:10}  {label[r.status]:12}  {short(r.pinned):18} {short(r.computed):18}{tail}")
        if r.detail and r.status not in ("ok", "absent"):
            print(f"{'':{width}}    {r.detail}")
    counts: dict[str, int] = {}
    for r in rows:
        counts[label[r.status]] = counts.get(label[r.status], 0) + 1
    print("\nsummary: " + ", ".join(f"{v} {k}" for k, v in sorted(counts.items())))
    for key in ["from.testnet-12.params_id", "genesis.testnet-12.hash", "testnet-12.premine_txid", "from.testnet-12.schedule_id",
                "rule_manifest.digest"]:
        print(f"  computed {key:30} {computed.get(key, '-')}")

    problems = []
    registered = computed.get("testnet-12.class.registered", "")
    if registered:
        on_chain = set(registered.split(",")) | {computed.get("testnet-12.class.floor", "")}
        app = open(os.path.join(REPO, APP), encoding="utf-8").read()
        start = app.find("const LLM_CLASSES = [")
        in_app = set(re.findall(r'id:"([0-9a-f]{128})"', app[start:app.find("];", start)]))
        problems += [f"{APP} LLM_CLASSES names {x[:16]}…, not a genesis class of this build — edit by hand" for x in sorted(in_app - on_chain)]
        problems += [f"{APP} LLM_CLASSES lacks genesis class {x[:16]}… — add its row by hand (name, model, tag)"
                     for x in sorted(on_chain - in_app)]
        order = registered.split(",")
        if order != [computed.get("testnet-12.class.8k"), computed.get("testnet-12.class.2m")]:
            problems.append(f"testnet-12's genesis model classes are {[o[:8] for o in order]}, not [8k, 2M]: "
                            "t12_regenesis::the_held_rows_are_the_fleets_classes and the kit's node tables need a hand edit")
    if computed.get("kit.cards") and computed["kit.cards"] != ",".join(str(i) for i in range(8)):
        problems.append(f"PALW_T12_GENESIS_BONDS declares premine indices {computed['kit.cards']}, not 0..7 in order: the kit's "
                        "`$PREMINE_TXID:N` / FEE_FLOAT_BASE+N naming breaks (t12_deploy_kit_constants)")
    gen = computed.get("genesis.testnet-12.hash")
    m = re.search(r'^FORBIDDEN_GENESIS="([0-9a-f\s]*)"', open(os.path.join(REPO, FLEET), encoding="utf-8").read(), re.M)
    if gen and m and gen in m.group(1).split():
        problems.append(f"{FLEET} FORBIDDEN_GENESIS lists this build's own genesis {gen[:16]}…")
    for p in problems:
        print("CHECK: " + p)
    unreg = unregistered(rows)
    if unreg:
        print("\nUNREGISTERED quoted hex literals in pin files (not checked — add a registry entry):")
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
    return drift, refused, moves, broken, problems, unreg

if __name__ == "__main__":
    sys.exit(main())

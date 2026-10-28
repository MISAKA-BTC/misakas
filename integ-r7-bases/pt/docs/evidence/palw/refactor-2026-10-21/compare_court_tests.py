"""Compare completed, named tests; never count an interrupted suite as a pass.
Run after copying the final output to panel-final-unit-and-court-tests.log.
"""
from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parent


def completed(name):
    text = (ROOT / name).read_text()
    result = re.findall(
        r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text, re.M
    )
    if len(result) != 1:
        raise ValueError(f"{name}: no single completed test summary")
    status, passed, failed, ignored = result[0]
    tests = dict(re.findall(r"^test (\S+) \.\.\. (ok|FAILED|ignored)$", text, re.M))
    if len(tests) != int(passed) + int(failed) + int(ignored):
        raise ValueError(f"{name}: incomplete named results")
    return tests, {"status": status, "passed": int(passed), "failed": int(failed), "ignored": int(ignored)}


before, before_result = completed("panel-before-unit-and-court-tests.log")
after, after_result = completed("panel-final-unit-and-court-tests.log")
missing = sorted(before.keys() - after.keys())
regressions = sorted(name for name, status in before.items() if status == "ok" and after.get(name) != "ok")
added = sorted(after.keys() - before.keys())
resolved = sorted(name for name, status in before.items() if status == "FAILED" and after.get(name) == "ok")
report = {
    "before": before_result,
    "after": after_result,
    "missing_old_tests": missing,
    "previous_passes_regressed": regressions,
    "added_tests": {name: after[name] for name in added},
    "existing_source_guard_failures_resolved": resolved,
    "scope": "All PALW panel tests except the two actual-service modules. Three representative service cases are compared separately.",
    "not_claimed": "The interrupted whole-panel baseline is not a completed suite. Selected results and a source projection are not an exhaustive execution equivalence proof.",
}
(ROOT / "court-test-comparison.json").write_text(json.dumps(report, indent=2) + "\n")
if missing or regressions or after_result["status"] != "ok":
    raise SystemExit("comparison failed; inspect court-test-comparison.json")
print(json.dumps(report, indent=2))

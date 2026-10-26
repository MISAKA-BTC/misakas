"""External/policy inputs the 15 campaign gates depend on, and the UNKNOWN cap.

This is a leaf of the existing campaign runner (rfc_integration_campaign.py,
misaka-rfc-integration-campaign.py); it adds no runtime. Two inputs are known
not to exist: the K3 formal specification/manifest/weights/signed inputs
(EXTERNAL) and the native39 -> RFC-0012 settlement mapping (POLICY, answered
"not formulated" by the user on 2026-10-25). A gate that needs one is capped at
UNKNOWN whatever its observed facts are, so fixture or toy output cannot
promote it. A FAIL is never hidden by the cap.
"""
import json
from pathlib import Path

# input id -> (class, why it is unavailable)
UNAVAILABLE = {
    'k3-formal-inputs': ('EXTERNAL', 'K3 formal specification, manifest, weights and signed inputs are not created'),
    'native39-rfc12-settlement-mapping': ('POLICY', 'native39 to RFC-0012 W/PWU, anchor, operator, retention, maturity and void mapping is not formulated'),
}
# Conservative assignment: the gates whose evidence kind needs the input.
GATE_INPUTS = {
    'rfc06-model-boundaries-cost': ('k3-formal-inputs',),
    'rfc07-income-cost': ('native39-rfc12-settlement-mapping',),
    'rfc08-all-rewards-once': ('native39-rfc12-settlement-mapping',),
    'rfc12-market': ('native39-rfc12-settlement-mapping',),
}


class InputRefused(ValueError):
    pass


def load_input_manifest(path):
    """Optional operator manifest {"schemaVersion":1,"inputs":[{"id":..,"sha256":..}]}.

    An entry naming an unavailable input is refused: no adopted specification
    exists for it, so a file cannot stand in for one."""
    value = json.loads(Path(path).read_text())
    if not isinstance(value, dict) or value.get('schemaVersion') != 1 or not isinstance(value.get('inputs'), list):
        raise InputRefused('input manifest schema differs')
    seen = set()
    for row in value['inputs']:
        if not isinstance(row, dict) or set(row) != {'id', 'sha256'} or not isinstance(row['id'], str):
            raise InputRefused('input manifest row differs from {id, sha256}')
        if row['id'] in seen:
            raise InputRefused('duplicate input id ' + row['id'])
        seen.add(row['id'])
        if row['id'] in UNAVAILABLE:
            raise InputRefused('input %s is refused: %s' % (row['id'], UNAVAILABLE[row['id']][1]))
    return value


def missing_for(gate):
    return [dict(input=name, cls=UNAVAILABLE[name][0], reason=UNAVAILABLE[name][1])
            for name in GATE_INPUTS.get(gate, ()) if name in UNAVAILABLE]


def cap_gates(gates):
    """Return gates with PASS lowered to UNKNOWN where a required input is missing."""
    result = {}
    for name, gate in gates.items():
        missing = missing_for(name)
        gate = dict(gate)
        if missing:
            gate['missingExternalInputs'] = missing
            if gate.get('status') == 'PASS':
                gate['status'] = 'UNKNOWN'
                gate['cappedBy'] = 'missing external/policy input; not counted toward production acceptance'
        result[name] = gate
    return result


class LedgerRefused(ValueError):
    pass


def verify_aggregate_root(root, freeze):
    """Refuse to re-aggregate a ledger that the current freeze did not produce.

    The root must hold the campaign-freeze.json and campaign-once.json written
    before its first child, the once marker must pin those exact freeze bytes,
    and campaign id, canonical plan and candidate SHA256 must equal the current
    freeze. Otherwise another run's rows would be reported under this plan and
    candidate."""
    import hashlib
    root = Path(root)
    freeze_path, once_path = root / 'campaign-freeze.json', root / 'campaign-once.json'
    if not freeze_path.is_file() or not once_path.is_file():
        raise LedgerRefused('aggregate root lacks its campaign-freeze.json/campaign-once.json markers')
    freeze_bytes = freeze_path.read_bytes()
    try:
        stored, once = json.loads(freeze_bytes), json.loads(once_path.read_text())
    except ValueError:
        raise LedgerRefused('aggregate root markers are not JSON')
    if not isinstance(stored, dict) or not isinstance(once, dict):
        raise LedgerRefused('aggregate root markers are not objects')
    if once.get('freezeSha256') != hashlib.sha256(freeze_bytes).hexdigest():
        raise LedgerRefused('once marker does not pin the stored freeze bytes')
    current = (freeze['campaignId'], freeze['canonicalPlanSha256'], freeze['candidate']['sha256'])
    recorded = (stored.get('campaignId'), stored.get('canonicalPlanSha256'), (stored.get('candidate') or {}).get('sha256'))
    if recorded != current or (once.get('campaignId'), once.get('candidateSha256')) != (current[0], current[2]):
        raise LedgerRefused('aggregate root was frozen for another campaign, plan or candidate')
    return stored


def aggregate_ledger(lines, planned_ids):
    """Rebuild scenario rows from an append-only ledger (resume = re-aggregate, never re-run).

    One campaign attempts each scenario at most once, so a repeated id is
    refused rather than letting a later row replace an earlier FAIL. Only the
    final non-empty line may be torn (a killed run); a malformed row anywhere
    else, or an id outside the plan, refuses the ledger. Planned ids never
    recorded stay UNKNOWN."""
    planned = list(planned_ids)
    body = [(n, line.strip()) for n, line in enumerate(lines, 1) if line.strip()]
    rows = {}
    for index, (number, line) in enumerate(body):
        try:
            row = json.loads(line)
        except ValueError:
            if index == len(body) - 1:
                continue  # a torn final line from a killed run is not a result
            raise LedgerRefused('ledger line %d is malformed before the tail' % number)
        if not isinstance(row, dict) or not isinstance(row.get('id'), str):
            raise LedgerRefused('ledger line %d is not a scenario row' % number)
        if row['id'] not in planned:
            raise LedgerRefused('ledger line %d names scenario %s outside the plan' % (number, row['id']))
        if row['id'] in rows:
            raise LedgerRefused('ledger repeats scenario %s (line %d)' % (row['id'], number))
        rows[row['id']] = row
    for ident in planned:
        rows.setdefault(ident, dict(id=ident, status='UNKNOWN', reason='no ledger row; stage never reached', facts=[]))
    return list(rows.values())

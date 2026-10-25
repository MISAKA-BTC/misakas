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


def aggregate_ledger(lines, planned_ids):
    """Rebuild scenario rows from an append-only ledger (resume = re-aggregate, never re-run).

    Later rows for an id replace earlier ones; planned ids never recorded stay UNKNOWN."""
    rows = {}
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            row = json.loads(line)
        except ValueError:
            continue  # a torn final line from a killed run is not a result
        if isinstance(row, dict) and isinstance(row.get('id'), str):
            rows[row['id']] = row
    for ident in planned_ids:
        rows.setdefault(ident, dict(id=ident, status='UNKNOWN', reason='no ledger row; stage never reached', facts=[]))
    return list(rows.values())

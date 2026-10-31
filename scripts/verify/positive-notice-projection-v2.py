#!/usr/bin/env python3
"""Independent serialization/hash vectors, NOT native admission or LLM tests.

Models the *unchanged ordinary PAV2* attempt projection using Panel-owned
computational roots. No magic/domain/algorithm is allocated for a new form.
Run --write only when intentionally creating a new reviewed vector artifact.
"""
import argparse
import hashlib
import json
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VECTOR = ROOT / "docs/evidence/positive-court-a0-2026-10-31/integration/panel-root-pav2-projection-vectors.json"
U16 = lambda x: struct.pack("<H", x)
U32 = lambda x: struct.pack("<I", x)
U64 = lambda x: struct.pack("<Q", x)
H = lambda key, x: hashlib.blake2b(x, key=key, digest_size=64).digest()
B = lambda x: bytes([x]) * 64
FIELDS = ["version", "network_domain", "challenge", "class_id", "executor_bond", "executor_pubkey",
          "operator_id", "artifact_root", "trace_root", "output_root", "pwu", "trace_manifest_root",
          "trace_chunk_count", "trace_retention_daa", "execution_root"]


def check_actual_source():
    source = (ROOT / "consensus/core/src/palw_attempt_v2.rs").read_text()
    body = source.split("pub struct PalwAttemptUnsignedV2 {", 1)[1].split("\n}", 1)[0]
    assert re.findall(r"^    pub (\w+):", body, re.M) == FIELDS
    for domain in [b"misaka-palw/attempt-v2/execution-commitment/v3", b"misaka-palw/attempt-v2/class-ticket/v3",
                   b"misaka-palw/attempt-v2/l1-tag/v1", b"misaka-palw/attempt-v2/trace-manifest/v1",
                   b"misaka-palw/attempt-v2/challenge/v1", b"misaka-palw/base0/rc-job-anchor/v1"]:
        assert ('b"' + domain.decode() + '"') in source
    assert "pub const PALW_ATTEMPT_V2_VERSION: u16 = 6;" in source
    assert "view.challenge = Hash64::default();" in source
    assert "header_daa_score.saturating_add(facts.min_trace_retention_daa)" in source
    assert "palw_attempt_trace_manifest_root_v2(execution.trace_root, 1 + facts.witness_chunks)" in source
    mesh = (ROOT / "consensus/core/src/palw_mesh_v1.rs").read_text()
    assert 'b"misaka-palw/mesh/trace-manifest/v2"' in mesh


def serialize(a):
    return (U16(a["version"]) + a["network_domain"] + a["challenge"] + a["class_id"]
            + a["executor_bond"] + U32(len(a["executor_pubkey"])) + a["executor_pubkey"]
            + a["operator_id"] + a["artifact_root"] + a["trace_root"] + a["output_root"]
            + U64(a["pwu"]) + a["trace_manifest_root"] + U32(a["trace_chunk_count"])
            + U64(a["trace_retention_daa"]) + a["execution_root"])


def priced(a, anchor):
    raw = serialize(dict(a, challenge=bytes(64)))
    return H(b"misaka-palw/attempt-v2/execution-commitment/v3", anchor + U64(len(raw)) + raw)


def summary(a, anchor):
    p = priced(a, anchor)
    tag = b"".join(H(b"misaka-palw/attempt-v2/l1-tag/v1", p + U32(i)) for i in range(4))[:200]
    return {"unsigned_bytes": len(serialize(a)), "unsigned_blake2b512": hashlib.blake2b(serialize(a)).hexdigest(),
            "challenge_zeroed_blake2b512": hashlib.blake2b(serialize(dict(a, challenge=bytes(64)))).hexdigest(),
            "execution_commitment_v3": p.hex(),
            "class_ticket_v3_decimal": str(int.from_bytes(H(b"misaka-palw/attempt-v2/class-ticket/v3", p)[:16], "little")),
            "l1_tag_v2_hex": tag.hex()}


def build():
    check_actual_source()
    key = bytes(i % 256 for i in range(2592))
    network, pre_pow, class_id, bond = B(0x11), B(0x22), B(0x33), B(0x44) + U32(7)
    nonce, timestamp, daa, retention = (3 << 22) + 17, 1_700_000_000_000, 900_000, 600
    anchor = H(b"misaka-palw/base0/rc-job-anchor/v1", network + pre_pow + class_id + bond + U64(nonce >> 22))
    challenge = H(b"misaka-palw/attempt-v2/challenge/v1", network + pre_pow + U64(timestamp) + U64(nonce) + class_id + bond)
    operator = H(b"misaka-palw/state-v2/operator-id/v1", U64(len(key)) + key)
    # Roots are synthetic byte fixtures. This file proves hash/serialization
    # equality only, not that any inference produced these roots.
    record = {"full_logits_trace_root": B(0x66), "committed_execution_root": B(0x88)}
    native = dict(version=6, network_domain=network, challenge=challenge, class_id=class_id,
                  executor_bond=bond, executor_pubkey=key, operator_id=operator,
                  artifact_root=B(0x55), output_root=B(0x77), pwu=1000,
                  trace_retention_daa=min(daa + retention, (1 << 64) - 1))
    vectors = []
    assertions = 0
    for witness_chunks in [0, 2]:
        count = 1 + witness_chunks
        domain = b"misaka-palw/mesh/trace-manifest/v2" if witness_chunks else b"misaka-palw/attempt-v2/trace-manifest/v1"
        old = dict(native, trace_root=B(0x66), execution_root=B(0x88),
                   trace_manifest_root=H(domain, B(0x66) + U32(count)), trace_chunk_count=count)
        panel_projection = dict(native, trace_root=record["full_logits_trace_root"],
                                execution_root=record["committed_execution_root"],
                                trace_manifest_root=H(domain, record["full_logits_trace_root"] + U32(count)),
                                trace_chunk_count=count)
        assert serialize(old) == serialize(panel_projection)
        assert summary(old, anchor) == summary(panel_projection, anchor)
        assert len(serialize(old)) == 3262
        assertions += 3
        # None of these are fields of the actual old unsigned-attempt struct.
        for field in ["panel_key", "panel_signature", "witness_id", "witness_attempt", "result_id", "service_contract", "funding_signature"]:
            changed = dict(panel_projection, **{field: B(0x99)})
            assert serialize(changed) == serialize(panel_projection)
            assertions += 1
        for field in ["trace_root", "execution_root", "output_root", "trace_manifest_root"]:
            assert priced(dict(panel_projection, **{field: B(0x99)}), anchor) != priced(old, anchor)
            assertions += 1
        assert priced(dict(panel_projection, challenge=B(0x99)), anchor) == priced(old, anchor)
        assertions += 1
        # The wrong v1-manifest shortcut breaks witness-enabled equivalence.
        if witness_chunks:
            wrong = dict(panel_projection, trace_manifest_root=H(b"misaka-palw/attempt-v2/trace-manifest/v1", B(0x66) + U32(count)))
            assert priced(wrong, anchor) != priced(old, anchor)
            assertions += 1
        vectors.append({"witness_chunks": witness_chunks, "trace_chunk_count": count,
                        "trace_manifest_root": old["trace_manifest_root"].hex(), **summary(old, anchor)})
    # A pre-PoW intent mutation changes the template/job, unlike post-PoW fees.
    other_anchor = H(b"misaka-palw/base0/rc-job-anchor/v1", network + B(0x23) + class_id + bond + U64(nonce >> 22))
    assert other_anchor != anchor and priced(old, other_anchor) != priced(old, anchor)
    assertions += 1
    return {"scope": "ordinary PAV2 pricing only; synthetic roots; no inference/header/native acceptance",
            "fixture": {"network_byte": 0x11, "pre_pow_byte": 0x22, "class_byte": 0x33, "bond_tx_byte": 0x44,
                        "bond_index": 7, "key": "byte i modulo 256 for i=0..2591", "artifact_byte": 0x55,
                        "trace_byte": 0x66, "output_byte": 0x77, "execution_byte": 0x88,
                        "pwu": 1000, "timestamp": timestamp, "nonce": nonce, "daa": daa, "retention": retention},
            "anchor": anchor.hex(), "challenge": challenge.hex(), "assertions": assertions, "vectors": vectors}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    actual = build()
    if args.write:
        VECTOR.parent.mkdir(parents=True, exist_ok=True)
        VECTOR.write_text(json.dumps(actual, indent=2) + "\n")
    else:
        assert json.loads(VECTOR.read_text()) == actual, "fixed vector changed; review before regenerating"
    print(f"PASS: {actual['assertions']} assertions; 2 ordinary-PAV2 vectors; no native admission claim")

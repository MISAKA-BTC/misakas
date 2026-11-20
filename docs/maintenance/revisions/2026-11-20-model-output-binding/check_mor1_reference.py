#!/usr/bin/env python3
"""Small executable examples for the MOR1 registration/output-binding draft.

THIS IS A TEST CODEC, NOT AN ASSIGNED NATIVE WIRE FORMAT. It treats C as opaque
bytes already canonicalized by an external context codec. It does not parse,
authorize, or validate the internal native context, execute Qwen, authenticate
beacons, verify a trace, produce a validity proof, or certify computation cost.

The finite tests demonstrate framing, reproducibility and rejection of malformed
test-codec input. Hash non-collision in these examples is NOT the collision
probability proof. The birthday expression is evaluated exactly as a rational
number under the separate classical random-oracle assumption in RFC-0060.
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass, replace
from decimal import Decimal, localcontext
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import struct
import sys
import unittest

sys.dont_write_bytecode = True

RECEIPT_DOMAIN = b'PALW-MOR1/receipt/v1\0'
REFERENCE_CODEC = b'MOR1-EXAMPLE-CODEC-OPAQUE-C-v1\0'
REFERENCE_SIGMA_SCHEME = b'EXAMPLE-SIGMA32'
MAX_CONTEXT_BYTES = 16384  # Example-only bound; not a proposed native limit.
MAX_INPUT_BYTES = 32768   # Example-only decoder limit; not a native parameter.
FIELDS = ('context', 'sigma', 's', 'input_root', 'output_root', 'trace_root')


def b32(value: bytes) -> bytes:
    if type(value) is not bytes or len(value) != 32:
        raise ValueError('exact bytes[32] required')
    return value


@dataclass(frozen=True)
class ReceiptFields:
    context: bytes
    sigma: bytes
    s: bytes
    input_root: bytes
    output_root: bytes
    trace_root: bytes

    def validate(self):
        if type(self.context) is not bytes or not 0 < len(self.context) <= MAX_CONTEXT_BYTES:
            raise ValueError('nonempty bounded opaque context bytes required')
        for name in FIELDS[1:]:
            b32(getattr(self, name))


def frame(tag: int, payload: bytes) -> bytes:
    if type(payload) is not bytes:
        raise ValueError('payload type')
    return bytes([tag]) + struct.pack('<I', len(payload)) + payload


def encode_receipt_input(fields: ReceiptFields) -> bytes:
    """D_receipt || example Enc_P(C,sigma,s,Rx,Ry,Rtau), all typed/framed."""
    if type(fields) is not ReceiptFields:
        raise ValueError('exact field container required')
    fields.validate()
    result = RECEIPT_DOMAIN + REFERENCE_CODEC + frame(0, REFERENCE_SIGMA_SCHEME)
    for tag, name in enumerate(FIELDS, 1):
        result += frame(tag, getattr(fields, name))
    return result


def decode_receipt_input(data: bytes) -> ReceiptFields:
    if type(data) is not bytes or len(data) > MAX_INPUT_BYTES:
        raise ValueError('bounded byte input required')
    prefix = RECEIPT_DOMAIN + REFERENCE_CODEC
    if not data.startswith(prefix):
        raise ValueError('wrong domain or example codec')
    cursor = len(prefix)

    def read(expected_tag: int):
        nonlocal cursor
        if cursor + 5 > len(data) or data[cursor] != expected_tag:
            raise ValueError('missing, reordered or unknown field')
        length = struct.unpack('<I', data[cursor + 1:cursor + 5])[0]
        cursor += 5
        if length > MAX_CONTEXT_BYTES or cursor + length > len(data):
            raise ValueError('invalid field length')
        payload = data[cursor:cursor + length]
        cursor += length
        return payload

    if read(0) != REFERENCE_SIGMA_SCHEME:
        raise ValueError('unsupported example sigma scheme')
    values = [read(i) for i in range(1, 7)]
    if cursor != len(data):
        raise ValueError('trailing bytes')
    fields = ReceiptFields(*values)
    fields.validate()
    if encode_receipt_input(fields) != data:
        raise ValueError('noncanonical encoding')
    return fields


def receipt(fields: ReceiptFields) -> bytes:
    return hashlib.sha256(encode_receipt_input(fields)).digest()


def collision_bound(q: int) -> Fraction:
    if type(q) is not int or q < 0:
        raise ValueError('nonnegative integer query bound required')
    return min(Fraction(1), Fraction(q * (q - 1), 2**257))


def scientific(value: Fraction) -> str:
    with localcontext() as ctx:
        ctx.prec = 60
        return format(Decimal(value.numerator) / Decimal(value.denominator), '.12E')


def example() -> ReceiptFields:
    # Opaque *example* context, not a native C encoder. Internal canonicality is
    # assumed; the full bytes, not merely their digest, enter the receipt.
    return ReceiptFields(
        b'EXAMPLE-C:owner=A;nonce=1;model=fixed;profile=example;epoch=1',
        b'\x11' * 32, b'\x22' * 32, b'\x33' * 32,
        hashlib.sha256('4'.encode()).digest(), b'\x55' * 32,
    )


class MOR1Examples(unittest.TestCase):
    def test_reproducible_encoding_and_receipt(self):
        a = example()
        self.assertEqual(decode_receipt_input(encode_receipt_input(a)), a)
        self.assertEqual(receipt(a), receipt(replace(a)))
        self.assertEqual(len(receipt(a)), 32)
        self.assertTrue(encode_receipt_input(a).startswith(RECEIPT_DOMAIN + REFERENCE_CODEC))

    def test_each_context_beacon_id_or_root_changes_hash_input(self):
        a = example()
        for name in FIELDS:
            old = getattr(a, name)
            changed = bytes([old[0] ^ 1]) + old[1:]
            b = replace(a, **{name: changed})
            with self.subTest(field=name):
                self.assertNotEqual(encode_receipt_input(a), encode_receipt_input(b))
                # Observed inequality in a finite example, not a universal proof.
                self.assertNotEqual(receipt(a), receipt(b))

    def test_complete_context_separates_even_for_forced_same_s(self):
        a = example()
        b = replace(a, context=a.context.replace(b'owner=A', b'owner=B'))
        self.assertEqual(a.s, b.s)
        self.assertEqual(a.input_root, b.input_root)
        self.assertEqual(a.output_root, b.output_root)
        self.assertEqual(a.trace_root, b.trace_root)
        self.assertNotEqual(encode_receipt_input(a), encode_receipt_input(b))
        self.assertNotEqual(receipt(a), receipt(b))

    def test_invalid_lengths_and_type_aliases_are_rejected(self):
        a = example()
        for name in FIELDS[1:]:
            for invalid in (b'', b'\0', b'\0' * 31, b'\0' * 33,
                            bytearray(b'\0' * 32), '00' * 32, 0, False):
                with self.subTest(field=name, invalid_type=type(invalid).__name__):
                    with self.assertRaises(ValueError):
                        encode_receipt_input(replace(a, **{name: invalid}))
        for invalid in (b'', b'x' * (MAX_CONTEXT_BYTES + 1), 'context', bytearray(b'context')):
            with self.assertRaises(ValueError):
                encode_receipt_input(replace(a, context=invalid))

    def test_framing_wrong_domain_unknown_fields_and_trailing_bytes(self):
        encoded = encode_receipt_input(example())
        for malformed in (encoded[:-1], encoded + b'\0', b'X' + encoded[1:]):
            with self.assertRaises(ValueError):
                decode_receipt_input(malformed)
        offset = len(RECEIPT_DOMAIN + REFERENCE_CODEC)
        unknown_tag = encoded[:offset] + b'\xff' + encoded[offset + 1:]
        wrong_length = encoded[:offset + 1] + struct.pack('<I', 0xffffffff) + encoded[offset + 5:]
        for malformed in (unknown_tag, wrong_length):
            with self.assertRaises(ValueError):
                decode_receipt_input(malformed)

    def test_same_natural_language_answer_can_have_different_outputs(self):
        answer = '4'
        a = example()
        b = replace(a, context=a.context.replace(b'nonce=1', b'nonce=2'), s=b'\x66' * 32,
                    input_root=b'\x77' * 32, trace_root=b'\x88' * 32)
        self.assertEqual(a.output_root, b.output_root)
        self.assertNotEqual((answer, a.s, receipt(a)), (answer, b.s, receipt(b)))
        # No inference was executed here: this demonstrates the output format only.

    def test_a_receipt_can_be_computed_for_unverified_or_fake_roots(self):
        fake = replace(example(), input_root=b'\0' * 32, output_root=b'\0' * 32,
                       trace_root=b'\0' * 32)
        self.assertEqual(len(receipt(fake)), 32)
        self.assertEqual(decode_receipt_input(encode_receipt_input(fake)), fake)
        # Success here is intentional. Correct receipt equality is NOT a trace
        # validity proof; Panel/Court and native Final conditions remain required.

    def test_exact_birthday_arithmetic(self):
        self.assertEqual(collision_bound(0), 0)
        self.assertEqual(collision_bound(1), 0)
        self.assertEqual(collision_bound(2), Fraction(1, 2**256))
        self.assertEqual(scientific(collision_bound(10**12)), '4.318084277543E-54')
        self.assertEqual(scientific(collision_bound(2**64)), '1.469367938528E-39')
        self.assertEqual(collision_bound(2**129), 1)
        for invalid in (-1, 1.0, True, '12'):
            with self.assertRaises(ValueError):
                collision_bound(invalid)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path(__file__).with_name('mor1_reference_results.json'))
    args = parser.parse_args()
    run = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(MOR1Examples))
    a = example()
    data = {
        'status': 'REFERENCE_EXAMPLES_ONLY_NOT_NATIVE_WIRE',
        'test_groups': run.testsRun, 'failures': len(run.failures), 'errors': len(run.errors),
        'passed': run.wasSuccessful(),
        'script_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        'example_codec': REFERENCE_CODEC.decode('ascii').rstrip('\0'),
        'receipt_domain_hex': RECEIPT_DOMAIN.hex(),
        'native_wire_implemented': False,
        'opaque_context_canonicality_verified': False,
        'example_vector': {
            'context_hex': a.context.hex(), 'sigma_hex': a.sigma.hex(), 's_hex': a.s.hex(),
            'input_root_hex': a.input_root.hex(), 'output_root_hex': a.output_root.hex(),
            'trace_root_hex': a.trace_root.hex(),
            'encoded_receipt_input_hex': encode_receipt_input(a).hex(), 'rho_hex': receipt(a).hex(),
        },
        'birthday_examples': [
            {'query_bound': q, 'exact_numerator': collision_bound(q).numerator,
             'exact_denominator': collision_bound(q).denominator,
             'decimal_upper_bound': scientific(collision_bound(q))}
            for q in (10**12, 2**40, 2**64)
        ],
        'limitations': [
            'Finite examples do not prove hash collision resistance or computation cost.',
            'Birthday bounds assume the classical random-oracle model and a total query upper bound.',
            'C is assumed canonical opaque bytes; native C semantics, account authentication and eligibility are not verified.',
            'The example sigma scheme is 32-byte only; no native beacon suite or wire identifier is assigned.',
            'No Qwen execution, full trace verification, Panel/Court networking, Final, or one-time credit is implemented.',
            'Arbitrary unverified roots can produce a receipt; its equality alone is not inference validity.',
            'The AN1 Ticket formula and existing wire/claims are not changed by this example.',
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps({key: data[key] for key in ('status', 'test_groups', 'failures', 'errors', 'passed')}, ensure_ascii=False))
    return 0 if run.wasSuccessful() else 1


if __name__ == '__main__':
    raise SystemExit(main())

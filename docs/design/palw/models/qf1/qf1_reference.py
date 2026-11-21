#!/usr/bin/env python3
"""QF1 logical position/byte/hash reference. Not a Qwen runner or native validator."""
from __future__ import annotations

import hashlib
import struct
from dataclasses import dataclass
from typing import Sequence

LAYERS = 36
WIDTH = 4096
STATE_ROLES = 37
MAX_POSITIONS = 8192
MAX_NEW_TOKENS = 1024
EOS = frozenset((151643, 151645))
VOCAB = 151936
CHUNK_BYTES = 4096
DOM_Z = b"misaka/palw/qf1/readout/v1\0"
DOM_ATTEMPT = b"misaka/palw/qf1/attempt/v1\0"
DOM_MERKLE = b"misaka/palw/qf1/merkle/v1\0"
DOM_TICKET = b"misaka/palw/qf1/ticket/v1\0"


def uint(v: int, bits: int, label: str = "integer") -> int:
    if type(v) is not int or not 0 <= v < (1 << bits):
        raise ValueError(label + ": noncanonical unsigned integer")
    return v


def le(v: int, bits: int) -> bytes:
    return uint(v, bits).to_bytes(bits // 8, "little")


def exact_bytes(v: bytes, length: int, label: str) -> bytes:
    if type(v) is not bytes or len(v) != length:
        raise ValueError(label + ": wrong byte type/length")
    return v


def token(v: int) -> int:
    uint(v, 32, "token")
    if v >= VOCAB:
        raise ValueError("token outside vocabulary")
    return v


def b2(m: bytes, n: int = 64) -> bytes:
    return hashlib.blake2b(m, digest_size=n).digest()


def hex_frame(s: bytes, table: Sequence[int]) -> list[int]:
    exact_bytes(s, 32, "attempt")
    if len(table) != 16:
        raise ValueError("hex table length")
    values = [token(t) for t in table]
    if len(set(values)) != 16:
        raise ValueError("hex table not injective")
    # Ordinary-token status and tokenizer provenance require registration validation.
    return [values[n] for b in s for n in (b >> 4, b & 15)]


def schedule(prompt: Sequence[int], generated: Sequence[int], limit: int) -> dict:
    uint(limit, 16, "limit")
    if not 1 <= limit <= MAX_NEW_TOKENS or not prompt:
        raise ValueError("prompt/limit")
    p = [token(t) for t in prompt]
    y = [token(t) for t in generated]
    if len(p) + limit - 1 > MAX_POSITIONS:
        raise ValueError("position budget")
    if not 1 <= len(y) <= limit:
        raise ValueError("generation length")
    if any(t in EOS for t in y[:-1]):
        raise ValueError("generation continued after EOS")
    if y[-1] in EOS:
        reason = "EOS"
    elif len(y) == limit:
        reason = "LIMIT"
    else:
        raise ValueError("premature success stop")
    return {
        "decision_positions": list(range(len(p) - 1, len(p) + len(y) - 1)),
        "decision_input_tokens": [p[-1]] + y[:-1],
        "processed_positions": len(p) + len(y) - 1,
        "last_selected_token_is_forwarded": False,
        "stop_reason": reason,
    }


def encode_states(vectors: Sequence[Sequence[int]]) -> bytes:
    if len(vectors) != STATE_ROLES:
        raise ValueError("requires 36 residual vectors and one final norm vector")
    result = bytearray()
    for vector in vectors:
        if len(vector) != WIDTH:
            raise ValueError("vector width")
        for v in vector:
            if type(v) is not int or not -32767 <= v <= 32767:
                raise ValueError("noncanonical A16 Code")
            result.extend(v.to_bytes(2, "little", signed=True))
    return bytes(result)


def readout_parts(steps: Sequence[Sequence[Sequence[int]]]):
    if not 1 <= len(steps) <= MAX_NEW_TOKENS:
        raise ValueError("readout step count")
    yield DOM_Z
    for j, vectors in enumerate(steps):
        yield b"\x01" + le(j, 32) + encode_states(vectors)
    yield b"\xff" + le(len(steps), 32)


def readout(steps: Sequence[Sequence[Sequence[int]]]) -> bytes:
    h = hashlib.blake2b(digest_size=64)
    for part in readout_parts(steps):
        h.update(part)
    return h.digest()


def attempt(c_bytes: bytes, sigma_bytes: bytes) -> bytes:
    if type(c_bytes) is not bytes or type(sigma_bytes) is not bytes:
        raise ValueError("context/beacon must be bytes")
    # Does not authenticate C, TB1 or a native freeze; callers must perform those gates.
    return b2(DOM_ATTEMPT + le(len(c_bytes), 32) + c_bytes
              + le(len(sigma_bytes), 32) + sigma_bytes, 32)


def _base(s: bytes, kind: int, count: int) -> bytes:
    exact_bytes(s, 32, "attempt")
    uint(kind, 8, "tree kind")
    uint(count, 64, "leaf count")
    if kind not in (1, 2, 3, 4) or count < 1 or (kind == 3 and count != 1):
        raise ValueError("tree kind/count")
    return s + bytes((kind,)) + le(count, 64)


def real_leaf(s: bytes, kind: int, count: int, index: int, data: bytes) -> bytes:
    base = _base(s, kind, count)
    uint(index, 64, "leaf index")
    if index >= count or type(data) is not bytes:
        raise ValueError("leaf index/bytes")
    if kind in (1, 2):
        exact_bytes(data, 4, "token leaf")
        token(int.from_bytes(data, "little"))
    elif kind == 3:
        exact_bytes(data, 64, "z leaf")
    # TRACE records are opaque here. Full trace grammar is not implemented by this module.
    return b2(DOM_MERKLE + b"\0" + base + le(index, 64) + le(len(data), 32) + data)


def pad_leaf(s: bytes, kind: int, count: int, index: int) -> bytes:
    base = _base(s, kind, count)
    uint(index, 64, "padding index")
    size = 1 << (count - 1).bit_length()
    if not count <= index < size:
        raise ValueError("padding range")
    return b2(DOM_MERKLE + b"\x01" + base + le(index, 64))


def parent(s: bytes, kind: int, count: int, height: int, index: int,
           left: bytes, right: bytes) -> bytes:
    exact_bytes(left, 64, "left digest")
    exact_bytes(right, 64, "right digest")
    return b2(DOM_MERKLE + b"\x02" + _base(s, kind, count) + le(height, 32)
              + le(index, 64) + left + right)


def merkle_levels(s: bytes, kind: int, leaves: Sequence[bytes]) -> list[list[bytes]]:
    count = len(leaves)
    _base(s, kind, count)
    size = 1 << (count - 1).bit_length()
    level = [real_leaf(s, kind, count, i, b) for i, b in enumerate(leaves)]
    level.extend(pad_leaf(s, kind, count, i) for i in range(count, size))
    levels = [level]
    height = 1
    while len(level) > 1:
        level = [parent(s, kind, count, height, j // 2, level[j], level[j + 1])
                 for j in range(0, len(level), 2)]
        levels.append(level)
        height += 1
    return levels


def root(s: bytes, kind: int, leaves: Sequence[bytes]) -> bytes:
    return merkle_levels(s, kind, leaves)[-1][0]


def opening(s: bytes, kind: int, leaves: Sequence[bytes], index: int) -> list[bytes]:
    uint(index, 64, "opening index")
    if index >= len(leaves):
        raise ValueError("opening index")
    levels = merkle_levels(s, kind, leaves)
    siblings = []
    for level in levels[:-1]:
        siblings.append(level[index ^ 1])
        index //= 2
    return siblings


def verify_opening(s: bytes, kind: int, count: int, index: int, value: bytes,
                   siblings: Sequence[bytes], expected_root: bytes) -> bool:
    exact_bytes(expected_root, 64, "root")
    h = real_leaf(s, kind, count, index, value)
    if len(siblings) != (count - 1).bit_length():
        return False
    for height, sibling in enumerate(siblings, 1):
        exact_bytes(sibling, 64, "sibling")
        h = parent(s, kind, count, height, index // 2,
                   sibling if index & 1 else h, h if index & 1 else sibling)
        index //= 2
    return h == expected_root


def ticket(s: bytes, counts: Sequence[int], roots: Sequence[bytes]) -> bytes:
    exact_bytes(s, 32, "attempt")
    if len(counts) != 4 or len(roots) != 4:
        raise ValueError("requires four roots")
    data = DOM_TICKET + s
    for kind, (count, digest) in enumerate(zip(counts, roots), 1):
        _base(s, kind, count)
        exact_bytes(digest, 64, "root")
        data += le(count, 64) + digest
    return b2(data, 32)


def is_winner(ticket_bytes: bytes, theta: int) -> bool:
    exact_bytes(ticket_bytes, 32, "ticket")
    if type(theta) is not int or not 0 <= theta <= 1 << 256:
        raise ValueError("target outside [0,2^256]")
    return int.from_bytes(ticket_bytes, "big") < theta


# Straight transcription of RFC 7693 compression for chunk-state conformance.
# It is not an optimized mining kernel or a verified cryptographic library.
IV = (0x6A09E667F3BCC908, 0xBB67AE8584CAA73B, 0x3C6EF372FE94F82B,
      0xA54FF53A5F1D36F1, 0x510E527FADE682D1, 0x9B05688C2B3E6C1F,
      0x1F83D9ABFB41BD6B, 0x5BE0CD19137E2179)
SIGMA = (
    (0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
    (14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3),
    (11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4),
    (7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8),
    (9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13),
    (2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9),
    (12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11),
    (13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10),
    (6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5),
    (10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0),
)
MASK = (1 << 64) - 1


def _ror(x: int, n: int) -> int:
    return ((x >> n) | (x << (64 - n))) & MASK


def compress(h: tuple[int, ...], block: bytes, counter: int, final: bool) -> tuple[int, ...]:
    exact_bytes(block, 128, "compression block")
    uint(counter, 64, "byte counter")
    if len(h) != 8 or type(final) is not bool:
        raise ValueError("compression state/flag")
    for word in h:
        uint(word, 64, "chain word")
    m = struct.unpack("<16Q", block)
    v = list(h) + list(IV)
    v[12] ^= counter
    if final:
        v[14] ^= MASK

    def g(a, b, c, d, x, y):
        v[a] = (v[a] + v[b] + x) & MASK
        v[d] = _ror(v[d] ^ v[a], 32)
        v[c] = (v[c] + v[d]) & MASK
        v[b] = _ror(v[b] ^ v[c], 24)
        v[a] = (v[a] + v[b] + y) & MASK
        v[d] = _ror(v[d] ^ v[a], 16)
        v[c] = (v[c] + v[d]) & MASK
        v[b] = _ror(v[b] ^ v[c], 63)

    for r in range(12):
        s = SIGMA[r % 10]
        for k, ids in enumerate(((0, 4, 8, 12), (1, 5, 9, 13), (2, 6, 10, 14), (3, 7, 11, 15),
                                 (0, 5, 10, 15), (1, 6, 11, 12), (2, 7, 8, 13), (3, 4, 9, 14))):
            g(*ids, m[s[2 * k]], m[s[2 * k + 1]])
    return tuple(h[i] ^ v[i] ^ v[i + 8] for i in range(8))


@dataclass(frozen=True)
class ChunkRecord:
    index: int
    consumed: int
    length: int
    final_flag: int
    chain_after: bytes


def chunk_records(message: bytes) -> list[ChunkRecord]:
    if type(message) is not bytes or not message:
        raise ValueError("QF1 message must be nonempty bytes")
    h = (IV[0] ^ 0x01010040,) + IV[1:]
    records = []
    for offset in range(0, len(message), CHUNK_BYTES):
        data = message[offset:offset + CHUNK_BYTES]
        is_last = offset + len(data) == len(message)
        for i in range(0, len(data), 128):
            part = data[i:i + 128]
            h = compress(h, part.ljust(128, b"\0"), offset + i + len(part),
                         is_last and i + len(part) == len(data))
        records.append(ChunkRecord(offset // CHUNK_BYTES, offset + len(data), len(data),
                                   int(is_last), struct.pack("<8Q", *h)))
    return records


def verify_chunks(message: bytes, records: Sequence[ChunkRecord], z: bytes) -> bool:
    exact_bytes(z, 64, "z")
    for r in records:
        uint(r.index, 32); uint(r.consumed, 64); uint(r.length, 16); uint(r.final_flag, 8)
        exact_bytes(r.chain_after, 64, "chain")
        if not 1 <= r.length <= CHUNK_BYTES or r.final_flag not in (0, 1):
            return False
    expected = chunk_records(message)
    return list(records) == expected and expected[-1].chain_after == z


def synthetic_step(j: int) -> list[list[int]]:
    """Public toy data for format tests; never claim these are Qwen states."""
    return [[((j + 1) * 257 + l * 31 + c * 17) % 65535 - 32767
             for c in range(WIDTH)] for l in range(STATE_ROLES)]

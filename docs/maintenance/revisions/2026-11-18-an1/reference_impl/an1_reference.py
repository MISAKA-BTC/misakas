#!/usr/bin/env python3
"""Independent PALW-AN1 toy implementation, not the linked author's source.

Panel re-execution and authenticated local Court evidence only. This is not a
cryptographic computation proof, production LLM, signature implementation,
beacon implementation, network, DB, or consensus implementation.
"""
from __future__ import annotations

import copy
import hashlib
import json
from dataclasses import dataclass, field
from typing import Any


PROFILE = {
    "name": "AN1-INDEPENDENT-TOY-20261118",
    "status": "DRAFT_NOT_ACTIVATED",
    "vocabulary": 259,
    "dimension": 2,
    "generation_length": 2,
    "weight_count": 1060,
    "integer": "checked-signed-64; floor division; no floating point",
    "attention": "last-position causal; score=max(0,dot(Q,K))+1",
    "selection": "smallest token index attaining maximum logit",
    "tokens": {"BOS": 0, "POW": 1, "SEP": 2, "byte_offset": 3},
    "maximum_job_tokens": 8,
    "maximum_record_bytes": 512,
    "tree": "next-power-of-two, canonical padding, scoped every hash",
    "encoding": "canonical JSON UTF-8, sorted keys, compact separators, integers only",
    "hash": "SHA-256 with length-delimited fields and role-specific domains",
    "weight_layout": {
        "embedding": [0, 518], "Q": [518, 522], "K": [522, 526],
        "V": [526, 530], "F1": [530, 534], "F2": [534, 538],
        "bias1": [538, 540], "bias2": [540, 542], "output": [542, 1060],
    },
}


def encode(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=True, sort_keys=True,
                      separators=(",", ":"), allow_nan=False).encode("ascii")


def H(domain: str, *parts: bytes) -> bytes:
    data = bytearray()
    for part in (domain.encode("ascii"),) + parts:
        data += len(part).to_bytes(8, "big") + part
    return hashlib.sha256(data).digest()


PROFILE_ID = H("AN1-profile", encode(PROFILE)).hex()
I64_MIN, I64_MAX = -(1 << 63), (1 << 63) - 1
MAX_LEAF_BYTES = 512


def checked(value: int) -> int:
    if type(value) is not int or not I64_MIN <= value <= I64_MAX:
        raise ValueError("non-canonical or overflowing signed-64 value")
    return value


def unsigned64(value: int) -> int:
    if type(value) is not int or not 0 <= value < (1 << 64):
        raise ValueError("non-canonical unsigned-64 integer")
    return value


def hex32(value: str) -> str:
    if (type(value) is not str or len(value) != 64 or value != value.lower()
            or bytes.fromhex(value).hex() != value):
        raise ValueError("non-canonical 32-byte hexadecimal value")
    return value


def canonical_record(raw: bytes, keys: set[str]) -> dict:
    obj = json.loads(raw)
    if type(obj) is not dict or set(obj) != keys or encode(obj) != raw:
        raise ValueError("non-canonical record")
    return obj


def parse_value(raw: bytes, index: int) -> int:
    obj = canonical_record(raw, {"i", "v"})
    if type(obj["i"]) is not int or obj["i"] != index:
        raise ValueError("incorrect record index")
    return checked(obj["v"])


def pad_record(index: int) -> bytes:
    return encode({"padding": index, "value": 0})


@dataclass(frozen=True)
class Opening:
    index: int
    payload: bytes
    siblings: tuple[bytes, ...]


class Tree:
    """All real leaves, padding leaves, and internal nodes are context bound."""

    def __init__(self, kind: str, scope: str, records: list[bytes],
                 padding_overrides: dict[int, bytes] | None = None):
        if not records:
            raise ValueError("empty trees are not part of this toy profile")
        if any(type(record) is not bytes or len(record) > MAX_LEAF_BYTES for record in records):
            raise ValueError("noncanonical or oversized tree record")
        self.kind, self.scope, self.n = kind, scope, len(records)
        self.L = 1 << (self.n - 1).bit_length()
        padding_overrides = padding_overrides or {}
        if any(type(i) is not int or not self.n <= i < self.L
               for i in padding_overrides):
            raise ValueError("invalid padding position")
        if any(type(record) is not bytes or len(record) > MAX_LEAF_BYTES
               for record in padding_overrides.values()):
            raise ValueError("noncanonical or oversized padding record")
        self.records = list(records) + [padding_overrides.get(i, pad_record(i))
                                       for i in range(self.n, self.L)]
        self.levels = [[self.leaf_hash(i, payload)
                        for i, payload in enumerate(self.records)]]
        self.hash_inputs: list[bytes] = []
        for i, payload in enumerate(self.records):
            self.hash_inputs.append(self.leaf_preimage(i, payload))
        height = 1
        while len(self.levels[-1]) > 1:
            old = self.levels[-1]
            level = []
            for index in range(len(old) // 2):
                pre = self.node_preimage(height, index, old[2 * index], old[2 * index + 1])
                self.hash_inputs.append(pre)
                level.append(hashlib.sha256(pre).digest())
            self.levels.append(level)
            height += 1
        self.root = self.levels[-1][0].hex()

    @staticmethod
    def preimage(domain: str, kind: str, scope: str, n: int, L: int,
                 coordinates: list[int], extra: list[bytes]) -> bytes:
        parts = [domain.encode(), kind.encode(), scope.encode(),
                 encode([n, L] + coordinates)] + extra
        return b"".join(len(p).to_bytes(8, "big") + p for p in parts)

    def leaf_preimage(self, index: int, payload: bytes) -> bytes:
        return self.preimage("AN1-real-leaf" if index < self.n else "AN1-padding-leaf",
                             self.kind, self.scope, self.n, self.L, [index], [payload])

    def leaf_hash(self, index: int, payload: bytes) -> bytes:
        return hashlib.sha256(self.leaf_preimage(index, payload)).digest()

    def node_preimage(self, height: int, index: int, left: bytes, right: bytes) -> bytes:
        return self.preimage("AN1-internal", self.kind, self.scope, self.n,
                             self.L, [height, index], [left, right])

    def opening(self, index: int) -> Opening:
        if type(index) is not int or not 0 <= index < self.L:
            raise ValueError("opening index")
        j = index
        siblings = []
        for level in self.levels[:-1]:
            siblings.append(level[j ^ 1])
            j //= 2
        return Opening(index, self.records[index], tuple(siblings))


def verify_opening(kind: str, scope: str, n: int, root: str, opening: Opening) -> bool:
    try:
        if type(n) is not int or n < 1:
            return False
        L = 1 << (n - 1).bit_length()
        if (type(opening.index) is not int or not 0 <= opening.index < L
                or len(opening.siblings) != L.bit_length() - 1
                or any(type(s) is not bytes or len(s) != 32 for s in opening.siblings)
                or type(opening.payload) is not bytes or len(opening.payload) > MAX_LEAF_BYTES):
            return False
        index = opening.index
        pre = Tree.preimage("AN1-real-leaf" if index < n else "AN1-padding-leaf",
                            kind, scope, n, L, [index], [opening.payload])
        digest = hashlib.sha256(pre).digest()
        for height, sibling in enumerate(opening.siblings, 1):
            pair = [digest, sibling] if index % 2 == 0 else [sibling, digest]
            index //= 2
            pre = Tree.preimage("AN1-internal", kind, scope, n, L,
                                [height, index], pair)
            digest = hashlib.sha256(pre).digest()
        return digest.hex() == root
    except (AttributeError, TypeError, ValueError, OverflowError):
        return False


@dataclass(frozen=True)
class Program:
    instructions: tuple[dict, ...]
    output_refs: tuple[int, ...]
    input_length: int
    tree: Tree = field(compare=False, repr=False)


def compile_program(input_length: int) -> Program:
    if type(input_length) is not int or not 35 <= input_length <= 43:
        raise ValueError("unsupported input length")
    instructions: list[dict] = []

    def put(op: str, args=(), imm=()) -> int:
        i = len(instructions)
        if any(type(j) is not int or not 0 <= j < i for j in args):
            raise ValueError("non-causal program")
        instructions.append({"i": i, "op": op, "args": list(args), "imm": list(imm)})
        return i

    def matvec(values, base, rows=2, bias=None):
        outputs = []
        for row in range(rows):
            products = []
            for col in range(2):
                weight = put("WEIGHT", imm=[base + row * 2 + col])
                products.append(put("MUL", [values[col], weight]))
            total = put("ADD", products)
            if bias is not None:
                total = put("ADD", [total, put("WEIGHT", imm=[bias + row])])
            outputs.append(total)
        return outputs

    zero = put("CONST", imm=[0])
    one = put("CONST", imm=[1])
    tokens = [put("INPUT", imm=[i]) for i in range(input_length)]
    output_refs = []
    for _ in range(PROFILE["generation_length"]):
        embeddings = [[put("EMBED", [token], [coord]) for coord in range(2)]
                      for token in tokens]
        keys = [matvec(e, 522) for e in embeddings]
        values = [matvec(e, 526) for e in embeddings]
        query = matvec(embeddings[-1], 518)
        scores = []
        for key in keys:
            dot = put("ADD", [put("MUL", [query[k], key[k]]) for k in range(2)])
            scores.append(put("ADD", [put("MAX0", [dot]), one]))
        denominator = zero
        numerators = [zero, zero]
        for j, score in enumerate(scores):
            denominator = put("ADD", [denominator, score])
            for k in range(2):
                numerators[k] = put("ADD", [numerators[k], put("MUL", [score, values[j][k]])])
        attention = [put("DIV", [numerators[k], denominator]) for k in range(2)]
        residual = [put("ADD", [attention[k], embeddings[-1][k]]) for k in range(2)]
        middle = [put("MAX0", [v]) for v in matvec(residual, 530, bias=538)]
        projected = matvec(middle, 534, bias=540)
        hidden = [put("ADD", [projected[k], residual[k]]) for k in range(2)]
        logits = matvec(hidden, 542, rows=259)
        best_index, best_value = zero, logits[0]
        for i in range(1, 259):
            best_index = put("ARGMAX_UPDATE", [best_index, best_value, logits[i]], [i])
            best_value = put("MAX", [best_value, logits[i]])
        output_refs.append(best_index)
        tokens.append(best_index)
    scope = H("AN1-program-scope", bytes.fromhex(PROFILE_ID), encode(input_length)).hex()
    tree = Tree("program", scope, [encode(x) for x in instructions])
    return Program(tuple(instructions), tuple(output_refs), input_length, tree)


def expected_weight_index(instruction: dict, parent_values: list[int]) -> int | None:
    if instruction["op"] == "WEIGHT":
        return instruction["imm"][0]
    if instruction["op"] == "EMBED":
        token = parent_values[0]
        if not 0 <= token < 259:
            raise ValueError("non-token embedding input")
        return token * 2 + instruction["imm"][0]
    return None


def evaluate(instruction: dict, parents: list[int], input_token=None, weight=None) -> int:
    op = instruction["op"]
    if op == "CONST":
        result = instruction["imm"][0]
    elif op == "INPUT":
        if type(input_token) is not int or not 0 <= input_token < 259:
            raise ValueError("input token range")
        result = input_token
    elif op in ("WEIGHT", "EMBED"):
        result = weight
    elif op == "ADD":
        result = parents[0] + parents[1]
    elif op == "MUL":
        result = parents[0] * parents[1]
    elif op == "MAX0":
        result = max(0, parents[0])
    elif op == "MAX":
        result = max(parents)
    elif op == "DIV":
        if parents[1] <= 0:
            raise ValueError("non-positive attention denominator")
        result = parents[0] // parents[1]
    elif op == "ARGMAX_UPDATE":
        result = instruction["imm"][0] if parents[2] > parents[1] else parents[0]
    else:
        raise ValueError("unknown instruction")
    return checked(result)


def model_tree(weights: list[int]) -> Tree:
    if len(weights) != 1060:
        raise ValueError("weight count")
    return Tree("model", PROFILE_ID,
                [encode({"i": i, "v": checked(v)}) for i, v in enumerate(weights)])


def deterministic_weights(seed: int = 23) -> list[int]:
    return [int.from_bytes(H("AN1-test-weight", encode([seed, i])), "big") % 5 - 2
            for i in range(1060)]


def job_tree(job_tokens: list[int]) -> Tree:
    if not 1 <= len(job_tokens) <= 8:
        raise ValueError("toy job must contain 1..8 tokens")
    for token in job_tokens:
        if type(token) is not int or not 0 <= token < 259:
            raise ValueError("job token")
    return Tree("job", PROFILE_ID,
                [encode({"i": i, "v": t}) for i, t in enumerate(job_tokens)])


CONTEXT_FIELDS = {"chain", "profile_id", "epoch", "beacon_key", "beacon_round",
                  "owner", "model_root", "job_root", "nonce", "target", "deadline",
                  "panel_event", "accounting_version"}


def validate_context(context: dict) -> None:
    if type(context) is not dict or set(context) != CONTEXT_FIELDS:
        raise ValueError("context schema")
    if (context["profile_id"] != PROFILE_ID or context["accounting_version"] not in
            {"AN1-TEST-UNIT-1", "AN1-TEST-USEFUL-ONCE-1"}):
        raise ValueError("unsupported profile or accounting")
    for key in ["owner", "model_root", "job_root", "nonce"]:
        hex32(context[key])
    for key in ["epoch", "beacon_round", "deadline", "panel_event"]:
        unsigned64(context[key])
    if type(context["target"]) is not int or not 0 <= context["target"] <= (1 << 256):
        raise ValueError("strict Ticket threshold must use 257-bit endpoint")
    for key in ["chain", "beacon_key"]:
        if type(context[key]) is not str or not context[key] or len(context[key]) > 64:
            raise ValueError("context name")


def attempt_id(context: dict, sigma: bytes) -> str:
    validate_context(context)
    if type(sigma) is not bytes or len(sigma) != 32:
        raise ValueError("canonical beacon output")
    return H("AN1-attempt", encode(context), sigma).hex()


def slot_id(context: dict) -> str:
    validate_context(context)
    return H("AN1-slot", encode(context)).hex()


def derive_input(attempt: str, job_tokens: list[int]) -> list[int]:
    job_tree(job_tokens)  # Validate the token range and length.
    return [0, 1] + [3 + b for b in bytes.fromhex(attempt)] + [2] + list(job_tokens)


def computational_work_key(context: dict, sigma: bytes, job_tokens: list[int]) -> str:
    # Uses actual canonical tokens, not an alias of their attempt-scoped Root.
    tokens = derive_input(attempt_id(context, sigma), job_tokens)
    return H("AN1-computational-work", bytes.fromhex(context["model_root"]),
             bytes.fromhex(PROFILE_ID), encode(tokens), encode(2)).hex()


def base_work_key(context: dict) -> str:
    # A toy canonical original-job identity, not a proof of human demand.
    return H("AN1-base-useful-work", bytes.fromhex(context["model_root"]),
             bytes.fromhex(context["job_root"]), bytes.fromhex(PROFILE_ID), encode(2)).hex()


def ticket(attempt: str, rx: str, ry: str, rt: str) -> int:
    return int.from_bytes(H("AN1-ticket", *[bytes.fromhex(x) for x in [attempt, rx, ry, rt]]), "big")


@dataclass(frozen=True)
class Claim:
    slot: str
    owner: str
    attempt: str
    input_root: str
    output_root: str
    trace_root: str
    program_root: str
    trace_count: int
    output_count: int = 2

    def record(self) -> dict:
        validate_claim(self)
        return dict(self.__dict__)

    @property
    def original_id(self) -> str:
        return H("AN1-original", encode(self.record())).hex()

    @property
    def ticket(self) -> int:
        validate_claim(self)
        return ticket(self.attempt, self.input_root, self.output_root, self.trace_root)


def validate_claim(claim: Claim) -> None:
    if type(claim) is not Claim:
        raise ValueError("claim object schema")
    for name in ["slot", "owner", "attempt", "input_root", "output_root", "trace_root", "program_root"]:
        hex32(getattr(claim, name))
    for name in ["trace_count", "output_count"]:
        value = unsigned64(getattr(claim, name))
        if value == 0:
            raise ValueError("empty claim array")


@dataclass
class Material:
    weights: list[int]
    trace_records: list[bytes]
    output_records: list[bytes]
    trace_padding: dict[int, bytes] = field(default_factory=dict)
    # Deliberately untrusted conveniences. Verification never relies on them.
    claimed_model_root: str | None = None
    claimed_trace_root: str | None = None


@dataclass(frozen=True)
class NativeContext:
    """Must be constructed from authenticated state, never from Court payload."""
    registration: dict
    sigma: bytes
    job_tokens: tuple[int, ...]
    claim: Claim
    program: Program
    input_tree: Tree

    def court_view(self) -> CourtContext:
        return CourtContext(self.registration["model_root"], self.claim,
                            self.program.tree.scope, self.program.input_length,
                            self.program.output_refs)


@dataclass(frozen=True)
class CourtContext:
    """Only authenticated native metadata: no full model/program/trace arrays."""
    model_root: str
    claim: Claim
    program_scope: str
    input_length: int
    output_refs: tuple[int, ...]


def parse_instruction(raw: bytes, index: int, input_length: int) -> dict:
    ins = canonical_record(raw, {"i", "op", "args", "imm"})
    counts = {"CONST": (0, 1), "INPUT": (0, 1), "WEIGHT": (0, 1),
              "EMBED": (1, 1), "ADD": (2, 0), "MUL": (2, 0),
              "MAX0": (1, 0), "MAX": (2, 0), "DIV": (2, 0),
              "ARGMAX_UPDATE": (3, 1)}
    if (type(ins["i"]) is not int or ins["i"] != index or type(ins["op"]) is not str
            or ins["op"] not in counts or type(ins["args"]) is not list
            or type(ins["imm"]) is not list or (len(ins["args"]), len(ins["imm"])) != counts[ins["op"]]
            or any(type(j) is not int or not 0 <= j < index for j in ins["args"])
            or any(type(v) is not int for v in ins["imm"])):
        raise ValueError("invalid authenticated program record")
    op = ins["op"]
    if op == "CONST":
        checked(ins["imm"][0])
    elif op == "INPUT" and not 0 <= ins["imm"][0] < input_length:
        raise ValueError("input instruction index")
    elif op == "WEIGHT" and not 0 <= ins["imm"][0] < 1060:
        raise ValueError("static weight instruction index")
    elif op == "EMBED" and not 0 <= ins["imm"][0] < 2:
        raise ValueError("embedding coordinate")
    elif op == "ARGMAX_UPDATE" and not 1 <= ins["imm"][0] < 259:
        raise ValueError("argmax token")
    return ins


def build_native_context(context: dict, sigma: bytes, job_tokens: list[int], claim: Claim) -> NativeContext:
    validate_claim(claim)
    s = attempt_id(context, sigma)
    if job_tree(job_tokens).root != context["job_root"]:
        raise ValueError("job bytes do not match registered root")
    tokens = derive_input(s, job_tokens)
    program = compile_program(len(tokens))
    inputs = Tree("input", s, [encode({"i": i, "v": token}) for i, token in enumerate(tokens)])
    if (claim.slot != slot_id(context) or claim.owner != context["owner"]
            or claim.attempt != s or claim.input_root != inputs.root
            or claim.program_root != program.tree.root
            or claim.trace_count != len(program.instructions) or claim.output_count != 2):
        raise ValueError("claim is not the native registered statement")
    return NativeContext(copy.deepcopy(context), sigma, tuple(job_tokens), claim, program, inputs)


def execute(program: Program, tokens: list[int], weights: list[int]) -> tuple[list[int], list[int]]:
    model_tree(weights)  # Checks actual weight bytes/ranges, not an attached root attribute.
    values = []
    for instruction in program.instructions:
        parents = [values[i] for i in instruction["args"]]
        weight_index = expected_weight_index(instruction, parents)
        weight = weights[weight_index] if weight_index is not None else None
        token = tokens[instruction["imm"][0]] if instruction["op"] == "INPUT" else None
        values.append(evaluate(instruction, parents, token, weight))
    return values, [values[i] for i in program.output_refs]


def build_claim(context: dict, sigma: bytes, job_tokens: list[int], weights: list[int]) -> tuple[Claim, Material]:
    if model_tree(weights).root != context["model_root"]:
        raise ValueError("actual weights do not match registered root")
    if job_tree(job_tokens).root != context["job_root"]:
        raise ValueError("actual job does not match registered root")
    s = attempt_id(context, sigma)
    tokens = derive_input(s, job_tokens)
    program = compile_program(len(tokens))
    values, outputs = execute(program, tokens, weights)
    records = [encode({"i": i, "v": v}) for i, v in enumerate(values)]
    output_records = [encode({"i": i, "v": v}) for i, v in enumerate(outputs)]
    rx = Tree("input", s, [encode({"i": i, "v": v}) for i, v in enumerate(tokens)]).root
    rt = Tree("trace", s, records).root
    ry = Tree("output", s, output_records).root
    claim = Claim(slot_id(context), context["owner"], s, rx, ry, rt,
                  program.tree.root, len(records))
    return claim, Material(list(weights), records, output_records)


def material_trees(native: NativeContext, material: Material) -> tuple[Tree, Tree, Tree]:
    return (model_tree(material.weights),
            Tree("trace", native.claim.attempt, material.trace_records, material.trace_padding),
            Tree("output", native.claim.attempt, material.output_records))


def make_step_evidence(native: NativeContext, material: Material, index: int) -> dict:
    model, trace, _ = material_trees(native, material)
    instruction = native.program.instructions[index]
    parents = [trace.opening(i) for i in instruction["args"]]
    evidence = {"kind": "step", "original_id": native.claim.original_id, "index": index,
                "program": native.program.tree.opening(index), "target": trace.opening(index),
                "parents": parents}
    # A malformed authenticated parent can be refuted without a weight lookup.
    try:
        parent_values = [parse_value(p.payload, p.index) for p in parents]
        weight_index = expected_weight_index(instruction, parent_values)
        if weight_index is not None:
            evidence["weight"] = model.opening(weight_index)
    except ValueError:
        pass
    if instruction["op"] == "INPUT":
        evidence["input"] = native.input_tree.opening(instruction["imm"][0])
    return evidence


def make_output_evidence(native: NativeContext, material: Material, index: int) -> dict:
    _, trace, output = material_trees(native, material)
    return {"kind": "output", "original_id": native.claim.original_id, "index": index,
            "output": output.opening(index), "trace": trace.opening(native.program.output_refs[index])}


def make_padding_evidence(native: NativeContext, material: Material, index: int) -> dict:
    _, trace, _ = material_trees(native, material)
    return {"kind": "padding", "original_id": native.claim.original_id, "index": index,
            "target": trace.opening(index)}


def judge_court(native: NativeContext | CourtContext, evidence: dict) -> str:
    """Node local checker: no weights array, no full model, no re-execution.

    Returns InvalidEvidence / ComputationRefuted / NoRefutation. A truthful
    local Court is not an approval of the whole trace.
    """
    # The public CourtContext interface does not contain any full data arrays.
    native = native.court_view() if isinstance(native, NativeContext) else native
    c, s = native.claim, native.claim.attempt
    try:
        if evidence.get("original_id") != c.original_id or type(evidence.get("index")) is not int:
            return "InvalidEvidence"
        i = evidence["index"]
        kind = evidence.get("kind")
        if kind == "padding":
            if not c.trace_count <= i < 1 << (c.trace_count - 1).bit_length():
                return "InvalidEvidence"
            opening = evidence["target"]
            if opening.index != i or not verify_opening("trace", s, c.trace_count, c.trace_root, opening):
                return "InvalidEvidence"
            return "NoRefutation" if opening.payload == pad_record(i) else "ComputationRefuted"
        if kind == "output":
            if not 0 <= i < c.output_count:
                return "InvalidEvidence"
            out, tr = evidence["output"], evidence["trace"]
            expected_ref = native.output_refs[i]
            if (out.index != i or tr.index != expected_ref
                    or not verify_opening("output", s, c.output_count, c.output_root, out)
                    or not verify_opening("trace", s, c.trace_count, c.trace_root, tr)):
                return "InvalidEvidence"
            try:
                outv, tracev = parse_value(out.payload, i), parse_value(tr.payload, expected_ref)
                if not 0 <= outv < 259:
                    return "ComputationRefuted"
            except (ValueError, TypeError, json.JSONDecodeError):
                return "ComputationRefuted"
            return "NoRefutation" if outv == tracev else "ComputationRefuted"
        if kind != "step" or not 0 <= i < c.trace_count:
            return "InvalidEvidence"
        popen, target = evidence["program"], evidence["target"]
        if (popen.index != i or target.index != i
                or not verify_opening("program", native.program_scope, c.trace_count, c.program_root, popen)
                or not verify_opening("trace", s, c.trace_count, c.trace_root, target)):
            return "InvalidEvidence"
        try:
            instruction = parse_instruction(popen.payload, i, native.input_length)
        except (ValueError, TypeError, json.JSONDecodeError):
            return "InvalidEvidence"
        parents = evidence["parents"]
        if (type(parents) is not list or len(parents) != len(instruction["args"])
                or any(o.index != j or not verify_opening("trace", s, c.trace_count, c.trace_root, o)
                       for o, j in zip(parents, instruction["args"]))):
            return "InvalidEvidence"
        try:
            recorded = parse_value(target.payload, i)
            parent_values = [parse_value(o.payload, o.index) for o in parents]
            weight_index = expected_weight_index(instruction, parent_values)
        except (ValueError, TypeError, json.JSONDecodeError):
            return "ComputationRefuted"
        weight = token = None
        if weight_index is not None:
            opening = evidence.get("weight")
            if (not isinstance(opening, Opening) or opening.index != weight_index
                    or not verify_opening("model", PROFILE_ID, 1060,
                                          native.model_root, opening)):
                return "InvalidEvidence"
            try:
                weight = parse_value(opening.payload, weight_index)
            except (ValueError, TypeError, json.JSONDecodeError):
                return "ComputationRefuted"
        if instruction["op"] == "INPUT":
            opening = evidence.get("input")
            expected_index = instruction["imm"][0]
            if (not isinstance(opening, Opening) or opening.index != expected_index
                    or not verify_opening("input", s, native.input_length, c.input_root, opening)):
                return "InvalidEvidence"
            try:
                token = parse_value(opening.payload, expected_index)
            except (ValueError, TypeError, json.JSONDecodeError):
                return "ComputationRefuted"
        try:
            expected = evaluate(instruction, parent_values, token, weight)
        except (ValueError, TypeError, IndexError, ZeroDivisionError):
            return "ComputationRefuted"
        return "NoRefutation" if recorded == expected else "ComputationRefuted"
    except (KeyError, AttributeError, TypeError, ValueError, IndexError):
        return "InvalidEvidence"


def panel_review(native: NativeContext, material: Material | None) -> dict:
    """Authenticate actual bytes, then replay the entire deterministic graph."""
    def problem(status: str, reason: str, kind="trace", index=0) -> dict:
        return {"status": status, "original_id": native.claim.original_id,
                "reason": reason, "required_scope": {"kind": kind, "index": index}}

    def refutation(court: dict) -> dict:
        # Never label an unserviceable/invalid local witness as a completed
        # refutation. The selected Panel must obtain a native material hold.
        if judge_court(native.court_view(), court) == "ComputationRefuted":
            return {"status": "Refuted", "original_id": native.claim.original_id, "court": court}
        return problem("MaterialsUnavailable", "bounded local evidence could not be established")

    if material is None:
        return problem("MaterialsUnavailable", "required material was not supplied")
    if type(material) is not Material:
        return problem("MaterialsUnauthenticated", "material transport schema")
    if type(material.weights) is not list or len(material.weights) != 1060:
        return problem("MaterialsUnauthenticated", "model weight array shape", "model", 0)
    for i, weight in enumerate(material.weights):
        if type(weight) is not int or not I64_MIN <= weight <= I64_MAX:
            return problem("MaterialsUnauthenticated", "model weight encoding or range", "model", i)
    if type(material.trace_records) is not list or len(material.trace_records) != native.claim.trace_count:
        return problem("MaterialsUnauthenticated", "trace shape is not the committed profile shape")
    for i, raw in enumerate(material.trace_records):
        if type(raw) is not bytes or len(raw) > MAX_LEAF_BYTES:
            return problem("MaterialsUnauthenticated", "trace record cannot enter bounded Court", "trace", i)
    if type(material.trace_padding) is not dict:
        return problem("MaterialsUnauthenticated", "padding transport is not a position map")
    for i, raw in material.trace_padding.items():
        if (type(i) is not int or not native.claim.trace_count <= i < 1 << (native.claim.trace_count - 1).bit_length()
                or type(raw) is not bytes or len(raw) > MAX_LEAF_BYTES):
            return problem("MaterialsUnauthenticated", "padding cannot enter bounded Court")
    if type(material.output_records) is not list or len(material.output_records) != 2:
        return problem("MaterialsUnauthenticated", "output shape", "output", 0)
    for i, raw in enumerate(material.output_records):
        if type(raw) is not bytes or len(raw) > MAX_LEAF_BYTES:
            return problem("MaterialsUnauthenticated", "output record cannot enter bounded Court", "output", i)
    try:
        model, trace, output = material_trees(native, material)
        c = native.claim
        if model.root != native.registration["model_root"]:
            return problem("MaterialsUnauthenticated", "model bytes do not match registered root", "model", 0)
        if trace.root != c.trace_root or trace.n != c.trace_count:
            return problem("MaterialsUnauthenticated", "trace bytes do not match original root")
        if output.root != c.output_root or output.n != 2:
            return problem("MaterialsUnauthenticated", "output bytes do not match original root", "output", 0)
        for i in range(trace.n, trace.L):
            if trace.records[i] != pad_record(i):
                return refutation(make_padding_evidence(native, material, i))
        tokens = derive_input(c.attempt, list(native.job_tokens))
        values, outputs = execute(native.program, tokens, material.weights)
        for i, value in enumerate(values):
            if material.trace_records[i] != encode({"i": i, "v": value}):
                return refutation(make_step_evidence(native, material, i))
        for i, value in enumerate(outputs):
            if material.output_records[i] != encode({"i": i, "v": value}):
                return refutation(make_output_evidence(native, material, i))
        return {"status": "ApprovedFullReplay", "checked_records": len(values),
                "output_tokens": outputs}
    except (ValueError, TypeError, IndexError):
        return problem("MaterialsUnauthenticated", "malformed or unserviceable material")


class Ledger:
    """Deterministic toy native state. Authentication is a supplied fixture.

    An event loop's finalization transition models atomicity, not a real
    transactional DB. No result here establishes actual Panel honesty,
    network availability, inclusion, beacon security, or stable consensus.
    """
    def __init__(self, beacons: dict[tuple[str, int], tuple[bytes, int]],
                 panel_events: dict[int, bytes] | None = None,
                 chain_id: str = "an1-independent-test-chain"):
        self.beacons = copy.deepcopy(beacons)
        self.panel_events = copy.deepcopy({100: bytes.fromhex("44" * 32)} if panel_events is None else panel_events)
        for (key, round_number), (value, access_time) in self.beacons.items():
            if type(key) is not str or not key or type(value) is not bytes or len(value) != 32:
                raise ValueError("beacon fixture schema")
            unsigned64(round_number)
            unsigned64(access_time)
        for event, value in self.panel_events.items():
            unsigned64(event)
            if type(value) is not bytes or len(value) != 32:
                raise ValueError("Panel beacon fixture schema")
        self.chain_id = chain_id
        self.rows: dict[str, dict] = {}
        self.consumed: set[str] = set()
        self.attempt_consumed: set[str] = set()
        self.work_consumed: set[str] = set()
        self.useful_consumed: set[str] = set()
        self.registration_slots: dict[str, str] = {}
        self.credits: dict[str, int] = {}
        self.journal: list[tuple] = []

    def register(self, context: dict, job_tokens: list[int], now: int, authenticated_owner: str,
                 funding_ref: str = "authenticated-funding-fixture") -> str:
        unsigned64(now)
        validate_context(context)
        key = (context["beacon_key"], context["beacon_round"])
        _, access_time = self.beacons[key]
        if (now >= access_time or authenticated_owner != context["owner"]
                or context["chain"] != self.chain_id
                or job_tree(job_tokens).root != context["job_root"]
                or not access_time < context["panel_event"] < context["deadline"]):
            raise ValueError("registration must precede earliest beacon access")
        slot = slot_id(context)
        registration_key = H("AN1-registration-key", encode(context)).hex()
        if slot in self.rows or registration_key in self.registration_slots:
            raise ValueError("duplicate registration")
        self.registration_slots[registration_key] = slot
        self.rows[slot] = {"context": copy.deepcopy(context), "job": list(job_tokens),
                           "funding_ref": funding_ref,
                           "claim": None, "panel": None, "pending": set(), "refuted": False,
                           "active": True, "approved": set(), "materials_ready": False,
                           "closed": False, "delivered": False, "final": False}
        return slot

    def seal(self, slot: str, claim: Claim, now: int, authenticated_owner: str) -> None:
        unsigned64(now)
        validate_claim(claim)
        row = self.rows[slot]
        c = row["context"]
        sigma, access_time = self.beacons[(c["beacon_key"], c["beacon_round"])]
        if (row["claim"] is not None or authenticated_owner != c["owner"]
                or not access_time <= now < c["panel_event"] or claim.ticket >= c["target"]):
            raise ValueError("original seal, owner, timing or Ticket")
        build_native_context(c, sigma, row["job"], claim)
        row["claim"] = claim

    def select_panel(self, slot: str, later_seed: bytes, now: int, candidates: list[str]) -> tuple[str, ...]:
        unsigned64(now)
        if (type(candidates) is not list or any(type(actor) is not str or not actor
                or len(actor) > 64 or not actor.isascii() for actor in candidates)):
            raise ValueError("native Panel candidate schema")
        row = self.rows[slot]
        if (row["claim"] is None or row["panel"] is not None
                or now < row["context"]["panel_event"] or len(set(candidates)) < 5
                or type(later_seed) is not bytes or len(later_seed) != 32
                or later_seed != self.panel_events.get(row["context"]["panel_event"])):
            raise ValueError("claim must be sealed before unique later Panel event")
        # A native authenticated later seed is a fixture; not derived from sigma.
        ranked = sorted(set(candidates), key=lambda actor: H("AN1-panel-draw", later_seed,
                        bytes.fromhex(row["claim"].original_id), actor.encode()))
        row["panel"] = tuple(ranked[:5])
        return row["panel"]

    def native(self, slot: str) -> NativeContext:
        row = self.rows[slot]
        c = row["context"]
        sigma, _ = self.beacons[(c["beacon_key"], c["beacon_round"])]
        return build_native_context(c, sigma, row["job"], row["claim"])

    def approve(self, slot: str, authenticated_actor: str) -> None:
        row = self.rows[slot]
        if row["panel"] is None or authenticated_actor not in row["panel"]:
            raise ValueError("not selected Panel")
        # A dishonest selected actor can lie. Safety uses an active honest
        # selected actor with complete inspection and timely negative access.
        row["approved"].add(authenticated_actor)

    def court(self, slot: str, evidence: dict) -> str:
        result = judge_court(self.native(slot), evidence)
        if result == "ComputationRefuted":
            self.rows[slot]["refuted"] = True
        return result

    def material_notice(self, slot: str, authenticated_actor: str, report: dict, now: int) -> str:
        """Models an included, selected-Panel material request, not a fraud verdict.

        Native metadata determines eligibility/scope. A real implementation
        still needs bounded acquisition, response, objective closure and
        capacity. This model deliberately provides no success-on-timeout.
        """
        unsigned64(now)
        row = self.rows[slot]
        c = row["context"]
        if (row["claim"] is None or row["panel"] is None or authenticated_actor not in row["panel"]
                or row["final"] is not False or not c["panel_event"] <= now <= c["deadline"]
                or type(report) is not dict or report.get("original_id") != row["claim"].original_id
                or type(report.get("status")) is not str
                or report["status"] not in {"MaterialsUnavailable", "MaterialsUnauthenticated"}):
            raise ValueError("unqualified material notice")
        scope = report.get("required_scope")
        if (type(scope) is not dict or set(scope) != {"kind", "index"}
                or type(scope["kind"]) is not str or type(scope["index"]) is not int):
            raise ValueError("material scope schema")
        limits = {"trace": 1 << (row["claim"].trace_count - 1).bit_length(),
                  "output": 2, "model": 1060, "input": 35 + len(row["job"]),
                  "program": row["claim"].trace_count}
        if scope["kind"] not in limits or not 0 <= scope["index"] < limits[scope["kind"]]:
            raise ValueError("material scope outside authenticated claim")
        case = H("AN1-material-case", bytes.fromhex(row["claim"].original_id),
                 authenticated_actor.encode(), encode(scope)).hex()
        row["pending"].add(case)
        return case

    def finalize(self, slot: str, now: int) -> bool:
        unsigned64(now)
        row = self.rows[slot]
        c = row["context"]
        if row["claim"] is None:
            return False
        try:
            validate_context(c)
            validate_claim(row["claim"])
        except (ValueError, TypeError):
            return False
        sigma, _ = self.beacons[(c["beacon_key"], c["beacon_round"])]
        work_key = computational_work_key(c, sigma, row["job"])
        useful_key = base_work_key(c) if c["accounting_version"] == "AN1-TEST-USEFUL-ONCE-1" else None
        if (slot in self.consumed or row["final"] is not False
                or slot != slot_id(c) or row["claim"].slot != slot
                or row["claim"].attempt in self.attempt_consumed
                or work_key in self.work_consumed or useful_key in self.useful_consumed
                or row["active"] is not True or not c["panel_event"] <= now <= c["deadline"]
                or type(row["panel"]) is not tuple or len(row["panel"]) != 5
                or type(row["pending"]) is not set or row["pending"] or row["refuted"] is not False
                or type(row["approved"]) is not set or not row["approved"]
                or not row["approved"] <= set(row["panel"])
                or row["materials_ready"] is not True or row["closed"] is not True or row["delivered"] is not True
                or row["claim"].ticket >= c["target"]):
            return False
        # All gate checks and updates belong to one modeled transition.
        # Reconstruct the registered statement again at the state transition.
        try:
            self.native(slot)
        except (ValueError, TypeError, KeyError):
            return False
        previous = (slot, self.credits.get(c["owner"], 0), row["final"], row["claim"].attempt,
                    work_key, useful_key)
        self.consumed.add(slot)
        self.attempt_consumed.add(row["claim"].attempt)
        self.work_consumed.add(work_key)
        if useful_key is not None:
            self.useful_consumed.add(useful_key)
        row["final"] = True
        self.credits[c["owner"]] = previous[1] + 1
        self.journal.append(previous)
        return True

    def apply_batch(self, slot: str, evidences: list[dict], request_final: bool, now: int,
                    material_notices: list[tuple[str, dict]] | None = None) -> dict:
        held = [self.material_notice(slot, actor, report, now)
                for actor, report in (material_notices or [])]
        results = [self.court(slot, evidence) for evidence in evidences]
        final = self.finalize(slot, now) if request_final else False
        return {"material_holds": held, "court_results": results, "final": final}

    def rollback_last_final(self) -> None:
        slot, old_credit, old_final, attempt, work_key, useful_key = self.journal.pop()
        row = self.rows[slot]
        self.consumed.remove(slot)
        self.attempt_consumed.remove(attempt)
        self.work_consumed.remove(work_key)
        if useful_key is not None:
            self.useful_consumed.remove(useful_key)
        row["final"] = old_final
        self.credits[row["context"]["owner"]] = old_credit


def fixture_context(weights=None, job_tokens=None, owner="11" * 32, nonce="22" * 32) -> tuple[dict, bytes, list[int], list[int]]:
    weights = deterministic_weights() if weights is None else weights
    job_tokens = [17, 25] if job_tokens is None else job_tokens
    context = {
        "chain": "an1-independent-test-chain", "profile_id": PROFILE_ID,
        "epoch": 7, "beacon_key": "fixture-key-1", "beacon_round": 13,
        "owner": owner, "model_root": model_tree(weights).root,
        "job_root": job_tree(job_tokens).root, "nonce": nonce,
        "target": 1 << 256, "deadline": 300, "panel_event": 100,
        "accounting_version": "AN1-TEST-UNIT-1",
    }
    return context, bytes.fromhex("33" * 32), job_tokens, weights

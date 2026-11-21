"""Recompute the private class identity from a complete, pinned PALWTIR1 file."""
import argparse
import hashlib
import io
import json
import math
import struct
from pathlib import Path

HERE = Path(__file__).resolve().parent
PIECE = 32768


def exact(stream, count):
    data = stream.read(count)
    if len(data) != count:
        raise ValueError("truncated PALWTIR1")
    return data


def integer(stream, width):
    return int.from_bytes(exact(stream, width), "little")


def field(stream):
    return exact(stream, integer(stream, 4))


def digest(domain, data):
    return hashlib.blake2b(data, key=b"misaka-palw/" + domain).digest()


def join(left, right):
    return digest(b"artifact/node/v1", left + right)


def program_declarations(program):
    stream = io.BytesIO(program)
    exact(stream, 2 + 64)
    if integer(stream, 4) != 151936 or integer(stream, 4) != 1 << 18:
        raise ValueError("wrong token or position bounds")
    declarations = []
    for _ in range(integer(stream, 4)):
        name = field(stream)
        width = [1, 2, 4, 8][integer(stream, 1)]
        shape = [integer(stream, 4) for _ in range(integer(stream, 4))]
        copies = 36 if integer(stream, 1) else 1
        declarations.append((name, width, shape, copies))
    return declarations


def check(path):
    source = (HERE / "integer-weights.json").read_bytes()
    inventory = json.loads(source)["tensors"]
    recipe = (HERE.parents[3] / "misaka/palw/sdk/src/qf1_local/fixed-scale.json").read_bytes()
    pins = json.loads(recipe)
    if hashlib.sha256(source).hexdigest() != pins["integer_inventory_sha256"]:
        raise ValueError("source inventory differs from pinned manifest")
    expected_program = pins["program_sha256"]
    frontier = []
    seen = set()
    leaves = 0
    with path.open("rb") as stream:
        if exact(stream, 10) != b"PALWTIR1\1\0":
            raise ValueError("wrong PALWTIR1 magic/version")
        header = io.BytesIO(field(stream))
        if integer(header, 2) != 1:
            raise ValueError("unexpected header version")
        program = field(header)
        if hashlib.sha256(program).hexdigest() != expected_program:
            raise ValueError("program differs from the independently replayed graph")
        layout = field(header)
        if layout[:6] != struct.pack("<HI", 1, 8192):
            raise ValueError("wrong registered context")
        tokenizer = exact(header, 64)
        tensors = integer(header, 4)
        params = program_declarations(program)
        ordered = [(j, l) for j, (_, _, _, copies) in enumerate(params)
                   for l in (range(copies) if copies > 1 else [None])]
        if len(ordered) != tensors:
            raise ValueError("unexpected instance count")
        for j, layer in ordered:
            index = integer(header, 2)
            flag = integer(header, 1)
            actual_layer = integer(header, 2) if flag else None
            off, size = struct.unpack("<QQ", exact(header, 16))
            name, width, shape, _ = params[j]
            row_bytes = math.prod(shape[1:] if len(shape) >= 2 else shape) * width
            if ((index, actual_layer) != (j, layer) or flag > 1 or size != math.prod(shape)*width
                    or off != (stream.tell()+63)//64*64 or any(exact(stream, off-stream.tell()))):
                raise ValueError("invalid tensor table or padding")
            source_name = ("" if layer is None else f"model.layers.{layer}.") + name.decode()
            codes = hashlib.sha256()
            prefix = struct.pack("<I", len(name)) + name + struct.pack("<I", 0xffffffff if layer is None else layer)
            for row in range(0, size, row_bytes):
                for piece in range(0, row_bytes, PIECE):
                    raw = exact(stream, min(PIECE, row_bytes-piece))
                    codes.update(raw)
                    leaf = digest(b"artifact/leaf/v1", prefix + struct.pack("<II", row+piece, len(raw)) + raw)
                    level = 0
                    while level < len(frontier) and frontier[level] is not None:
                        leaf = join(frontier[level], leaf)
                        frontier[level] = None
                        level += 1
                    if level == len(frontier):
                        frontier.append(leaf)
                    else:
                        frontier[level] = leaf
                    leaves += 1
            if source_name in inventory:
                if codes.hexdigest() != inventory[source_name]["codes_sha256"]:
                    raise ValueError(f"converted source tensor mismatch: {source_name}")
                seen.add(source_name)
        embedded_recipe = field(header)
        if embedded_recipe != recipe:
            raise ValueError("conversion manifest mismatch")
        manifest = json.loads(embedded_recipe)
        if header.read(1) or stream.read(1) or set(inventory) != seen:
            raise ValueError("trailing bytes or missing source tensor")
    root = None
    for node in frontier:
        if node is not None:
            root = node if root is None else join(node, root)
    graph = digest(b"tir/graph-ir-root/v1", program)
    layout_id = digest(b"tir/layout/v1", layout)
    class_id = digest(b"tir/class-id/v1", b"\1\0" + graph + layout_id + root + tokenizer)
    return dict(artifact_bytes=path.stat().st_size, source_tensors=len(seen), parameter_instances=tensors,
                inventory_leaves=leaves, artifact_root=root.hex(), class_id=class_id.hex(),
                graph_ir_root=graph.hex(), layout_digest=layout_id.hex(), tokenizer=tokenizer.hex(),
                manifest=manifest)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifact", type=Path)
    print(json.dumps(check(parser.parse_args().artifact), indent=2))

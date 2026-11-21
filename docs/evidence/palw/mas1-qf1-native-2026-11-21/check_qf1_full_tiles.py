"""Hash the entire ordinary PALWTIR1 on stdin and rederive its flat registration identity."""
import argparse
import hashlib
import io
import json
import math
import struct
import sys
from pathlib import Path

from check_class import HERE, PIECE, digest, exact, field, integer, join, program_declarations
from check_qf1_native_tiles import require, scales, section


class Reader:
    def __init__(self, stream):
        self.stream = stream
        self.sha = hashlib.sha256()
        self.pos = 0

    def read(self, count):
        raw = self.stream.read(count)
        self.sha.update(raw)
        self.pos += len(raw)
        return raw


class Frontier:
    def __init__(self):
        self.levels = []
        self.count = 0

    def push(self, leaf):
        self.count += 1
        for i, held in enumerate(self.levels):
            if held is None:
                self.levels[i] = leaf
                return
            leaf = join(held, leaf)
            self.levels[i] = None
        self.levels.append(leaf)

    def root(self):
        result = None
        for node in self.levels:
            if node is not None:
                result = node if result is None else join(node, result)
        return result


def verify(stream, section_path):
    s = Reader(stream)
    candidate = section(section_path)
    parent = json.loads((HERE / "class-check.json").read_bytes())
    tables = scales()
    require(exact(s, 10) == b"PALWTIR1\1\0", "expected the full ordinary file")
    header = io.BytesIO(field(s))
    require(integer(header, 2) == 1, "unexpected version")
    program, layout, tokenizer = field(header), field(header), exact(header, 64)
    require(hashlib.sha256(program).hexdigest() == candidate["program_sha256"]
            and tokenizer.hex() == parent["tokenizer"], "different program or tokenizer")
    decls = program_declarations(program)
    entries = [(j, l) for j, (_, _, _, qty) in enumerate(decls) for l in (range(qty) if qty > 1 else [None])]
    require(integer(header, 4) == len(entries), "missing tensor instances")
    frontier = Frontier()
    parent_checked = False
    for j, layer in entries:
        index, flag = integer(header, 2), integer(header, 1)
        copy = integer(header, 2) if flag else None
        off, size = struct.unpack("<QQ", exact(header, 16))
        name, width, shape, _ = decls[j]
        require((index, copy) == (j, layer) and flag <= 1 and size == math.prod(shape)*width
                and off == (s.pos + 63)//64*64 and not any(exact(s, off-s.pos)), "bad extent")
        scale = name.startswith(b"tile.")
        if scale and not parent_checked:
            require(frontier.root().hex() == parent["artifact_root"] and frontier.count == parent["inventory_leaves"],
                    "parent weight prefix differs")
            parent_checked = True
        expected = tables.pop((name.decode(),layer)) if scale else None
        stride = width*math.prod(shape[1:] if len(shape)>=2 else shape)
        prefix = struct.pack("<I",len(name))+name+struct.pack("<I",0xffffffff if layer is None else layer)
        for row in range(0,size,stride):
            for i in range(0,stride,PIECE):
                raw = exact(s,min(PIECE,stride-i))
                if expected is not None:
                    require(raw == expected[row+i:row+i+len(raw)], "full tensor differs from scale recipe")
                frontier.push(digest(b"artifact/leaf/v1",prefix+struct.pack("<II",row+i,len(raw))+raw))
    recipe = (HERE.parents[3]/"misaka/palw/sdk/src/qf1_local/residual-tiles.json").read_bytes()
    require(field(header) == hashlib.blake2b(recipe,key=b"MISAKA/private/QF1/residual-tiles/v1").hexdigest().encode()
            and not header.read(1) and not s.read(1) and parent_checked and not tables, "incomplete or trailing data")
    root=frontier.root()
    class_id=digest(b"tir/class-id/v1",b"\1\0"+digest(b"tir/graph-ir-root/v1",program)+digest(b"tir/layout/v1",layout)+root+tokenizer)
    return dict(full_bytes=s.pos,full_sha256=s.sha.hexdigest(),class_id=class_id.hex(),inventory=root.hex(),
                parent=parent["class_id"],instances=len(entries),leaves=frontier.count,program_sha256=candidate["program_sha256"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("section",type=Path)
    args=parser.parse_args()
    print(json.dumps(verify(sys.stdin.buffer,args.section),indent=2))

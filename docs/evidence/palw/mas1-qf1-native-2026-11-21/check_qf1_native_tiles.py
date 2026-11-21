"""Verify the candidate section against calibration, class identity, and native decode reports."""
import argparse
import hashlib
import io
import itertools
import json
import math
import struct
from pathlib import Path

from check_class import HERE, PIECE, digest, exact, field, integer, join, program_declarations


def require(ok, reason):
    if not ok:
        raise ValueError(reason)


def scales():
    data = json.loads((HERE / "qf1-residual-tiles.json").read_bytes())
    stage = data["stage_bits"]
    bits = data["evaluation"]["first_prediction"]["layer_bits"]
    prev = [15] * 64
    tables = {}
    require(len(stage) == len(bits) == 36, "expected 36 layers")

    def epsilon(n):
        return struct.pack("<Q", ((1 << (24 + 2*n)) + 500000) // 1000000)

    def vector(row, fmt):
        return b"".join(struct.pack(fmt, v) * 64 for v in row)

    def factors(row):
        return vector([1 << (max(row)-v) for v in row], "<i")

    for layer, (s, row) in enumerate(zip(stage, bits)):
        require(len(row) == 64 and all(0 <= n <= p for n, p in zip(row, prev)), "invalid residual progression")
        scales = {name + ".eps": epsilon(value) for name, value in
                  [("input", max(prev)), ("post", max(row)), ("q", s["q"]), ("k", s["k"])]}
        scales.update({n + ".delta": struct.pack("b", 10 - s[n]) for n in ("q", "k", "v", "up")})
        scales.update({"input.factor": factors(prev), "post.factor": factors(row),
                       "skip.shift": vector([p-n for p, n in zip(prev, row)], "b"),
                       "o.delta": vector([15-n + s["v"]-10 for n in row], "b"),
                       "down.delta": vector([15-n + s["product"]-10 for n in row], "b"),
                       "product.shift": struct.pack("b", 24+s["up"]-s["product"])})
        tables.update({("tile."+k, layer): v for k, v in scales.items()})
        prev = row
    tables["tile.final.factor", None] = factors(prev)
    tables["tile.final.eps", None] = epsilon(max(prev))
    return tables


def section(path):
    tables = scales()
    parent = json.loads((HERE / "class-check.json").read_bytes())
    leaves = []
    with path.open("rb") as stream:
        require(exact(stream, 10) == b"PALWTIRS\1\0", "wrong file format")
        header = io.BytesIO(field(stream))
        require(integer(header, 2) == 1, "wrong header version")
        program, layout, tokenizer = field(header), field(header), exact(header, 64)
        require(tokenizer.hex() == parent["tokenizer"], "incorrect tokenizer")
        params = program_declarations(program)
        count = integer(header, 4)
        require(count == len(tables), "missing scale instances")
        split = next(j for j,p in enumerate(params) if p[0].startswith(b"tile."))
        expected = [(j, l) for j, (_, _, _, qty) in enumerate(params[split:],split)
                    for l in (range(qty) if qty > 1 else [None])]
        require(len(expected)==count, "incorrect scale declarations")
        for j, layer in expected:
            index, flag = integer(header, 2), integer(header, 1)
            actual_layer = integer(header, 2) if flag else None
            off, size = struct.unpack("<QQ",exact(header, 16))
            require((index, actual_layer) == (j, layer) and flag <= 1 and off == (stream.tell()+63)//64*64
                    and not any(exact(stream, off-stream.tell())), "invalid table index or offset")
            name, width, shape, _ = params[index]
            require(size == math.prod(shape)*width and len(shape)<=1, "invalid scale geometry")
            raw = exact(stream,size)
            require(raw == tables.pop((name.decode(),layer)), "registered scales differ from frozen calibration")
            prefix = struct.pack("<I",len(name)) + name + struct.pack("<I",0xffffffff if layer is None else layer)
            for at in range(0,size,PIECE):
                piece = raw[at:at+PIECE]
                leaves.append(digest(b"artifact/leaf/v1",prefix+struct.pack("<II",at,len(piece))+piece))
        metadata = json.loads(field(header))
        pinned = (HERE.parents[3] / "misaka/palw/sdk/src/qf1_local/residual-tiles.json").read_bytes()
        require(metadata["manifest"] == hashlib.blake2b(pinned,key=b"MISAKA/private/QF1/residual-tiles/v1").hexdigest(),
                "wrong conversion manifest")
        require(not header.read(1) and not stream.read(1) and not tables, "trailing or missing bytes")
    while len(leaves)>1:
        leaves=[join(a,b) if b is not None else a for a,b in itertools.zip_longest(*[iter(leaves)]*2)]
    adapter, = leaves
    record = metadata["composite"]
    require(record["parent_class"]==parent["class_id"] and record["parent_root"]==parent["artifact_root"]
            and record["p"]==split and record["parent_leaves"]==parent["inventory_leaves"]
            and record["adapter_root"]==adapter.hex() and record["adapter_leaves"]==count, "wrong composite record")
    composite=digest(b"improve/composite-artifact/v1", bytes.fromhex(parent["class_id"]+parent["artifact_root"])+adapter+struct.pack("<I",split))
    class_id=digest(b"tir/class-id/v1",b"\1\0"+digest(b"tir/graph-ir-root/v1",program)+digest(b"tir/layout/v1",layout)+composite+tokenizer)
    require(class_id.hex()!=parent["class_id"], "candidate shares the original class id")
    return dict(parent=parent["class_id"], class_id=class_id.hex(), inventory=composite.hex(), adapter_root=adapter.hex(),
                program_sha256=hashlib.sha256(program).hexdigest(), section_bytes=path.stat().st_size, instances=count)


def verify(path, reports):
    observed = section(path)
    expected = json.loads((HERE / "qf1-tile-transfer.json").read_bytes())
    require(len(expected["jobs"])==len(reports)==2, "expected the two held-out questions")
    results=[]
    for job, file in zip(expected["jobs"], reports):
        row=json.loads(file.read_bytes())
        require(row["class"]==observed["class_id"] and all(row[k]==observed[k] for k in
                ("parent","inventory","adapter_root","program_sha256","section_bytes")), "native candidate mismatch")
        require(all(row[k]==job[k] for k in ("input_ids","output_ids","logits_sha256")) and row["stop"]=="Eos",
                "native tokens, termination or full logit vectors differ")
        results.append(dict(source=job["source"],text=job["text"],input_tokens=len(job["input_ids"]),
                            output_ids=row["output_ids"],logits_sha256=row["logits_sha256"]))
    return dict(**observed,jobs=results)


if __name__ == "__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("section",type=Path)
    parser.add_argument("reports",type=Path,nargs=2)
    args=parser.parse_args()
    print(json.dumps(verify(args.section,args.reports),indent=2))

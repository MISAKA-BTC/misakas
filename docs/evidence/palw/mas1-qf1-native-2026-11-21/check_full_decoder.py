"""Layer-major independent replay of all 36 blocks, with verified upstream reconstruction."""
import argparse
import hashlib
import json
import sys
import tempfile
from pathlib import Path

import numpy as np
from check_attention import HERE, inputs, rms
from check_decoder import IntegerLayer, decoder_tail, ms, narrow
from check_embedding import ROW, lift, source_rows
from check_integer_weights import encode
from check_prediction import qf1
from check_output_head import PIN as HEAD_SHARD_SHA
from materialize_integer_layer import materialize


def digest(a, dtype="<i2"):
    return hashlib.sha256(a.astype(dtype).tobytes()).hexdigest()


def heads(matrix, gain):
    return np.array([rms(x,gain,10) for x in matrix.reshape(-1,128)]).reshape(matrix.shape)


def block(data, hidden):
    def gain(name):
        return data.get(name,"codes","codes_sha256","<i4")
    gain_input = gain("input_layernorm")
    norm = np.array([rms(row,gain_input,15) for row in hidden])
    q,k,v = [data.project(f"self_attn.{n}_proj",norm,width,0) for n,width in [("q",4096),("k",1024),("v",1024)]]
    tail,_ = decoder_tail(data,hidden,[norm,q,k,v,heads(q,gain("self_attn.q_norm")),heads(k,gain("self_attn.k_norm"))])
    return tail[-1]


def head_logits(path, normalized, inventory):
    rows,width = 151936,4096
    logits = np.empty((len(normalized),rows),dtype="<i4")
    with path.open("rb") as f:
        n = f.read(8)
        hlen = int.from_bytes(n,"little")
        if not 0 < hlen < 65536:
            raise ValueError("invalid output head header")
        header = f.read(hlen)
        if json.loads(header)["lm_head.weight"]["shape"] != [rows,width]:
            raise ValueError("incorrect head geometry")
        whole = hashlib.sha256(n+header)
        weights = hashlib.sha256()
        peaks = hashlib.sha256()
        for start in range(0,rows,256):
            count = min(256,rows-start)
            raw = f.read(count*width*2)
            if len(raw) != count*width*2:
                raise ValueError("truncated head")
            whole.update(raw)
            wb,pb = encode(raw,width,"i1")
            weights.update(wb)
            peaks.update(pb)
            w = np.frombuffer(wb,dtype="i1").reshape(count,width)
            dots = normalized.astype(np.float64) @ w.astype(np.float64).T
            maxima = (np.frombuffer(pb,dtype="<u2").astype("<u4")<<16).view("<f4").astype(np.float64)
            m,s = zip(*(ms((a/127 if a else 1.)*4) for a in maxima))
            logits[:,start:start+count] = narrow(dots.astype(np.int64),np.array(m),np.array(s),wide=True)
        if f.read(1) or whole.hexdigest()!=HEAD_SHARD_SHA or weights.hexdigest()!=inventory["codes_sha256"] or peaks.hexdigest()!=inventory["absmax_bf16le_sha256"]:
            raise ValueError("output head identity mismatch")
    return logits


def check(args):
    native=json.loads(args.native.read_text())
    peers=inputs(args.log)
    prompt=next(iter(peers.values()))
    answer=native["output_ids"]
    if (len(prompt)!=native["input_positions"] or not 1<=len(answer)<=native["decode_limit"]<=1024
            or len(prompt)+native["decode_limit"]-1>8192 or any(not 0<=i<151936 for i in answer)
            or any(i in qf1.EOS for i in answer[:-1])):
        raise ValueError("invalid transcript lengths or token IDs")
    stop="Eos" if answer[-1] in qf1.EOS else "Limit"
    if stop!=native["stop"] or (stop=="Limit" and len(answer)!=native["decode_limit"]):
        raise ValueError("invalid stop")
    tokens=prompt+answer[:-1]
    raw,_=source_rows(set(tokens))
    rows={int.from_bytes(raw[i:i+4],"little"):np.frombuffer(lift(raw[i+4:i+4+ROW]),dtype="<i2") for i in range(0,len(raw),4+ROW)}
    hidden=np.array([rows[t] for t in tokens])
    manifest=json.loads((HERE/"integer-weights.json").read_text())
    index=args.index.read_bytes()
    captured=[]
    layer_hashes=[]
    for i in range(36):
        with tempfile.TemporaryDirectory() as tmp:
            directory = args.prepared / f"qf1-integer-layer{i}" if args.prepared else None
            if directory is None or not directory.is_dir():
                directory = Path(tmp) / "layer"
                materialize(i,manifest,index,directory)
            hidden=block(IntegerLayer(directory,i,manifest["tensors"]),hidden)
        if i==0 and digest(hidden[:len(prompt)])!=native["prefix_layer0_sha256"]:
            raise ValueError("first block disagrees with native replay")
        captured.append(hidden[len(prompt)-1:].astype("<i2"))
        layer_hashes.append(digest(hidden))
        print(f"verified layer {i+1}/36",file=sys.stderr,flush=True)
    gain_bytes=args.gain.read_bytes()
    source=json.loads((HERE/"upstream-payloads.json").read_text())["tensor_sha256"]
    if hashlib.sha256(gain_bytes).hexdigest()!=source["model.norm.weight"]:
        raise ValueError("final gain source")
    gain,_=encode(gain_bytes,4096,"<i4")
    if hashlib.sha256(gain).hexdigest()!=manifest["tensors"]["model.norm.weight"]["codes_sha256"]:
        raise ValueError("final gain codes")
    normalized=np.array([rms(row,np.frombuffer(gain,dtype="<i4"),15) for row in hidden[len(prompt)-1:]])
    captured.append(normalized)
    vectors=np.stack(captured,axis=1).astype("<i2")
    readout=vectors.tobytes()
    if readout!=args.vectors.read_bytes() or hashlib.sha256(readout).hexdigest()!=native["readout_i16le_sha256"] or len(readout)!=native["readout_bytes"]:
        raise ValueError("prediction vectors differ")
    logits=head_logits(args.head,normalized,manifest["tensors"]["lm_head.weight"])
    if np.argmax(logits,axis=1).tolist()!=answer or [digest(l,"<i4") for l in logits]!=native["logits_sha256"]:
        raise ValueError("greedy output or logits disagree")
    return dict(layer_output_sha256=layer_hashes,positions=len(tokens),vocab=151936,all_logits_matched=True,
                prediction_vectors=vectors.shape[0]*vectors.shape[1],readout_z=qf1.readout(vectors.tolist()).hex(),
                output_ids=answer,stop=stop)


if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    for n in ("native","log","vectors","index","gain","head"):
        p.add_argument(n,type=Path)
    p.add_argument("--prepared",type=Path,help="reuse hash-verified qf1-integer-layerN directories here")
    print(json.dumps(check(p.parse_args()),indent=2))

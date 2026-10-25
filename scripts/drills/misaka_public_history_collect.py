#!/usr/bin/env python3
"""Serial read-only native wRPC census over SSH stdin, writing a NEW local segment.

Initial: --ssh-host misaka-ibm --anchor HASH --output segment1.ndjson --max-pages 1200 --max-seconds 2400 --max-frame-bytes 67108864 --max-segment-bytes 2147483648
Resume from plain or closed .ndjson.gz AFTER prior job terminates: --resume segment1.ndjson [segment2.ndjson ...] --output NEW.ndjson ...
No remote files, configuration, node restarts, DB access, or EVM compatibility verdict.
"""
import argparse
import math
import json
from pathlib import Path
import subprocess
import struct
import sys
from misaka_wrpc_json import WsRpc
from misaka_public_history_analyze import analyze

class BoundedWsRpc(WsRpc):
    """Reject the declared frame length before payload read/allocation."""
    def __init__(self, *args, max_frame_bytes, **kwargs):
        self.max_frame_bytes = max_frame_bytes
        super().__init__(*args, **kwargs)

    def _recv_frame(self):
        first, second = self._read(2)
        opcode, length = first & 0x0f, second & 0x7f
        if length == 126:
            length = struct.unpack('!H', self._read(2))[0]
        elif length == 127:
            length = struct.unpack('!Q', self._read(8))[0]
        if length > self.max_frame_bytes:
            raise ValueError('response frame exceeds max_frame_bytes: '+str(length))
        mask = self._read(4) if second & 0x80 else None
        payload = self._read(length) if length else b''
        if mask:
            payload = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
        return opcode, payload


class BoundedOutput:
    """Hard byte ceiling, reserving terminal stamp space before accepting pages."""
    def __init__(self, stream, maximum, terminal_reserve):
        self.stream, self.maximum, self.terminal_reserve = stream, maximum, terminal_reserve
        self.written = 0

    def emit(self, value, page=False):
        encoded = (json.dumps(value, separators=(',', ':'), ensure_ascii=False)+'\n').encode()
        ceiling = self.maximum - self.terminal_reserve if page else self.maximum
        if self.written + len(encoded) > ceiling:
            if page:
                return False
            raise ValueError('terminal output exceeds max_segment_bytes')
        self.stream.write(encoded)
        self.stream.flush()
        self.written += len(encoded)
        return True


REMOTE = r'''
import datetime,hashlib,time,json
rpc=BoundedWsRpc(host="127.0.0.1",port=config["port"],timeout=config["timeout"],max_frame_bytes=config["max_frame_bytes"])
output=BoundedOutput(sys.stdout.buffer,config["max_segment_bytes"],4*config["max_frame_bytes"]+16384)
def emit(value): return output.emit(value)
def stamp():
    node=rpc.call("getPalwNodeStatus");dag=rpc.call("getBlockDagInfo")
    block=rpc.call("getBlock",{"hash":config["anchor"],"includeTransactions":False})
    return {"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),
            "node":{key:node.get(key) for key in IDENTITY+("pruningImportStable",)},
            "dag":{key:dag.get(key) for key in ("sink","blockCount","pruningPointHash","virtualDaaScore")},"anchor":block}
def validate(value):
    actual,header=stamp_identity(value)
    if header["hash"]!=config["anchor"]:raise ValueError("requested fixed anchor mismatch")
    if config["identity"] is not None and (actual!=config["identity"] or header!=config["anchor_header"]):
        raise ValueError("original identity/anchor changed; refusing reads")
    return actual,header
started=time.monotonic()
failed=False
after_emitted=False
try:
    before=stamp();emit({"kind":"before","data":before});identity,anchor_header=validate(before)
    if config["identity"] is None:config["identity"],config["anchor_header"]=identity,anchor_header
    cursor=config["cursor"] or identity["genesisHash"]
    found=False
    for page in range(config["max_pages"]):
        if time.monotonic()-started>=config["max_seconds"]:
            emit({"kind":"stopped","reason":"max_seconds","cursor":cursor});break
        response=rpc.call("getBlocks",{"lowHash":cursor,"includeBlocks":True,"includeTransactions":False})
        hashes,blocks=response["blockHashes"],response["blocks"]
        if len(hashes)!=len(blocks) or not hashes or hashes[0]!=cursor:raise ValueError("invalid inclusive page shape")
        compact=[]
        for block_hash,block in zip(hashes,blocks):
            if block["header"]["hash"]!=block_hash:raise ValueError("ordered header identity mismatch")
            compact.append({"hash":block_hash,"header":block["header"],"verboseData":block.get("verboseData"),"evmPayloadCount":len(block.get("evmPayload",[]))})
        saved=output.emit({"kind":"page","page":page,"utc":datetime.datetime.now(datetime.timezone.utc).isoformat(),"cursor":cursor,"count":len(hashes),
              "raw_response_sha256":hashlib.sha256(json.dumps(response,sort_keys=True,separators=(",",":")).encode()).hexdigest(),"blocks":compact},page=True)
        if not saved:
            emit({"kind":"stopped","reason":"max_segment_bytes","cursor":cursor,"unsaved_page":page});break
        if config["anchor"] in hashes:found=True;break
        if hashes[-1]==cursor:raise ValueError("pagination made no progress")
        cursor=hashes[-1]
    else:emit({"kind":"stopped","reason":"max_pages","cursor":cursor})
    after=stamp();emit({"kind":"after","data":after,"anchor_found":found,"elapsed_seconds":time.monotonic()-started});after_emitted=True;validate(after)
except Exception as error:
    failed=True
    emit({"kind":"error","error":str(error),"elapsed_seconds":time.monotonic()-started})
    if not after_emitted:
        try:emit({"kind":"after_error","data":stamp()})
        except Exception as final_error:emit({"kind":"after_error_failed","error":str(final_error)})
finally:rpc.close()
raise SystemExit(2 if failed else 0)
'''


def prepare(args):
    if args.ssh_host.startswith('-') or any(c.isspace() for c in args.ssh_host):
        raise ValueError('SSH host must be a single destination, not an option')
    if not 0 < args.port < 65536 or args.max_pages < 1 or not math.isfinite(args.max_seconds) or not math.isfinite(args.timeout) or args.max_seconds <= 0 or args.timeout <= 0:
        raise ValueError('positive bounds and valid TCP port required')
    if args.max_frame_bytes <= 0 or args.max_segment_bytes < 8*args.max_frame_bytes+65536:
        raise ValueError('max_segment_bytes must reserve at least 8*max_frame_bytes+65536 bytes')
    identity = header = cursor = None
    anchor = args.anchor
    if args.resume:
        previous = analyze(args.resume)
        unsafe = [f for f in previous['faults'] if f['kind'] not in ('incomplete_line', 'segment_error')]
        if unsafe or not previous['original_identity'] or not previous['last_complete_page_cursor']:
            raise ValueError('prior stamps/pages are not a safe resume authority: '+json.dumps(unsafe))
        if previous['anchor_present']:
            raise ValueError('anchor already enumerated; analyze missing evidence instead of duplicating scan')
        identity, header, cursor = previous['original_identity'], previous['anchor_header'], previous['last_complete_page_cursor']
        if anchor is not None and anchor != previous['anchor']:
            raise ValueError('resume anchor differs from original')
        anchor = previous['anchor']
    if not isinstance(anchor, str) or len(anchor) != 128 or any(c not in '0123456789abcdef' for c in anchor):
        raise ValueError('exact 128-character lowercase native anchor hash required')
    if any(Path(args.output).resolve() == Path(p).resolve() for p in args.resume):
        raise ValueError('output must be a NEW segment')
    return {'port': args.port, 'timeout': args.timeout, 'max_pages': args.max_pages, 'max_seconds': args.max_seconds,
            'max_frame_bytes': args.max_frame_bytes, 'max_segment_bytes': args.max_segment_bytes,
            'anchor': anchor, 'identity': identity, 'anchor_header': header, 'cursor': cursor}


def remote_program(config):
    import inspect
    from misaka_public_history_analyze import IDENTITY, anchor_block, stamp_identity
    rpc_source = Path(__file__).with_name('misaka_wrpc_json.py').read_text()
    # Existing WsRpc code is reused with its CLI disabled. Values are Python/JSON data,
    # never shell fragments. The only remote command is the fixed "python3 -".
    return ('import json,struct,sys\nscope={"__name__":"misaka_wrpc_module"}\nexec('+repr(compile_source(rpc_source))+',scope)\nWsRpc=scope["WsRpc"]\n'
            +'config='+repr(config)+'\nIDENTITY='+repr(IDENTITY)+'\n'
            +inspect.getsource(BoundedWsRpc)+'\n'+inspect.getsource(BoundedOutput)+'\n'
            +inspect.getsource(anchor_block)+'\n'+inspect.getsource(stamp_identity)+'\n'+REMOTE)


def compile_source(source):
    compile(source, 'misaka_wrpc_json.py', 'exec')
    return source


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--ssh-host', required=True)
    parser.add_argument('--port', type=int, default=26314)
    parser.add_argument('--anchor')
    parser.add_argument('--resume', nargs='*', default=[])
    parser.add_argument('--output', required=True)
    parser.add_argument('--max-pages', type=int, required=True)
    parser.add_argument('--max-seconds', type=float, required=True)
    parser.add_argument('--max-frame-bytes', type=int, required=True)
    parser.add_argument('--max-segment-bytes', type=int, required=True)
    parser.add_argument('--timeout', type=float, default=45)
    args = parser.parse_args()
    config = prepare(args)
    program = remote_program(config)
    compile(program, 'readonly-remote-census', 'exec')
    with open(args.output, 'xb') as output:
        result = subprocess.run(['ssh','-o','BatchMode=yes','-o','ConnectTimeout=8','-o','StrictHostKeyChecking=yes',args.ssh_host,'python3','-'],
                                input=program.encode(), stdout=output, check=False)
    raise SystemExit(result.returncode)


if __name__ == '__main__':
    main()

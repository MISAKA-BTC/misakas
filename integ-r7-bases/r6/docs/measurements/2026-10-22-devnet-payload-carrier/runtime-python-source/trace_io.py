"""Streaming evidence IO. Compression changes storage, never the JSON record meaning."""
import gzip
import json
from pathlib import Path

def open_trace_writer(path):
    path=Path(path)
    if path.suffix=='.gz':return gzip.open(path,'xt',encoding='utf-8',compresslevel=1)
    return path.open('x',encoding='utf-8')

def iter_rows(path):
    path=Path(path)
    opener=gzip.open if path.suffix=='.gz' else open
    with opener(path,'rt',encoding='utf-8') as stream:
        for line in stream:
            if line.strip():yield json.loads(line)

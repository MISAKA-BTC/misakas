"""transformers' auto-model mapping tables (class name -> AutoModel kind), offline. No model is built, nothing is downloaded."""
import json, sys
import transformers
from transformers.models.auto import modeling_auto as ma
out = {}
for name in dir(ma):
    if name.startswith('MODEL_') and name.endswith('_MAPPING_NAMES'):
        v = getattr(ma, name)
        if isinstance(v, dict):
            out[name] = {k: (list(x) if isinstance(x, (list, tuple)) else [x]) for k, x in v.items()}
json.dump({"transformers": transformers.__version__, "tables": out}, sys.stdout, indent=0)

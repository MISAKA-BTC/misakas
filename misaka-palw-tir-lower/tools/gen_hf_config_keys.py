#!/usr/bin/env python3
"""The configuration keys each transformers architecture READS (`src/hf_schema/data/hf-config-keys-v1.json`).

A `config.json` key the reader does not model is refused ("a key that might change the math is refused, never ignored"). For an
architecture that IS a transformers class (not remote code), a key the class's configuration does not define and its modelling code
never reads cannot change the reference's forward pass: transformers loads it as a passive attribute. This tool derives, per
`model_type`, from the installed transformers' own source:

  fields   the parameters of the configuration class's __init__, its attribute_map (both sides), its sub-configs, the base
           PreTrainedConfig attributes
  reads    every `config.<name>`, `self.config.<name>`, `getattr(config, "<name>"…)`, `hasattr(config, "<name>")` and `config.get("<name>")`
           in the architecture's modelling files and in the shared infrastructure they call (rope utils, masking, cache, layers)
  classes  the classes the modelling files define (an `architectures[0]` must be one of them: the reference is that class)

Run with the repository venv:  tir-venv/bin/python misaka-palw-tir-lower/tools/gen_hf_config_keys.py
The file records the transformers version it was derived from; the reader (hf_schema::hf_keys) refuses nothing new because of it: a
key in `reads`/`fields` is exactly as refused as before.
"""
import dataclasses, importlib, inspect, json, os, re, sys, pathlib
import transformers
from transformers import CONFIG_MAPPING
from transformers.configuration_utils import PreTrainedConfig

OUT = pathlib.Path(__file__).resolve().parent.parent / "src" / "hf_schema" / "data" / "hf-config-keys-v1.json"
READ_RES = [
    re.compile(r"\bconfig\.([A-Za-z_][A-Za-z0-9_]*)"),
    re.compile(r"getattr\(\s*(?:self\.)?config\s*,\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']"),
    re.compile(r"hasattr\(\s*(?:self\.)?config\s*,\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']"),
    re.compile(r"\bconfig\.get\(\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']"),
    re.compile(r"\bconfig\[\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']\s*\]"),
]
# A configuration class takes the keys it has no parameter for through **kwargs (legacy spellings: `rope_theta`, `rope_scaling`,
# `use_sliding_window`, …): `kwargs.pop("x")`, `kwargs.get("x")`, `kwargs["x"]`, `"x" in kwargs`.
KWARGS_RES = [
    re.compile(r"\bkwargs\.(?:pop|get|setdefault)\(\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']"),
    re.compile(r"\bkwargs\[\s*[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']\s*\]"),
    re.compile(r"[\"']([A-Za-z_][A-Za-z0-9_]*)[\"']\s+(?:not\s+)?in\s+kwargs\b"),
]
# Names that are methods/attributes of PreTrainedConfig machinery, not keys.
NOT_KEYS = {"to_dict", "to_json_string", "to_diff_dict", "get_text_config", "save_pretrained", "from_pretrained", "update", "get", "keys", "items", "values", "copy", "rope_parameters_validation"}

def reads_in(src: str, kwargs: bool = False) -> set:
    out = set()
    for rx in READ_RES + (KWARGS_RES if kwargs else []):
        out |= set(rx.findall(src))
    return out - NOT_KEYS

root = pathlib.Path(transformers.__file__).parent
shared_files = [root / "modeling_rope_utils.py", root / "modeling_utils.py", root / "masking_utils.py", root / "cache_utils.py",
                root / "modeling_layers.py", root / "modeling_attn_mask_utils.py", root / "modeling_outputs.py", root / "activations.py"]
shared = set()
for f in shared_files:
    if f.exists():
        shared |= reads_in(f.read_text())
# The base configuration's own code reads attributes of `self` (rope normalisation, layer-type validation, …): shared by every model.
shared |= set(re.findall(r"self\.([A-Za-z_][A-Za-z0-9_]*)", (root / "configuration_utils.py").read_text())) - NOT_KEYS
for f in (root / "configuration_utils.py", root / "modeling_rope_utils.py"):
    shared |= reads_in(f.read_text(), kwargs=True)
shared |= set(re.findall(r"self\.([A-Za-z_][A-Za-z0-9_]*)", (root / "modeling_rope_utils.py").read_text())) - NOT_KEYS
base_fields = set(PreTrainedConfig().to_dict().keys()) | set(inspect.signature(PreTrainedConfig.__init__).parameters)
base_fields -= {"self", "kwargs"}

# The model types the built-in adapters claim (by model_type or by architecture class), and the model types of their sub-configs
# (a VLM's text_config is read by the text model's own configuration class).
ADAPTERS = pathlib.Path(__file__).resolve().parent.parent / "adapters"
want_types, want_classes = set(), set()
for f in sorted(ADAPTERS.glob("*.json")):
    m = json.load(open(f)).get("match", {})
    want_types |= set(m.get("model_types", []))
    want_classes |= set(m.get("architectures", [])) | set(m.get("tower_of", []))

def analyse(model_type):
    cls = CONFIG_MAPPING[model_type]
    mod = importlib.import_module(cls.__module__)
    fields = set(base_fields)
    try:
        fields |= set(inspect.signature(cls.__init__).parameters) - {"self", "kwargs", "args"}
    except Exception:
        pass
    if dataclasses.is_dataclass(cls):
        fields |= {f.name for f in dataclasses.fields(cls)}
    am = getattr(cls, "attribute_map", {}) or {}
    fields |= set(am.keys()) | set(am.values())
    fields |= set(getattr(cls, "sub_configs", {}) or {})
    for base in cls.__mro__:
        fields |= {k for k in getattr(base, "__annotations__", {}) if not k.startswith("_")}
    pkg = pathlib.Path(mod.__file__).parent
    reads, classes = set(), set()
    for mf in sorted(pkg.glob("modeling_*.py")):
        reads |= reads_in(mf.read_text())
        try:
            m = importlib.import_module(f"{cls.__module__.rsplit('.', 1)[0]}.{mf.stem}")
        except Exception:
            continue
        for n, c in inspect.getmembers(m, inspect.isclass):
            if c.__module__ == m.__name__:
                classes.add(n)
    # The configuration class's own code can read attributes too (properties, validation).
    cfg_src = pathlib.Path(mod.__file__).read_text()
    reads |= set(re.findall(r"self\.([A-Za-z_][A-Za-z0-9_]*)", cfg_src)) | reads_in(cfg_src, kwargs=True)
    return cls, sorted(classes), (fields | reads)

all_info = {}
for mt in sorted(CONFIG_MAPPING.keys()):
    try:
        all_info[mt] = analyse(mt)
    except Exception:
        continue
for mt, (cls, classes, keys) in list(all_info.items()):
    if set(classes) & want_classes:
        want_types.add(mt)
for mt in list(want_types):
    if mt in all_info:
        for name, sub in (getattr(all_info[mt][0], "sub_configs", {}) or {}).items():
            st = getattr(sub, "model_type", None)
            if st in all_info:
                want_types.add(st)

table = {}
for mt in sorted(want_types):
    if mt not in all_info:
        continue
    cls, classes, keys = all_info[mt]
    table[mt] = {"classes": classes, "keys": sorted(keys - shared)}

OUT.parent.mkdir(parents=True, exist_ok=True)
json.dump({"schema": "misaka.palw.hf-config-keys.v1", "transformers": transformers.__version__, "shared_reads": sorted(shared),
           "models": table}, open(OUT, "w"), separators=(",", ":"), sort_keys=True)
print(f"{len(table)} model types, {OUT.stat().st_size} bytes, transformers {transformers.__version__}")

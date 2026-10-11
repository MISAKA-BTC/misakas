"""Generate misaka-palw-sdk/src/census/inference_tables.rs: (1) transformers' auto-model class -> task (unique only), (2) llama.cpp's
converter: GGUF architecture -> the one task every HF class it is registered for names. Offline; inputs are data files extracted
from the pinned transformers 5.17.0 (automap.json) and llama.cpp @030ebb55 (conv_map.json).

Run in a scratch directory holding this folder's two name lists (HFX, 2026-10-08):

    python3 automap.py > automap.json                      # inside a venv with transformers==5.17.0 (reads its tables; builds no model)
    python3 -I automap_table.py > automap_table.json       # class -> task, unique and ambiguous
    python3 -I conv_map.py <llama.cpp@030ebb55> > conv_map.json
    python3 -I gen_rust_tables.py <repo>/misaka-palw-sdk/src/census/inference_tables.rs

`gguf_v3_names.txt` is the words of `census::listing::task_of_gguf_architecture`'s hand table (an architecture it names keeps its v2
answer); `llama_arch_names.txt` is `LLM_ARCH_NAMES` of llama.cpp @030ebb55 `src/llama-arch.cpp` less `clip`."""
import json, re, sys
am = json.load(open('automap_table.json'))
conv = json.load(open('conv_map.json'))
uniq = am['unique']
SUFFIX = [("ForCausalLM", "text-generation"), ("LMHeadModel", "text-generation"), ("ForSequenceClassification", "text-classification"),
          ("ForTokenClassification", "token-classification"), ("ForQuestionAnswering", "question-answering"), ("ForMaskedLM", "fill-mask"),
          ("ForImageClassification", "image-classification"), ("ForCTC", "automatic-speech-recognition"),
          ("ForSemanticSegmentation", "image-segmentation"), ("ForObjectDetection", "object-detection"), ("ForAudioClassification", "audio-classification")]
def task_of_class(c):
    for s, t in SUFFIX:
        if c.endswith(s):
            return t
    return uniq.get(c)
gguf = {}
gguf_why = {}
V3 = set(open('gguf_v3_names.txt').read().split())
for arch, classes in conv.items():
    if arch.startswith('(') or arch in V3:
        continue
    ts = {task_of_class(c) for c in classes}
    if None in ts or len(ts) != 1:
        continue
    gguf[arch] = next(iter(ts))
    gguf_why[arch] = classes
def rs_str(s):
    return json.dumps(s)
out = []
out.append('//! **Generated data** (HFX, 2026-10-08) for the census task inference v4 (`census::listing::task_of`). Do not edit by hand:')
out.append('//! regenerate with `tools/hf_census/inference_v4/` (its module docstrings name the pinned sources and the order to run them).')
out.append('//!')
out.append('//! * [`HF_AUTOMAP_TASKS_V1`]: transformers ' + am['transformers'] + "'s auto-model tables (`MODEL_FOR_<KIND>_MAPPING_NAMES`), each table read as")
out.append('//!   the pipeline task it serves ([`HF_AUTOMAP_TABLE_TASKS_V1`]); a class listed under exactly ONE task is that task, a class under')
out.append('//!   two (`BartForConditionalGeneration`: fill-mask and text2text) is not listed. `MODEL_MAPPING_NAMES` (bare `AutoModel`), pre-training,')
out.append('//!   backbones and the multimodal-LM table name no task and are not read.')
out.append('//! * [`GGUF_CONVERTER_TASKS_V1`]: llama.cpp @030ebb55\'s converter (`conversion/*.py`, `@ModelBase.register(<HF classes>)` with')
out.append('//!   `model_arch`): a GGUF `general.architecture` whose registered Hugging Face classes ALL name one task (by the head-class suffixes')
out.append('//!   inference v2 reads, then the table above) is that task. An architecture registered for classes of two tasks is not listed, and')
out.append('//!   neither is one the hand table of inference v2 already names (`task_of_gguf_architecture` decides those, unchanged).')
out.append('')
out.append('/// The transformers version the class table was read from.')
out.append('pub const HF_AUTOMAP_SOURCE_V1: &str = "transformers ' + am['transformers'] + '";')
out.append('/// The llama.cpp commit the converter table was read from.')
out.append('pub const GGUF_CONVERTER_SOURCE_V1: &str = "llama.cpp 030ebb558a5820b444a8f836ed5cdd46c9b4bd7a (conversion/*.py)";')
out.append('')
out.append('/// The auto-model tables read, and the task each serves.')
out.append('pub const HF_AUTOMAP_TABLE_TASKS_V1: &[(&str, &str)] = &[')
for t, k in am['tables']:
    out.append(f'    ({rs_str(t)}, {rs_str(k)}),')
out.append('];')
out.append('')
out.append('/// (class, task), sorted by class: transformers\' own classes listed under exactly one task.')
out.append('pub const HF_AUTOMAP_TASKS_V1: &[(&str, &str)] = &[')
for c in sorted(uniq):
    out.append(f'    ({rs_str(c)}, {rs_str(uniq[c])}),')
out.append('];')
out.append('')
out.append('/// (GGUF architecture, task), sorted: every Hugging Face class llama.cpp\'s converter registers for the architecture names this task.')
out.append('pub const GGUF_CONVERTER_TASKS_V1: &[(&str, &str)] = &[')
for a in sorted(gguf):
    out.append(f'    ({rs_str(a)}, {rs_str(gguf[a])}), // {", ".join(gguf_why[a][:3])}{" …" if len(gguf_why[a]) > 3 else ""}')
out.append('];')
out.append('')
names = sorted(open('llama_arch_names.txt').read().split())
out.append('/// llama.cpp @030ebb55\'s model architectures (`LLM_ARCH_NAMES`, `src/llama-arch.cpp`) less `clip` (a projector, not a model): a GGUF')
out.append('/// whose `general.architecture` is one of them is a model file of a known runtime — a model class, whether or not a task is derivable.')
out.append('pub const LLAMA_CPP_MODEL_ARCHS_V1: &[&str] = &[')
for n in names:
    out.append(f'    {rs_str(n)},')
out.append('];')
out.append('')
out.append('/// The task transformers\' auto-model tables give `class`, when exactly one.')
out.append('pub fn hf_automap_task_v1(class: &str) -> Option<&\'static str> {')
out.append('    HF_AUTOMAP_TASKS_V1.binary_search_by(|(c, _)| (*c).cmp(class)).ok().map(|i| HF_AUTOMAP_TASKS_V1[i].1)')
out.append('}')
out.append('')
out.append('/// The task llama.cpp\'s converter fixes for a GGUF architecture, when its registered classes name exactly one.')
out.append('pub fn gguf_converter_task_v1(arch: &str) -> Option<&\'static str> {')
out.append('    GGUF_CONVERTER_TASKS_V1.binary_search_by(|(a, _)| (*a).cmp(arch)).ok().map(|i| GGUF_CONVERTER_TASKS_V1[i].1)')
out.append('}')
open(sys.argv[1], 'w').write('\n'.join(out) + '\n')
print(len(uniq), 'classes;', len(gguf), 'gguf archs', file=sys.stderr)
print(sorted(gguf.items()), file=sys.stderr)

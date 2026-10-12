"""Map llama.cpp converter registrations (HF architecture class names) to GGUF architecture names. Read-only text parsing."""
import collections, glob, json, re, sys
root = sys.argv[1]
names = {}
src = open(root + '/gguf-py/gguf/constants.py').read()
# MODEL_ARCH enum members -> names
m = re.search(r'MODEL_ARCH_NAMES[^{]*\{(.*?)\n\}', src, re.S)
for k, v in re.findall(r'MODEL_ARCH\.(\w+)\s*:\s*"([^"]+)"', m.group(1)):
    names[k] = v
by = collections.defaultdict(set)
for f in sorted(glob.glob(root + '/conversion/*.py')):
    s = open(f).read()
    for m in re.finditer(r'@ModelBase\.register\((.*?)\)\s*\n(?:@[^\n]*\n)*class (\w+)\(([^)]*)\):', s, re.S):
        regs = re.findall(r'"([^"]+)"', m.group(1))
        start = m.end()
        nxt = re.search(r'\n(?:@ModelBase|class )', s[start:])
        body = s[start:start + (nxt.start() if nxt else len(s))]
        a = re.search(r'model_arch\s*=\s*gguf\.MODEL_ARCH\.(\w+)', body)
        arch = names.get(a.group(1)) if a else None
        if arch is None:
            # inherit from a base class defined with an arch
            arch = '(inherits:' + m.group(3) + ')'
        for r in regs:
            by[arch].add(r)
json.dump({k: sorted(v) for k, v in sorted(by.items())}, sys.stdout, indent=0)

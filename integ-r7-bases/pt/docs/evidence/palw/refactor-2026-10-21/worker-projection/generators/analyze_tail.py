from pathlib import Path
import importlib.util,re
p=Path('kaspad/src/palw_panel.rs');s=p.read_text();sp=importlib.util.spec_from_file_location('h','docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h);m=h.mask(s);wa=m.index('pub async fn worker(')
a=s.index('            if self.config.fee_outpoint.is_some()',s.index('self.tir_public_filer_tick_v1',wa));b=s.index('            // Forget what the chain',a)
pre=m[wa:a];globals=list(dict.fromkeys(re.findall(r'\blet\s+(?:mut\s+)?([a-z_]\w*)',pre)))
print(s[a:a+160]); print('funding candidates:',[n for n in globals if re.search(r'(?<!\.)\b'+n+r'\b',m[a:b])]);Path('/tmp/collector-original-stage.rs').write_text(s[a:b]);print('range',a,b)

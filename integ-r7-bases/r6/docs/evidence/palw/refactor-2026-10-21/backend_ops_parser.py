"""Read-only parser for the unchanged compatibility trait, used by the audit."""
from pathlib import Path
import importlib.util,re,json
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'consensus/core/src/palw_backend.rs';s=p.read_text();m=h.mask(s);a=m.index('pub trait PalwExecutionBackendV1:');a=m.index('{',a)
def close(a,left='(',right=')'):
 d=1;i=a+1
 while d:d+=(m[i]==left)-(m[i]==right);i+=1
 return i
end=close(a,'{','}');methods=[]
for x in re.finditer(r'^    fn (\w+)\(',m[a:end],re.M):
 start=a+x.start();name=x[1];open=m.index('(',start);args_end=close(open);b=args_end
 while m[b] not in '{;':b+=1
 signature=s[start:b].strip();params=s[open+1:args_end-1].strip();assert params.startswith('&self') or params.startswith('&mut self')
 # Parameter types have nested commas; split only at balanced punctuation.
 masked=h.mask(params);parts=[];level=0;last=0
 for i,c in enumerate(masked):
  if c in '([{<':level+=1
  elif c in ')]}>':level-=1
  elif c==',' and level==0:parts.append(params[last:i].strip());last=i+1
 parts.append(params[last:].strip());args=[]
 for part in parts[1:]:
  if not part:continue
  ident=part.split(':',1)[0].strip();assert re.fullmatch(r'\w+',ident),(name,part);args.append(ident)
 methods.append({'name':name,'signature':signature,'args':args,'mutable':'&mut self' in params[:12]})

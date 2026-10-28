"""Read-only source projection helpers for the court ownership refactor.
The projection expresses the reviewed mechanical substitutions; it does not run them
against the workspace. Runtime equivalence remains a separate test obligation.
"""
from pathlib import Path
import re
root = Path(__file__).resolve().parents[4]
def mask(s):
 out=list(s);i=0
 while i<len(s):
  start=i;j=i
  if s.startswith('//',i):
   j=s.find('\n',i);j=len(s) if j<0 else j
  elif s.startswith('/*',i):
   j=i+2;d=1
   while j<len(s) and d:
    if s.startswith('/*',j):d+=1;j+=2
    elif s.startswith('*/',j):d-=1;j+=2
    else:j+=1
  elif s[i] in 'br' and (m:=re.match(r'(?:b)?r(#+)?"',s[i:i+64])):
   end='"'+(m[1] or '');k=s.find(end,i+m.end());j=len(s) if k<0 else k+len(end)
  elif s[i]=='"':
   j=i+1
   while j<len(s):
    if s[j]=='\\':j+=2
    elif s[j]=='"':j+=1;break
    else:j+=1
  elif s[i]=="'" and (m:=re.match(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^'\\\n])'",s[i:i+16])):j=i+m.end()
  if j>i:
   for k in range(i,j):
    if out[k]!='\n':out[k]=' '
   i=j
  else:i+=1
 return ''.join(out)
def edit(s, pattern, sub):
 m=mask(s); spans=list(re.finditer(pattern,m,re.S))
 for x in reversed(spans):
  new=sub(s[x.start():x.end()],x) if callable(sub) else x.expand(sub)
  s=s[:x.start()]+new+s[x.end():]
 return s
val=r'(?:Vec<\(Hash64,\s*u32,\s*bool,\s*(?:kaspa_consensus_core::palw_state_v2::)?PalwConsensusObjectV2\)>|\[\(Hash64,\s*u32,\s*bool,\s*(?:kaspa_consensus_core::palw_state_v2::)?PalwConsensusObjectV2\)\]|(?:std::collections::)?HashMap<(?:\(Hash64,\s*u32,\s*bool\)|PalwCourtQueueKeyV1),\s*u64>)'
def port(s):
 if not re.search('court_(pending|due|moved)',mask(s)):return s
 # Collapse local state in worker, initialized at the old first map declaration.
 s=edit(s,r'let mut court_moved:\s*HashMap<\(Hash64,\s*u32,\s*bool\),\s*u64>\s*=\s*HashMap::new\(\);', 'let mut court_outbox = CourtOutboxV1::default();')
 s=edit(s,r'let mut court_(?:pending|due):\s*'+val+r'\s*=\s*(?:Vec|HashMap)::new\(\);','')
 # References carry a single owner; preserve immutable observers where applicable.
 def decl(old,m):
  life=re.search(r"&('[a-z]+\s+)?",old)[1] or ''
  mut='mut ' if re.search(r'&(?:\x27[a-z]+\s+)?mut\s',old) else ''
  return 'court_outbox: &'+life+mut+'CourtOutboxV1'
 s=edit(s,r'court_(?:pending|due|moved):\s*&(?:\x27[a-z]+\s+)?(?:mut\s+)?'+val,decl)
 # Explicit entry semantics: only some sites insert missing deadlines.
 s=edit(s,r'court_due\.entry\(\(claim,\s*PALW_FRAUD_FILER_STAGED_ROUND_V1,\s*false\)\)\.and_modify\(\|held\|\s*\*held\s*=\s*\(\*held\)\.min\(due\)\);', 'court_outbox.tighten_existing_due((claim, PALW_FRAUD_FILER_STAGED_ROUND_V1, false), due);')
 s=edit(s,r'court_due\.entry\(key\)\.and_modify\(\|at\| \*at = \(\*at\)\.min\(due\)\)\.or_insert\(due\);','court_outbox.tighten_due(key, due);')
 s=edit(s,r'court_due\.entry\(key\)\.or_insert\(current_daa\);','court_outbox.date_if_absent(key, current_daa);')
 for var,old,new in [('pending','push','push_pending'),('pending','retain','retain_pending'),('due','insert','set_due'),('due','retain','retain_due'),('due','extend','extend_due'),('moved','get','submitted_at'),('moved','insert','mark_submitted'),('moved','remove','forget_submitted'),('moved','retain','retain_submitted'),('moved','contains_key','has_submitted'),('moved','keys','submitted_keys'),('moved','len','submitted_len')]:
  s=edit(s,r'court_'+var+r'\s*\.\s*'+old+r'\b','court_outbox.'+new)
 s=edit(s,r'court_pending\s*\.\s*(iter|len|is_empty)\b',r'court_outbox.pending().\1')
 # The carrier loop is the only consumer of the pending vector.
 s=edit(s,r'palw_court_queue_edf_v1\(court_pending, court_due\);','')
 s=edit(s,r'std::mem::take\(court_pending\)','court_outbox.take_priority()')
 s=edit(s,r'\*court_pending = unsent;\s*court_outbox.retain_due\(\|key, _\| court_outbox.pending\(\).iter\(\).any\(\|\(sid, round, responder, _\)\| \(\*sid, \*round, \*responder\) == \*key\)\);','court_outbox.finish_flush(unsent);')
 # Remaining bare references are arguments, context fields, destructuring or comments (masked).
 s=edit(s,r'\bcourt_(?:pending|due|moved)\b','court_outbox')
 # Duplicate owner parameters/fields/arguments, including priority's separated moved argument.
 # Scoped comma-list recognition below uses bracket depths and ignores literals/comments.
 def dedup(s):
  m=mask(s); stack=[]; containers=[]
  for i,c in enumerate(m):
   if c in '({[':stack.append((c,i))
   elif c in ')}]' and stack:
    o,a=stack.pop()
    if (o,c) in [('(',')'),('{','}')]:containers.append((a,i))
  deletes=[]
  for a,b in containers:
   # Only lists, at this nesting level; angle brackets guard generic type commas.
   depth=0;angle=0;beg=a+1;parts=[]
   for j in range(a+1,b):
    c=m[j]
    if c in '({[':depth+=1
    elif c in ')}]':depth-=1
    elif not depth:
     if c=='<':angle+=1
     elif c=='>' and angle:angle-=1
     elif c==',' and not angle:parts.append((beg,j+1));beg=j+1
   parts.append((beg,b));seen=False
   for l,r in parts:
    t=m[l:r].strip().rstrip(',').strip()
    if re.fullmatch(r'(?:pub(?:\([^)]*\))?\s+)?court_outbox:\s*&(?:\x27[a-z]+\s+)?(?:mut\s+)?CourtOutboxV1|court_outbox:\s*&(?:mut\s+)?court_outbox|&(?:mut\s+)?court_outbox|court_outbox',t):
     if seen:deletes.append((l,r))
     seen=True
  for l,r in sorted(set(deletes),reverse=True):s=s[:l]+s[r:]
  return s
 s=dedup(s)
 return s

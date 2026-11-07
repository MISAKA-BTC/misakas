"""Offline Markdown target extraction. Not a full CommonMark renderer."""
from __future__ import annotations
import re,html,posixpath,unicodedata
from pathlib import Path
from urllib.parse import unquote,urlsplit
from collections import Counter

def mask(text):
 out=[];fence=None
 for line in text.splitlines(keepends=True):
  check=re.sub(r'^\s*(?:>\s*)+','',line)
  m=re.match(r'^ {0,3}(`{3,}|~{3,})(.*)',check);hide=False
  if fence:
   hide=True
   if m and m[1][0]==fence[0] and len(m[1])>=fence[1] and not m[2].strip():fence=None
  elif m:fence=(m[1][0],len(m[1]));hide=True
  elif line.startswith(('    ','\t')):hide=True
  out.append(re.sub(r'[^\r\n]',' ',line) if hide else line)
 s=''.join(out)
 s=re.sub(r'<!--[\s\S]*?-->',lambda m:re.sub(r'[^\r\n]',' ',m[0]),s)
 s=re.sub(r'(`+)([^`]|(?!(?<!`)\1(?!`))`)*?\1',lambda m:re.sub(r'[^\r\n]',' ',m[0]),s)
 return s

def spans(text):
 s=mask(text);hits={};n=len(s)
 for m in re.finditer(r'(?m)^ {0,3}\[[^\]\n]+\]:[ \t]*(?:<([^>\n]+)>|([^\s]+))',s):
  a,b=m.span(1 if m[1] is not None else 2);hits[a]=(a,b,text[a:b])
 i=0
 while i<n:
  if s[i]!='[' or i and s[i-1]=='\\':i+=1;continue
  j=i+1;dep=1
  while j<n and dep and j-i<10000:
   if s[j]=='\\':j+=2;continue
   if s[j]=='[':dep+=1
   elif s[j]==']':dep-=1
   j+=1
  if dep or j>=n or s[j]!='(':i+=1;continue
  j+=1
  while j<n and s[j].isspace():j+=1
  if j>=n:i+=1;continue
  if s[j]=='<':
   a=j+1;b=s.find('>',a)
   if b!=-1 and '\n' not in s[a:b]:hits[a]=(a,b,text[a:b]);i=b+1;continue
  else:
   a=j;dep=0
   while j<n:
    if s[j]=='\\' and j+1<n:j+=2;continue
    if s[j]=='(':dep+=1
    elif s[j]==')':
     if dep==0:break
     dep-=1
    if s[j].isspace() and dep==0:break
    j+=1
   if j>a:hits[a]=(a,j,text[a:j]);i=j;continue
  i+=1
 for m in re.finditer(r'\b(?:href|src)\s*=\s*([\"\'])(.*?)\1',s,re.I):
  a,b=m.span(2);hits[a]=(a,b,text[a:b])
 return [hits[k] for k in sorted(hits)]

def slug(s):
 s=html.unescape(s);s=re.sub(r'<[^>]*>','',s);s=re.sub(r'!?\[([^\]]*)\]\([^)]*\)',r'\1',s)
 s=s.replace('`','').replace('*','').replace('~','').strip().lower()
 return ''.join(c for c in s if c in '-_' or c.isspace() or unicodedata.category(c)[0] in 'LN').replace(' ','-')
def anchors(text):
 a=set(re.findall(r'\b(?:id|name)\s*=\s*[\"\']([^\"\']+)[\"\']',text,re.I));counts=Counter();fence=None
 for line in text.splitlines():
  f=re.match(r'^ {0,3}(`{3,}|~{3,})(.*)',line)
  if fence:
   if f and f[1][0]==fence[0] and len(f[1])>=fence[1] and not f[2].strip():fence=None
   continue
  if f:fence=(f[1][0],len(f[1]));continue
  m=re.match(r'^ {0,3}#{1,6}\s+(.+?)\s*#*\s*$',line)
  if m:
   h=slug(m[1]);c=counts[h];counts[h]+=1;a.add(h+(f'-{c}' if c else ''))
 return a

def resolve(src,url):
 url=html.unescape(url.strip())
 try:u=urlsplit(url)
 except ValueError:return 'malformed',url,''
 if u.scheme or u.netloc or url.startswith('//'):return 'external',url,''
 path=unquote(u.path).replace('\\','/')
 target=src if not path else posixpath.normpath(path.lstrip('/') if path.startswith('/') else posixpath.join(posixpath.dirname(src),path))
 return 'local',target,unquote(u.fragment)
ERRORS={'missing-doc-file','missing-heading','malformed-target'}
def key(r):return '|'.join([r['status'],r['source'],r['target'],r.get('fragment','')])
def scan(root):
 rows=[];cache={}
 for p in sorted((root/'docs').rglob('*.md')):
  src=p.relative_to(root).as_posix();text=p.read_text(encoding='utf-8')
  for a,b,url in spans(text):
   kind,target,frag=resolve(src,url)
   r={'source':src,'line':text.count('\n',0,a)+1,'url':url,'target':target,'fragment':frag}
   if kind=='external':status='external-unchecked'
   elif kind=='malformed':status='malformed-target'
   elif not (target=='docs' or target.startswith('docs/')):status='outside-package-unchecked'
   elif not (root/target).exists():status='missing-doc-file'
   elif frag:
    q=root/target
    if q.is_dir():q=q/'README.md'
    if not q.is_file() or q.suffix!='.md':status='fragment-unchecked'
    else:
     if str(q) not in cache:cache[str(q)]=anchors(q.read_text())
     status='ok' if frag in cache[str(q)] else 'missing-heading'
   else:status='ok'
   r['status']=status;rows.append(r)
 return rows

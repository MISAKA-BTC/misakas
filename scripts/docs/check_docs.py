#!/usr/bin/env python3
"""Documentation checker for docs/ (standard library only).

Run it as an isolated interpreter, from anywhere in the repository:

    python3 -I scripts/docs/check_docs.py            # all checks
    python3 -I scripts/docs/check_docs.py --links    # only broken relative links
    python3 -I scripts/docs/check_docs.py --index    # only ADR/RFC index completeness
    python3 -I scripts/docs/check_docs.py --dups     # only duplicate numbers (report only)

Checks
  1. Broken relative markdown links under docs/ (inline links, images and reference
     definitions; fenced code blocks and inline code spans are ignored; http(s)/mailto/
     anchor-only links are not followed; under docs/wiki/ a link without an extension
     is a GitHub-wiki page link and resolves to <name>.md).
  2. ADR / RFC files that are missing from the number index (docs/adr/README.md,
     docs/rfc/README.md): every NNNN-*.md must be linked from its index.
  3. Duplicate ADR numbers, plus RFC numbers and spec chapter numbers. REPORT ONLY:
     renumbering is the owner's decision (see docs/adr/NUMBERING.md), so duplicates
     never change the exit status.

Exit status: 1 if a link is broken or a file is missing from its index, else 0.
It writes nothing and needs no network.
"""
import argparse
import os
import re
import sys
import urllib.parse

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
DOCS = os.path.join(ROOT, 'docs')

INLINE = re.compile(r'!?\[[^\]\n]*\]\(\s*(<[^>\n]+>|(?:[^()\s]|\([^()\n]*\))+)(?:\s+(?:"[^"\n]*"|\'[^\'\n]*\'))?\s*\)')
REFDEF = re.compile(r'^\s{0,3}\[[^\]\n]+\]:\s*(<[^>\n]+>|\S+)')
CODESPAN = re.compile(r'(`+)(?:(?!\1).)+?\1')
SKIP_SCHEMES = ('http://', 'https://', 'mailto:', 'ftp://', 'tel:', 'data:', 'javascript:')


def md_files(base):
    for dirpath, dirnames, filenames in os.walk(base):
        dirnames[:] = sorted(d for d in dirnames if d != '.git')
        for f in sorted(filenames):
            if f.endswith('.md'):
                yield os.path.join(dirpath, f)


def links_in(path):
    """yield (line_number, target) for every relative link candidate in a markdown file"""
    in_fence = False
    fence = ''
    with open(path, encoding='utf-8', errors='replace') as fh:
        for n, line in enumerate(fh, 1):
            stripped = line.lstrip()
            m = re.match(r'(`{3,}|~{3,})', stripped)
            if m:
                if not in_fence:
                    in_fence, fence = True, m.group(1)[0]
                elif stripped.startswith(fence * 3):
                    in_fence = False
                continue
            if in_fence:
                continue
            text = CODESPAN.sub('', line)
            for mm in INLINE.finditer(text):
                yield n, mm.group(1)
            mr = REFDEF.match(text)
            if mr:
                yield n, mr.group(1)


def is_wiki(path):
    return os.path.relpath(path, DOCS).replace(os.sep, '/').startswith('wiki/')


def check_links():
    broken = []
    checked = 0
    for path in md_files(DOCS):
        for n, raw in links_in(path):
            t = raw.strip()
            if t.startswith('<') and t.endswith('>'):
                t = t[1:-1]
            if not t or t.startswith('#') or t.startswith('/') or t.lower().startswith(SKIP_SCHEMES):
                continue
            t = t.split('#', 1)[0].split('?', 1)[0]
            if not t:
                continue
            t = urllib.parse.unquote(t)
            checked += 1
            target = os.path.normpath(os.path.join(os.path.dirname(path), t))
            if not os.path.exists(target) and is_wiki(path) and os.path.exists(target + '.md'):
                continue  # GitHub-wiki style link: [text](Page-Name) is Page-Name.md
            if not os.path.exists(target):
                broken.append((os.path.relpath(path, ROOT), n, raw))
    return checked, broken


def numbered(dirpath, pattern):
    out = {}
    if not os.path.isdir(dirpath):
        return out
    for f in sorted(os.listdir(dirpath)):
        m = re.match(pattern, f)
        if m and os.path.isfile(os.path.join(dirpath, f)):
            out.setdefault(m.group(1), []).append(f)
    return out


def index_targets(index_path):
    found = set()
    if not os.path.isfile(index_path):
        return found
    for _n, raw in links_in(index_path):
        t = raw.strip().strip('<>').split('#', 1)[0]
        t = urllib.parse.unquote(t)
        if t.endswith('.md'):
            found.add(os.path.basename(t))
    return found


def check_index():
    problems = []
    for name, d in (('ADR', 'adr'), ('RFC', 'rfc')):
        base = os.path.join(DOCS, d)
        files = numbered(base, r'^(\d{4})-.*\.md$')
        listed = index_targets(os.path.join(base, 'README.md'))
        for num, names in sorted(files.items()):
            for f in names:
                if f not in listed:
                    problems.append('%s-%s: docs/%s/%s is not linked from docs/%s/README.md' % (name, num, d, f, d))
    return problems


def check_dups():
    lines = []
    for name, d, pat in (('ADR', 'adr', r'^(\d{4})-.*\.md$'), ('RFC', 'rfc', r'^(\d{4})-.*\.md$'), ('spec/palw chapter', 'spec/palw', r'^(\d{2}[a-z]?)-.*\.md$')):
        files = numbered(os.path.join(DOCS, d), pat)
        for num, names in sorted(files.items()):
            if len(names) > 1:
                lines.append('%s %s: %s' % (name, num, ', '.join(names)))
    return lines


def main():
    ap = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    ap.add_argument('--links', action='store_true')
    ap.add_argument('--index', action='store_true')
    ap.add_argument('--dups', action='store_true')
    args = ap.parse_args()
    run_all = not (args.links or args.index or args.dups)
    status = 0

    if run_all or args.links:
        checked, broken = check_links()
        print('links: %d relative links checked, %d broken' % (checked, len(broken)))
        for f, n, raw in broken:
            print('  BROKEN %s:%d -> %s' % (f, n, raw))
        if broken:
            status = 1
    if run_all or args.index:
        problems = check_index()
        print('index: %d ADR/RFC file(s) missing from the number index' % len(problems))
        for p in problems:
            print('  MISSING %s' % p)
        if problems:
            status = 1
    if run_all or args.dups:
        dups = check_dups()
        print('duplicates (report only; see docs/adr/NUMBERING.md): %d' % len(dups))
        for d in dups:
            print('  DUP %s' % d)
    return status


if __name__ == '__main__':
    sys.exit(main())

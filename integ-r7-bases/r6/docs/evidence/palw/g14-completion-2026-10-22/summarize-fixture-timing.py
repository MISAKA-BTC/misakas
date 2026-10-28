import pathlib,json,re
root=pathlib.Path(__file__).resolve().parents[4]
ev=root/'docs/evidence/palw/g14-completion-2026-10-22'
rows=[]
for log in sorted(ev.glob('numerical-preflight-repair-measure-*.log')):
 text=log.read_text()
 line=next(x for x in text.splitlines() if x.startswith('MEASURE fixture_only=true '))
 row={}
 for key,value in re.findall(r'(\w+)=(\S+)',line):
  row[key]=int(value) if value.isdigit() else value
 timing=re.search(r'([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys',text)
 assert timing,log
 row.update(process_real_seconds=float(timing[1]),process_user_seconds=float(timing[2]),process_sys_seconds=float(timing[3]))
 rss=re.search(r'(\d+)\s+maximum resident set size',text);assert rss,log
 row['process_peak_rss_bytes']=int(rss[1]);row['log']=str(log.relative_to(root))
 assert row['observed_expected_outcomes']==row['n']
 assert re.search(r'test result: ok\. 1 passed; 0 failed;',text),log
 rows.append(row)
assert len(rows)==10,len(rows)
assert len({(r['op'],r['n']) for r in rows})==10
out={'scope':'Small fixture verification only; repeated cases, not IID or all-model/all-branch maximum; no native paid transport/network measurement.',
'wall_statistics':'Instant timer around selected verification only, ns; setup, phase clones and output sorting excluded.',
'process_statistics':'External macOS time -l, fresh process per op/N. Includes setup and harness; user/sys rounded by time. N=0 baseline retained separately; no per-op CPU or RSS derived.',
'bytes':'First exemplar serialized verifier body only, not maximum whole transaction/evidence/transport bytes.',
'rows':rows}
(ev/'fixture-verifier-summary.json').write_text(json.dumps(out,indent=2)+'\n')
for r in rows:
 if r['n']:
  print(r['op'],r['distinct'],r['median_wall_ns'],r['p99_wall_ns'],r['process_user_seconds'],r['process_sys_seconds'],r['process_peak_rss_bytes'])

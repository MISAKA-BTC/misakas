import runpy,json
ns=runpy.run_path('/tmp/misaka-optimized-controller-20261102.py');out=ns['out'];previous=json.loads((out/'optimized-runtime-r2.json').read_text())
r=ns['run']('hashed-walk-runtime-first',previous['command'],900,600*1024**2,16*1024**3)
assert r['exit_code']==0 and r['stop_reason'] is None and r['test_counts']==[(11,0,0),(4,0,0),(20,0,2)]

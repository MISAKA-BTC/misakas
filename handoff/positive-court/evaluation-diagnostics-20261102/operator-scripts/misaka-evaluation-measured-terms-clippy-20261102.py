import runpy
ns=runpy.run_path('/tmp/misaka-evaluation-clippy-base-20261102.py')
base=ns['old']['command'][:ns['old']['command'].index('--')];base[1]='clippy'
base[base.index('positive_court_a4_integration')]='positive_wide_material_v2'
r=ns['run']('clippy-panel-measured-terms',base+['--no-deps','--','-D','warnings'],900,600*1024**2)
assert r['exit_code']==0 and r['stop_reason'] is None

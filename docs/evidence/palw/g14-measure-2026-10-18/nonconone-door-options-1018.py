import json,sys
# Shipped terms (door-breakdown-1018.md). All DAA.
FIXED=103; CONE_SHIPPED=1062231-103-11120
T={'trace_tiles':4209,'output_check':2346,'reorg_restart':2346,'owner_queue':1536,'descent':369,'rows_tile_tree':208,'ids':106}
LIMIT=3120
def door(cone,terms): return FIXED+sum(terms.values())+cone
def run(name,cone,terms,note,rule_change=False):
    d=door(cone,terms)
    return {'option':name,'cone_term':cone,'non_cone_terms':terms,'door_daa':d,'fits_3120':d<=LIMIT,'rule_change':rule_change,'note':note}
rows=[]
base=dict(T)
rows.append(run('current (shipped)',CONE_SHIPPED,base,'as shipped, 448 serial'))
cone_best={'C one round sparse, body=measured wire':12537-103-11120,'D one round sparse, body=value bytes':11296-103-11120,'floor cone=0':0}
for k,v in cone_best.items(): rows.append(run('cone: '+k,v,base,'cone term only (read-set-1018)'))
# Savings ceilings for non-cone options (a term cannot drop below 0; no residual cost assumed = optimistic ceiling)
rows.append(run('non-cone: owner queue tied to tenure (queue 0)',0,{**base,'owner_queue':0},'DECIDED RULE (tenure): user decision',True))
rows.append(run('non-cone: output/reorg share authenticated bodies (one body saved)',0,{**base,'reorg_restart':0},'ceiling 2,346 saved if reorg body = output body',False))
rows.append(run('non-cone: trace tiles sparse (fetch only demanded tiles), ceiling 100% saved',0,{**base,'trace_tiles':0},'ceiling; residual unknown, not measured',False))
rows.append(run('non-cone: trace tiles sparse, 50% saved (illustrative)',0,{**base,'trace_tiles':2105},'illustrative split, not measured',False))
# Combinations (ceilings)
rows.append(run('combo: cone one-round sparse (D) + output/reorg shared + trace sparse 50%',cone_best['D one round sparse, body=value bytes'],{**base,'reorg_restart':0,'trace_tiles':2105},'ceiling-based, not measured'))
rows.append(run('combo: cone 0, trace 0, reorg 0, queue kept (output 2,346 kept)',0,{**base,'trace_tiles':0,'reorg_restart':0},'still above limit'))
rows.append(run('combo: cone 0, trace 0, reorg 0, queue 0 (output 2,346 kept)',0,{**base,'trace_tiles':0,'reorg_restart':0,'owner_queue':0},'uses the decided tenure rule change',True))
# Required reduction of the non-cone sum with cone=0 and queue kept
need=FIXED+sum(base.values())-LIMIT
print(json.dumps({'rows':rows,'required_cut_nonCone_with_cone0_queue_kept':max(0,need)}, ensure_ascii=False, indent=1)) if '--json' in sys.argv else None
for r in rows: print(f"{r['door_daa']:>10}  {'FIT' if r['fits_3120'] else 'no '}  {r['option']}")
print('required cut from non-cone (cone 0, queue kept):',need, 'of', sum(base.values()))
out=sys.argv[1] if len(sys.argv)>1 and not sys.argv[1].startswith('--') else None
if out: json.dump({'limit':LIMIT,'fixed':FIXED,'shipped_terms':base,'rows':rows,'required_cut':need},open(out,'w'),ensure_ascii=False,indent=1)

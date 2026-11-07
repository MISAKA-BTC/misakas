from pathlib import Path
import subprocess,re,json
root=Path.cwd()
def old(p):return subprocess.check_output(['git','show','efab11bd8:'+p],text=True)
def norm(s):return re.sub(r'\s+','',s)
paths=['consensus/core/src/palw_state_v2.rs','consensus/core/src/palw_state_v2/block_preparation.rs','consensus/core/src/palw_state_v2/fold_read.rs','consensus/core/src/palw_state_v2/transition_builder.rs'];rows=[]
for p in paths:
 s=Path(p).read_text();t=s
 if p.endswith('palw_state_v2.rs'):
  t=t.replace('mod block_preparation;\nmod input_views;\npub use input_views::{PalwBlockFactsV1, PalwRulesAtBlockV1, PalwRulesForClaimV1};','mod block_preparation;').replace('            include_str!("palw_state_v2/input_views.rs"),\n','').replace('parent, ctx, extras.block_facts(), extras.rules_at(ctx),','parent, ctx, extras,')
 elif p.endswith('block_preparation.rs'):
  t=t.replace("ctx: &PalwBlockContextV2, facts: PalwBlockFactsV1<'_>, rules: PalwRulesAtBlockV1<'_>,",'ctx: &PalwBlockContextV2, extras: &PalwTransitionExtrasV1,').replace('facts.round_fee_incomes()','&extras.round_fee_incomes').replace('rules.round_lane()','extras.round_lane')
 elif p.endswith('fold_read.rs'):
  t=t.replace('self.extras.rules_at(ctx).work_target_active()','self.extras.work_target_active').replace('self.extras.rules_at(ctx).single_lottery_active()','self.extras.single_lottery_active').replace('self.extras.rules_at(ctx).canonical_work_active()','self.extras.canonical_work_daa.is_some_and(|height| ctx.daa_score >= height)')
 else:t=t.replace('self.extras.rules_for_claim(claim).canonical_weight(&self.state)','self.state.palw_claim_canonical_weight_v1(claim, self.extras.canonical_work_daa)')
 assert norm(t)==norm(old(p)),p
 rows.append(p)
report={'base':'efab11bd8','production_callers_projected':rows,'result':'All four production source files match after reversing the explicitly enumerated view substitutions.','limits':'New view getter bodies require review; a source projection is not an exhaustive execution equivalence proof.'}
Path('/tmp/input-views-source-projection.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))

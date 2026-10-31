import torch,json
from pathlib import Path
p=Path(__file__).parent/'EleutherAI__pythia-70m'
seqs=json.loads((p/'calibration.json').read_text())['sequences']
with (p/'hf-reference.f32').open('wb') as f:
 for i,ids in enumerate(seqs):
  v=torch.load(p/f'hf-logits-{i}.pt',weights_only=True,map_location='cpu').contiguous()
  assert v.shape[0]==len(ids)
  vocab=v.shape[1]
  f.write(v.numpy().astype('<f4').tobytes())
(p/'hf-reference.json').write_text(json.dumps({'schema':'misaka.palw.hf-reference.v1','producer':{'name':'transformers','version':'4.57.6','torch':'2.15.0','repository':'EleutherAI/pythia-70m','revision':'a39f36b100fe8a5377810d56c3f4789b9c53ac42','dtype':'float32','attention':'eager','trust_remote_code':False},'vocab':vocab,'logits_file':'hf-reference.f32','sequences':[{'tokens':ids} for ids in seqs]},indent=2))

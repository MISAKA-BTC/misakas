import json,torch,transformers
from pathlib import Path
root=Path(__file__).parent/'EleutherAI__pythia-70m';src=root/'run/checkpoint';seqs=json.loads((root/'calibration.json').read_text())['sequences']
torch.set_num_threads(4)
model=transformers.AutoModelForCausalLM.from_pretrained(src,torch_dtype=torch.float32,local_files_only=True,trust_remote_code=False,attn_implementation='eager').eval()
results=[]
with torch.no_grad():
 for ids in seqs:
  out=model(torch.tensor([ids]),output_hidden_states=True,use_cache=False)
  logits=out.logits[0];results.append({'min':logits.min().item(),'max':logits.max().item(),'absmax':logits.abs().max().item(),'mean':logits.mean().item(),'position_min':logits.min(-1).values.tolist(),'position_max':logits.max(-1).values.tolist(),'hidden_absmax':[x.abs().max().item() for x in out.hidden_states]})
  torch.save(logits,root/f'hf-logits-{len(results)-1}.pt')
(root/'hf-reference-diagnostic.json').write_text(json.dumps({'torch':torch.__version__,'transformers':transformers.__version__,'sequences':results},indent=2))
print(json.dumps(results,indent=2))

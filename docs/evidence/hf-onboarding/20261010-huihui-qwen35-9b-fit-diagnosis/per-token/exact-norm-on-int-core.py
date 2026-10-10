import numpy as np, json
from safetensors import safe_open
P='/Users/wata/Downloads/MISAKA-wt-b/wh-h1-run/diag/prefix-hui-L4/model.safetensors'
g=None
with safe_open(P,'np') as f:
    ks=[k for k in f.keys() if 'layers.0.linear_attn.norm' in k]; print(ks)
    g=f.get_tensor(ks[0]).astype(float)
cfg=json.load(open('/Users/wata/Downloads/MISAKA-wt-b/wh-h1-run/diag/prefix-hui-L4/config.json')); 
eps=cfg.get('text_config',cfg).get('rms_norm_eps',1e-6); print('eps',eps,'gamma',g.shape,g[:4])
def L(k,sub='b/'):
    return np.fromfile(f'{sub}{k}.float.f32','<f4').reshape(96,32,128).astype(float), np.fromfile(f'{sub}{k}.int.f32','<f4').reshape(96,32,128).astype(float)
fc,ic=L('L0.gdn.core'); fn,inn=L('L0.gdn.normed'); fz,iz=L('L0.gdn.z')
silu=lambda x: x/(1+np.exp(-x))
def norm(x): return x/np.sqrt((x**2).mean(-1,keepdims=True)+eps)
rms=lambda x: np.sqrt((x**2).mean(-1))
ref=norm(fc)*g*silu(fz)
print("check float chain vs dumped float normed: max rel", np.abs(ref-fn).max()/np.abs(fn).max())
A=norm(ic)*g*silu(fz)       # exact norm on the INT core, float z
B=norm(ic)*g*silu(iz)       # + int z
rel=lambda a,b: rms(a-b)/np.maximum(rms(b),1e-30)
for name,x in (("exact norm on int core",A),("exact norm on int core, int z",B),("int program (dumped)",inn)):
    r=rel(x,fn); print(f"{name:32s}: rel err vs float normed: median over (tok,head) {np.median(r):.4f}; share of heads>0.5: {np.mean(r>0.5):.3f}; overall energy-weighted {np.sqrt(((x-fn)**2).sum()/ (fn**2).sum()):.4f}")
bad=[1,11,12,13,20,21,24,25,30,31]
for name,x in (("exact norm on int core",A),("int program",inn)):
    print(name,"bad heads' median rel err", np.round(np.median(rel(x,fn)[:,bad],0),2).tolist())
print("float core head rms (bad heads, med)", np.round(np.median(rms(fc)[:,bad],0),5).tolist())
print("eps vs ms: median ms/eps for bad heads", np.round(np.median((rms(fc)[:,bad]**2)/eps,0),2).tolist())

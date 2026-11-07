import numpy as np
def L(k):
    f=np.fromfile(f'{k}.float.f32','<f4').reshape(96,32,128).astype(float); i=np.fromfile(f'{k}.int.f32','<f4').reshape(96,32,128).astype(float); return f,i
rms=lambda x: np.sqrt((x**2).mean(-1))
fc,ic=L('L0.gdn.core'); fn,inn=L('L0.gdn.normed')
for h in (12,13,20,0,5,3):
    print(f"head {h:2d}: core float rms med {np.median(rms(fc)[:,h]):.2e} int rms {np.median(rms(ic)[:,h]):.2e} | normed float rms {np.median(rms(fn)[:,h]):.2e} int rms {np.median(rms(inn)[:,h]):.2e} | cos(int,float) normed {np.median((fn[:,h]*inn[:,h]).sum(-1)/np.maximum(np.linalg.norm(fn[:,h],axis=-1)*np.linalg.norm(inn[:,h],axis=-1),1e-30)):.3f} core {np.median((fc[:,h]*ic[:,h]).sum(-1)/np.maximum(np.linalg.norm(fc[:,h],axis=-1)*np.linalg.norm(ic[:,h],axis=-1),1e-30)):.3f} ; nonzero int core elems {np.mean(ic[:,h]!=0):.2f}")
# the part of normed energy carried by bad heads
e=(fn**2).sum(-1).mean(0); r=rms(inn-fn)/rms(fn); bad=np.median(r,0)>0.5
print("heads with normed rel err>0.5:",np.where(bad)[0].tolist()," share of normed float energy %.3f ; share of squared error %.3f"%(e[bad].sum()/e.sum(), ((inn-fn)**2).sum((0,2))[bad].sum()/((inn-fn)**2).sum()))

import numpy as np, json
d=json.load(open('dump.json'))
out={}
for k,v in d.items():
    w,n=v['width'],v['positions']
    f=np.fromfile(f'{k}.float.f32',dtype='<f4').reshape(n,w).astype(np.float64)
    i=np.fromfile(f'{k}.int.f32',dtype='<f4').reshape(n,w).astype(np.float64)
    rms=np.sqrt((f**2).mean(1)); err=np.sqrt(((i-f)**2).mean(1)); rel=err/np.maximum(rms,1e-30)
    a=np.unique(np.abs(i.ravel())); a=a[a>0]; g=np.diff(a); step=g[g>0].min() if g.size else 0
    c=np.corrcoef(np.log(rms[1:]),np.log(rel[1:]))[0,1]; s=np.polyfit(np.log(rms[1:]),np.log(rel[1:]),1)[0]
    print(f"{k:14s} w{w:6d} tokrms med {np.median(rms):.3g} p10 {np.percentile(rms,10):.3g} p90 {np.percentile(rms,90):.3g} max {rms.max():.3g}@{rms.argmax()} | abserr med {np.median(err):.3g} | rel med {np.median(rel):.3f} p90 {np.percentile(rel,90):.3f} | log-log corr {c:.2f} slope {s:.2f} | step~{step:.3g} absmax {np.abs(f).max():.3g}")
    out[k]=dict(rms=rms.tolist(),err=err.tolist(),rel=rel.tolist())
json.dump(out,open('pertoken.json','w'))

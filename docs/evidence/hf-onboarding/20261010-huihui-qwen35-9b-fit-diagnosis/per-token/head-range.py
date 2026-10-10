import numpy as np
def L(k):
    n=96
    f=np.fromfile(f'{k}.float.f32','<f4').reshape(n,-1).astype(float); i=np.fromfile(f'{k}.int.f32','<f4').reshape(n,-1).astype(float); return f,i
for hd in (128,):
  fc,ic=L('L0.gdn.core'); fn,inn=L('L0.gdn.normed')
  H=fc.shape[1]//hd
  fc=fc.reshape(96,H,hd); ic=ic.reshape(96,H,hd); fn=fn.reshape(96,H,hd); inn=inn.reshape(96,H,hd)
  rms=lambda x: np.sqrt((x**2).mean(-1))
  mag=rms(fc); rel_c=rms(ic-fc)/mag; rel_n=rms(inn-fn)/rms(fn)
  abs_c=rms(ic-fc)
  print("heads",H,"head-rms of core: p10 %.3g med %.3g p90 %.3g max/min %.1f"%(*np.percentile(mag,[10,50,90]),mag.max()/mag.min()))
  print("abs error of core per head: p10 %.3g med %.3g p90 %.3g (near-uniform if quantisation noise is per-tensor)"%tuple(np.percentile(abs_c,[10,50,90])))
  print("rel core err per head: med %.4f p90 %.4f max %.4f"%(*np.percentile(rel_c,[50,90]),rel_c.max()))
  print("rel normed err per head: med %.4f p90 %.4f max %.4f"%(*np.percentile(rel_n,[50,90]),rel_n.max()))
  m=mag.ravel(); a=rel_c.ravel(); b=rel_n.ravel()
  print("corr(log head-rms, log rel_core) %.2f slope %.2f ; corr(log rel_core, log rel_normed) %.2f slope %.2f"%(np.corrcoef(np.log(m),np.log(a))[0,1],np.polyfit(np.log(m),np.log(a),1)[0],np.corrcoef(np.log(a),np.log(b))[0,1],np.polyfit(np.log(a),np.log(b),1)[0]))
  # bin by head magnitude quartiles
  q=np.percentile(m,[0,25,50,75,100])
  for lo,hi in zip(q[:-1],q[1:]):
      s=(m>=lo)&(m<=hi); print("  head rms in [%.3g,%.3g]: rel core %.4f -> rel normed %.4f ; abs core err %.3g"%(lo,hi,np.median(a[s]),np.median(b[s]),np.median(abs_c.ravel()[s])))
  # per head index medians
  print("  per-head median head-rms:", np.round(np.median(mag,0),3).tolist())
  print("  per-head median rel normed err:", np.round(np.median(rel_n,0),3).tolist())

from pathlib import Path
import re,importlib.util
r=Path.cwd();sp=importlib.util.spec_from_file_location('h',r/'docs/evidence/palw/refactor-2026-10-21/court_port_projection_helpers.py');h=importlib.util.module_from_spec(sp);sp.loader.exec_module(h)
p=r/'kaspad/src/palw_panel.rs';s=p.read_text()
def close(m,a):
 d=1;i=a+1
 while d:d+=(m[i]=='{')-(m[i]=='}');i+=1
 return i
m=h.mask(s);a=m.index('while let Ok(event) = inbox.try_recv()',m.index('pub async fn worker('));b=close(m,m.index('{',a));body=s[a:b];Path('/tmp/panel-ingress-before-extraction.rs').write_text(body)
body=body.replace('&mut first_seen','first_seen').replace('&mut shard_books.arrivals','shard_arrivals').replace('&mut receipt_arrivals','receipt_arrivals')
new='''//! Synchronous inbox admission, before this tick's chain/session reads. Only
//! received-material and arrival queues are writable here; there is no court,
//! fee-chain or verification-book argument and no new asynchronous task.
use super::*;

impl PalwPanelService {
    pub(super) fn drain_panel_inbox_v1(
        &self,
        inbox: &mut tokio::sync::mpsc::Receiver<kaspa_p2p_flows::palw_gossip::PalwGossipEvent>,
        pool_books: &mut PoolBooksV1,
        first_seen: &mut HashMap<Hash64, u64>,
        shard_arrivals: &mut std::collections::VecDeque<kaspa_consensus_core::palw_tir_shard_v1::PalwSeatReceiptV4>,
        receipt_arrivals: &mut std::collections::VecDeque<crate::palw_receipt_pool::ArrivedReceiptV1>,
    ) {
'''+body+'''
    }
}
''';(r/'kaspad/src/palw_panel/ingress.rs').write_text(new);s=s[:a]+'self.drain_panel_inbox_v1(\n                &mut inbox, &mut pool_books, &mut first_seen, &mut shard_books.arrivals, &mut receipt_arrivals,\n            );'+s[b:]
# Keep the original serving block's lexical boundary, including weak callback ownership.
m=h.mask(s);wa=m.index('pub async fn worker(');marker=s.index('// **This node now answers material pulls',wa);brace=m.index('{',marker);end=close(m,brace);block=s[brace:end];Path('/tmp/panel-startup-before-extraction.rs').write_text(block)
new='''//! Material-serving startup. The gossip callback retains only the original
//! weak service owner; directory/RPC declaration order is unchanged.
use super::*;

impl PalwPanelService {
    pub(super) fn install_material_cache_service_v1(self: &Arc<Self>) {
'''+block+'''
    }
}
''';(r/'kaspad/src/palw_panel/startup.rs').write_text(new);s=s[:brace]+'self.install_material_cache_service_v1();'+s[end:]
needle='mod material_pool;';assert s.count(needle)==1;s=s.replace(needle,'mod ingress;\nmod startup;\n\n'+needle);p.write_text(s)
# Align the copied fragments without changing any statements, literals or call shapes.
for path,needle,shift,first in [('kaspad/src/palw_panel/ingress.rs','while let Ok(event)',4,8),('kaspad/src/palw_panel/startup.rs','{\n            // ADR-0084',0,8),('kaspad/src/palw_panel/material_pool.rs','while self.values.len()',16,8)]:
 p=r/path;t=p.read_text();m=h.mask(t);a=t.index(needle);a=m.index('{',a) if path.endswith('startup.rs') else a;br=m.index('{',a) if not path.endswith('startup.rs') else a;b=close(m,br);frag=t[a:b];lines=frag.splitlines();lines[0]=' '*first+lines[0]
 for i in range(1,len(lines)):
  if shift and lines[i].strip():assert lines[i].startswith(' '*shift);lines[i]=lines[i][shift:]
 t=t[:a]+'\n'.join(lines)+t[b:];p.write_text(t)

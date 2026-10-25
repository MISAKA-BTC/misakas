mod stores {
#[path="/Users/wata/.codex/worktrees/rfc-6-7-8-12/MISAKA-testnet/consensus/src/model/stores/overlay_absent_cache.rs"] mod overlay_absent_cache;
#[path="/Users/wata/.codex/worktrees/rfc-6-7-8-12/MISAKA-testnet/consensus/src/model/stores/rewarded_epochs.rs"] pub mod rewarded_epochs;
}
use kaspa_database::{create_permanent_db, prelude::{CachePolicy, CachedDbAccess, ConnBuilder, DbKey}};
use kaspa_database::prelude::DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES;
use kaspa_database::registry::DatabaseStorePrefixes;
use kaspa_consensus_core::{BlockHash, BlockHasher};
use kaspa_hashes::Hash64;
use stores::rewarded_epochs::{DbRewardedEpochsStore, RewardedEpochsStoreReader, RewardedEpochKeys};
use std::{sync::Arc, time::Instant};
fn main() {
 let path=std::env::args().nth(1).expect("owned new DB path");
 let (_lifetime, db)=create_permanent_db!(&path, ConnBuilder::default().with_files_limit(10).with_cache_budget(Some(DEFAULT_PRESET_CONSENSUS_BLOCK_CACHE_BYTES)));
 let n=160usize<<20;
 let mut blob=vec![0u8;n];
 let mut word=0x9e3779b97f4a7c15u64;
 for chunk in blob.chunks_exact_mut(8) {
  word^=word>>12;word^=word<<25;word^=word>>27;
  chunk.copy_from_slice(&word.wrapping_mul(0x2545F4914F6CDD1D).to_le_bytes());
 }
 db.put([196], [1]).unwrap();
 db.put([223], &blob).unwrap();
 drop(blob);
 db.flush().unwrap();
 db.compact_range::<&[u8], &[u8]>(None, None);
 let prefix:Vec<u8>=DatabaseStorePrefixes::RewardedEpochs.into();
 let absent=Hash64::from_bytes([0x40;64]);
 assert!(db.get_pinned(DbKey::new(&prefix, absent)).unwrap().is_none());
 let baseline=CachedDbAccess::<BlockHash,Arc<RewardedEpochKeys>,BlockHasher>::new(db.clone(),CachePolicy::Count(16),prefix);
 let enhanced=DbRewardedEpochsStore::new(db.clone(),CachePolicy::Count(16));
 let loops=12u32;
 let mut before=Vec::new();let mut after=Vec::new();
 for round in 0..3 {
  let t=Instant::now();
  for _ in 0..loops {assert!(baseline.read(absent).is_err());}
  before.push(t.elapsed().as_micros());
  let t=Instant::now();
  for _ in 0..loops {assert!(enhanced.get(absent).is_err());}
  after.push(t.elapsed().as_micros());
  eprintln!("round={round} legacy_us={} cached_us={}",before.last().unwrap(),after.last().unwrap());
 }
 println!("{{\"kind\":\"controlled-adjacent-SST-absence\",\"giantValueBytes\":{n},\"readsPerRound\":{loops},\"legacyMicros\":{before:?},\"cachedMicros\":{after:?},\"native100kAccepted\":false,\"realP2pAccepted\":false}}");
}

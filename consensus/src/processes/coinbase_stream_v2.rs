//! Borrowed reward partitions and payload bytes shared by real coinbase build
//! and validation. These views change no wire, reward order or carve arithmetic.
use super::*;

pub(super) struct CoinbaseMergesetV2<'a> {
    ghostdag: &'a GhostdagData,
    rounds: &'a BlockHashSet,
}
impl<'a> CoinbaseMergesetV2<'a> {
    pub(super) fn new(ghostdag: &'a GhostdagData, rounds: &'a BlockHashSet) -> Self {
        Self { ghostdag, rounds }
    }
    pub(super) fn blues(&self) -> impl Iterator<Item = &kaspa_consensus_core::BlockHash> {
        self.ghostdag.mergeset_blues.iter().filter(|h| !self.rounds.contains(h))
    }
    pub(super) fn genuine_reds(&self) -> impl Iterator<Item = &kaspa_consensus_core::BlockHash> {
        self.ghostdag.mergeset_reds.iter().filter(|h| !self.rounds.contains(h))
    }
    pub(super) fn rounds(&self) -> impl Iterator<Item = &kaspa_consensus_core::BlockHash> {
        self.ghostdag.mergeset_blues.iter().chain(self.ghostdag.mergeset_reds.iter()).filter(|h| self.rounds.contains(h))
    }
    /// The one remaining comparison-only temporary is the existing round-payee
    /// grouping Vec of borrowed script references. Cover geometric old/new
    /// storage before the original grouping algorithm allocates or grows.
    pub(super) fn round_requests(&self) -> Result<u64, kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1> {
        use kaspa_consensus_core::palw_native_resource_v1::NativeCoverageV1;
        let count = self.rounds().count();
        if count == 0 {
            return Ok(0);
        }
        count
            .max(4)
            .checked_mul(3)
            .and_then(|n| n.checked_mul(size_of::<(&ScriptPublicKey, u64)>()))
            .and_then(|n| u64::try_from(n).ok())
            .ok_or_else(|| NativeCoverageV1::overflow::<Self>("coinbase borrowed round payees"))
    }
}

pub(super) fn payload_bytes<T: AsRef<[u8]>>(
    data: &CoinbaseData<T>,
    maximum_script: u8,
) -> CoinbaseResult<impl Iterator<Item = u8> + '_> {
    let script = data.miner_data.script_public_key.script();
    if script.len() > usize::from(maximum_script) {
        return Err(CoinbaseError::PayloadScriptPublicKeyLenAboveMax(script.len(), maximum_script));
    }
    Ok(data
        .blue_score
        .to_le_bytes()
        .into_iter()
        .chain(data.subsidy.to_le_bytes())
        .chain(data.miner_data.script_public_key.version().to_le_bytes())
        .chain([script.len() as u8])
        .chain(script.iter().copied())
        .chain(data.miner_data.extra_data.as_ref().iter().copied()))
}

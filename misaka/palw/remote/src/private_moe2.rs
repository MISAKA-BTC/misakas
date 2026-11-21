//! Read the selection and contents of an isolated MOE2 accounting checkpoint.
//! The reader's roots come from its own native settlement replay. A block's
//! acknowledgement of these bytes grants neither maturity nor a spendable output.
use kaspa_consensus_core::{Hash64, api::ConsensusApi, palw_model_market_v1::moe2::settlement::Moe2SettlementBook};
use std::io;

#[derive(Clone, Copy)]
pub struct PrivateMoe2Checkpoint {
    pub network: Hash64,
    pub reservation: Hash64,
    pub enrollment: Hash64,
    pub execution: Hash64,
    pub native_root: Hash64,
    pub book_root: Hash64,
}

impl PrivateMoe2Checkpoint {
    pub fn marker(&self) -> Hash64 {
        let mut hash = blake2b_simd::Params::new().hash_length(64).key(b"misaka-palw/private/moe2/checkpoint-carrier/v1").to_state();
        for value in [self.network, self.reservation, self.enrollment, self.execution, self.native_root, self.book_root] {
            hash.update(value.as_byte_slice());
        }
        Hash64::from_bytes(*hash.finalize().as_array())
    }

    /// Read the branch containing the enrollment, settlement position and later
    /// checkpoint acknowledgement. Recheck the selected tip after decoding.
    pub fn read_selected(
        &self,
        native: &dyn ConsensusApi,
        carrier: Hash64,
        bytes: &[u8],
        byte_budget: usize,
    ) -> io::Result<Moe2SettlementBook> {
        let tip = native.get_sink();
        if self.enrollment == self.execution
            || self.execution == carrier
            || !native.is_chain_ancestor_of(self.enrollment, self.execution).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(self.execution, carrier).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(carrier, tip).map_err(io::Error::other)?
            || !native
                .palw_coinbase_commits_to(self.network.as_bytes(), self.enrollment, &self.reservation.as_bytes())
                .map_err(io::Error::other)?
            || !native
                .palw_coinbase_commits_to(self.network.as_bytes(), carrier, &self.marker().as_bytes())
                .map_err(io::Error::other)?
        {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "MOE2 checkpoint is absent from the selected settlement branch"));
        }
        let book = Moe2SettlementBook::from_checkpoint(self.native_root, self.book_root, bytes, byte_budget)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "MOE2 settlement checkpoint differs from the replay"))?;
        if native.get_sink() != tip {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "selected tip changed during MOE2 checkpoint read"));
        }
        Ok(book)
    }
}

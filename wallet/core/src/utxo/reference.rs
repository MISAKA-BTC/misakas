//!
//! Extensions for [`UtxoEntryReference`] for handling UTXO maturity.
//!

use crate::imports::*;
pub use kaspa_consensus_client::{TryIntoUtxoEntryReferences, UtxoEntryReference};
use kaspa_consensus_core::dns_finality::{DnsCoinbaseSettlement, coinbase_spend_settled};

pub enum Maturity {
    /// Coinbase UTXO that has not reached stasis period.
    Stasis,
    /// Coinbase UTXO that has reached stasis period
    /// but has not reached coinbase maturity period or
    /// user UTXO that has not reached user maturity period.
    Pending,
    /// UTXO that has reached maturity period.
    Confirmed,
}

impl std::fmt::Display for Maturity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Maturity::Stasis => write!(f, "stasis"),
            Maturity::Pending => write!(f, "pending"),
            Maturity::Confirmed => write!(f, "confirmed"),
        }
    }
}

pub trait UtxoEntryReferenceExtension {
    fn maturity(&self, params: &NetworkParams, current_daa_score: u64) -> Maturity;
    fn balance(&self, params: &NetworkParams, current_daa_score: u64) -> Balance;

    /// DNS-accelerated coinbase settlement, evaluated by the SAME
    /// [`kaspa_consensus_core::dns_finality::coinbase_spend_settled`] rule the node's mempool
    /// enforces — one function, two callers, no drift. `true` for networks without settlement
    /// (`coinbase_settlement_long_maturity_daa == 0`) and for non-coinbase entries.
    ///
    /// The wallet intentionally passes `coinbase_maturity: 0` here: the classical maturity
    /// ladder (stasis → pending → confirmed) is already applied by [`Self::maturity`] before
    /// this is consulted, using the WALLET's stricter display periods, so re-applying the
    /// consensus floor inside the settlement call would be redundant.
    fn dns_settled(&self, params: &NetworkParams, current_daa_score: u64) -> bool;
}

impl UtxoEntryReferenceExtension for UtxoEntryReference {
    fn maturity(&self, params: &NetworkParams, current_daa_score: u64) -> Maturity {
        if self.is_coinbase() {
            if self.block_daa_score() + params.coinbase_transaction_stasis_period_daa() > current_daa_score {
                Maturity::Stasis
            } else if self.block_daa_score() + params.coinbase_transaction_maturity_period_daa() > current_daa_score {
                Maturity::Pending
            } else if !self.dns_settled(params, current_daa_score) {
                // Past classical maturity but not yet DNS-settled: the node's mempool will not
                // relay a spend of this output yet, so the wallet must not offer it as spendable.
                Maturity::Pending
            } else {
                Maturity::Confirmed
            }
        } else if self.block_daa_score() + params.user_transaction_maturity_period_daa() > current_daa_score {
            Maturity::Pending
        } else {
            Maturity::Confirmed
        }
    }

    fn balance(&self, params: &NetworkParams, current_daa_score: u64) -> Balance {
        match self.maturity(params, current_daa_score) {
            Maturity::Pending => Balance::new(0, self.amount(), self.amount(), 0, 1, 0),
            Maturity::Stasis => Balance::new(0, 0, 0, 0, 0, 1),
            Maturity::Confirmed => Balance::new(self.amount(), 0, 0, 1, 0, 0),
        }
    }

    fn dns_settled(&self, params: &NetworkParams, current_daa_score: u64) -> bool {
        let long_maturity = params.coinbase_settlement_long_maturity_daa();
        if long_maturity == 0 || !self.is_coinbase() {
            return true;
        }
        let anchor = params.dns_confirmed_anchor_daa();
        let settlement =
            DnsCoinbaseSettlement { long_maturity_daa: long_maturity, confirmed_anchor_daa: (anchor > 0).then_some(anchor) };
        coinbase_spend_settled(self.block_daa_score(), current_daa_score, 0, Some(&settlement))
    }
}

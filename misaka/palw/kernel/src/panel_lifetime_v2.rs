//! Prospective, signed native Panel service limits. Existing claim and source
//! codecs retain their meaning; a sponsor cannot install or rewrite this consent.
use crate::hash::{Digest, object_id};
use borsh::{BorshDeserialize, BorshSerialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PanelConsumerLifetimeTermsV2 {
    pub version: u16,
    /// Producer's previously signed prospective authorization, consumed by
    /// this exact native claim before any Panel entropy was available.
    pub authorization: Digest,
    pub network: Digest,
    pub ruleset: Digest,
    pub claim: Digest,
    pub class: Digest,
    pub work: Digest,
    pub producer: Digest,
    pub immutable_fields: Digest,
    /// Last inclusive actual native acceptance position, fixed before admission.
    pub accept_by_daa: u64,
    /// Last inclusive receipt-service position, including every retry and pause.
    pub service_until_daa: u64,
    /// Native challenge/court liability remains after service stops.
    pub liability_until_daa: u64,
    pub maximum_retries: u32,
    pub maximum_pause_daa: u64,
}

impl PanelConsumerLifetimeTermsV2 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/PANEL-CONSUMER-LIFETIME/V2", self)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 2
            || [
                self.authorization,
                self.network,
                self.ruleset,
                self.claim,
                self.class,
                self.work,
                self.producer,
                self.immutable_fields,
            ]
            .contains(&[0; 64])
            || self.accept_by_daa >= self.service_until_daa
            || self.service_until_daa >= self.liability_until_daa
            || self.liability_until_daa == u64::MAX
        {
            return Err("Panel consumer requires a complete finite prospective signed lifetime");
        }
        Ok(())
    }
    pub fn can_start_receipt_window(&self, now: u64, receipt_window_daa: u64, retry: u32) -> bool {
        retry <= self.maximum_retries && now.checked_add(receipt_window_daa).is_some_and(|end| end <= self.service_until_daa)
    }
}

/// Non-circular consent signed before native admission. Claim/work/trace roots
/// depend on a carrying header/body and are attached only after actual native
/// admission, while its Panel is still PendingSeal. This authorization binds
/// one future class execution and finite monetary/resource/clock maxima.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PanelLifetimeAuthorizationV2 {
    pub version: u16,
    pub network: Digest,
    pub ruleset: Digest,
    pub nonce: Digest,
    pub producer: Digest,
    pub class: Digest,
    pub accept_from_daa: u64,
    pub accept_by_daa: u64,
    pub service_until_daa: u64,
    pub liability_until_daa: u64,
    pub maximum_retries: u32,
    pub maximum_pause_daa: u64,
    pub maximum_pwu: u64,
    pub maximum_reserved_sompi: u128,
    pub maximum_escrow_sompi: u64,
    pub maximum_rights_sompi: u128,
}
impl PanelLifetimeAuthorizationV2 {
    pub fn id(&self) -> Digest {
        object_id(b"MISAKA/PALW/PANEL-LIFETIME-AUTHORIZATION/V2", self)
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.version != 2
            || [self.network, self.ruleset, self.nonce, self.producer, self.class].contains(&[0; 64])
            || self.accept_from_daa > self.accept_by_daa
            || self.accept_by_daa >= self.service_until_daa
            || self.service_until_daa >= self.liability_until_daa
            || self.liability_until_daa == u64::MAX
            || self.maximum_pwu == 0
            || self.maximum_reserved_sompi == 0
        {
            return Err("Panel authorization needs one bounded prospective native execution and finite signed clocks");
        }
        Ok(())
    }
}

//! Compile-time reviewed RFC23 implementation/profile readiness evidence.
//! RFC23 §6 fixes one MEASURED candidate artifact/genesis for separate normal
//! and attack multi-node campaigns. Its artifact/source IDs are historical
//! measurement provenance, not an embedded self-hash of this executable.
//! Reviewed fixed records approve exact network/height/Params profiles; genuine
//! retained manifests and detached final-release attestation establish what ran.
//! Empty production registry refuses every Some-height startup today. No CLI,
//! RPC, Params payload or transition extras can inject a readiness record.
use crate::Hash64;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct PalwClaimLocalReadinessRecordV1{
    pub version:u16,
    pub genesis:Hash64,
    pub activation_daa:u64,
    /// Exact real Params fingerprint, already includes the Some-only fence.
    pub params_id:kaspa_hashes::Hash,
    /// Source commit of the implementation that was actually measured, not
    /// necessarily the later record-bearing release commit.
    pub measured_source_commit:[u8;20],
    /// Exact historical tested artifact shared by BOTH campaign manifests.
    /// Adding this record changes a rebuilt artifact; no self-hash equality
    /// or automatic current-source equivalence is claimed here.
    pub measured_binary_artifact:[u8;32],
    /// Canonical evidence manifests. Each names measured_binary_artifact,
    /// genesis/params, commands, exact outcomes and retained raw receipts.
    pub normal_campaign:[u8;32],
    pub attack_campaign:[u8;32],
    pub restoration_and_outsider_rebuttal:[u8;32],
    pub transition_drill:[u8;32],
    /// Actual CPU/RAM/material transport+retention, native capacity limits and
    /// tested network maximum; no invented monetary or duration coefficient.
    pub resource_measurements:[u8;32],
    pub review:[u8;32],
}
/// No genuine stage2 acceptance/measurement record has been supplied here.
/// This list is the only production gate input; callers cannot inject records
/// through Params, node extras, a command line, or an RPC.
pub const PALW_CLAIM_LOCAL_READINESS_RECORDS_V1:&[PalwClaimLocalReadinessRecordV1]=&[];

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum PalwClaimLocalReadinessRefusalV1{NoRecord,Duplicate,Version,Params,Source,Artifact,Evidence,Review}
impl PalwClaimLocalReadinessRefusalV1{
    pub fn reason_v1(self)->&'static str{match self{
        Self::NoRecord=>"RFC-0023 stage 2 cannot arm: no pinned measured readiness record for this genesis and activation",
        Self::Duplicate=>"RFC-0023 readiness records duplicate a genesis and activation",
        Self::Version=>"RFC-0023 readiness record format version is unsupported",
        Self::Params=>"RFC-0023 readiness record differs from exact consensus parameters",
        Self::Source=>"RFC-0023 readiness record has no measured source commit",
        Self::Artifact=>"RFC-0023 readiness record has no historical measured binary artifact",
        Self::Evidence=>"RFC-0023 readiness record lacks a required campaign/drill/resource evidence manifest",
        Self::Review=>"RFC-0023 readiness record has no independent review manifest",
    }}
}
/// Select the reviewed implementation/profile record, as frame-cost admission
/// selects its reviewed cost. Hash checks are structural; repository review
/// must verify actual retained manifest contents/results and the measured-to-
/// release implementation relation. This does not inspect the running binary
/// or verify model computation. Production Params validation always supplies
/// the fixed registry above, never arbitrary input to this pure helper.
pub fn claim_local_readiness_record_admits_v1(
    genesis:Hash64,activation_daa:u64,params_id:kaspa_hashes::Hash,
    records:&[PalwClaimLocalReadinessRecordV1],
)->Result<(),PalwClaimLocalReadinessRefusalV1>{
    use PalwClaimLocalReadinessRefusalV1 as R;
    let mut matching=records.iter().filter(|r|r.genesis==genesis&&r.activation_daa==activation_daa);
    let r=matching.next().ok_or(R::NoRecord)?;
    if matching.next().is_some(){return Err(R::Duplicate)}
    if r.version!=1{return Err(R::Version)}
    if r.params_id!=params_id{return Err(R::Params)}
    if r.measured_source_commit==[0;20]{return Err(R::Source)}
    if r.measured_binary_artifact==[0;32]{return Err(R::Artifact)}
    let evidence=[r.normal_campaign,r.attack_campaign,r.restoration_and_outsider_rebuttal,r.transition_drill,r.resource_measurements];
    if evidence.contains(&[0;32])||r.normal_campaign==r.attack_campaign{return Err(R::Evidence)}
    if r.review==[0;32]||r.review==r.measured_binary_artifact||evidence.contains(&r.review){return Err(R::Review)}
    Ok(())
}
#[cfg(test)]
mod tests{
    use super::*;
    fn synthetic()->PalwClaimLocalReadinessRecordV1{
        // Structural unit fixture only. Never in the production registry and
        // never represented as a measured or reviewed network campaign.
        PalwClaimLocalReadinessRecordV1{version:1,genesis:Hash64::from_u64_word(1),activation_daa:100,params_id:kaspa_hashes::Hash::from_bytes([9;32]),measured_source_commit:[1;20],measured_binary_artifact:[2;32],normal_campaign:[3;32],attack_campaign:[4;32],restoration_and_outsider_rebuttal:[5;32],transition_drill:[6;32],resource_measurements:[7;32],review:[8;32]}
    }
    #[test]
    fn rfc23s2_readiness_pinned_record_scope_and_missing_evidence_refuse(){
        use PalwClaimLocalReadinessRefusalV1 as R;
        let r=synthetic();let check=|rows:&[PalwClaimLocalReadinessRecordV1]|claim_local_readiness_record_admits_v1(r.genesis,r.activation_daa,r.params_id,rows);
        assert_eq!(check(PALW_CLAIM_LOCAL_READINESS_RECORDS_V1),Err(R::NoRecord));
        assert_eq!(check(&[r]),Ok(()));
        assert_eq!(check(&[r,r]),Err(R::Duplicate));
        assert_eq!(claim_local_readiness_record_admits_v1(Hash64::from_u64_word(2),r.activation_daa,r.params_id,&[r]),Err(R::NoRecord));
        assert_eq!(claim_local_readiness_record_admits_v1(r.genesis,r.activation_daa+1,r.params_id,&[r]),Err(R::NoRecord));
        for bad in [PalwClaimLocalReadinessRecordV1{version:2,..r},PalwClaimLocalReadinessRecordV1{params_id:kaspa_hashes::Hash::from_bytes([10;32]),..r},PalwClaimLocalReadinessRecordV1{measured_source_commit:[0;20],..r},PalwClaimLocalReadinessRecordV1{measured_binary_artifact:[0;32],..r},PalwClaimLocalReadinessRecordV1{normal_campaign:[0;32],..r},PalwClaimLocalReadinessRecordV1{attack_campaign:[0;32],..r},PalwClaimLocalReadinessRecordV1{restoration_and_outsider_rebuttal:[0;32],..r},PalwClaimLocalReadinessRecordV1{transition_drill:[0;32],..r},PalwClaimLocalReadinessRecordV1{resource_measurements:[0;32],..r},PalwClaimLocalReadinessRecordV1{review:[0;32],..r},PalwClaimLocalReadinessRecordV1{review:r.normal_campaign,..r},PalwClaimLocalReadinessRecordV1{attack_campaign:r.normal_campaign,..r}]{assert!(check(&[bad]).is_err());}
    }
    #[test]
    fn rfc23s2_readiness_historical_subject_is_not_a_self_hash_assertion(){
        let r=synthetic();
        let historical=PalwClaimLocalReadinessRecordV1{measured_source_commit:[11;20],measured_binary_artifact:[12;32],..r};
        // The fixed registry's reviewed provenance describes the artifact that
        // ran. This selector does not receive or compare a current binary hash.
        // These are structural fixtures only, never actual reviewed records.
        assert_eq!(claim_local_readiness_record_admits_v1(r.genesis,r.activation_daa,r.params_id,&[historical]),Ok(()));
        assert_eq!(claim_local_readiness_record_admits_v1(r.genesis,r.activation_daa,r.params_id,PALW_CLAIM_LOCAL_READINESS_RECORDS_V1),Err(PalwClaimLocalReadinessRefusalV1::NoRecord));
    }

}

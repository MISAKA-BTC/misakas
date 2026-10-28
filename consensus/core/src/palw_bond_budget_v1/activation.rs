//! Evidence gate for the monetary schema migrations. Records do not cure an
//! open implementation blocker and are never synthesized from an activation request.
use crate::{
    config::params::Params,
    palw_mode_v2::{PalwConsensusMode, PalwModeV2Error},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwIncomeSchemaV1 {
    ReporterV2 = 2,
    MaterialV3 = 3,
}

/// Four distinct artifacts: production fold/migration tests, complete monetary
/// lifecycle tests, node crossing/restart/reorg/pruned-join, independent review.
/// Their content must cover this exact network, height and assembled state policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwIncomeArmingRecordV1 {
    pub schema: PalwIncomeSchemaV1,
    pub network_domain: [u8; 64],
    pub policy_root: [u8; 64],
    pub activation_daa: u64,
    pub evidence: [[u8; 32]; 4],
}

/// No shipping approval is implied by the local migration tests. A reviewed
/// release change pins records only after all four artifacts exist.
pub const PALW_INCOME_ARMING_RECORDS_V1: &[PalwIncomeArmingRecordV1] = &[];

impl Params {
    pub fn palw_income_policy_root_v1(&self, schema: PalwIncomeSchemaV1) -> Option<[u8; 64]> {
        let PalwConsensusMode::ConsensusV2(bundle) = &self.palw_consensus_mode else { return None };
        Some(misaka_palw_kernel::hash::object_id(
            b"misaka-palw/income-activation/v1",
            &(schema, self.consensus_params_id().as_bytes(), &bundle.state),
        ))
    }

    pub fn validate_palw_income_record_v1(
        &self,
        schema: PalwIncomeSchemaV1,
        activation_daa: u64,
        records: &[PalwIncomeArmingRecordV1],
    ) -> Result<(), PalwModeV2Error> {
        // The V3 signed material/TTL and physical payment lifecycle must be
        // implemented before a record can authorize it. Evidence cannot waive it.
        if schema == PalwIncomeSchemaV1::MaterialV3 {
            return Err(PalwModeV2Error::Invalid(
                "palw_material_income_v3 cannot be armed: complete signed material/TTL and all consumer lifecycle validation remain pending",
            ));
        }
        let domain = self.palw_rfc0608_network_domain_v1();
        let policy = self.palw_income_policy_root_v1(schema);
        let matches = records.iter().any(|r| {
            r.schema == schema
                && r.activation_daa == activation_daa
                && r.network_domain == domain
                && Some(r.policy_root) == policy
                && r.evidence.iter().enumerate().all(|(i, digest)| *digest != [0; 32] && !r.evidence[..i].contains(digest))
        });
        if !matches {
            return Err(PalwModeV2Error::Invalid(
                "palw_reporter_income_v2 cannot be armed: no exact-network/height/policy record for legacy cash migration, complete lifecycle, node crossing/restart/reorg/pruned-join and independent review",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::{ForkActivation, palw_t12_shipped_params};

    #[test]
    fn migration_evidence_cannot_be_reused_on_a_different_contract_or_replace_missing_artifacts() {
        let mut p = palw_t12_shipped_params();
        p.palw_reporter_income_v2 = Some(ForkActivation::new(17_400));
        p.sync_palw_bond_budget_v1();
        let kind = PalwIncomeSchemaV1::ReporterV2;
        let record = PalwIncomeArmingRecordV1 {
            schema: kind,
            network_domain: p.palw_rfc0608_network_domain_v1(),
            policy_root: p.palw_income_policy_root_v1(kind).unwrap(),
            activation_daa: 17_400,
            evidence: [[1; 32], [2; 32], [3; 32], [4; 32]],
        };
        p.validate_palw_income_record_v1(kind, 17_400, &[record]).unwrap();
        assert!(p.validate_palw_income_record_v1(kind, 17_400, &[]).is_err());
        assert!(p.validate_palw_reporter_income_v2().is_err(), "test records are not shipping records");
        assert!(p.validate_palw_income_record_v1(kind, 17_401, &[record]).is_err());
        for index in 0..4 {
            let mut missing = record;
            missing.evidence[index] = [0; 32];
            assert!(p.validate_palw_income_record_v1(kind, 17_400, &[missing]).is_err());
        }
        let mut reused = record;
        reused.evidence[3] = reused.evidence[0];
        assert!(p.validate_palw_income_record_v1(kind, 17_400, &[reused]).is_err());
        let mut foreign = record;
        foreign.network_domain[0] ^= 1;
        assert!(p.validate_palw_income_record_v1(kind, 17_400, &[foreign]).is_err());
        p.palw_material_income_v3 = Some(ForkActivation::new(17_400));
        p.sync_palw_bond_budget_v1();
        assert!(p.validate_palw_income_record_v1(kind, 17_400, &[record]).is_err(), "the assembled policy changed");
        let mut material = record;
        material.schema = PalwIncomeSchemaV1::MaterialV3;
        material.policy_root = p.palw_income_policy_root_v1(material.schema).unwrap();
        assert!(
            p.validate_palw_income_record_v1(material.schema, 17_400, &[material]).is_err(),
            "records never waive the implementation gap"
        );
    }
}

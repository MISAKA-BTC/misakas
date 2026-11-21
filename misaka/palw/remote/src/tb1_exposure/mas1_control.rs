//! Live reader for an isolated MODEL_STOP certificate. Three independent native
//! inclusions fix the authorized roster, open the case, then accept its signatures.
//! Roster selection, reservations and public activation are separate owning folds.
use super::invalid;
use kaspa_consensus_core::{Hash64, api::ConsensusApi, palw_mas1::StopReason};
use libcrux_ml_dsa::ml_dsa_65::{self, MLDSA65Signature, MLDSA65VerificationKey};
use sha3::{Digest, Sha3_512};
use std::{collections::BTreeSet, io};

pub const SIGN_DOMAIN: &[u8] = b"MISAKA/private/MAS1/MODEL_STOP/v1";

pub struct StopAuthority<'a> {
    pub network: [u8; 64],
    pub epoch: u64,
    /// Native TB1 holder roster reference and corresponding dedicated control keys, in roster order.
    pub tb1_roster: [u8; 64],
    pub keys: &'a [MLDSA65VerificationKey],
    pub fault_budget: u32,
}

impl StopAuthority<'_> {
    pub fn marker(&self, max_roster: usize) -> io::Result<[u8; 64]> {
        let n = self.keys.len();
        if self.fault_budget == 0
            || n > max_roster
            || Some(n) != (self.fault_budget as usize).checked_mul(3).and_then(|x| x.checked_add(1))
        {
            return Err(invalid("invalid MAS1 fault budget or roster length"));
        }
        let mut seen = BTreeSet::new();
        let mut h = Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/MODEL_STOP/authority/v1")
            .chain_update(self.network)
            .chain_update(self.epoch.to_be_bytes())
            .chain_update(self.tb1_roster)
            .chain_update(self.fault_budget.to_be_bytes());
        for key in self.keys {
            if !seen.insert(key.as_slice()) {
                return Err(invalid("duplicate MAS1 control key"));
            }
            h.update(key.as_slice());
        }
        Ok(h.finalize().into())
    }
}

#[derive(Clone, Copy)]
pub enum StopScope {
    ArtifactProfile,
    RuleVersion,
}

/// The descriptor and rule references stay bound even for a rule-wide stop.
/// No owner or class alias can change the scope of the signed record.
#[derive(Clone, Copy)]
pub struct MiningDescriptor {
    pub artifact: [u8; 64],
    pub program: [u8; 64],
    pub profile: [u8; 64],
    pub readout: [u8; 64],
    pub rule_version: [u8; 64],
}

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum RiskReason {
    UnresolvedAdmission = 1,
    ShortcutRisk,
    WorkRuleRisk,
    UsefulnessRisk,
    CheckerCoverageRisk,
}

pub struct StopCase {
    pub scope: StopScope,
    pub descriptor: MiningDescriptor,
    pub generation: u64,
    pub case_id: [u8; 64],
    pub reason: RiskReason,
    pub evidence: [u8; 64],
    pub policy: [u8; 64],
    pub deadline_exclusive_daa: u64,
}

impl StopCase {
    pub fn marker(&self, authority_block: Hash64, authority_marker: &[u8; 64]) -> [u8; 64] {
        let scope = match self.scope {
            StopScope::ArtifactProfile => 1,
            StopScope::RuleVersion => 2,
        };
        Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/MODEL_STOP/case/v1")
            .chain_update(authority_block.as_bytes())
            .chain_update(authority_marker)
            .chain_update([scope])
            .chain_update(self.descriptor.artifact)
            .chain_update(self.descriptor.program)
            .chain_update(self.descriptor.profile)
            .chain_update(self.descriptor.readout)
            .chain_update(self.descriptor.rule_version)
            .chain_update(self.generation.to_be_bytes())
            .chain_update(self.case_id)
            .chain_update([self.reason as u8])
            .chain_update(self.evidence)
            .chain_update(self.policy)
            .chain_update(self.deadline_exclusive_daa.to_be_bytes())
            .finalize()
            .into()
    }
}

pub struct StopSignature<'a> {
    pub slot: u32,
    pub signature: &'a MLDSA65Signature,
}

/// Supplied by the trusted, local isolation profile (in production the native
/// role/activation fold would own it), never selected by a certificate sender.
pub struct StopEnrollment {
    pub accepting_block: Hash64,
    pub authorized_marker: [u8; 64],
    pub max_roster: usize,
}

pub struct OpenStopCase<'a> {
    authority: &'a StopAuthority<'a>,
    origin: Hash64,
    deadline: u64,
    scope: StopScope,
    body: [u8; 64],
    descriptor: MiningDescriptor,
}

impl<'a> OpenStopCase<'a> {
    pub fn read(
        native: &dyn ConsensusApi,
        authority: &'a StopAuthority<'a>,
        enrollment: &StopEnrollment,
        case: &StopCase,
        origin: Hash64,
    ) -> io::Result<Self> {
        let tip = native.get_sink();
        let authority_block = enrollment.accepting_block;
        let auth = authority.marker(enrollment.max_roster)?;
        if auth != enrollment.authorized_marker {
            return Err(invalid("unassigned MODEL_STOP authority"));
        }
        let marker = case.marker(authority_block, &auth);
        if origin == authority_block
            || !native.is_chain_ancestor_of(authority_block, origin).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(origin, tip).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(authority.network, authority_block, &auth).map_err(io::Error::other)?
            || !native.palw_coinbase_commits_to(authority.network, origin, &marker).map_err(io::Error::other)?
            || native.get_header(origin).map_err(io::Error::other)?.daa_score >= case.deadline_exclusive_daa
        {
            return Err(invalid("MAS1 case is outside its native authority or window"));
        }
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let body = Sha3_512::new().chain_update(SIGN_DOMAIN).chain_update(origin.as_bytes()).chain_update(marker).finalize().into();
        Ok(Self { authority, origin, deadline: case.deadline_exclusive_daa, scope: case.scope, descriptor: case.descriptor, body })
    }

    pub fn signing_body(&self) -> &[u8; 64] {
        &self.body
    }

    pub fn certificate_marker(&self, rows: &[StopSignature<'_>]) -> io::Result<[u8; 64]> {
        let quorum = 2 * (self.authority.fault_budget as usize) + 1;
        if rows.len() < quorum || rows.len() > self.authority.keys.len() {
            return Err(invalid("MAS1 signature quorum"));
        }
        let mut prior = None;
        let mut h = Sha3_512::new()
            .chain_update(b"MISAKA/private/MAS1/MODEL_STOP/certificate/v1")
            .chain_update(self.body)
            .chain_update(u32::try_from(rows.len()).map_err(|_| invalid("MAS1 certificate count"))?.to_be_bytes());
        for row in rows {
            let key = self.authority.keys.get(row.slot as usize).ok_or_else(|| invalid("unknown MAS1 control slot"))?;
            if prior.is_some_and(|s| s >= row.slot) {
                return Err(invalid("duplicated or unordered MAS1 control slots"));
            }
            ml_dsa_65::verify(key, &self.body, SIGN_DOMAIN, row.signature).map_err(|_| invalid("invalid MAS1 control signature"))?;
            h.update(row.slot.to_be_bytes());
            h.update(row.signature.as_slice());
            prior = Some(row.slot);
        }
        Ok(h.finalize().into())
    }

    /// Check native inclusion whenever applying the reason to new mining effects.
    /// The acceptance deadline does not expire an already included stop.
    pub fn read_stop(
        &self,
        native: &dyn ConsensusApi,
        accepted: Hash64,
        rows: &[StopSignature<'_>],
        target: &MiningDescriptor,
    ) -> io::Result<StopReason> {
        let tip = native.get_sink();
        if accepted == self.origin
            || !native.is_chain_ancestor_of(self.origin, accepted).map_err(io::Error::other)?
            || !native.is_chain_ancestor_of(accepted, tip).map_err(io::Error::other)?
            || native.get_header(accepted).map_err(io::Error::other)?.daa_score >= self.deadline
            || !native
                .palw_coinbase_commits_to(self.authority.network, accepted, &self.certificate_marker(rows)?)
                .map_err(io::Error::other)?
        {
            return Err(invalid("MAS1 stop not included in selected native history within its deadline"));
        }
        if native.get_sink() != tip {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let subject = &self.descriptor;
        match self.scope {
            StopScope::ArtifactProfile
                if (subject.artifact, subject.program, subject.profile, subject.readout)
                    == (target.artifact, target.program, target.profile, target.readout) =>
            {
                Ok(StopReason::BftSafetyStop)
            }
            StopScope::RuleVersion if subject.rule_version == target.rule_version => Ok(StopReason::RuleDisabled),
            _ => Err(invalid("MAS1 control does not target this descriptor or rule")),
        }
    }
}

//! Actual native P fold. Private TransitionBuilder hooks, rooted C/A reads,
//! actual round anchor/global bond pool/shared N queue, literal new attestations.
//! This never calls old S/K/V/PublicCheckCertified or credits issuance/weight.
use super::*;
use super::{positive_assignment_case::NativeAssignedCaseSessionV2, positive_assignment_verifiers::AssignmentVerifierCandidatesV2};
use crate::{
    palw_kernel_route_v1::palw_kernel_bond_id_v1,
    palw_model_eligibility_v1::PalwEligibilityVerifierV1,
    palw_native_resource_v1::{NativeCoverageV1, NativeDemandLeaseV1, TirNativeRequestsV1, reserve_native_requests_v1},
    palw_positive_assignment_material_v2::{AssignmentAdmissionMaterialV2, assignment_custody_bytes_id_v2},
    palw_positive_certification_v2::*,
    palw_positive_check_v2::CheckOutcome,
    palw_positive_court_v2::coordinator::contract::TerminalKindV2,
    palw_positive_rows_v2::PositiveRowsV2,
    palw_security_certification_v1::{
        PalwCertificationNetworkRowV1, certification_network_key_v1, palw_certification_draw_v1, palw_certification_pool_admits_v1,
        palw_certification_seed_v1,
    },
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    assignment::{
        AssignmentId,
        open_material::AssignmentOpenPreimageKindV2,
        rows::{AssignmentRow, AssignmentStatusV2},
    },
    certification::*,
    identity::*,
    messages::{ClaimStatus, WitnessStatus},
    native::{NativeRowKey, certification_subject},
    rows::*,
};
fn refuse(why: impl std::fmt::Debug) -> PalwStateV2Error {
    PalwStateV2Error::KernelRouteRefused(format!("Positive P: {why:?}"))
}
fn unavailable(_why: &str) -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable)
}
fn bond(point: NativeOutpoint) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_bytes(point.transaction.0), point.index))
}
fn key(subject: Digest) -> [u8; 65] {
    let mut key = [0; 65];
    key[0] = b'P';
    key[1..].copy_from_slice(&subject.0);
    key
}
struct OwnedP {
    row: PositiveCertificationRowV2,
    _lease: NativeDemandLeaseV1,
}
fn pool_workspace_bytes(count: u64) -> Result<u64, NativeCoverageV1> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<PalwEligibilityVerifierV1>(count)?;
    q.vec_v1::<([u8; 64], u64)>(count)?;
    // The shared eligibility draw stably sorts its projected weights. Cover
    // a full row-length scratch buffer while both rosters are live.
    q.exact_v1::<([u8; 64], u64)>(count)?;
    Ok(q.bytes_v1())
}
pub(super) fn positive_certification_workspace_bytes_v2(m: u16, pool: u64) -> Result<u64, PalwStateV2Error> {
    if m == 0 || m > MAX_POSITIVE_DRAW_POSITIONS_V2 {
        return Err(refuse("unsupported actual P draw codec"));
    }
    // The shared N owner counts claims, not positions. Always reserve the
    // adopted75-position envelope per certification, without changing its
    // actual frozen m/k or the old queue's units.
    let positions = u64::from(MAX_POSITIVE_DRAW_POSITIONS_V2);
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<NativeRowValue>(8).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<u8>((8 * MAX_NATIVE_KEY_BYTES * 2) as u64).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<PositiveCertificationRowV2>(2).map_err(PalwStateV2Error::NativeResource)?;
    q.exact_v1::<PositiveCertificationVoteV2>(2).map_err(PalwStateV2Error::NativeResource)?;
    q.vec_v1::<PositiveCertificationVoteV2>(positions).map_err(PalwStateV2Error::NativeResource)?;
    q.vec_v1::<Digest>(positions).map_err(PalwStateV2Error::NativeResource)?;
    q.vec_v1::<[u8; 64]>(positions).map_err(PalwStateV2Error::NativeResource)?;
    q.add_v1(pool_workspace_bytes(pool).map_err(PalwStateV2Error::NativeResource)?).map_err(PalwStateV2Error::NativeResource)?;
    // Exact fixed Borsh maxima: header/finality/outcome/release4823, drawmeta440,
    // 64 per draw position,8348 per literal signed vote; no encoded/RSS guess.
    let encoded = 4823u64
        .checked_add(1 + 440 + 4 + 4)
        .and_then(|n| positions.checked_mul(64 + 8348).and_then(|x| n.checked_add(x)))
        .ok_or_else(|| refuse("P codec arithmetic"))?;
    q.vec_v1::<u8>(encoded.max(1024)).map_err(PalwStateV2Error::NativeResource)?;
    Ok(q.bytes_v1())
}
fn lease(m: u16, pool: u64) -> Result<NativeDemandLeaseV1, PalwStateV2Error> {
    reserve_p_workspace(positive_certification_workspace_bytes_v2(m, pool)?)
}
pub(super) fn reserve_p_workspace(bytes: u64) -> Result<NativeDemandLeaseV1, PalwStateV2Error> {
    reserve_native_requests_v1(bytes).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(|| {
        PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PositiveCertificationRowV2>(
            "P memory owner",
            "actual checker is not armed",
        ))
    })
}
fn raw(state: &PalwChainStateV2, subject: Digest) -> Option<&[u8]> {
    let key = key(subject);
    let route = state.kernel_route_state_v1()?;
    route.aux.range((136, Vec::new())..(137, Vec::new())).find(|((_, k), _)| k.as_slice() == key).map(|(_, v)| v.as_slice())
}
fn read(state: &PalwChainStateV2, subject: Digest, pool: u64) -> Result<OwnedP, PalwStateV2Error> {
    let bytes = raw(state, subject).ok_or_else(|| refuse("missing P row"))?;
    if bytes.len() > MAX_POSITIVE_CERTIFICATION_ROW_BYTES_V2 {
        return Err(refuse("P codec bound"));
    }
    let mut prefix = bytes;
    let fixed = PositiveCertificationFixedV2::deserialize(&mut prefix).map_err(refuse)?;
    fixed.header.validate().map_err(refuse)?;
    let owner = lease(fixed.header.policy.m, pool)?;
    let row = PositiveCertificationRowV2::decode(&key(subject), bytes).map_err(refuse)?;
    Ok(OwnedP { row, _lease: owner })
}
fn write(builder: &mut TransitionBuilder<'_>, row: &PositiveCertificationRowV2) -> Result<(), PalwStateV2Error> {
    row.validate_shape().map_err(refuse)?;
    let bytes = borsh::object_length(row).map_err(refuse)?;
    if bytes > MAX_POSITIVE_CERTIFICATION_ROW_BYTES_V2 {
        return Err(refuse("P encoded bound"));
    }
    builder.write_kernel_row_encoded_v1(136, &key(row.fixed.header.subject), Some(row))
}
fn actual_origin(ctx: &PalwBlockContextV2, origin: AcceptedOrigin) -> Result<(), PalwStateV2Error> {
    if origin.accepting_block.0 != ctx.block.as_bytes()
        || origin.accepted_daa != ctx.daa_score
        || origin.accepted_blue_score != ctx.blue_score
    {
        return Err(refuse("foreign actual event origin"));
    }
    Ok(())
}
fn assignment(state: &PalwChainStateV2, id: AssignmentId) -> Result<AssignmentRow, PalwStateV2Error> {
    match PositiveRowsV2::new(state).read(NativeRowKey::Assignment(id)).map_err(refuse)?.map(|r| r.body) {
        Some(NativeRowBody::Assignment(a)) => Ok(a),
        _ => Err(refuse("actual Assignment missing")),
    }
}
fn queue_policy(builder: &TransitionBuilder<'_>, h: &PositiveCertificationHeaderV2, now: u64) -> Result<(), PalwStateV2Error> {
    let mirror = builder.params.rfc0018_v1().filter(|m| m.gate_c_at(now)).ok_or_else(|| refuse("actual Gate C policy unavailable"))?;
    if mirror.certification.epoch_daa != h.policy.epoch_daa
        || mirror.certification.draws_per_epoch != h.policy.draws_per_epoch
        || mirror.certification.m > MAX_POSITIVE_DRAW_POSITIONS_V2
    {
        return Err(refuse("frozen P policy conflicts with actual shared N owner"));
    }
    Ok(())
}
fn case_live(state: &PalwChainStateV2, h: &PositiveCertificationHeaderV2) -> Result<bool, PalwStateV2Error> {
    let Some(value) = PositiveRowsV2::new(state).read(NativeRowKey::Case(h.case)).map_err(refuse)? else {
        return Err(unavailable("actual P Case unavailable; missing is not withdrawn"));
    };
    let NativeRowBody::Case(case) = value.body else { return Err(refuse("wrong native Case row")) };
    Ok(case.result == h.result
        && !matches!(case.status, ClaimStatus::ClaimRefuted | ClaimStatus::Revoked | ClaimStatus::EndedUnverified))
}
pub(super) fn live_verifiers<'a>(
    state: &'a PalwChainStateV2,
    policy: &'a crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    commit_daa: u64,
    worker_operator: Digest,
    keys: impl Iterator<Item = PalwBondKeyV2> + 'a,
) -> impl Iterator<Item = PalwEligibilityVerifierV1> + 'a {
    keys.filter_map(move |key| {
        let bond = state.bonds.get(&key)?;
        let operator = bond.operator_id.as_bytes();
        palw_certification_pool_admits_v1(
            policy,
            commit_daa,
            &worker_operator.0,
            matches!(bond.status, PalwBondStatusV2::Active),
            bond.registered_daa,
            bond.collateral,
            &operator,
        )
        .then(|| PalwEligibilityVerifierV1 { bond: palw_kernel_bond_id_v1(&key), operator, stake: bond.collateral })
    })
}
impl TransitionBuilder<'_> {
    /// Call INSIDE the C install checkpoint, after exact aux151 Result/Case/Use
    /// and actual fresh C funds/reservation admission. Original C is the commit.
    pub(super) fn begin_positive_certification_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        id: AssignmentId,
        material: &AssignmentAdmissionMaterialV2,
    ) -> Result<(), PalwStateV2Error> {
        let a = assignment(&self.state, id)?;
        material.check_row(&a).map_err(refuse)?;
        let AssignmentStatusV2::Consumed { case, result } = a.status else {
            return Err(refuse("unconsumed Assignment has no P subject"));
        };
        let bundle = PositiveRowsV2::new(&self.state)
            .bundle(
                PositiveRowsV2::new(&self.state)
                    .read(NativeRowKey::Result(result))
                    .map_err(refuse)?
                    .and_then(|v| match v.body {
                        NativeRowBody::Result(r) => Some(r.result.claim_id()),
                        _ => None,
                    })
                    .ok_or_else(|| refuse("actual Result missing"))?,
            )
            .map_err(refuse)?;
        if bundle.case.case != case || bundle.result.result.id() != result {
            return Err(refuse("C/Result binding"));
        }
        actual_origin(ctx, bundle.result.origin)?;
        let worker = self.state.bonds.get(&bond(a.bundle.terms.worker)).ok_or_else(|| refuse("actual worker bond missing"))?;
        if worker.pubkey.as_slice() != a.worker_key.as_slice() {
            return Err(refuse("worker key changed"));
        }
        let subject = certification_subject(&bundle.result.result);
        if raw(&self.state, subject).is_some() {
            return Err(refuse("P begin is once only, never resets its clock"));
        }
        let h = PositiveCertificationHeaderV2 {
            version: 2,
            revision: 1,
            assignment: id,
            subject,
            claim: bundle.result.result.claim_id(),
            result,
            case,
            scope: bundle.result.result.scope,
            policy: material.certification_policy,
            policy_id: positive_certification_policy_id_v2(&material.certification_policy),
            commit_origin: bundle.result.origin,
            committed_span: self.state.round_seed_anchor.as_ref().map(|a| a.span),
            worker: a.bundle.terms.worker,
            worker_operator: Digest(worker.operator_id.as_bytes()),
            worker_key: a.worker_key,
            worker_gain: a.bundle.terms.worker_reward,
            liability: bundle.case.liability,
        };
        h.validate().map_err(refuse)?;
        queue_policy(self, &h, ctx.daa_score)?;
        let _owner = lease(h.policy.m, 0)?;
        write(
            self,
            &PositiveCertificationRowV2 {
                fixed: PositiveCertificationFixedV2 { header: h, finality: None, outcome: None, liability_release: None },
                draw: None,
                votes: Vec::new(),
            },
        )
    }
    /// Draw with the actual later seed and live global bond state. The
    /// original reservation preimage bounds which keys need to be looked up.
    fn draw_positive_certification_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        subject: Digest,
        material: &AssignmentAdmissionMaterialV2,
        reservation_preimage: &[u8],
    ) -> Result<(), PalwStateV2Error> {
        let origin = PositiveCertificationDrawOriginV2 {
            block: NativeHash64(ctx.block.as_bytes()),
            daa: ctx.daa_score,
            blue_score: ctx.blue_score,
        };
        let mut owned = read(&self.state, subject, 0)?;
        let row = &mut owned.row;
        if row.fixed.outcome.is_some() || row.draw.is_some() {
            return Err(refuse("P draw already fixed"));
        }
        let h = &row.fixed.header;
        queue_policy(self, h, ctx.daa_score)?;
        if ctx.daa_score
            > h.commit_origin.accepted_daa.checked_add(h.policy.certify_by_daa).ok_or_else(|| refuse("draw clock overflow"))?
        {
            return Err(refuse("P draw deadline elapsed"));
        }
        let anchor = self.state.round_seed_anchor.ok_or_else(|| refuse("actual later execution seed unavailable"))?;
        let earliest = h
            .committed_span
            .unwrap_or(0)
            .checked_add(h.policy.anchor_delay_spans.max(1))
            .ok_or_else(|| refuse("seed span overflow"))?;
        if anchor.span < earliest {
            return Err(refuse("seed existed too early"));
        }
        let network_key = certification_network_key_v1();
        let mut network = match self.state.kernel_route.as_ref().and_then(|r| r.aux.get(&(136, network_key.clone()))) {
            None => PalwCertificationNetworkRowV1::default(),
            Some(b) => borsh::from_slice(b).map_err(refuse)?,
        };
        let epoch = ctx.daa_score / h.policy.epoch_daa;
        if network.epoch != epoch {
            network = PalwCertificationNetworkRowV1 { epoch, draws: 0 };
        }
        if network.draws >= h.policy.draws_per_epoch {
            return Err(refuse("actual shared N queue full"));
        }
        let a = assignment(&self.state, h.assignment)?;
        material.check_row(&a).map_err(refuse)?;
        if a.status != (AssignmentStatusV2::Consumed { case: h.case, result: h.result }) || material.certification_policy != h.policy {
            return Err(refuse("P draw differs from the original Assignment and Case"));
        }
        if reservation_preimage.len() as u64 > material.money.usage.retained_bytes.min(material.money.usage.replay_bytes)
            || assignment_custody_bytes_id_v2(reservation_preimage)
                != material.custody.preimage_bytes[AssignmentOpenPreimageKindV2::ReservationCustody as usize]
        {
            return Err(unavailable("original P reservation preimage unavailable or corrupted"));
        }
        let candidates = AssignmentVerifierCandidatesV2::decode(reservation_preimage)
            .map_err(|_| unavailable("invalid retained verifier candidates"))?;
        let keys = candidates.keys();
        let count = keys.len();
        let _pool_lease = reserve_p_workspace(pool_workspace_bytes(count as u64).map_err(PalwStateV2Error::NativeResource)?)?;
        let mut pool = Vec::new();
        let allocation = || {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<PalwEligibilityVerifierV1>(
                "P eligible roster",
                "allocation did not fit the leased capacity",
            ))
        };
        pool.try_reserve_exact(count).map_err(|_| allocation())?;
        if pool.capacity() > count {
            return Err(allocation());
        }
        pool.extend(live_verifiers(&self.state, &h.policy, h.commit_origin.accepted_daa, h.worker_operator, keys));
        pool.sort_unstable_by_key(|member| member.bond);
        struct Roster<'a>(&'a [PalwEligibilityVerifierV1]);
        impl BorshSerialize for Roster<'_> {
            fn serialize<W: std::io::Write>(&self, w: &mut W) -> std::io::Result<()> {
                (2u16, 1u16, self.0.len() as u64).serialize(w)?;
                for p in self.0 {
                    (p.bond, p.operator, p.stake).serialize(w)?;
                }
                Ok(())
            }
        }
        let roster = p_hash(b"misaka/positive/certification-roster/v2/r1", &Roster(&pool));
        let roster_stake =
            pool.iter().try_fold(0u128, |n, p| n.checked_add(u128::from(p.stake))).ok_or_else(|| refuse("roster stake overflow"))?;
        let seed = Digest(palw_certification_seed_v1(&subject.0, anchor.span, &anchor.execution_key.as_bytes()));
        let positions = if pool.is_empty() {
            Vec::new()
        } else {
            palw_certification_draw_v1(&seed.0, &subject.0, &pool, h.policy.m)
                .ok_or_else(|| refuse("actual nonempty pool draw failed"))?
                .into_iter()
                .map(Digest)
                .collect()
        };
        let mut draw = PositiveCertificationDrawV2 {
            meta: PositiveCertificationDrawMetaV2 {
                roster,
                roster_count: pool.len() as u64,
                roster_stake,
                seed,
                anchor_span: anchor.span,
                anchor_block: NativeHash64(anchor.block.as_bytes()),
                execution_key: NativeHash64(anchor.execution_key.as_bytes()),
                origin,
                deadline_daa: ctx.daa_score.checked_add(h.policy.attest_window_daa).ok_or_else(|| refuse("attest clock overflow"))?,
                id: Digest([0; 64]),
            },
            positions,
        };
        draw.meta.id = draw.expected_id(h);
        if draw.positions.is_empty() {
            row.fixed.outcome = Some((PositiveCertificationOutcomeV2::NoPopulation, ctx.daa_score));
        }
        row.draw = Some(draw);
        network.draws = network.draws.checked_add(1).ok_or_else(|| refuse("N overflow"))?;
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            write(self, row)?;
            self.write_kernel_row_encoded_v1(136, &network_key, Some(&network))
        })();
        if result.is_err() {
            self.restore(checkpoint)
        }
        result
    }
    /// New op7 only. A positive vote additionally requires the concrete fresh
    /// native replay/closed-topology session before installing real collateral.
    pub(super) fn attest_positive_certification_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        signer: &PalwBondKeyV2,
        statement: &PositiveCourtAttestationV2,
        signature: &[u8; SIGNATURE_BYTES],
        origin: AcceptedOrigin,
        session: Option<&NativeAssignedCaseSessionV2>,
    ) -> Result<(), PalwStateV2Error> {
        actual_origin(ctx, origin)?;
        let mut owned = read(&self.state, statement.subject, 0)?;
        let row = &mut owned.row;
        let h = &row.fixed.header;
        if row.fixed.outcome.is_some() {
            return Err(refuse("P already decided"));
        }
        let draw = row.draw.as_ref().ok_or_else(|| refuse("P not drawn"))?;
        let finality = row.fixed.finality.ok_or_else(|| refuse("actual full Finish target not fixed"))?;
        if ctx.daa_score > draw.meta.deadline_daa {
            return Err(refuse("P attestation deadline elapsed"));
        }
        let verifier = Digest(palw_kernel_bond_id_v1(signer));
        if !draw.positions.contains(&verifier) || row.votes.iter().any(|v| v.statement.verifier_bond == verifier) {
            return Err(refuse("not drawn or duplicate bond vote"));
        }
        let record = self.state.bonds.get(signer).ok_or(PalwStateV2Error::MissingBond(*signer))?;
        if !palw_certification_pool_admits_v1(
            &h.policy,
            h.commit_origin.accepted_daa,
            &h.worker_operator.0,
            matches!(record.status, PalwBondStatusV2::Active),
            record.registered_daa,
            record.collateral,
            &record.operator_id.as_bytes(),
        ) {
            return Err(refuse("actual verifier eligibility changed"));
        }
        let key: &[u8; PUBKEY_BYTES] = record.pubkey.as_slice().try_into().map_err(refuse)?;
        let result = PositiveRowsV2::new(&self.state).bundle(h.claim).map_err(refuse)?;
        statement
            .authenticate(
                &result.result.result,
                &PositiveAttestationExpectationsV2 {
                    scope: h.scope,
                    policy: h.policy_id,
                    draw_anchor: draw.meta.id,
                    witness: finality.witness,
                    verifier_bond: verifier,
                    verifier: principal(key),
                    checked_material: finality.material.digest,
                },
                key,
                signature,
            )
            .map_err(refuse)?;
        let liability_amount = if statement.verdict == PositiveCourtVerdictV2::PositiveChecked {
            let session = session.ok_or_else(|| unavailable("complete native checked/closed-topology session missing"))?;
            check_session(self, ctx, row, session)?;
            let amount = crate::palw_g14_lane_closure_v1::palw_kernel_signer_lock_amount_v1(h.worker_gain, h.policy.k as u8);
            let free = u128::from(record.collateral)
                .checked_sub(self.committed_at(signer, ctx.daa_score))
                .ok_or_else(|| refuse("verifier already overcommitted"))?;
            if u128::from(amount) > free {
                return Err(refuse("verifier cannot back P liability"));
            }
            amount
        } else {
            0
        };
        let vote = PositiveCertificationVoteV2 {
            bond: NativeOutpoint { transaction: NativeHash64(signer.0.transaction_id.as_bytes()), index: signer.0.index },
            key: *key,
            statement: *statement,
            signature: *signature,
            origin,
            liability_amount,
        };
        row.votes.push(vote);
        let checkpoint = self.try_checkpoint()?;
        let result = (|| {
            write(self, row)?;
            self.note_kernel_bond(signer)?;
            Ok(())
        })();
        if result.is_err() {
            self.restore(checkpoint)
        }
        result
    }
    /// The actual Finish receipt and checked terminal come from a fresh concrete
    /// native session. Saved Passed/Matched, DTO flags and local replay cannot
    /// call this hook. It can pin Final before sufficient P votes exist.
    pub(super) fn advance_positive_certification_session_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        session: &NativeAssignedCaseSessionV2,
    ) -> Result<(), PalwStateV2Error> {
        session.ensure_current(&self.state, ctx)?;
        let subject = certification_subject(session.result());
        let mut owned = read(&self.state, subject, 0)?;
        let row = &mut owned.row;
        check_session(self, ctx, row, session)?;
        let fact = session.checked_terminal().ok_or_else(|| refuse("native checked Court terminal missing"))?;
        let origin = session.positive_final_origin().ok_or_else(|| refuse("actual Finish origin missing"))?;
        let finality = PositiveCertificationFinalV2 { origin, witness: fact.witness(), material: session.court_material() };
        if row.fixed.finality.is_some_and(|old| old != finality) {
            return Err(refuse("P Final/material cannot be rebound"));
        }
        row.fixed.finality = Some(finality);
        if row.fixed.outcome.is_none()
            && row.positive_positions() >= u32::from(row.fixed.header.policy.k)
            && !(row.fixed.header.policy.veto > 0 && row.negative_positions() >= u32::from(row.fixed.header.policy.veto))
        {
            let prosecution = origin
                .accepted_daa
                .checked_add(row.fixed.header.policy.prosecution_window_daa)
                .ok_or_else(|| refuse("Final prosecution overflow"))?;
            let case = PositiveRowsV2::new(&self.state).bundle(row.fixed.header.claim).map_err(refuse)?.case;
            if ctx.daa_score >= prosecution && ctx.daa_score >= case.maturity_daa {
                row.fixed.outcome = Some((PositiveCertificationOutcomeV2::Certified, ctx.daa_score));
            }
        }
        write(self, row)
    }
    /// Failure decisions require no fake complete Session. Never converts
    /// k-positive pending-fraud/unknown-Final into NotCertified after a TTL.
    pub(super) fn fail_positive_certification_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        subject: Digest,
    ) -> Result<(), PalwStateV2Error> {
        let mut owned = read(&self.state, subject, 0)?;
        let row = &mut owned.row;
        let live = case_live(&self.state, &row.fixed.header)?;
        let failure = positive_certification_failure_v2(row, ctx.daa_score, live)
            .ok_or_else(|| refuse("no P failure transition at this clock"))?;
        row.fixed.outcome = Some((failure, ctx.daa_score));
        write(self, row)
    }
}
#[derive(Default)]
pub(super) struct PositiveCertificationSweepReportV2 {
    pub draws: u64,
    pub failures: u64,
    pub deferred_queue_or_seed: u64,
    pub waiting_native_session: u64,
}
impl TransitionBuilder<'_> {
    /// Run after the legacy sweep so both use the SAME actual N claim-unit
    /// owner. A full queue/missinglaterseed defers; after certify_by the original
    /// frozen clock expires. No fallback policy or synthetic seed is supplied.
    pub(super) fn sweep_positive_certification_v2<'a>(
        &mut self,
        ctx: &PalwBlockContextV2,
        mut original_reservation: impl FnMut(AssignmentId) -> Result<(&'a AssignmentAdmissionMaterialV2, &'a [u8]), PalwStateV2Error>,
    ) -> Result<PositiveCertificationSweepReportV2, PalwStateV2Error> {
        let count = self.state.kernel_route.as_ref().map_or(0, |route| {
            route.aux.range((136, Vec::new())..(137, Vec::new())).filter(|((_, key), _)| key.first() == Some(&b'P')).count()
        });
        // Existing chains with no P rows perform no P allocation and do not
        // require a resource owner merely to preserve their legacy sweep.
        if count == 0 {
            return Ok(PositiveCertificationSweepReportV2::default());
        }
        let mut q = TirNativeRequestsV1::new();
        q.vec_v1::<Digest>(count as u64).map_err(PalwStateV2Error::NativeResource)?;
        let _keys_owner = reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?.ok_or_else(|| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Vec<Digest>>("P sweep keys", "actual checker not armed"))
        })?;
        let mut subjects = Vec::new();
        subjects.try_reserve_exact(count).map_err(|_| {
            PalwStateV2Error::NativeResource(NativeCoverageV1::unsupported::<Vec<Digest>>("P sweep keys", "allocator refusal"))
        })?;
        if let Some(route) = self.state.kernel_route.as_ref() {
            for ((_, key), _) in route.aux.range((136, Vec::new())..(137, Vec::new())) {
                if key.first() != Some(&b'P') {
                    continue;
                }
                if key.len() != 65 {
                    return Err(refuse("malformed P key"));
                }
                subjects.push(Digest(key[1..].try_into().map_err(refuse)?));
            }
        }
        let mut report = PositiveCertificationSweepReportV2::default();
        for subject in subjects {
            let owned = read(&self.state, subject, 0)?;
            let row = &owned.row;
            if positive_certification_failure_v2(row, ctx.daa_score, case_live(&self.state, &row.fixed.header)?).is_some() {
                drop(owned);
                self.fail_positive_certification_v2(ctx, subject)?;
                report.failures += 1;
                continue;
            }
            if row.fixed.outcome.is_some() {
                continue;
            }
            if row.draw.is_some() {
                report.waiting_native_session += 1;
                continue;
            }
            let h = &row.fixed.header;
            let earliest = h.committed_span.unwrap_or(0).checked_add(h.policy.anchor_delay_spans.max(1));
            let later = self.state.round_seed_anchor.is_some_and(|a| earliest.is_some_and(|s| a.span >= s));
            let policy_ok = queue_policy(self, h, ctx.daa_score).is_ok();
            let network_key = certification_network_key_v1();
            let network: PalwCertificationNetworkRowV1 =
                match self.state.kernel_route.as_ref().and_then(|r| r.aux.get(&(136, network_key))) {
                    None => PalwCertificationNetworkRowV1::default(),
                    Some(bytes) => borsh::from_slice(bytes).map_err(refuse)?,
                };
            let queue_free = network.epoch != ctx.daa_score / h.policy.epoch_daa || network.draws < h.policy.draws_per_epoch;
            if !later || !policy_ok || !queue_free {
                report.deferred_queue_or_seed += 1;
                continue;
            }
            let (material, reservation_preimage) = original_reservation(h.assignment)?;
            drop(owned);
            self.draw_positive_certification_v2(ctx, subject, material, reservation_preimage)?;
            report.draws += 1;
        }
        Ok(report)
    }
    /// Release only the P attestors' computation-liability slice after an actual
    /// fully replayed closed native boundary. This does not release source/join
    /// capital or service dues. A Refuted/Revoked claim needs a separate actual
    /// native deterministic charge owner; it NEVER falls through to release.
    pub(super) fn release_positive_certification_liability_v2(
        &mut self,
        ctx: &PalwBlockContextV2,
        session: &NativeAssignedCaseSessionV2,
    ) -> Result<(), PalwStateV2Error> {
        session.ensure_current(&self.state, ctx)?;
        let closed = session.pending_fraud_bound();
        if closed.has_pending() || ctx.daa_score <= closed.latest_decision_daa() {
            return Err(refuse("actual P liability/fraud boundary remains live"));
        }
        let subject = certification_subject(session.result());
        let mut owned = read(&self.state, subject, 0)?;
        let row = &mut owned.row;
        if session.assignment() != row.fixed.header.assignment || session.result().id() != row.fixed.header.result {
            return Err(refuse("foreign native closure session"));
        }
        let bundle = PositiveRowsV2::new(&self.state).bundle(row.fixed.header.claim).map_err(refuse)?;
        if ctx.daa_score < bundle.case.maturity_daa || bundle.case.unresolved_fraud != 0 {
            return Err(refuse("actual native Case remains liable"));
        }
        if matches!(bundle.case.status, ClaimStatus::ClaimRefuted | ClaimStatus::Revoked) {
            return Err(unavailable("actual P attestor deterministic charge owner missing; no release"));
        }
        if !matches!(bundle.case.status, ClaimStatus::PositivePassed | ClaimStatus::EndedUnverified) {
            return Err(refuse("native Case is not terminal"));
        }
        if row.fixed.liability_release.is_some() {
            return Err(refuse("P liability already closed"));
        }
        row.fixed.liability_release = Some(PositiveCertificationLiabilityReleaseV2 {
            point: PositiveCertificationDrawOriginV2 {
                block: NativeHash64(ctx.block.as_bytes()),
                daa: ctx.daa_score,
                blue_score: ctx.blue_score,
            },
            closed_material: session.court_material(),
            latest_decision_daa: closed.latest_decision_daa(),
        });
        write(self, row)
    }
}

fn check_session(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    row: &PositiveCertificationRowV2,
    session: &NativeAssignedCaseSessionV2,
) -> Result<(), PalwStateV2Error> {
    session.ensure_current(&builder.state, ctx)?;
    let h = &row.fixed.header;
    if &h.policy != session.certification_policy() {
        return Err(refuse("P policy differs from original authenticated native admission"));
    }
    row.authenticate_votes(session.result()).map_err(refuse)?;
    if session.assignment() != h.assignment
        || session.result().id() != h.result
        || session.result().case_id() != h.case
        || session.result().scope != h.scope
    {
        return Err(refuse("foreign native checked session"));
    }
    let fact = session.checked_terminal().ok_or_else(|| refuse("native full checked terminal missing"))?;
    if fact.terminal_kind() != TerminalKindV2::PositivePassed
        || fact.terminal().outcome() != CheckOutcome::Matched
        || fact.result() != h.result
        || fact.case() != h.case
        || fact.scope() != h.scope
    {
        return Err(refuse("native full Court did not pass"));
    }
    let bundle = PositiveRowsV2::new(&builder.state).bundle(h.claim).map_err(refuse)?;
    if bundle.case.positive_witness != Some(fact.witness())
        || bundle.case.coverage != fact.terminal().descriptor()
        || bundle.case.witness_status != WitnessStatus::Complete
        || bundle.case.status != ClaimStatus::PositivePassed
    {
        return Err(refuse("current exact Case/coverage differs from checked session"));
    }
    // Calling this getter requires a factory-authenticated closed initial
    // topology and actual pending state, not unresolved_fraud==0 as a shortcut.
    let closed = session.pending_fraud_bound();
    if closed.has_pending() || bundle.case.unresolved_fraud != 0 {
        return Err(refuse("native case has pending fraud"));
    }
    Ok(())
}

/// Purpose-specific opaque approval, never Reward/issuance/DAG/receipt credit.
/// Native money owner still verifies actual A coin/debit/payee/amount/once.
pub(super) struct PositiveCertifiedWorkerPaymentV2 {
    assignment: AssignmentId,
    result: ResultId,
    case: CaseId,
    worker: NativeOutpoint,
    amount: u64,
    subject: Digest,
}
impl PositiveCertifiedWorkerPaymentV2 {
    pub(super) fn assignment(&self) -> AssignmentId {
        self.assignment
    }
    pub(super) fn result(&self) -> ResultId {
        self.result
    }
    pub(super) fn case(&self) -> CaseId {
        self.case
    }
    pub(super) fn worker(&self) -> NativeOutpoint {
        self.worker
    }
    pub(super) fn amount(&self) -> u64 {
        self.amount
    }
    pub(super) fn subject(&self) -> Digest {
        self.subject
    }
}
pub(super) fn approve_positive_worker_payment_v2(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    session: &NativeAssignedCaseSessionV2,
) -> Result<PositiveCertifiedWorkerPaymentV2, PalwStateV2Error> {
    let owned = read(&builder.state, certification_subject(session.result()), 0)?;
    let row = &owned.row;
    check_session(builder, ctx, row, session)?;
    if !row.fixed.outcome.is_some_and(|(o, _)| o == PositiveCertificationOutcomeV2::Certified) {
        return Err(refuse("actual P outcome is not Certified"));
    }
    if ctx.daa_score <= session.pending_fraud_bound().latest_decision_daa() {
        return Err(refuse("actual native liability/fraud bound is not closed"));
    }
    let expected_final = session.positive_final_origin().ok_or_else(|| refuse("actual native Finish missing"))?;
    let fact = session.checked_terminal().ok_or_else(|| refuse("full checked session missing"))?;
    if row.fixed.finality
        != Some(PositiveCertificationFinalV2 { origin: expected_final, witness: fact.witness(), material: session.court_material() })
    {
        return Err(refuse("P Final/material differs from current session"));
    }
    let bundle = PositiveRowsV2::new(&builder.state).bundle(row.fixed.header.claim).map_err(refuse)?;
    let prosecution = expected_final
        .accepted_daa
        .checked_add(row.fixed.header.policy.prosecution_window_daa)
        .ok_or_else(|| refuse("prosecution overflow"))?;
    if ctx.daa_score < bundle.case.maturity_daa || ctx.daa_score < prosecution {
        return Err(refuse("current worker payment is immature"));
    }
    let a = assignment(&builder.state, row.fixed.header.assignment)?;
    if a.status != (AssignmentStatusV2::Consumed { case: row.fixed.header.case, result: row.fixed.header.result }) {
        return Err(refuse("Assignment one-use target changed"));
    }
    Ok(PositiveCertifiedWorkerPaymentV2 {
        assignment: a.id(),
        result: row.fixed.header.result,
        case: row.fixed.header.case,
        worker: a.bundle.terms.worker,
        amount: a.bundle.terms.worker_reward,
        subject: row.fixed.header.subject,
    })
}

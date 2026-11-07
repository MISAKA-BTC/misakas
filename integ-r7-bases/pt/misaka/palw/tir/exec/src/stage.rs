//! **A pipeline stage on the executor** (RFC-0003 §I.2.3's pipelines, RFC-0004 §7.2's evaluation): the
//! positions of a stage whose program is a held class's own — an evaluation's subject stage, the class's
//! version-1 program lifted unchanged with no input — computed by the typed executor over the weights the
//! node holds (a residency's rows where the artifact is resident), for the reference pipeline runner to
//! build the stage's run, its leaves and every stage after it from
//! (`misaka_palw_tir::pipeline::{PipelineParams::stepper, StageStepperV1}`). The weightless stages of a
//! pipeline — the scoring library's — stay on the reference interpreter.
//!
//! **The reference's stage, byte for byte.** A stepper exists only for a stage whose program
//! (`TirProgramV2`) reads no input and outputs logits, and whose version-1 view is exactly the program
//! the executor was compiled from ([`TirStageStepperV1::serves`]). There the reference's
//! `InterpreterV2::step` is the version-1 interpreter over that very program — no input to fetch, no
//! `post` write to apply, no commit point filtered — which the executor equals at every node, commit and
//! error class (this crate's differential gates). What a position returns is what the reference records:
//! the logits node's value as the output, the commit points in slot order, and the `Fixed` states after
//! the position — every instance some `StateWrite` of the schedule writes, and no other: a full step runs
//! every occurrence and every node, so from the first position on the reference holds exactly those
//! (`tests/stage.rs` holds the two equal over random programs and the lowered fixtures).
//!
//! **Candidates of one parent, in lockstep** ([`TirLockstepHubV1`]): each member's pipeline runs on its
//! own thread exactly as it would alone, its subject stage's stepper a [`TirLockstepSeatV1`] that hands
//! the hub its token and waits. When every member still in its stage has asked, the hub steps them all one
//! position in lockstep ([`crate::lockstep`]: every member's layer `L` before any member's `L + 1`, so a
//! layer's routed rows are admitted once for the batch) and answers each. A member whose stage ends — its
//! generation stopped, its pipeline failed — leaves, and the rest go on without it. The hub owns the
//! executors and steps them on its own thread; a member's thread only carries tokens and answers.

use std::collections::BTreeMap;
use std::sync::mpsc;

use misaka_palw_tir::interp::{CommitRecord, StateKey};
use misaka_palw_tir::interp_v2::StepOutputV2;
use misaka_palw_tir::pipeline::{StageStepV1, StageStepperV1};
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::program_v2::{OutputDecl, TirProgramV2};
use misaka_palw_tir::{DType, Prim, Tensor, TirError, TirErrorKind, TirResult};

use crate::exec::{NodeValue, StepSink, TirExecutor};
use crate::params::TirParams;
use crate::plan::TirPlan;

/// Collects a step's commit points as the reference records them.
#[derive(Default)]
struct CommitSinkV1 {
    records: Vec<CommitRecord>,
}

impl StepSink for CommitSinkV1 {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.records.push(CommitRecord { slot: v.slot, block: v.block, layer: v.layer, node: v.node, value: v.to_tensor() });
        }
    }
}

/// A `Fixed` instance the schedule writes: its key as the reference holds it, its dtype and shape.
#[derive(Clone, Debug)]
struct WrittenV1 {
    key: StateKey,
    dtype: DType,
    shape: Vec<usize>,
}

/// **One stage's positions on one executor.**
pub struct TirStageStepperV1<'a> {
    exec: TirExecutor<'a>,
    /// Every `Fixed` instance some `StateWrite` of the schedule writes, in key order.
    written: Vec<WrittenV1>,
    logits_dtype: DType,
}

impl<'a> TirStageStepperV1<'a> {
    /// **Does an executor of `plan` compute the stage whose program is `decl`?** Exactly when `decl`
    /// reads no input, outputs logits, and its version-1 view is `plan`'s program — the class's program
    /// lifted unchanged (`palw_improve_subject_program_v1`). Any other stage is the reference's to run.
    pub fn serves(plan: &TirPlan, decl: &TirProgramV2) -> bool {
        decl.inputs.is_empty() && matches!(decl.output, OutputDecl::Logits { .. }) && decl.v1_view() == plan.program
    }

    /// **A stepper for the stage whose program is `decl`**, over a fresh executor of `plan` and `params`
    /// — `None` when the executor does not compute that stage ([`Self::serves`]) or refuses the params
    /// (the reference then refuses them too, and says how: the caller runs the reference).
    pub fn for_stage(plan: &'a TirPlan, params: &'a TirParams<'a>, decl: &TirProgramV2) -> Option<Self> {
        if !Self::serves(plan, decl) {
            return None;
        }
        TirExecutor::new(plan, params).ok().map(Self::of_executor)
    }

    /// A stepper over `exec`, which must be at position 0 with the initial state and of the stage's
    /// program ([`Self::serves`]).
    pub fn of_executor(exec: TirExecutor<'a>) -> Self {
        let p = &exec.plan().program;
        let mut written: BTreeMap<StateKey, WrittenV1> = BTreeMap::new();
        for &(block, layer) in &exec.plan().occurrences {
            for n in &p.blocks[block as usize].nodes {
                if let Prim::StateWrite { state } = n.prim {
                    let s = &p.states[state as usize];
                    if matches!(s.kind, StateKind::Fixed { .. }) {
                        let shape = s.shape.iter().map(|d| *d as usize).collect();
                        written.insert((state, layer), WrittenV1 { key: (state, layer), dtype: s.dtype, shape });
                    }
                }
            }
        }
        let logits_dtype = p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.dtype;
        Self { exec, written: written.into_values().collect(), logits_dtype }
    }

    pub fn executor(&self) -> &TirExecutor<'a> {
        &self.exec
    }

    pub fn executor_mut(&mut self) -> &mut TirExecutor<'a> {
        &mut self.exec
    }

    /// The position just stepped, as the reference records it: its logits as the output, `commits`, and
    /// the `Fixed` states after it.
    fn record(&self, pos: u32, commits: Vec<CommitRecord>) -> TirResult<StageStepV1> {
        let (shape, data) = self.exec.logits();
        let output = Tensor { dtype: self.logits_dtype, shape: shape.to_vec(), data: data.to_i128s() };
        let mut fixed = BTreeMap::new();
        for w in &self.written {
            let value = self.exec.fixed_value(w.key.0, w.key.1).ok_or_else(|| {
                TirError::new(TirErrorKind::Missing, format!("state {} at layer {:?} has no Fixed instance", w.key.0, w.key.1))
            })?;
            fixed.insert(w.key, Tensor { dtype: w.dtype, shape: w.shape.clone(), data: value.to_i128s() });
        }
        Ok((StepOutputV2 { pos, output, commits }, fixed))
    }
}

impl StageStepperV1 for TirStageStepperV1<'_> {
    fn step(&mut self, token: u32) -> TirResult<StageStepV1> {
        let pos = self.exec.pos();
        let mut sink = CommitSinkV1::default();
        self.exec.step(token, &mut sink)?;
        self.record(pos, sink.records)
    }
}

// ---------------------------------------------------------------------------------------------
// Several members, one hub
// ---------------------------------------------------------------------------------------------

enum HubMsgV1 {
    Step { member: usize, token: u32 },
    Leave { member: usize },
}

/// **A member's stepper in a lockstep batch**: it hands its token to the hub and waits for the
/// position; dropped, it leaves the batch (its stage is over), so the others no longer wait for it.
pub struct TirLockstepSeatV1 {
    member: usize,
    tx: mpsc::Sender<HubMsgV1>,
    rx: mpsc::Receiver<TirResult<StageStepV1>>,
}

impl TirLockstepSeatV1 {
    pub fn member(&self) -> usize {
        self.member
    }
}

fn hub_gone() -> TirError {
    TirError::new(TirErrorKind::Missing, "the lockstep hub stopped before answering")
}

impl StageStepperV1 for TirLockstepSeatV1 {
    fn step(&mut self, token: u32) -> TirResult<StageStepV1> {
        self.tx.send(HubMsgV1::Step { member: self.member, token }).map_err(|_| hub_gone())?;
        self.rx.recv().map_err(|_| hub_gone())?
    }
}

impl Drop for TirLockstepSeatV1 {
    fn drop(&mut self) {
        let _ = self.tx.send(HubMsgV1::Leave { member: self.member });
    }
}

/// What a hub served: rounds (positions stepped in lockstep) and member positions (one per member a
/// round stepped) — `member_steps / rounds` is the batch's mean width.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TirLockstepServedV1 {
    pub rounds: u64,
    pub member_steps: u64,
}

/// **The hub of a lockstep batch**: the members' steppers, stepped together on the hub's thread.
pub struct TirLockstepHubV1<'a> {
    members: Vec<TirStageStepperV1<'a>>,
    rx: mpsc::Receiver<HubMsgV1>,
    replies: Vec<mpsc::Sender<TirResult<StageStepV1>>>,
}

impl<'a> TirLockstepHubV1<'a> {
    /// A hub over `members` and one seat per member, in order — refused unless every member runs the
    /// same number of occurrences a position (a parent and its composite candidates share the parent's
    /// schedule: one lockstep schedule).
    pub fn new(members: Vec<TirStageStepperV1<'a>>) -> Result<(Self, Vec<TirLockstepSeatV1>), String> {
        let Some(first) = members.first() else { return Err("a lockstep batch of no member".into()) };
        let n = first.exec.plan().occurrences.len();
        if let Some(i) = members.iter().position(|m| m.exec.plan().occurrences.len() != n) {
            return Err(format!(
                "member {i} runs {} occurrences a position and member 0 runs {n}: a lockstep batch is one schedule",
                members[i].exec.plan().occurrences.len()
            ));
        }
        let (tx, rx) = mpsc::channel();
        let mut replies = Vec::with_capacity(members.len());
        let mut seats = Vec::with_capacity(members.len());
        for member in 0..members.len() {
            let (reply, answers) = mpsc::channel();
            replies.push(reply);
            seats.push(TirLockstepSeatV1 { member, tx: tx.clone(), rx: answers });
        }
        Ok((Self { members, rx, replies }, seats))
    }

    /// **Serve the seats until every one has left**: whenever every member still in its stage has
    /// asked for its next position, step them all one position in lockstep and answer each. Returns
    /// when every seat is dropped.
    pub fn serve(mut self) -> TirLockstepServedV1 {
        let n = self.members.len();
        let mut inside = vec![true; n];
        let mut remaining = n;
        let mut asked: Vec<Option<u32>> = vec![None; n];
        let mut served = TirLockstepServedV1::default();
        while remaining > 0 {
            if asked.iter().flatten().count() == remaining {
                self.round(&mut asked, &mut served);
                continue;
            }
            match self.rx.recv() {
                Ok(HubMsgV1::Step { member, token }) => {
                    if inside[member] && asked[member].is_none() {
                        asked[member] = Some(token);
                    } else {
                        let _ = self.replies[member].send(Err(TirError::new(
                            TirErrorKind::Malformed,
                            format!("member {member} asked for a position after leaving, or twice in one round"),
                        )));
                    }
                }
                Ok(HubMsgV1::Leave { member }) => {
                    if std::mem::replace(&mut inside[member], false) {
                        remaining -= 1;
                        asked[member] = None;
                    }
                }
                // Every seat is gone (each said it was leaving first).
                Err(_) => break,
            }
        }
        served
    }

    /// One position for every member that asked, in lockstep, each answered.
    fn round(&mut self, asked: &mut [Option<u32>], served: &mut TirLockstepServedV1) {
        let ids: Vec<usize> = asked.iter().enumerate().filter_map(|(i, a)| a.map(|_| i)).collect();
        let tokens: Vec<u32> = ids.iter().map(|i| asked[*i].expect("asked")).collect();
        let positions: Vec<u32> = ids.iter().map(|i| self.members[*i].exec.pos()).collect();
        let mut sinks: Vec<CommitSinkV1> = ids.iter().map(|_| CommitSinkV1::default()).collect();
        let results = {
            let mut execs: Vec<&mut TirExecutor<'a>> =
                self.members.iter_mut().enumerate().filter(|(i, _)| asked[*i].is_some()).map(|(_, m)| &mut m.exec).collect();
            let mut sink_refs: Vec<&mut dyn StepSink> = sinks.iter_mut().map(|s| s as &mut dyn StepSink).collect();
            crate::lockstep::tir_lockstep_step_v1(&mut execs, &tokens, &mut sink_refs, true)
        };
        for (k, (i, r)) in ids.iter().zip(results).enumerate() {
            let answer = r.and_then(|()| self.members[*i].record(positions[k], std::mem::take(&mut sinks[k].records)));
            // A member whose thread is gone has nobody to answer; its seat's drop says it left.
            let _ = self.replies[*i].send(answer);
            asked[*i] = None;
        }
        served.rounds += 1;
        served.member_steps += ids.len() as u64;
    }
}

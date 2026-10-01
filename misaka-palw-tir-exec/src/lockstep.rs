//! **Executors stepped together, weight-stationary** — RFC-0004's candidate evaluation over one
//! parent (`docs/design/palw/tir/runtime-residency.md` §8): several composite candidates of the same
//! parent, run on the same inputs, a position at a time and an OCCURRENCE at a time — every member's
//! layer `L` before any member's layer `L + 1` — so the parent's weights a layer reads serve the
//! whole batch while they are at hand: its pinned weights once through the CPU's caches instead of
//! once per candidate, and, over a shared residency ([`crate::rows`], the node's store: one per parent
//! root), its routed rows admitted once for the batch — the first member's admission reads them, the
//! rest find them held. Activations, states and a candidate's adapter stay each member's own.
//!
//! **Each member computes exactly what it computes alone.** A member's step is its own executor's
//! [`TirExecutor::step_opt`], cut at occurrence boundaries — its inputs checked and set first, its
//! effects applied or dropped at the end, exactly as alone — and nothing a member computes reads
//! another member's state; the store's state decides where a row's bytes come from, never which
//! bytes they are. (`tests/residency_node.rs` holds every member's commits and logits to the same
//! member run alone, and counts the reads the batch saves.)
//!
//! **How many members** ([`tir_lockstep_batch_v1`]): every member's admission of one layer must be
//! held at once for the next member to find it, so a batch holds at most the routed capacity over
//! the in-flight term (one admission) — and each member's own working memory (its states, its
//! histories, its node buffers) within what the host spares.

use crate::exec::{StepSink, TirExecutor};
use misaka_palw_tir::TirResult;

/// **A batch of executors stepped in lockstep.** Members are executors of programs with the same
/// number of block occurrences a position — a parent and its composite candidates have the parent's
/// schedule — over params that share one row source for the weight-stationary saving (correctness
/// does not depend on it).
pub struct TirLockstepV1<'a> {
    members: Vec<TirExecutor<'a>>,
}

impl<'a> TirLockstepV1<'a> {
    /// A batch of `members`, refused unless every member runs the same number of occurrences a
    /// position.
    pub fn new(members: Vec<TirExecutor<'a>>) -> Result<Self, String> {
        let Some(first) = members.first() else { return Err("a lockstep batch of no member".into()) };
        let n = first.plan().occurrences.len();
        if let Some(i) = members.iter().position(|m| m.plan().occurrences.len() != n) {
            return Err(format!(
                "member {i} runs {} occurrences a position and member 0 runs {n}: a lockstep batch is one schedule",
                members[i].plan().occurrences.len()
            ));
        }
        Ok(Self { members })
    }

    pub fn members(&self) -> &[TirExecutor<'a>] {
        &self.members
    }

    pub fn members_mut(&mut self) -> &mut [TirExecutor<'a>] {
        &mut self.members
    }

    pub fn into_members(self) -> Vec<TirExecutor<'a>> {
        self.members
    }

    /// **One position for every member**: `tokens[i]` into member `i`, whose committed values go to
    /// `sinks[i]`. Each member's result is the result its own [`TirExecutor::step`] would have had.
    pub fn step(&mut self, tokens: &[u32], sinks: &mut [&mut dyn StepSink]) -> Vec<TirResult<()>> {
        self.step_opt(tokens, sinks, true)
    }

    /// [`Self::step`], evaluating `post` only when `run_post` ([`TirExecutor::step_opt`]).
    pub fn step_opt(&mut self, tokens: &[u32], sinks: &mut [&mut dyn StepSink], run_post: bool) -> Vec<TirResult<()>> {
        let mut members: Vec<&mut TirExecutor<'a>> = self.members.iter_mut().collect();
        tir_lockstep_step_v1(&mut members, tokens, sinks, run_post)
    }
}

/// **One position for every member, in lockstep** — the body of [`TirLockstepV1::step_opt`], over
/// executors borrowed from wherever their owner keeps them (`crate::stage::TirLockstepHubV1` steps the
/// members still in their stage). `tokens[i]` into `members[i]`, its committed values to `sinks[i]`;
/// each result is the one the member's own [`TirExecutor::step_opt`] would have had. The members must
/// run one schedule (the same number of occurrences a position; [`TirLockstepV1::new`] checks it).
pub fn tir_lockstep_step_v1(
    members: &mut [&mut TirExecutor<'_>],
    tokens: &[u32],
    sinks: &mut [&mut dyn StepSink],
    run_post: bool,
) -> Vec<TirResult<()>> {
    assert_eq!(tokens.len(), members.len(), "a token per member");
    assert_eq!(sinks.len(), members.len(), "a sink per member");
    let begun: Vec<TirResult<usize>> = members.iter_mut().zip(tokens).map(|(m, t)| m.begin_step(*t, run_post)).collect();
    let mut status: Vec<Option<TirResult<()>>> = begun
        .iter()
        .map(|b| match b {
            Ok(_) => Some(Ok(())),
            Err(_) => None,
        })
        .collect();
    let occurrences = begun.iter().filter_map(|b| b.as_ref().ok().copied()).max().unwrap_or(0);
    for occ in 0..occurrences {
        for (i, member) in members.iter_mut().enumerate() {
            if let Some(Ok(())) = status[i] {
                status[i] = Some(member.run_occurrences(occ, occ + 1, &mut *sinks[i]));
            }
        }
    }
    members
        .iter_mut()
        .zip(status)
        .zip(begun)
        .map(|((member, status), begun)| match (status, begun) {
            (Some(r), Ok(_)) => member.end_step(r),
            (_, Err(e)) => Err(e),
            (None, Ok(_)) => unreachable!("a member that began has a status"),
        })
        .collect()
}

/// **How many members a lockstep batch over one residency may hold**: every member's admission of
/// one layer held at once — the routed capacity over the in-flight term (one admission, the most a
/// layer of one member reads) — and every member's own working memory (`member_bytes`) within
/// `spare`. At least one: a batch of one is a member run alone. A residency with no routed rows
/// (`in_flight == 0`) bounds the batch by memory alone.
pub fn tir_lockstep_batch_v1(routed_capacity: u64, in_flight: u64, member_bytes: u64, spare: u64) -> usize {
    let by_rows = if in_flight == 0 { usize::MAX } else { usize::try_from(routed_capacity / in_flight).unwrap_or(usize::MAX) };
    let by_memory = if member_bytes == 0 { usize::MAX } else { usize::try_from(spare / member_bytes).unwrap_or(usize::MAX) };
    by_rows.min(by_memory).max(1)
}

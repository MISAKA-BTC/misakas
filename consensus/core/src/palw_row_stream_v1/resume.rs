//! RFC-0037 §5.3 resume at a collection boundary. A resumed stream never trusts the stored spill: the spill prefix is
//! replayed through the same rooting rules, every completed collection is re-matched against the preimage, which is
//! itself re-matched against the header-derived root, and the byte budget is re-charged for the replayed rows.
//! Nothing authenticated is skipped, and the resumed total can never exceed what an uninterrupted stream would have
//! been allowed. A torn final collection (crash mid-write, `UnexpectedEof`) is dropped back to the last complete
//! boundary; any other damage — mismatch, duplicate label, malformed framing — refuses the whole spill. A local limit
//! or local I/O failure while reading the spill is a local refusal, not damage.
use super::{
    PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1, PalwCollectionRootStreamV1, PalwRowProjectionV1, PalwRowSpillEventV1, PalwRowSpillLimitsV1,
    PalwRowStreamErrorV1, PalwRowStreamVerifierV1, Rooter, is_non_standard, palw_visit_row_spill_v1,
};
use crate::Hash64;
use crate::palw_state_proof_v1::PalwCollectionOpeningV1;
use crate::palw_state_v2::palw_state_root_of_preimage_v1;
use std::collections::BTreeSet;
use std::io::{ErrorKind, Read, Write};

/// A verifier positioned at a collection boundary.
pub struct PalwRowStreamResumedV1<W: Write> {
    /// Ready for the next `begin_collection`. Its sink was opened at `boundary_bytes`.
    pub verifier: PalwRowStreamVerifierV1<W>,
    /// Length of the re-authenticated spill prefix. The caller truncates its spill file to this before appending.
    pub boundary_bytes: u64,
    pub collections_replayed: usize,
    /// True when an incomplete trailing collection was discarded.
    pub torn_tail: bool,
}

struct Pending {
    label: Vec<u8>,
    rooter: Rooter,
    project: Option<PalwRowProjectionV1>,
}

/// `open_sink(boundary_bytes)` must return a writer positioned at that length (truncating any torn tail).
/// `projection_of` must give the same projections the original stream used; a different one fails the root match.
pub fn palw_resume_row_stream_v1<W: Write>(
    expected_root: Hash64,
    preimage: Vec<u8>,
    spill: impl Read,
    limits: PalwRowSpillLimitsV1,
    max_row_bytes: u64,
    mut projection_of: impl FnMut(&[u8]) -> Option<PalwRowProjectionV1>,
    open_sink: impl FnOnce(u64) -> std::io::Result<W>,
) -> Result<PalwRowStreamResumedV1<W>, PalwRowStreamErrorV1> {
    if preimage.len() > PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 {
        return Err(PalwRowStreamErrorV1::PreimageTooLarge { len: preimage.len(), max: PALW_ROW_STREAM_MAX_PREIMAGE_BYTES_V1 });
    }
    if palw_state_root_of_preimage_v1(&preimage) != expected_root {
        return Err(PalwRowStreamErrorV1::PreimageDoesNotMatchRoot(expected_root));
    }
    let mut labels: BTreeSet<Vec<u8>> = BTreeSet::new();
    let mut used: Vec<usize> = Vec::new();
    let mut done: Vec<(Vec<u8>, u64)> = Vec::new();
    let mut committed_bytes = 0u64;
    let mut committed_row_bytes = 0u64;
    let mut pending_bytes = 0u64;
    let mut pending_row_bytes = 0u64;
    let mut cur: Option<Pending> = None;
    let mut fault: Option<PalwRowStreamErrorV1> = None;

    let outcome = palw_visit_row_spill_v1(spill, limits, |event| {
        let mut step = || -> Result<(), PalwRowStreamErrorV1> {
            match event {
                PalwRowSpillEventV1::Collection { label, rows } => {
                    if !labels.insert(label.to_vec()) {
                        return Err(PalwRowStreamErrorV1::DuplicateCollection(label.to_vec()));
                    }
                    pending_bytes = committed_bytes + 1 + 4 + label.len() as u64 + 8;
                    pending_row_bytes = committed_row_bytes;
                    let rooter = if is_non_standard(label) {
                        Rooter::Buffered { declared: rows, rows: Vec::new() }
                    } else {
                        Rooter::Standard(PalwCollectionRootStreamV1::new(label, rows))
                    };
                    cur = Some(Pending { label: label.to_vec(), rooter, project: projection_of(label) });
                }
                PalwRowSpillEventV1::Row { key, value } => {
                    let open = cur.as_mut().ok_or(PalwRowStreamErrorV1::NoOpenCollection)?;
                    let added = (key.len() + value.len()) as u64 + 9;
                    pending_row_bytes = pending_row_bytes.saturating_add(added);
                    if pending_row_bytes > max_row_bytes {
                        return Err(PalwRowStreamErrorV1::ByteBudget(max_row_bytes));
                    }
                    pending_bytes += 1 + 4 + key.len() as u64 + 4 + value.len() as u64;
                    let projected;
                    let rooted: &[u8] = match &open.project {
                        None => value,
                        Some(project) => {
                            projected = project(value).map_err(|why| PalwRowStreamErrorV1::Projection(open.label.clone(), why))?;
                            &projected
                        }
                    };
                    match &mut open.rooter {
                        Rooter::Standard(h) => h.row(key, rooted)?,
                        Rooter::Buffered { declared, rows } => {
                            if rows.len() as u64 == *declared {
                                return Err(PalwRowStreamErrorV1::TooManyRows { label: open.label.clone(), declared: *declared });
                            }
                            rows.push((key.to_vec(), rooted.to_vec()));
                        }
                    }
                }
                PalwRowSpillEventV1::End => {
                    let open = cur.take().ok_or(PalwRowStreamErrorV1::NoOpenCollection)?;
                    let (root, rows) = match open.rooter {
                        Rooter::Standard(h) => {
                            let rows = h.seen;
                            (h.finish()?, rows)
                        }
                        Rooter::Buffered { declared, rows } => {
                            if rows.len() as u64 != declared {
                                return Err(PalwRowStreamErrorV1::RowCountMismatch {
                                    label: open.label,
                                    declared,
                                    seen: rows.len() as u64,
                                });
                            }
                            let n = rows.len() as u64;
                            (PalwCollectionOpeningV1 { label: open.label.clone(), rows }.root(), n)
                        }
                    };
                    // The spill carries no offset (node-local framing); the committed preimage decides where this
                    // collection's root lives. A root that sits nowhere unused is not the committed collection.
                    let offset = if root == Hash64::default() {
                        None
                    } else {
                        preimage
                            .windows(64)
                            .enumerate()
                            .find(|(at, w)| *w == root.as_byte_slice() && !used.iter().any(|u| *at < u + 64 && *u < at + 64))
                            .map(|(at, _)| at)
                    };
                    let Some(offset) = offset else {
                        return Err(PalwRowStreamErrorV1::CollectionMismatch(open.label));
                    };
                    pending_bytes += 1;
                    used.push(offset);
                    done.push((open.label, rows));
                    committed_bytes = pending_bytes;
                    committed_row_bytes = pending_row_bytes;
                }
            }
            Ok(())
        };
        step().map_err(|e| {
            fault = Some(e);
            std::io::Error::other("row stream replay refused")
        })
    });

    let mut torn_tail = false;
    if let Err(error) = outcome {
        if let Some(fault) = fault {
            return Err(match fault {
                PalwRowStreamErrorV1::ByteBudget(max) => PalwRowStreamErrorV1::ByteBudget(max),
                other => PalwRowStreamErrorV1::ResumeSpill(other.to_string()),
            });
        }
        match error.kind() {
            // A crash mid-write leaves a prefix; everything after the last END is discarded and re-fetched.
            ErrorKind::UnexpectedEof => torn_tail = true,
            // Only malformed framing (`InvalidData`) is damage in the stored spill. Local limits (`Other`), allocation
            // refusal (`OutOfMemory`) and every local I/O failure (permission, timeout, device, ...) are this node's
            // condition, never evidence against the spill or a peer (ADR-0204 capacity/I/O is local).
            ErrorKind::InvalidData => return Err(PalwRowStreamErrorV1::ResumeSpill(error.to_string())),
            _ => return Err(PalwRowStreamErrorV1::ResumeLocalLimit(error.to_string())),
        }
    }
    if torn_tail {
        // Drop the incomplete collection's label so it can be streamed again.
        if let Some(open) = cur.take() {
            labels.remove(&open.label);
        }
    }
    let sink = open_sink(committed_bytes).map_err(|e| PalwRowStreamErrorV1::Sink(e.to_string()))?;
    let collections_replayed = done.len();
    Ok(PalwRowStreamResumedV1 {
        verifier: PalwRowStreamVerifierV1 {
            expected_root,
            preimage,
            sink,
            max_row_bytes,
            row_bytes: committed_row_bytes,
            open: None,
            done,
            labels,
            used,
            poisoned: false,
        },
        boundary_bytes: committed_bytes,
        collections_replayed,
        torn_tail,
    })
}

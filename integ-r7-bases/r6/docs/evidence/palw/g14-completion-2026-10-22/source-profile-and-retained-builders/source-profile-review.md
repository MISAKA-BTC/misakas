# Source-only normal-form profile independent read review

Reviewed candidate and actual `TirProgramV1::decode_canonical`, normal-form validator, StepSpace constructor and native Source call chain. No concrete coexisting heap underquote found by inspection. Unbuilt; no native execution or allocation instrumentation performed.

Source's two verify passes are sequential. Each normal-form decode runs Borsh, canonical reencode, validate; StepSpace then runs validate again after decode's temporary ProgramInfo has dropped. Neither path calls gate/cone/range/unrolled admission analysis. Keeping generic program_blob and numerical/parameter budgets unchanged while selecting this separate Source-only profile is justified by those call boundaries.

The new raw heap x2 covers parsed body plus still-live cloned constant payloads; parameter/state name trees and owned constant tuple tree are separately present. The declaration trees remain alive through block validation. Carry clones occur before node output shape validation and can duplicate the same oversized rank up to MAX_CARRY in retained pre carry and temporary layer output; 2*MAX_CARRY*n Dim envelope preserves that. Operands are allocated only after arity<=8; Ref::Node/CarryIn/Param/Const/State clones are within the eight rank-n envelope. Four inferred Vec envelopes at 2*n+1 conservatively cover actual Gather concatenation, overlapping Select broadcasts, HistAppend row/fixed/H and ordinary clone/MatMul growth; transpose bool scratch is separately counted. Per-block written/appended sets each insert only actual state IDs. Retained carry/info/three declaration trees coexist with these temporaries.

Liveness marks before expanding, so duplicate DFS stack entries do not duplicate expansion. Actual pushes are bounded by root nodes + <=MAX_CARRY carry roots + logits + actual Ref::Node edges, all included in nodes+n+9. Final used-map has only three category keys, inner sets plus one cloned set; the 2*n entry envelope retains the clone peak.

Actual error formatting contains bounded-name Display, scalar index/range diagnostics, Dim shape Debug and raw u8 transpose perm Debug. Current Dim grammar is Fixed(u32)/H: <=20 bytes/entry with separator; Gather may infer <=2*n before output equality, so output+inferred <=3*n. Four geometric buffers sized 1024+60*n are conservative for current nested want_shape/shape_err/TirError/StepError string ownership. This is grammar-specific and requires re-audit if error diagnostics change; not a generic formatting theorem or physical RSS proof.

Requested repair: the new source helper maps only io::ErrorKind::OutOfMemory to Resource, while new/old wire readers can also report TimedOut/Unsupported for local scanner inability. Preserve these as typed Resource in the new helper, without changing generic legacy mapper or string matching. Malformed/unknown version must remain Decode. D2 notified; review applies subject to this classification repair.

Tests use actual fixed/history execution plus real ProgramBuilder repeated-edge graph and honestly malformed valid-wire rank/carry cases. They verify accounted reservation lifecycle, not independent physical allocation peak. Source owner integration tests must still run. Reduced quote does not certify useful model capacity, fee/profile, complete original admission or door deadline.

## IO classification repair re-review

D2 revised the new helper mapper: OutOfMemory remains typed overflow Resource, TimedOut/Unsupported now typed unsupported Resource, malformed IO retains Decode. No string interpretation and no generic mapper change. The requested classification repair is satisfied by read review; no remaining concrete apply blocker found, still unbuilt.

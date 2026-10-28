# Independent read review — retained semantic material candidate

Reviewed `/tmp/g14-semantic-material-owner/module.rs`, `tests.rs`, and corresponding existing production primitives. Read-only review; no build or runtime verification and no repository modifications.

No concrete blocking authentication defect or coexisting heap underquote was found in the reviewed candidate.

- Current state context reconstructs the actual source and compares its source ID and header to the Store. Immutable semantics and rooted tokenizer EOF are checked against the original acceptance DAA, raw hash and declared byte count. Scope remains bound to source, claim, shared context, semantic snapshot and actual inventory identity.
- Rendering bodies must cover every vocabulary ID exactly once and in order. Existing rendering opening verification authenticates the table root and preserves `None` versus `Some(empty)`. Its leaf hash is streaming.
- EOG authentication uses the existing streaming court primitive, rather than `Eog::verifies`, which alone permits root-consistent out-of-range IDs. The native stream checks count, ordering, range, opening and complete digest. Empty relevant IDs mean no nonempty relevant/hits or finish-result allocation. It avoids the legacy EOG hash helper's copied/sorted ID vector.
- The source reconstruction peak is reserved before `c.source`; that source is dropped before decoded bodies are materialized. Native frame clone and hash-writer allowances remain leased. Per-body schema scans precede decode; outer rendering Vec, decoded payloads, copied raw bytes and fixed output/stream structure are covered by the grown retained lease.
- `sem.commitment()` uses a temporary Borsh Vec, and shared request scope hashing also allocates a small fixed Vec. For the current fixed snapshot and scope types, the retained frame writer allowance `2 * max(encoded_frame, 1024)` plus source construction peak did not expose a concrete underquote. If those structures gain variable bodies this assessment must be revisited; it is not a generic serialization bound proof.
- The result owns its copied raw bytes and decoded proof/body vectors. Borrowed accessors do not detach those owners from the ticket. This is authenticated data retention only, not tokenizer parser validity, remote availability, protocol retention fulfilment, funded admission or complete original-duty authority.

The local quota tests exercise quote/grow/drop and decode-owner transfer. The native fixture is still uncompiled; this review does not establish fixture validity, successful native admission, or all-grammar closure. Existing caller input-carrier ownership and downstream getter/terminal transient allocations remain separately admitted.

# Ordinary TFP1 Trace export

The remote adapter can open an admitted retained capture through
TirBackendV1::open_tir_public_capture_v1 and project its native StepLeaf
responses onto the existing public-replay-v1 provider paths. The same
CheckedManifest and exact signed companion accepted by the legacy replay
checker identify the Trace scope. The exporter requires an exact Source,
registered backend, and borrowed native paired-claim inputs.

- Each step holds the bounded capture owner, obtains a quoted native slice,
  delegates permit verification/pre-debit and per-unit RAM admission to
  the required owner interface, and polls once. Pending and error polls
  consume their admitted work. A StepLeaf response must authenticate
  against the original native Source before any provider write.
- Before opening the capture, the owner books the complete conservative
  retained Trace wire upper, including index and longest openings. The
  provider owner holds this reservation independently of the cursor.
  Every write has its own scoped admission for filesystem I/O, temporary
  storage and retention. Progress is advanced only after the file write.
  On an incomplete attempt, files may remain. Keep their reservation or
  clean them using the existing provider retention owner.
- The index is written after every leaf and exact manifest raw-byte totals.
  Completion means the Trace export completed. It does not publish the
  manifest or establish availability of its other scopes. Existing
  store_public_manifest_v1 validates the complete manifest bytes, and is
  called by the complete publisher only when all independently required
  bodies and the actual native provider retention are ready.
- The current daemon G14 work driver checks live debit receipts before
  native capture/response work. Its prepared-index writer uses dedicated
  phase 12 with zero request ID. That permission does not price or
  authorize this provider export. Connecting the trait to production
  needs the paired claimant/Source view, a current provider-export debit
  path with the new profile's actual bounds, and a retained provider owner
  for immutable partial and complete files. Existing gates are untouched.
- The current adapter consumes an already signed and manifest-bound
  original. To create that manifest on the real producer, the future
  ordinary profile needs independently obtained raw trace byte counts
  and exact limits. A caller may measure those from the retained original
  with the bounded reader before constructing the signed manifest; that
  prior pass must have separately reserved work, or the producer must
  capture sizes in its original retained execution. Copying them from
  the verifier or accepting provider-supplied lengths is not sufficient.
- The combined source also derives ordinary ExecutionRules and cold
  state/dependency material from the same native FP, Source, and class,
  then compares the manifest descriptors and provider bytes before replay.
  Remaining native relation semantics include complete public manifest
  publication, all required response paths, retention and protected-case
  accounting. The old stateless wide signature and
  plan scope are unchanged. No public transaction or model inference was
  used to test this publisher.

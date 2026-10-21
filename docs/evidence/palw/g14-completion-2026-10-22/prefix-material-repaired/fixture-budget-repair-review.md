# Independent read review — Prefix quote repair

Reviewed `prefix-quote-repair.patch` against the current Prefix builder and native fixture. Read-only; no build, execution or repository change.

No concrete blocking defect found. Production changes only widen two private module functions to `pub(crate)` so the actual fixture can calculate its local test capacity. Their formulas and the source/StepSpace construction quote remain unchanged. No protocol maximum, native rule, signature, deadline, verdict primitive or resource refusal is weakened.

The fixture now computes initial source/frame/context demand from the actual frame, registered record and layout, and replay demand from the actual canonical prefix context's leaf size. It combines these with `checked_add`, uses the total as the caller-local request budget, and creates the local checker with exactly that total instead of `u64::MAX`. The same initial reserve and subsequent growth are observed through the real production path. Checks at initial-minus-one and total-minus-one reject with Resource; exact total succeeds, retains the full ticket, and returns the same semantic verdict. All paths check release of outstanding demand after owner drop or refusal. Additional printed source/StepSpace metadata explains the large conservative quote.

This is an accounting-path test, not an independent proof that the quote measures physical peak memory accurately: capacity and expected counters intentionally use the same quote implementation. The still-conservative native StepSpace structural `n*(n+2)` allowance may exceed 4 GiB logically without that many physical bytes being allocated. Raising this fixture's local requested capacity to its actual computed quote does not raise a protocol cap or establish practical model feasibility.

The fixture constructs an additional prefix context before replay to obtain metadata. That test-owned object and the existing Source/Store/units remain outside the injected replay checker, just like caller-owned inputs. The checker assertions therefore describe replay-owned accounting only, not total process RSS. The production path continues to derive and reserve its own initial request before source reconstruction, and grow before lane/body replay allocations.

Native fixture execution and successful compilation remain unverified by this review; parent build 57343 is authoritative for those outcomes.

# RFC-0027 / Account Bond ADR-0193 import — 2026-10-20

User-requested document addition on `codex/rfc14-g14-completion`, after `925b8051c`. Proposed / Review Required remains unchanged. No consensus implementation, parameter adoption, arming, pre integration or deployment.

The [source package](source-package.zip) is retained byte-for-byte. Its 54 manifest entries verified before extraction. Main RFC/ADR and all `0027-assets` files were copied without edits. Shared indexes were regenerated locally, and the existing number registry was merged rather than overwritten. The package generator, shared indexes and patches were not applied over this branch. [Original package README](README_JA.md.txt) is kept as raw text so its package-root relative links are not misinterpreted from this archive directory. MANIFEST and PATCH_APPLY_CHECK describe the original package, not this branch's application.

Account Bond ADR-0193 has a different filename and scope from the filing-clock ADR-0193 already committed on the separate X3 branch. Both histories are preserved; use full paths/titles to distinguish them. This import does not silently supersede the clock proposal or claim its number was unused. Future allocation remains ADR-0194 / RFC-0028, subject to other branch reservations.

The source check found 28 of 29 local inputs matching. RFC-0026 differs on this work branch; see [local source check](local-source-check.json). This is not a fresh assertion that all 29 source hashes equal the uploaded baseline. Original logs, measurements and digests remain untouched.

Local reruns: analysis unit tests 32/32, accounting examples 16/16, reaggregation/conservation checks 1365/1365. [Local rerun outputs](local-validation/summary.json) and [comparison to supplied results](comparison.json) are separate from supplied evidence. Environment, runtime and source-manifest differences are expected and must not be described as new node or Panel measurements. The 100,000 participation floor is not demonstrated sufficient operating capital, 12,000 per claim remains illustrative, and hard deadline is not automatic release. Panel MSK prices are assumptions; heartbeat changes and new Account Bond node operation are unmeasured.

Panel sample JSON is not byte-identical: [field differences](panel-sample-differences.json) records all differences, including floating-point residual arithmetic. Do not claim all analysis outputs reproduce byte-for-byte. Source CPU log values and uploaded outputs were not rewritten.

Local docs check: FAIL on the same five imported RFC-0024/0025/0026 asset references, 43 known baseline occurrences; zero additional missing references from this import. Baseline remains unchanged.

Source ZIP SHA-256: `ec23d813ce1f6fcd8c5f7d78234f93ca48acb71df9306d8317711d10a96c0eb0`.

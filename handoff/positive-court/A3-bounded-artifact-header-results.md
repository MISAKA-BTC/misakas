# A3 — bounded PALWTIR1 metadata validation

Exact source **`e934dcc09393f77e0c047ca93e292d1465bd7800`**, tree
`2297fd0067ca4d8bc9c9c0e1a640a56611a681cd`, branch
`codex/a3-wide-artifact-startup-v2`. New executable source is `e49e5628a`;
`9df57dea8` clarifies logical program ownership versus a separate startup host
reservation. Hello2 test correction `61d243bca` is the local alias of `c99125eb8`.

**Five selected tests passed, zero failures** on the first frozen-source run.
Artifact library strict Clippy passed with zero warnings. Documentation check:
43 known missing references, zero new issues. The other12 artifact tests were
filtered out, not counted as executed successes.

| New metadata test scope | Passed |
|---|---:|
| Exact native/copy/operation backing and one below | 1 |
| Hostile field/prefix lengths and truncated headers | 1 |
| Empty table, valid original program borrow and changed-program refusal | 1 |
| Handwritten nonempty table,11 table/extent attacks and canonical-decode negative | 1 |
| Existing layout words retained without generic nested decoding | 1 |

[Commands, source/file hashes, before/after binary information and raw logs](A3-r1-evidence/bounded-artifact-header-e934dcc09/validation.json)
pin this run. The single actual test binary is SHA256
`a9260624f2490d071c997da4f035b5f518f632dc8bff4d4291e3cf09f1ade94f`.
The existing A3 target was reused with jobs1/debug0/incremental0, unset
`RUST_MIN_STACK`, offline/locked dependencies and no new cache or cleanup.
This artifact leaf package has no Core/RocksDB dependency and triggered no C++
build. The existing static archive was supplied/recorded for build policy;
recording it does not mean this binary links it. Source and archive hashes were
stable across checks. A runner directory-label error was corrected after the
run by renaming its enclosing evidence directory; raw log bytes are unchanged.

The original helper fixtures' invalid history bound/empty pre and module doc
placement were corrected during source review before this frozen run. No hidden
preflight failure is counted as success. Root's earlier Hello2 failure and
corrected integration batch remain separate records, with separate source and
counts. Neither the previous18/49 nor Root's30 is added to these five.

The parser uses the existing PALWTIR1 field order, borrowed existing layout
words and literal values. It checks nested native allocations before reserve,
then binds table/program bytes against a caller-owned, independently canonical
program without another generic decode. Existing default loaders and formats
are unchanged. [The contract and remaining owner dependencies](A3-bounded-artifact-header-v1.md)
specify the separate caller/source reservations.

This is metadata/data binding only. Tensor loading, whole-startup plan/params
admission, streamed artifact-root authentication, source mutation checks,
bounded V2-space construction and generic process/SDK/native integration remain
required. The separate child-process patch is still a draft. These five tests
do not establish a trained-model startup, RSS bound, native role, Court result,
Gate C, SecurityCertified or computation credit.

# Tokenizer operand length and finite court resources (2026-10-11)

Source checkpoints: `ad73e4bcd` (rooted typed tokenizer court), `fc93f3e29`
(actual operand length authenticated before declaration fault). Source parsing and
`git diff --check` passed. These checkpoints have not been compiled or executed
in the terminal worktree; integrated verification is coordinated by the parent.
This is not a G14 completion or activation approval.

## Registered-model premise and existing authentication

ADR-0177 D7 remains the premise: the outside verifier already holds the correct
registered model/tokenizer/config. Raw tokenizer bytes are a verifier-supplied
operand. This change does not impose producer distribution or a raw-tokenizer DA
request/default obligation.

The existing raw tokenizer identifier authenticates `le64(actual_length) || bytes`
under keyed BLAKE2b-512. The code is
`misaka-palw-base0/src/artifact.rs::Base0ArtifactV1::tokenizer_commitment_of` and
`misaka-palw-tir-lower/src/artifact.rs::tokenizer_id_of`.
Neither identifier exposes an independent length opening. The artifact inventory
in `consensus/core/src/palw_artifact.rs` commits tensor rows and their lengths;
`PalwArtifactInventorySummaryV1::artifact_bytes` sums those tensor rows. TIR/Gen
inventory declarations also describe program parameters, not the external raw
JSON tokenizer. `PalwModelManifestV1::artifact_bytes` is not a separately authenticated
raw-tokenizer length. SDK `preflight/model.rs::tokenizer_bytes` is a local source-file
aggregate, not a consensus commitment. No independent authenticated raw length
was found in these existing paths.

## Implemented exact EOF rule

The signed staged seed separately carries immutable descriptor `raw_tokenizer_bytes`
(L), immutable `tokenizer_key_events` (B), and verifier operand
`operand_tokenizer_bytes` (actual candidate L'). L' is not accepted as truth.
The parser hashes `le64(L') || every supplied byte` and refuses a verdict until
that hash equals the registered legacy tokenizer ID at exact EOF. The original
table leaf also must authenticate at EOF. Only then can L' != L convict a false
length declaration. Understated B still records the actual B+1 grammar-derived
key event, then hashes the complete actual L' operand before exposing overrun.
Wrong or truncated operands never become offence credentials.

Rooted owner rows, journal delta211, snapshots and certificate19 now support this
typed court. Certificate19 contains only literal claim/reserver/reservation/seed
identity and resolves a completed rooted court. Tokenizer faults use claim-wide
producer liability and AnyValid Panel attribution, as finite contradiction18 does.
The prospective fence stays dormant. Historical unqualified claims are not
silently assigned these new rules.

## Remaining resource limitation

Owner rent/deposit and bounded movement counts are derived from L' and B before
work. Admission still refuses when that actual workload exceeds the claim-wide
contract stamped at acceptance. A producer/registrant that understates L can
therefore nominate a correctly hashed file larger than its stamped resource
contract. The EOF predicate is correct; universal arrival at EOF is not yet
closed by this checkpoint.

At current positive unit prices, B=0 already costs at least 256 parser-step units
per raw byte plus byte/carrier charges. A 2^56-byte (64 PiB) operand needs
18,524,994,117,235,693,568 sompi before the extra table envelope, exceeding u64.
There is no old independent raw-tokenizer file cap that makes this legal format
case impossible. Raising a carrier limit or trusting asserted L cannot repair
that arithmetic. A newly invented cap cannot be counted as coverage of old
eligible classes.

A finite example is feasible at the primitive level: L'=256 MiB and B=1024 needs
6,212 tokenizer moves, 337,500,160 carrier bytes, and 69,058,746,368 sompi under
current prices (including 64 reorg replay moves, excluding the separate largest
table envelope). These are deterministic worst-case terms retaining full512-bit
SMT authentication; compressed common proofs do not weaken the bound.

## Prospective adaptive extension design, not implemented here

A verifier can request additional paid work before the original immutable
open-until deadline. At that point existing claim-wide holds already prevent
producer and every bound Panel seat from exiting. A new accepted extension could:

1. Check the literal existing claim, owner reservation and timely opening.
2. Derive candidate L' workload; charge corresponding rent and lock ordinary
   owner's deposit before mutation.
3. Preserve accepted/open clocks, monotonically extend stream/collection/material
   and accused exit holds for all actual bound seats, and journal the extension.
4. Keep L' untrusted until original flat-hash EOF. A false operand produces no
   conviction and still pays/occupies its own allowance.

This would cover finite fundable understated lengths without producer publication.
It would also let a funded bogus operand extend all accused holds; the grief cost
and reservation/registration liability must be part of the normative economics.
It does not by itself cover legal operands exceeding finite capital/time/platform
bounds. Closing that remaining case requires an independent authenticated length
commitment or a succinct authenticated computation route, or an explicit protocol
scope decision. No completion claim should rely on the adaptive design alone.

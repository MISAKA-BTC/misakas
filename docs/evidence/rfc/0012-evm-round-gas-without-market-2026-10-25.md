# RFC0012 permitted round gas — preparation note superseded

The current [actual reproduction and repair record](0012-evm-round-gas-production-parity-2026-10-25.md)
contains the accepted native round fixture, baseline and first-repair failures,
empty-path reproduction, final execution and historical-compatibility limits.
The initial preparation hypothesis predicted a commitment mismatch. The actual
baseline instead accepted a30M cap where its permitted round required33M;
the empty fast path masked both producer and speculative-worker omissions.
Use the linked observed results and [final verification](0006-0007-0008-0012-court-aux-provider-round-verification-2026-10-25.md).

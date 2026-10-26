# Native court / carrier saturation experiment

Status: harness prepared; build and execution pending. No new capacity result is
claimed. The earlier reduced-cap and six-filing observations remain unchanged.

`Net::send` serializes transactions from the same card because they spend its
change chain. Simply cycling six keys over 72 proofs would therefore send twelve
waves, not a concurrent 72-proof load. The new ignored
`g14_kernel_saturation_measure` funds independent outputs through ordinary native
transactions, validates the individual carriers through the mempool, and offers
all of them to the real node's template. Remaining transactions are offered again
in subsequent blocks; the harness never enlarges template or consensus limits.

Each observation keeps offered/carried counts, transaction Borsh bytes, block
DAA, wall time and the fold's block budget. The folding block is one block after
the carrier, so a final flushing block is also recorded. Funding transactions and
their time are separate. Borsh bytes are not network framing or mass units.

The configurations use two or twelve outstanding claims, six fixture accusers
per claim, and actual trace lengths five or 64 (the class maximum). Each planted
fault is at the trace's last position. The maximum batch is 72 junk proofs, with
two rounds using different junk bytes. Fees are measured by unique bond, avoiding
double-counting when one accuser files against multiple claims. An outsider then
derives a valid prosecution from public material and offers it alongside a third
junk batch. Native transaction/fold ordering is retained; being co-offered does
not guarantee that all transactions share a block or that the outsider has
priority. A failure to convict within the unchanged liability window fails the
experiment and is retained.

The policy is explicitly test-only: cap 64, reserve 32, approved external gain
500 MSK, early Panel=1 and already-shipped anchor-retention prerequisites. Public
activation validation still fails. The test does not modify the ordinary clocks,
block limits or the external-gain bound to make overload pass.

`run_saturation_measure.py` uses fresh processes for N=0/N=1, checks the pinned
binary/source, and retains machine/load, command, process CPU/RSS and log hashes.
N=0 constructs the same claim and funding setup but sends no junk filings; its
clock history differs and it is a setup control, not a claim-identity twin. A run
that never reaches 64 adjudications records `run_cap_saturated=false`, even if
all tests pass. Transport-bound observations are not relabelled as full court
saturation. Likewise the outsider's eventual conviction does not establish
priority under continuous refill.

Pending scope after this experiment: quiet-load repetitions, continuous refill
and queue service guarantees, worst legal court rather than cheap dismissed junk,
full TIR session reservations, restart/reorg/pruned replay, and minimum-node
bounds. Those remaining requirements keep the arming record unfilled.

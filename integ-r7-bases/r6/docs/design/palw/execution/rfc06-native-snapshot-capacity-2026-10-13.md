# RFC06: native snapshot capacity and key generation — 2026-10-13

The 100k native/four-population carriage cannot fit the existing 384 MiB total
pruning snapshot cap. This is a structural transport defect, not an unmeasured
performance conclusion. `PalwBondStateV2` retains one 2,592-byte ML-DSA-87 public key;
it derives the operator key into a 64-byte `operator_id`. The registration object
held by Config keeps both keys, which matters to setup RSS, but the carriage does
not contain two public keys per bond.

Actual canonical Borsh `object_length` calls over the production record/key/candidate
types measured: active record with the minimum one capability = 2,817 bytes,
outpoint key = 68 bytes (transaction ids are Hash64), candidate = 341 bytes.
For 100,000 native bonds and 400,000 retained candidates, just these entries total
**424,900,000 bytes**, already **22,246,816 bytes above** 384 MiB = 402,653,184.
Maps, claim rows, other native tables and class declarations add bytes. Arc sharing
of the retained candidates changes memory ownership, not canonical Borsh length.
This lower bound proves the current transport capacity failure for that shape.

The native four-100k measurement on the current test binary was predeclared at
900 seconds and ended with status 124 in key generation, before its setup-complete
log. That deadline is retained. The small native four-seal control passed: 72 bonds,
284 candidates, engine 102,932 bytes and whole carriage 328,046 bytes. That small
result is not a substitute for the timed-out 100k resource run.

A separate leaf probe was predeclared at 64 keys / 60 seconds. It generated 64
unique valid public keys with the current debug libcrux dependency and an opt2
harness: minimum/median/maximum keygen 6,375 / 7,971 / 15,351 microseconds, total
543 ms. No 100k latency or upper bound is inferred from these shared-host samples.
The next main resource run uses a separately predeclared optimized crypto profile.
[Component plan](../../../evidence/palw/design-records/rfc06-native-components-2026-10-13-plan.json),
[component results](../../../evidence/palw/design-records/rfc06-native-components-2026-10-13-result.log),
[probe source](../../../evidence/palw/design-records/rfc06-native-components-2026-10-13.rs),
[900-second plan](../../../evidence/palw/design-records/rfc06-native-four-100k-timeout-2026-10-13-plan.json),
[timeout log](../../../evidence/palw/design-records/rfc06-native-four-100k-timeout-2026-10-13.log) and
[small native control](../../../evidence/palw/design-records/rfc06-native-four-small-2026-10-13.log) are retained.

For the next native transport measurement, extra worker outpoints use canonical
ordinary transaction hashes containing their high-entropy public-key owner scripts,
not compressible integer-word ids. These unsigned funding descriptors are not mined
payments: the fixture's funded genesis UTXO commitment remains its authority.
Setup prints progress every 10k keys. `MISAKA_RFC06_NATIVE_CARRIAGE_OUT` optionally
writes the actual four-seal canonical carriage using create-new private-file
semantics, so exact transport measurements can consume its bytes. The new fixture compiled in the matching integrated native test binary. Its
separately predeclared opt3 run completed key setup in 45.450 seconds, then
timed out at 900.022 seconds without carriage output. [Raw result and bottleneck
analysis](rfc06-overlay-negative-cache-2026-10-13.md) preserve this failure; it is
not four-seal resource or snapshot transport acceptance.

A negotiated version-107 experimental compressed transport has been implemented
without changing state Borsh, consensus roots or public activation parameters.
It separates the existing 384 MiB compressed-wire cap from a 512 MiB canonical
encoded cap. The latter adds 111,970,912 bytes to the measured lower bound for this
one-capability/four-population shape; it is not an approval of arbitrary native
state, all model dimensions, decoded process RSS or operating costs. Actual valid
100k high-entropy keys/operators/outpoints must be measured through that profile;
compression of the small fixture cannot certify the required shape.

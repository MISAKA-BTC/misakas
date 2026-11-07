# RFC06: native100k completes; full codec times out — 2026-10-13

The renewed actual native four-population test completed under its independently
predeclared 900-second limit: test body 423.68 s, wrapper 426.429 s, exit 0. It has
100,001 funded native bonds, four 100k retained populations (400,000 candidates),
and refuses the fifth population. The engine encodes to 136,406,790 bytes; the
whole native carriage encodes to **437,719,699 bytes**, above the unchanged legacy
384 MiB cap. Native carriage restoration/root equality at the end of the test also
passed. This is `PASS_NATIVE_FIXTURE_ONLY`: reference entropy/genesis funding and
private activation are explicit fixture seams, not real P2P/work/market approval.
[Plan](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-native-plan.json),
[result](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-native-result.json) and
[raw test/RSS](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-native.log) are retained.

Whole-process maximum RSS was 12,262,309,888 bytes; macOS peak physical footprint
was 17,090,606,352 bytes. They are distinct OS measurements, neither an approved
node memory budget. This producer includes absent-row commit tickets, but not the
later atomic genesis-declaration-index patch. It therefore cannot prove actual
fresh/pruned serving of its full declaration set.

The separately predeclared **180-second total** codec measurement first recovered
the authentic held declaration vector in 0.827 s. All three native class bindings
matched; the two held admission carriages total 9,610 B in their Borsh vector, with
BASE0's binary-derived admission-None exception explicitly counted. The raw source
SHA-256 is `155cfcef615df61cdff2b57135117290716dc6ed271f88ffb7689083c4730e12`;
the declaration file SHA-256 is `cdc26b07c0908e2b717bc1840514c21b8a8ee436e15d288ef733e56970f1b996`.
[Dumper output](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-declarations.log)
and [authentic declaration vector](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-declarations.borsh)
are retained.

The full state-plus-declarations probe then hit the fixed 180-second deadline
(exit 124, whole measurement 180.007 s). Its original tool printed only after
completion, so the empty codec log does not identify encode, transfer or decode
as the cause. **Wire size, compression ratio, complete byte equality and codec
maximum RSS were not measured.** No cap or deadline is extended, and the native
PASS is not relabelled a full transport PASS. The canonical complete envelope
would be 437,729,319 B from exact framing and recovered declaration counts; that
arithmetic is not a measured compressed size.
[Codec plan](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-codec-plan.json),
[failed result](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-codec-result.json)
and [original empty log](../../../evidence/palw/design-records/rfc06-native-full-fixture-2026-10-13-codec.log)
are retained. Its owned subprocess group was terminated; unlinked spill lifetime
was process-bound. The user subsequently required all remaining code implementation
before consolidated validation, so no further codec/benchmark/test execution is
started. Progress instrumentation, bounded serialization and full serving/adoption
consumer repairs will be source changes; their validation remains pending.

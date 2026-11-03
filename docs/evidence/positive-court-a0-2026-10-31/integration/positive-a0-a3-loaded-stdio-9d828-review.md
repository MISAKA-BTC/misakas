# A3 Loaded stdio independent static review

Fixed source `9d828c26503349d1f77c6a24e79a36d2dc031454`; parent `37ab25298365b9027a7fef6f38c5aacbe1383acc`. No source blocker found for owner compilation/runtime validation. No build or execution performed.

Changed only723-line custom harness,8-line test manifest target and17-line handoff. Dependent production source matches observed Root `7c3cdf8de4263cec373ffa0ed05e9dc09bd92742`: `True`. Test SHA `fab179ee6297df0429fc1e1ff82a3731ed9550b56a3fc6797eedb053ceba0ae3`.

PASS occurs only after actual File→Loaded→CPU→Hello2 stdio two-cycle endpoint comparisons; independently derived wire/build-work and cumulative usage checks; pre-drop child completion; child Loaded guard release; process-owner Drop and specific-PID ECHILD; parent guard release and metadata equality. The expected response is only a parent comparator. Unknown profile/platform returns77 without PASS; dispatch/path/file/load failures cannot meet the record assertions.

Take exhaustion is a bounded synthetic EOF after final Close, not an OS-pipe EOF. Completion proves serve returned and Loaded guard dropped before owner cleanup; it does not prove child natural exit0. Cleanup covers the direct child, with inherited Unix adapters and OS kill/wait latency assumptions. The harness adds no timeout/cancellation negative-path test. Host guard is a synthetic lifetime callback, not native memory funding, RSS or all-workspace admission. Server work is positive/cumulative, not independently exact.

This target creates a temporary synthetic artifact internally and needs no external model path. Child path/control env are mandatory and missing artifacts fail. Existing synthetic binding roots establish data association only; no actual LLM, NativeResult, P, Court/payment or credit.

Future runtime must execute the obtained harness=false binary with **no arguments**, require exit0 and one complete fixed-field PASS marker, reject unsupported/refused markers, and pin source/binary/native inputs. Count it as one separate OS-main custom-harness result, never a libtest default-stack case.

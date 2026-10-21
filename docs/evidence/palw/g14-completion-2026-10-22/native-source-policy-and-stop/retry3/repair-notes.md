# Explicit native ladder activation versus dormant compatibility

Retry2 configured a two-leaf court, with actual clock/cost parameters preserved, but expected Source refusal while palw_court_ladder remained dormant. The actual resolver correctly retained historical PALW_STEP_LEG_MAX_LEAVES. Its resulting Missing(native source checked) exposed an incorrect test expectation, not a production defect. The failed source, binary and log remain under retry2.

Retry3 adds an explicit dormant-case Source-checked assertion, then activates palw_court_ladder only in the local TEST config and retains the two-leaf Source refusal assertion. It preserves all original native configuration checks. No production preset, resolver, cap or assertion is weakened. Test success is unproven until the raw execution succeeds.

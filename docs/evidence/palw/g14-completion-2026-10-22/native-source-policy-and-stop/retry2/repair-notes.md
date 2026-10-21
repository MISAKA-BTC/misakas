# Preserve the actual native court clock in the leaf-ladder negative test

Retry1 reached both native registration-time demand boundary assertions, then failed startup of the later MAINNET small-court test: `the rung window the bundle commits to is not the one the sweep enforces`. Its source, binary hash and log are preserved under retry1. This is a fixture configuration defect, not permission to bypass native startup validation.

Retry2 changes only the test court maximum leaf count to2. It preserves the actual previous court turn deadline, terminal rounds and all three cost ceilings via with_cost_ceilings, and asserts the original dissection arity is unchanged. Thus the existing state clock mirror remains consistent while the actual native Source sees the deliberately insufficient leaf ladder. Production configuration and startup validation remain unchanged. This retry is not yet a passed native test until its raw result confirms it.

# Checked native tile-index fixture repair

Initial compilation failed in the new Selection fixture: tir_logits_event_disclosure_v1 accepts logits_tile:u8, but the fixture supplied tile as u32. The initial frozen added source, tracked patch, compiler log and hash manifest remain preserved in the parent directory.

This retry uses u8::try_from(tile) with an explicit fixture expectation. It does not truncate indices, alter the existing wire, enlarge caps, or weaken any runtime assertion. Build/test success remains unproven until raw logs complete.

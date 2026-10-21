# Worker source projection

`worker_orchestration_audit.py` replays the scoped extractions on a temporary copy of the parent at `fd3c74aef`, checks every retained pre-extraction fragment, then compares the entire current worker body byte for byte. It never edits the checkout. The generator scripts in this directory are inputs to that isolated replay; do not run them against a checkout.

`worker_stage_audit.py` compares the moved bodies and the unchanged admission/sweep policies, with explicit inverses for borrowed bindings and the registration return. Both sides use the same Rust formatter; escaped line indentation is normalized by Rust string semantics. This targeted structural evidence does not prove exhaustive execution equivalence. Actual compiled runtime results are recorded separately.

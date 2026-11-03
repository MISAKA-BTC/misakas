# Genesis fit repair measurements - 2026-10-25

User instruction: repair Dolphin and Gemma fidelity without relaxing acceptance, upload after passing, and remove large local data after remote verification. These are source-to-integer conversion measurements, not model-tier reviews or claim certification.

## Fixed conditions

Source revisions, tokenizers, calibration/evaluation corpora and the 32,768 served context retain their registry pins. Acceptance remains mean KL <= 0.1, top-1 >= 0.8, every long window mean KL <= 0.1, and late/early drift <= 1.5 unless late KL <= the existing 0.01 floor. An unmeasured or failing fit stops publication. Claim verification uses its existing settings.

## Dolphin #1

The measured embedding weight absmax/127 scale is 0.0025067667322834645. The old converter rounded it upward to a power of two, 0.00390625, discarding weight precision. Existing integer multiplier/shift parameters can already represent the finer scale. The opt-in `--a16-precise-embedding` updates both gather lift and tied head together; the legacy API/default preserves existing roots. The #1 genesis conversion configuration explicitly selects the new option.

| Same held-out first 64 positions | Baseline | Precise embedding |
|---|---:|---:|
| Mean KL | 0.11120723978692765 | 0.09671283786502449 |
| Top-1 | 50/64 | 55/64 |

Calibration-prompt top-1 improved from 46/57 to 51/57. The new PALWART3 root is `65e92751e0f0326c47c25c596960d567a8466b37192267e1f87bb7fc80e2cf83434cd4aa6bc2f48b66e8f3421a6bcb486da086b10f07f397e9342895dd76176f`. The full 32K-prefix, 32-window rerun is pending; this first-window result is not a long-context pass.

## Gemma #4

The baseline short fit has mean KL 0.18752115954284584 and top-1 0.8515625. Attention-only 16-bit weights reduce KL but still fail (exact measurements in the attached JSON). The first all-linear-16 diagnostic was killed with SIGKILL during materialisation on the 32 GB host; it produced no score.

The next candidate also widens dense MLP down/output weights to 16 bits, keeping gate/up main weights at 8 bits. Its numerical comparison is pending. Both short and 32K fits must pass before publication. These options choose existing integer matmul parameter types and preserve the default 8-bit profile.

## Reproduction and operation

Conversion, short fit and long fit receive the same precision flags, recorded in the manifest. The older standalone runtime-pack profile does not represent selective precision, so it refuses rather than rebuilding with different defaults. The PALWART3 pipeline records the selected values explicitly.

After a passing short AND long fit, upload the index, segments, metadata and raw reports. Check remote hashes for all those files before deleting source/output/legacy/stage. Preserve reports, calibration and state. Failed generated containers may be removed after preserving their identities and reports; the pinned sources and statistics reproduce them.

Validation: A16 compatibility/reproducibility/both-seam regression (1 test), selective/main16 types/refusals/streamed equality (4 tests), pipeline tests (18), and SDK release compile check passed.

## Original reports (copied byte for byte)

- [dolphin-baseline-window-report.json](genesis-fit-repair-2026-10-25/dolphin-baseline-window-report.json) - SHA-256 `a7bcca33c07aebb9108b685879759b22dc0b3fdc69805a0c5f8b2b5573a31fd5`
- [dolphin-precise-first64-report.json](genesis-fit-repair-2026-10-25/dolphin-precise-first64-report.json) - SHA-256 `de9fe32acb42119ca84f228cc3950845c8f6983b179e289955e3878a035e0653`
- [gemma-baseline-short-report.json](genesis-fit-repair-2026-10-25/gemma-baseline-short-report.json) - SHA-256 `017007205eafce3eda01d7cf7a6d616bae27eceefd0ec2ac4e4b127304db88ed`
- [gemma-attention16-short-report.json](genesis-fit-repair-2026-10-25/gemma-attention16-short-report.json) - SHA-256 `567c88c62c7b5a0100629a758df63febf520e8d2f9be1dfb553a026703c06b34`

## Short-fit memory correction

The short typed comparison previously retained the float head while executing all integer weights. On Gemma this caused paging. The CLI now caches at most 256 MiB of reference logits for short checks and releases the float head before integer execution. A zero cache budget keeps the old streaming path. Long checks above that budget retain the streaming path. The two modes produce identical metrics, program digest, evaluation and calibration metadata on the Llama and Gemma fixtures (regression passed). This does not change artifact parameters or acceptance. The unfinished selective diagnostic was stopped before any score and restarted with this resource correction.

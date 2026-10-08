"""**The coverage buckets** (RFC-0011 §16.4, the Model Onboarding — Coverage Closure directive of 2026-10-08): the census code of a
repository's first failing (or not-run) gate → one *primary blocker* in the user's eight buckets, plus the earlier analysis's dimension.

    MISSING_WEIGHTS        no weights to read: none listed, a shard set incomplete, the repository unreachable, a header unreadable
    GATED                  access is closed to the census (the repository is gated or disabled)
    ADAPTER_BASE_MISSING   an adapter whose base is absent, ambiguous, not in the snapshot, or gated
    FRONTEND               the importer / lowerer / adapter data cannot read or lower it (IR-expressible, build-side)
    NEW_KERNEL             a canonical job profile, a semantic primitive or an inactive fence is missing (consensus-side, versioned)
    QUANT_FORMAT           the weight file's form or its quantisation has no reader (onnx, tensorflow, flax, gguf-split, a quant scheme)
    RESOURCE               a mandatory node / DA / court / census bound is over its ceiling (never answered by raising a cap)
    UNTESTED               a gate that was not run (`NOT_RUN_*`): a depth, bytes or a chain this run did not have. Never a pass.
    NO_MODEL_TASK          (ninth, external) no declared task and no configuration naming a model class, or no readable configuration:
                           the repository has no model task to be registered as, so it is outside denominator (b)

External (nothing in this tree can supply it): MISSING_WEIGHTS, GATED, ADAPTER_BASE_MISSING, NO_MODEL_TASK.
Software-closable: FRONTEND, NEW_KERNEL, QUANT_FORMAT, RESOURCE, UNTESTED (NEW_KERNEL needs a consensus flag day, and RESOURCE a
rule change or a new resource route — "closable by software" does not mean "without a fence").

The earlier analysis's dimension (`DIMENSION`): `external`, `feature-only` (a generic lowering feature or data adapter),
`frontend-only` (a reader / importer / tokenizer / task-inference change), `protocol-envelope` (a profile, fence or bound of the
protocol), `untested` (nothing was run).

Total on a census gate row: `bucket_of(gate, code, arg, cls)`; a code outside the table is `("UNTESTED", "unmapped")`, counted and
shown, never a pass. `cls` (the machine class of a row, when it has one) decides the codes whose class depends on the argument.
"""

from __future__ import annotations

EXTERNAL = ("MISSING_WEIGHTS", "GATED", "ADAPTER_BASE_MISSING", "NO_MODEL_TASK")
CLOSABLE = ("FRONTEND", "NEW_KERNEL", "QUANT_FORMAT", "RESOURCE", "UNTESTED")
ALL_BUCKETS = EXTERNAL[:3] + CLOSABLE + EXTERNAL[3:]

# census code -> (bucket, dimension)
_CODE = {
    # source
    "GATED_ACCESS": ("GATED", "external"),
    "REPO_DISABLED": ("GATED", "external"),
    "MISSING_WEIGHTS": ("MISSING_WEIGHTS", "external"),
    "WEIGHTS_INCOMPLETE": ("MISSING_WEIGHTS", "external"),
    "REPO_UNREACHABLE": ("MISSING_WEIGHTS", "external"),
    "FETCH_FAILED": ("MISSING_WEIGHTS", "external"),
    "HEADER_INVALID": ("MISSING_WEIGHTS", "external"),
    "BASE_UNPINNED": ("ADAPTER_BASE_MISSING", "external"),
    "HEADER_TOO_LARGE": ("RESOURCE", "protocol-envelope"),
    "LISTING_UNREADABLE": ("MISSING_WEIGHTS", "external"),
    # lower: the task and the model definition
    "CONFIG_MISSING": ("NO_MODEL_TASK", "external"),
    "CONFIG_INVALID": ("NO_MODEL_TASK", "external"),
    "MODALITY_PROFILE_MISSING": ("NEW_KERNEL", "protocol-envelope"),
    "PARTIAL_TASK_ONLY": ("NEW_KERNEL", "protocol-envelope"),
    # lower: the artifact's form and quantisation
    "FORMAT_UNSUPPORTED": ("QUANT_FORMAT", "frontend-only"),
    "QUANT_DESCRIPTOR_MISSING": ("QUANT_FORMAT", "feature-only"),
    "QUANT_REFUSED": ("QUANT_FORMAT", "feature-only"),
    # lower: the reader
    "ADAPTER_UNCHECKED": ("FRONTEND", "frontend-only"),
    "ADAPTER_REFUSED": ("FRONTEND", "frontend-only"),
    "CUSTOM_CODE_UNMODELLED": ("FRONTEND", "feature-only"),
    "ARCH_REFUSED": ("FRONTEND", "feature-only"),
    "CONFIG_KEY_UNREAD": ("FRONTEND", "feature-only"),
    "TOKENIZER_MISSING": ("FRONTEND", "frontend-only"),
    "TENSOR_MISSING": ("FRONTEND", "frontend-only"),
    "TENSOR_SHAPE": ("FRONTEND", "frontend-only"),
    "TASK_MISMATCH": ("FRONTEND", "frontend-only"),
    "PREFLIGHT_PANIC": ("FRONTEND", "frontend-only"),
    # admit: the bounds
    "COURT_BUDGET": ("RESOURCE", "protocol-envelope"),
    "CLOSE_TOO_LARGE": ("RESOURCE", "protocol-envelope"),
    "CONTEXT_BOUND": ("RESOURCE", "protocol-envelope"),
    "COURT_WINDOW_EXCEEDED": ("RESOURCE", "protocol-envelope"),
    "DA_LADDER_EXCEEDED": ("RESOURCE", "protocol-envelope"),
    "ADMISSION_EXCEEDS": ("RESOURCE", "protocol-envelope"),
    "FENCE_NOT_ARMED": ("NEW_KERNEL", "protocol-envelope"),
    # seat
    "SEAT_MEMORY": ("RESOURCE", "protocol-envelope"),
    "READY_SEATS_SHORT": ("RESOURCE", "protocol-envelope"),
    "INDEPENDENCE_SHORT": ("RESOURCE", "protocol-envelope"),
}


def bucket_of(gate: str, code: str, arg: str | None, cls: str | None) -> tuple[str, str]:
    """(bucket, dimension) of one gate result. `cls` is the row's machine class (`FRONTEND_REQUIRED`, …) when the row carries one."""
    if code.startswith("NOT_RUN"):
        return ("UNTESTED", "untested")
    if code == "TASK_UNKNOWN":
        # A repository whose configuration names no model class is not a repository the importer can be asked to read.
        return ("NO_MODEL_TASK", "external") if arg == "no-config" else ("FRONTEND", "frontend-only")
    if code == "FEATURE_C":
        # The registry's protocol requirement decides (`census::onboarding::class_of_feature`): a capability the kernel lacks is the
        # kernel's, anything else the lowerer's.
        if cls in ("KERNEL_EXTENSION_REQUIRED", "KERNEL_NOT_ACTIVE", "PROFILE_REQUIRED"):
            return ("NEW_KERNEL", "protocol-envelope")
        return ("FRONTEND", "feature-only")
    if code == "ADMISSION_REFUSED":
        # "no layout can be derived" is the layout's; any other refusal of a lowered program is the program's (the frontend wrote
        # something admission does not accept). Both are the build's.
        return ("FRONTEND", "feature-only")
    if code in _CODE:
        return _CODE[code]
    return ("UNTESTED", "untested")


def is_unmapped(code: str) -> bool:
    return not code.startswith("NOT_RUN") and code not in _CODE and code not in ("TASK_UNKNOWN", "FEATURE_C", "ADMISSION_REFUSED")


def external(bucket: str) -> bool:
    return bucket in EXTERNAL


# The class a bucket must agree with on a row that carries a machine class (a self-check of this table against `onboarding.rs`).
_CLASS_OF_BUCKET = {
    "FRONTEND": {"FRONTEND_REQUIRED", "LAYOUT_REQUIRED"},
    "QUANT_FORMAT": {"FRONTEND_REQUIRED"},
    "NEW_KERNEL": {"KERNEL_EXTENSION_REQUIRED", "KERNEL_NOT_ACTIVE", "PROFILE_REQUIRED"},
    "RESOURCE": {"RESOURCE_REFUSED"},
    "UNTESTED": {"NOT_RUN", "UNMAPPED", "EXTERNAL_BLOCKER"},
    "MISSING_WEIGHTS": {"EXTERNAL_BLOCKER"},
    "GATED": {"EXTERNAL_BLOCKER"},
    "ADAPTER_BASE_MISSING": {"EXTERNAL_BLOCKER"},
    "NO_MODEL_TASK": {"EXTERNAL_BLOCKER"},
}


def class_agrees(bucket: str, cls: str | None) -> bool:
    return cls is None or cls in _CLASS_OF_BUCKET.get(bucket, set())

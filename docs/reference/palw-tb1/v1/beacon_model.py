"""Executable protocol model; NOT a cryptographic beacon implementation.

The qualification fixture knows all polynomial coefficients and authentic shares.
Public share digests stand in for a verified BGW VSS backend and authenticated
channels. This is not that protocol, a PQ implementation, DKG, private transport,
signature verification, a distributed ledger, or a security proof. Test randomness
is deterministic. ACTIVE_CRYPTO_BACKEND = BGWVSS_EXTERNAL_UNIMPLEMENTED.
"""
from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
import hashlib
import itertools
import random
from typing import Iterable

N = 7
F = 2
T = 3
PUBLIC_QUORUM = N - F
FIELD = (1 << 127) - 1
ZERO32 = bytes(32)


class Rejected(ValueError):
    pass


def strict_int(value: object, minimum: int, maximum: int, label: str) -> int:
    if type(value) is not int or not minimum <= value <= maximum:
        raise Rejected(f"{label}: canonical integer required")
    return value


def fixed_bytes(value: object, width: int, label: str) -> bytes:
    if type(value) is not bytes or len(value) != width:
        raise Rejected(f"{label}: exactly {width} bytes required")
    return value


def u64(value: object, label: str) -> bytes:
    return strict_int(value, 0, (1 << 64) - 1, label).to_bytes(8, "big")


def lp(value: bytes) -> bytes:
    if type(value) is not bytes or len(value) > (1 << 32) - 1:
        raise Rejected("canonical byte string required")
    return len(value).to_bytes(4, "big") + value


def digest(domain: bytes, *parts: bytes) -> bytes:
    return hashlib.sha256(lp(domain) + b"".join(lp(p) for p in parts)).digest()


def purpose_code(purpose: object) -> bytes:
    if type(purpose) is not str or purpose not in ("EXEC", "PANEL"):
        raise Rejected("purpose must be EXEC or PANEL")
    return b"\x01" if purpose == "EXEC" else b"\x02"


@dataclass(frozen=True)
class TimingPlan:
    close_step: int
    stable_step: int
    release_step: int
    deadline_step: int

    def encode(self) -> bytes:
        values = [self.close_step, self.stable_step, self.release_step, self.deadline_step]
        encoded = b"".join(u64(v, "timing step") for v in values)
        if not self.close_step <= self.stable_step < self.release_step < self.deadline_step:
            raise Rejected("close <= stable < release < deadline required")
        return encoded

    @property
    def identity(self) -> bytes:
        return digest(b"TOY/TIMING", self.encode())


@dataclass(frozen=True)
class RoundDescriptor:
    chain_id: bytes
    epoch: int
    purpose: str
    ordinal: int
    roster_root: bytes
    dkg_profile: bytes
    schedule_id: bytes

    def encode(self) -> bytes:
        return (
            fixed_bytes(self.chain_id, 32, "chain_id")
            + u64(self.epoch, "epoch")
            + purpose_code(self.purpose)
            + u64(self.ordinal, "ordinal")
            + fixed_bytes(self.roster_root, 32, "roster_root")
            + fixed_bytes(self.dkg_profile, 32, "dkg_profile")
            + fixed_bytes(self.schedule_id, 32, "schedule_id")
        )

    @property
    def identity(self) -> bytes:
        return digest(b"TOY/ROUND", self.encode())


@dataclass(frozen=True)
class Share:
    round_id: bytes
    dealer_id: int
    holder_index: int
    value: int

    def encode(self) -> bytes:
        return (
            fixed_bytes(self.round_id, 32, "share round")
            + strict_int(self.dealer_id, 1, N, "dealer_id").to_bytes(2, "big")
            + strict_int(self.holder_index, 1, N, "holder index").to_bytes(2, "big")
            + strict_int(self.value, 0, FIELD - 1, "share value").to_bytes(16, "big")
        )


@dataclass(frozen=True)
class QualifiedDealer:
    """Public fixture output, assumed qualified; not a production certificate."""
    round_id: bytes
    dealer_id: int
    salt: bytes
    ciphertext_digest: bytes
    secret_commitment: bytes
    share_commitments: tuple[bytes, ...]

    def encode(self) -> bytes:
        if type(self.share_commitments) is not tuple or len(self.share_commitments) != N:
            raise Rejected("exactly N share commitments required")
        return (
            fixed_bytes(self.round_id, 32, "dealer round")
            + strict_int(self.dealer_id, 1, N, "dealer_id").to_bytes(2, "big")
            + fixed_bytes(self.salt, 32, "qualification salt")
            + fixed_bytes(self.ciphertext_digest, 32, "ciphertext digest")
            + fixed_bytes(self.secret_commitment, 32, "secret commitment")
            + b"".join(fixed_bytes(c, 32, "share commitment") for c in self.share_commitments)
        )

    @property
    def qualification_id(self) -> bytes:
        return digest(b"TOY/QUALIFIED-DEALER", self.encode())


def share_digest(record: QualifiedDealer, share: Share) -> bytes:
    return digest(b"TOY/AUTHENTIC-SHARE-FIXTURE", record.salt,
                  record.ciphertext_digest, share.encode())


def secret_commit(round_id: bytes, dealer_id: int, salt: bytes, value: int) -> bytes:
    return digest(b"TOY/SECRET-COMMIT", round_id, u64(dealer_id, "dealer"), salt,
                  strict_int(value, 0, FIELD - 1, "secret").to_bytes(16, "big"))


def canonical_beacon_preimage(descriptor: RoundDescriptor, qroot: bytes,
                             freeze_id: bytes, dealer_values: tuple[tuple[int, int], ...]) -> bytes:
    """Toy encoding of D_b and the entire ordered (dealer, secret) vector.

    No modular sum or XOR is used. Qualification/ciphertext transcript binding
    enters through qroot. Authentication/proof bytes and accepted subsets do not.
    This encoding has not been allocated as production wire format.
    """
    if type(descriptor) is not RoundDescriptor:
        raise Rejected("typed round descriptor required")
    if type(dealer_values) is not tuple or not N - F <= len(dealer_values) <= N:
        raise Rejected("fixed full Q vector required")
    ids = []
    body = bytearray()
    for item in dealer_values:
        if type(item) is not tuple or len(item) != 2:
            raise Rejected("canonical dealer-secret pair required")
        dealer_id, secret = item
        ids.append(strict_int(dealer_id, 1, N, "dealer"))
        body += dealer_id.to_bytes(2, "big")
        body += strict_int(secret, 0, FIELD - 1, "secret").to_bytes(16, "big")
    if ids != sorted(ids) or len(set(ids)) != len(ids):
        raise Rejected("secret vector must follow canonical fixed Q order")
    return (
        lp(b"TOY/BEACON-SEED/VECTOR")
        + (1).to_bytes(2, "big")  # toy codec version, not a network allocation
        + purpose_code(descriptor.purpose)
        + descriptor.identity
        + fixed_bytes(qroot, 32, "qualification root")
        + fixed_bytes(freeze_id, 32, "freeze id")
        + lp(b"TOY/SHAKE256/BGWVSS_EXTERNAL_UNIMPLEMENTED")
        + len(dealer_values).to_bytes(2, "big")
        + bytes(body)
    )


def derive_beacon_seed(descriptor: RoundDescriptor, qroot: bytes,
                       freeze_id: bytes, dealer_values: tuple[tuple[int, int], ...]) -> bytes:
    return hashlib.shake_256(canonical_beacon_preimage(
        descriptor, qroot, freeze_id, dealer_values)).digest(32)


def eval_polynomial(coefficients: tuple[int, ...], x: int) -> int:
    strict_int(x, 1, N, "holder index")
    if type(coefficients) is not tuple or len(coefficients) != T:
        raise Rejected("fixture requires a degree-at-most-(T-1) polynomial")
    value = 0
    for coefficient in reversed(coefficients):
        strict_int(coefficient, 0, FIELD - 1, "coefficient")
        value = (value * x + coefficient) % FIELD
    return value


def interpolate_at(shares: Iterable[Share], x: int) -> int:
    strict_int(x, 0, N, "interpolation coordinate")
    values = tuple(shares)
    if len(values) != T:
        raise Rejected("exactly T authenticated shares required")
    for share in values:
        if type(share) is not Share:
            raise Rejected("typed Share required")
        share.encode()
    if len({s.holder_index for s in values}) != T:
        raise Rejected("duplicate holder index")
    if len({(s.round_id, s.dealer_id) for s in values}) != 1:
        raise Rejected("cross-round or cross-dealer interpolation")
    result = 0
    for share in values:
        numerator = denominator = 1
        for other in values:
            if other.holder_index == share.holder_index:
                continue
            numerator = numerator * (x - other.holder_index) % FIELD
            denominator = denominator * (share.holder_index - other.holder_index) % FIELD
        result = (result + share.value * numerator * pow(denominator, -1, FIELD)) % FIELD
    return result


def interpolate_at_zero(shares: Iterable[Share]) -> int:
    return interpolate_at(shares, 0)


@dataclass(frozen=True)
class QualificationFixture:
    descriptor: RoundDescriptor
    records: tuple[QualifiedDealer, ...]
    private_shares: tuple[tuple[Share, ...], ...]
    private_secrets: tuple[int, ...]
    qualification_root: bytes


def qualification_root(records: tuple[QualifiedDealer, ...]) -> bytes:
    if type(records) is not tuple or not N - F <= len(records) <= N:
        raise Rejected("fixed Q must contain N-F through N dealers")
    ids = tuple(record.dealer_id for record in records)
    if ids != tuple(sorted(ids)) or len(set(ids)) != len(ids):
        raise Rejected("Q must be canonical, sorted, and duplicate-free")
    if len({record.round_id for record in records}) != 1:
        raise Rejected("Q contains cross-round records")
    return digest(b"TOY/Q-ROOT", *(r.qualification_id for r in records))


def make_qualification_fixture(descriptor: RoundDescriptor, test_seed: int,
                               dealers: tuple[int, ...] = (1, 2, 3, 4, 5)) -> QualificationFixture:
    """Trusted local setup, with complete secrets; not a PVSS protocol.

    A deterministic Python PRNG exists only for reproducible test vectors.
    Full polynomial evaluation provides the *fixture* consistency guarantee.
    """
    descriptor.encode()
    strict_int(test_seed, 0, (1 << 64) - 1, "test seed")
    if type(dealers) is not tuple:
        raise Rejected("tuple required for Q")
    if len(set(dealers)) != len(dealers) or dealers != tuple(sorted(dealers)):
        raise Rejected("duplicate or noncanonical dealer set")
    prng = random.Random(test_seed)
    records, share_sets, secrets = [], [], []
    for dealer_id in dealers:
        strict_int(dealer_id, 1, N, "dealer id")
        coefficients = tuple(prng.randrange(FIELD) for _ in range(T))
        salt = prng.getrandbits(256).to_bytes(32, "big")
        # Placeholder identity of an encrypted-share transcript, not ciphertext.
        cipher_id = digest(b"TOY/FAKE-CIPHERTEXT-TRANSCRIPT", descriptor.identity,
                           u64(dealer_id, "dealer"), salt)
        shares = tuple(Share(descriptor.identity, dealer_id, i,
                             eval_polynomial(coefficients, i)) for i in range(1, N + 1))
        proto = QualifiedDealer(descriptor.identity, dealer_id, salt, cipher_id,
                                secret_commit(descriptor.identity, dealer_id, salt, coefficients[0]),
                                (ZERO32,) * N)
        record = QualifiedDealer(proto.round_id, proto.dealer_id, proto.salt,
                                 proto.ciphertext_digest, proto.secret_commitment,
                                 tuple(share_digest(proto, s) for s in shares))
        # Exhaustive fixture validation is deliberately not public PVSS verification.
        for subset in itertools.combinations(shares, T):
            if interpolate_at_zero(subset) != coefficients[0]:
                raise AssertionError("inconsistent setup fixture")
        records.append(record)
        share_sets.append(shares)
        secrets.append(coefficients[0])
    fixed_records = tuple(records)
    qroot = qualification_root(fixed_records)
    return QualificationFixture(descriptor, fixed_records, tuple(share_sets), tuple(secrets), qroot)


@dataclass(frozen=True)
class FreezeContext:
    purpose: str
    native_anchor: bytes
    registration_root: bytes
    original_root: bytes | None = None
    pool_root: bytes | None = None
    prior_exec_round: bytes | None = None
    prior_exec_release_step: int | None = None

    def encode(self) -> bytes:
        common = (purpose_code(self.purpose)
                  + fixed_bytes(self.native_anchor, 32, "native anchor")
                  + fixed_bytes(self.registration_root, 32, "registration root"))
        if self.purpose == "EXEC":
            if any(v is not None for v in (self.original_root, self.pool_root,
                                           self.prior_exec_round, self.prior_exec_release_step)):
                raise Rejected("EXEC freeze cannot contain future PANEL state")
            return common
        return (common + fixed_bytes(self.original_root, 32, "original root")
                + fixed_bytes(self.pool_root, 32, "pool root")
                + fixed_bytes(self.prior_exec_round, 32, "prior exec round")
                + u64(self.prior_exec_release_step, "prior exec release"))


class Status(Enum):
    SETUP = "SETUP"
    FROZEN = "FROZEN"
    RELEASING = "RELEASING"
    READY = "READY"
    TERMINAL_UNAVAILABLE = "TERMINAL_UNAVAILABLE"


@dataclass(frozen=True)
class ReconstructionCertificate:
    round_id: bytes
    freeze_id: bytes
    qualification_root: bytes
    dealer_shares: tuple[tuple[Share, ...], ...]


class BeaconRound:
    """Single canonical history model. Native stability is an explicit fixture."""

    def __init__(self, fixture: QualificationFixture, plan: TimingPlan):
        if type(fixture) is not QualificationFixture or type(plan) is not TimingPlan:
            raise Rejected("typed setup required")
        if fixture.descriptor.schedule_id != plan.identity:
            raise Rejected("schedule mismatch")
        if qualification_root(fixture.records) != fixture.qualification_root:
            raise Rejected("qualification root mismatch")
        if any(r.round_id != fixture.descriptor.identity for r in fixture.records):
            raise Rejected("qualification round mismatch")
        self.descriptor = fixture.descriptor
        self.plan = plan
        self.records = {r.dealer_id: r for r in fixture.records}
        self.qroot = fixture.qualification_root
        self.q = tuple(self.records)
        self.status = Status.SETUP
        self.freeze_id: bytes | None = None
        self.freeze_context: FreezeContext | None = None
        self.last_step = 0
        self.received: dict[int, dict[int, Share]] = {d: {} for d in self.q}
        self.recovered: dict[int, int] = {}
        self.seed: bytes | None = None

    def _check_now(self, now: int) -> None:
        strict_int(now, 0, (1 << 64) - 1, "now")
        if now < self.last_step:
            raise Rejected("logical time cannot move backward")

    def bind_freeze(self, context: FreezeContext, now: int, native_stable: bool) -> bytes:
        self._check_now(now)
        if type(native_stable) is not bool or not native_stable:
            raise Rejected("stable native freeze required (model fixture)")
        if type(context) is not FreezeContext:
            raise Rejected("typed freeze required")
        context_bytes = context.encode()
        if context.purpose != self.descriptor.purpose:
            raise Rejected("freeze purpose mismatch")
        if now < self.plan.stable_step or now >= self.plan.release_step:
            raise Rejected("freeze binding must be stable and precede release")
        if context.purpose == "PANEL" and not context.prior_exec_release_step < self.plan.close_step:
            raise Rejected("PANEL close must be strictly after EXEC release")
        fid = digest(b"TOY/FREEZE", self.descriptor.identity, self.qroot, context_bytes)
        if self.freeze_id is not None:
            if fid != self.freeze_id:
                raise Rejected("round is already bound to another freeze")
            return fid
        if self.status is not Status.SETUP:
            raise Rejected("round cannot be rebound")
        self.freeze_context = context
        self.freeze_id = fid
        self.status = Status.FROZEN
        self.last_step = now
        return fid

    def submit_share(self, share: Share, now: int, *, signature_bytes: bytes = b"",
                     proof_bytes: bytes = b"") -> bool:
        """Envelope bytes are ignored, not authenticated; backend is a fixture."""
        self._check_now(now)
        if type(signature_bytes) is not bytes or type(proof_bytes) is not bytes:
            raise Rejected("envelope bytes must have exact byte type")
        if len(signature_bytes) > 16384 or len(proof_bytes) > 16384:
            raise Rejected("toy envelope bound exceeded")
        if self.freeze_id is None or self.status is Status.SETUP:
            raise Rejected("no stable freeze")
        if self.status is Status.TERMINAL_UNAVAILABLE:
            raise Rejected("terminal round cannot restart")
        if not self.plan.release_step <= now < self.plan.deadline_step:
            raise Rejected("share outside fixed release interval")
        if type(share) is not Share:
            raise Rejected("typed Share required")
        share.encode()
        if share.round_id != self.descriptor.identity or share.dealer_id not in self.records:
            raise Rejected("cross-round, purpose, or non-Q dealer")
        record = self.records[share.dealer_id]
        if share_digest(record, share) != record.share_commitments[share.holder_index - 1]:
            raise Rejected("unauthenticated or altered share")
        existing = self.received[share.dealer_id].get(share.holder_index)
        if existing is not None:
            if existing != share:
                raise Rejected("conflicting duplicate share")
            return False
        self.received[share.dealer_id][share.holder_index] = share
        self.last_step = now
        if self.status is not Status.READY:
            self.status = Status.RELEASING
        shares = self.received[share.dealer_id]
        if len(shares) >= T and share.dealer_id not in self.recovered:
            chosen = tuple(shares[k] for k in sorted(shares)[:T])
            secret = interpolate_at_zero(chosen)
            if secret_commit(record.round_id, record.dealer_id, record.salt, secret) != record.secret_commitment:
                raise Rejected("qualification fixture violated")
            self.recovered[share.dealer_id] = secret
        if all(len(self.received[d]) >= PUBLIC_QUORUM for d in self.q) and self.seed is None:
            self.seed = self.verify_certificate(self.build_certificate())
            self.status = Status.READY
        return True

    def build_certificate(self) -> ReconstructionCertificate:
        if self.freeze_id is None:
            raise Rejected("no freeze for certificate")
        if any(len(self.received[d]) < PUBLIC_QUORUM for d in self.q):
            raise Rejected("public certificate requires N-F distinct shares per dealer")
        return ReconstructionCertificate(self.descriptor.identity, self.freeze_id, self.qroot,
                tuple(tuple(self.received[d][i] for i in sorted(self.received[d])) for d in self.q))

    def verify_certificate(self, cert: ReconstructionCertificate) -> bytes:
        """Public verification model: hashes represent authentication fixtures.

        T shares suffice to interpolate a private candidate. PUBLIC_QUORUM
        authenticated, polynomial-consistent shares per Q dealer are required
        for a public certificate. No production signature is verified here.
        """
        if type(cert) is not ReconstructionCertificate:
            raise Rejected("typed reconstruction certificate required")
        if self.freeze_id is None or cert.round_id != self.descriptor.identity:
            raise Rejected("certificate round mismatch")
        if cert.freeze_id != self.freeze_id or cert.qualification_root != self.qroot:
            raise Rejected("certificate freeze or Q mismatch")
        if type(cert.dealer_shares) is not tuple or len(cert.dealer_shares) != len(self.q):
            raise Rejected("certificate must contain all fixed Q dealers")
        secrets = []
        for dealer_id, shares in zip(self.q, cert.dealer_shares):
            if type(shares) is not tuple or not PUBLIC_QUORUM <= len(shares) <= N:
                raise Rejected("public certificate needs N-F to N shares per dealer")
            if any(type(s) is not Share for s in shares):
                raise Rejected("typed certificate shares required")
            indices = tuple(s.holder_index for s in shares)
            if indices != tuple(sorted(indices)) or len(set(indices)) != len(indices):
                raise Rejected("certificate shares need distinct sorted indices")
            record = self.records[dealer_id]
            for share in shares:
                share.encode()
                if share.dealer_id != dealer_id or share.round_id != self.descriptor.identity:
                    raise Rejected("certificate contains cross-dealer or cross-round share")
                if share_digest(record, share) != record.share_commitments[share.holder_index - 1]:
                    raise Rejected("certificate share authentication failed")
            basis = shares[:T]
            if any(interpolate_at(basis, s.holder_index) != s.value for s in shares):
                raise Rejected("certificate has no single supported degree-F polynomial")
            secret = interpolate_at_zero(basis)
            if secret_commit(record.round_id, record.dealer_id, record.salt, secret) != record.secret_commitment:
                raise Rejected("certificate secret does not match fixture commitment")
            secrets.append(secret)
        values = tuple(zip(self.q, secrets))
        return derive_beacon_seed(self.descriptor, self.qroot, self.freeze_id, values)

    def accept_certificate(self, cert: ReconstructionCertificate, now: int) -> bytes:
        self._check_now(now)
        if self.status is Status.TERMINAL_UNAVAILABLE:
            raise Rejected("terminal round cannot accept certificate")
        if not self.plan.release_step <= now < self.plan.deadline_step:
            raise Rejected("certificate outside fixed release interval")
        candidate = self.verify_certificate(cert)
        if self.seed is not None and self.seed != candidate:
            raise Rejected("certificate changed a fixed output")
        self.seed = candidate
        self.status = Status.READY
        self.last_step = now
        return candidate

    def tick(self, now: int) -> Status:
        self._check_now(now)
        self.last_step = now
        if now >= self.plan.deadline_step and self.status is not Status.READY:
            self.status = Status.TERMINAL_UNAVAILABLE
        return self.status

    def drop_dealer(self, dealer_id: int) -> None:
        raise Rejected("Q is fixed before release; no dealer dropping")

    def replace_freeze(self, context: FreezeContext) -> None:
        raise Rejected("no alternative freeze, pool, or seed retry")

    def fallback(self, seed: bytes) -> None:
        raise Rejected("no fallback seed")

    def result(self) -> bytes:
        if self.status is not Status.READY or self.seed is None:
            raise Rejected("no successful beacon result")
        return self.seed


class AttemptLedger:
    """Native binding fixture. Normal failure does not erase a consumed binding."""
    def __init__(self):
        self.binding: dict[bytes, bytes] = {}
        self.terminal: set[bytes] = set()

    def bind(self, attempt_id: bytes, round_id: bytes) -> None:
        fixed_bytes(attempt_id, 32, "attempt id")
        fixed_bytes(round_id, 32, "round id")
        if attempt_id in self.terminal:
            raise Rejected("terminal attempt cannot be rescued")
        prior = self.binding.get(attempt_id)
        if prior is not None and prior != round_id:
            raise Rejected("attempt already assigned to another round")
        self.binding[attempt_id] = round_id

    def fail(self, attempt_id: bytes) -> None:
        fixed_bytes(attempt_id, 32, "attempt id")
        if attempt_id not in self.binding:
            raise Rejected("unregistered attempt")
        self.terminal.add(attempt_id)

"""Independent protocol-model checks, not a cryptographic security test."""
from dataclasses import replace
from itertools import combinations
from pathlib import Path
import hashlib
import json
import sys
import unittest

from beacon_model import (
    N, F, T, PUBLIC_QUORUM, FIELD, Rejected, TimingPlan, RoundDescriptor,
    Share, FreezeContext, BeaconRound, Status, ReconstructionCertificate,
    AttemptLedger, digest, interpolate_at_zero, make_qualification_fixture,
    canonical_beacon_preimage, derive_beacon_seed,
)


def tag(name: str) -> bytes:
    return hashlib.sha256(name.encode("ascii")).digest()


EXEC_PLAN = TimingPlan(10, 12, 15, 30)
PANEL_PLAN = TimingPlan(30, 32, 35, 50)


def descriptor(purpose="EXEC", plan=EXEC_PLAN, ordinal=1):
    return RoundDescriptor(tag("test-chain"), 1, purpose, ordinal,
                           tag("roster-7"), tag("BGWVSS_EXTERNAL_UNIMPLEMENTED"), plan.identity)


def execution():
    fixture = make_qualification_fixture(descriptor(), 9981)
    model = BeaconRound(fixture, EXEC_PLAN)
    context = FreezeContext("EXEC", tag("native-stable"), tag("registrations"))
    return fixture, model, context


def panel():
    fixture = make_qualification_fixture(descriptor("PANEL", PANEL_PLAN), 9982)
    model = BeaconRound(fixture, PANEL_PLAN)
    context = FreezeContext("PANEL", tag("later-native-stable"), tag("registrations"),
                            tag("originals"), tag("panel-pool"), descriptor().identity, 15)
    return fixture, model, context


def bind_exec(model, context):
    model.bind_freeze(context, now=12, native_stable=True)


def feed(fixture, model, indices=(1, 2, 3, 4, 5), now=15, envelope=b"signature-A"):
    for dealer_shares in fixture.private_shares:
        for index in indices:
            model.submit_share(dealer_shares[index - 1], now,
                               signature_bytes=envelope, proof_bytes=b"fixture-proof")


def certificate(fixture, model, indices=(1, 2, 3, 4, 5)):
    return ReconstructionCertificate(model.descriptor.identity, model.freeze_id,
        fixture.qualification_root,
        tuple(tuple(shares[i - 1] for i in indices) for shares in fixture.private_shares))


class BeaconModelTests(unittest.TestCase):
    def test_01_descriptor_binds_every_field(self):
        base = descriptor()
        changes = [dict(chain_id=tag("other-chain")), dict(epoch=2),
                   dict(purpose="PANEL"), dict(ordinal=2), dict(roster_root=tag("other-roster")),
                   dict(dkg_profile=tag("other-profile")), dict(schedule_id=tag("other-schedule"))]
        for change in changes:
            self.assertNotEqual(base.identity, replace(base, **change).identity)

    def test_02_canonical_types_and_widths(self):
        for value in (True, False, 1.0, "1", -1, 1 << 64):
            with self.subTest(value=repr(value)), self.assertRaises(Rejected):
                replace(descriptor(), epoch=value).encode()
        for value in (b"short", bytearray(32), None):
            with self.subTest(value=repr(value)), self.assertRaises(Rejected):
                replace(descriptor(), roster_root=value).encode()

    def test_03_only_two_purposes(self):
        for value in ("TICKET", "exec", "", b"EXEC", True):
            with self.subTest(value=repr(value)), self.assertRaises(Rejected):
                replace(descriptor(), purpose=value).encode()

    def test_04_schedule_must_match_and_be_finite(self):
        for plan in (TimingPlan(10, 9, 15, 30), TimingPlan(10, 12, 12, 30),
                     TimingPlan(10, 12, 15, 15), TimingPlan(10, True, 15, 30)):
            with self.assertRaises(Rejected):
                plan.encode()
        fixture, _, _ = execution()
        with self.assertRaises(Rejected):
            BeaconRound(fixture, PANEL_PLAN)

    def test_05_fixed_qualified_set_minimum_and_canonicality(self):
        for dealers in ((1, 2, 3, 4), (1, 2, 3, 4, 4), (5, 4, 3, 2, 1), (1, 2, 3, 4, 8)):
            with self.subTest(dealers=dealers), self.assertRaises(Rejected):
                make_qualification_fixture(descriptor(), 9981, dealers)

    def test_06_all_175_threshold_subsets_reconstruct_same_secrets(self):
        fixture, _, _ = execution()
        checks = 0
        for secret, shares in zip(fixture.private_secrets, fixture.private_shares):
            for subset in combinations(shares, T):
                self.assertEqual(interpolate_at_zero(subset), secret)
                checks += 1
        self.assertEqual(checks, 175)

    def test_07_threshold_private_recovery_is_not_public_certificate(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        feed(fixture, model, indices=(1, 2, 3))
        self.assertEqual(len(model.recovered), len(model.q))
        self.assertEqual(model.status, Status.RELEASING)
        with self.assertRaises(Rejected):
            model.result()
        with self.assertRaises(Rejected):
            model.build_certificate()
        with self.assertRaises(Rejected):
            model.verify_certificate(certificate(fixture, model, (1, 2, 3)))

    def test_08_five_honest_holders_recover_every_dealer_without_dealers_online(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        # Holders 1 and 2 are silent. No dealer API is called after setup.
        feed(fixture, model, indices=(3, 4, 5, 6, 7))
        self.assertEqual(model.status, Status.READY)
        self.assertEqual(len(model.result()), 32)
        self.assertEqual(tuple(sorted(model.recovered)), (1, 2, 3, 4, 5))

    def test_09_all_21_public_subsets_give_same_output(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        outputs = set()
        for indices in combinations(range(1, N + 1), PUBLIC_QUORUM):
            outputs.add(model.verify_certificate(certificate(fixture, model, indices)))
        self.assertEqual(len(outputs), 1)

    def test_10_wrong_polynomial_has_no_authenticated_support(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        cert = certificate(fixture, model)
        groups = list(cert.dealer_shares)
        # A complete alternative degree-zero polynomial is mathematically valid,
        # but no supplied point is authenticated to the fixed qualified dealer.
        groups[0] = tuple(replace(s, value=42) for s in groups[0])
        with self.assertRaises(Rejected):
            model.verify_certificate(replace(cert, dealer_shares=tuple(groups)))

    def test_11_changed_share_or_holder_index_rejected(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        share = fixture.private_shares[0][0]
        for changed in (replace(share, value=(share.value + 1) % FIELD),
                        replace(share, holder_index=2), replace(share, holder_index=0),
                        replace(share, holder_index=8), replace(share, holder_index=True),
                        replace(share, value=1.0)):
            with self.assertRaises(Rejected):
                model.submit_share(changed, 15)

    def test_12_cross_purpose_cross_round_cross_dealer_rejected(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        share = fixture.private_shares[0][0]
        for changed in (replace(share, round_id=descriptor("PANEL", PANEL_PLAN).identity),
                        replace(share, round_id=descriptor(ordinal=2).identity),
                        replace(share, dealer_id=2), replace(share, dealer_id=7)):
            with self.assertRaises(Rejected):
                model.submit_share(changed, 15)

    def test_13_missing_and_duplicate_shares_do_not_form_certificate(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        for shares in fixture.private_shares:
            for share in shares[:4]:
                model.submit_share(share, 15)
                self.assertFalse(model.submit_share(share, 15))
        with self.assertRaises(Rejected):
            model.build_certificate()
        cert = certificate(fixture, model)
        duplicate = (cert.dealer_shares[0][0],) * 5
        with self.assertRaises(Rejected):
            model.verify_certificate(replace(cert, dealer_shares=(duplicate,) + cert.dealer_shares[1:]))

    def test_14_no_public_release_before_stable_freeze(self):
        fixture, model, context = execution()
        with self.assertRaises(Rejected):
            model.submit_share(fixture.private_shares[0][0], 15)
        for now, stable in ((11, True), (12, False), (12, 1)):
            with self.assertRaises(Rejected):
                model.bind_freeze(context, now, stable)
        bind_exec(model, context)
        with self.assertRaises(Rejected):
            model.submit_share(fixture.private_shares[0][0], 14)

    def test_15_panel_requires_later_phase_and_both_roots(self):
        _, model, context = panel()
        for changed in (replace(context, pool_root=None), replace(context, original_root=None),
                        replace(context, prior_exec_release_step=30),
                        replace(context, prior_exec_release_step=True)):
            with self.assertRaises(Rejected):
                model.bind_freeze(changed, 32, True)
        self.assertEqual(len(model.bind_freeze(context, 32, True)), 32)

    def test_16_exec_cannot_smuggle_future_panel_state(self):
        _, model, context = execution()
        with self.assertRaises(Rejected):
            model.bind_freeze(replace(context, pool_root=tag("future-pool")), 12, True)

    def test_17_signature_and_proof_bytes_do_not_change_seed(self):
        fixture, model_a, context = execution()
        _, model_b, _ = execution()
        bind_exec(model_a, context)
        bind_exec(model_b, context)
        feed(fixture, model_a, envelope=b"one-encoding")
        for shares in reversed(fixture.private_shares):
            for share in reversed(shares[2:]):
                model_b.submit_share(share, 15, signature_bytes=b"different-encoding",
                                     proof_bytes=b"different-proof-randomness")
        self.assertEqual(model_a.result(), model_b.result())

    def test_18_alternative_freeze_or_pool_after_release_rejected(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        feed(fixture, model)
        with self.assertRaises(Rejected):
            model.bind_freeze(replace(context, registration_root=tag("changed-set")), 15, True)
        with self.assertRaises(Rejected):
            model.replace_freeze(replace(context, registration_root=tag("changed-set")))
        pfixture, pmodel, pcontext = panel()
        pmodel.bind_freeze(pcontext, 32, True)
        feed(pfixture, pmodel, now=35)
        with self.assertRaises(Rejected):
            pmodel.replace_freeze(replace(pcontext, pool_root=tag("favorable-pool")))

    def test_19_Q_cannot_shrink_after_partial_reconstruction(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        for shares in fixture.private_shares[:-1]:
            for share in shares[:5]:
                model.submit_share(share, 15)
        with self.assertRaises(Rejected):
            model.drop_dealer(5)
        with self.assertRaises(Rejected):
            model.result()
        cert = certificate(fixture, model)
        with self.assertRaises(Rejected):
            model.verify_certificate(replace(cert, dealer_shares=cert.dealer_shares[:-1]))

    def test_20_unavailable_is_finite_terminal_no_fallback(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        feed(fixture, model, indices=(1, 2))
        self.assertEqual(model.tick(30), Status.TERMINAL_UNAVAILABLE)
        self.assertEqual(model.tick(31), Status.TERMINAL_UNAVAILABLE)
        with self.assertRaises(Rejected):
            model.submit_share(fixture.private_shares[0][2], 31)
        with self.assertRaises(Rejected):
            model.fallback(tag("fallback"))
        with self.assertRaises(Rejected):
            model.accept_certificate(certificate(fixture, model), 31)

    def test_21_next_round_cannot_rescue_same_attempt(self):
        ledger = AttemptLedger()
        attempt = tag("attempt")
        ledger.bind(attempt, descriptor().identity)
        ledger.bind(attempt, descriptor().identity)  # same assignment is idempotent
        with self.assertRaises(Rejected):
            ledger.bind(attempt, descriptor(ordinal=2).identity)
        ledger.fail(attempt)
        for rid in (descriptor().identity, descriptor(ordinal=2).identity):
            with self.assertRaises(Rejected):
                ledger.bind(attempt, rid)

    def test_22_qualification_salt_and_ciphertext_transcript_bound(self):
        fixture, _, _ = execution()
        first = fixture.records[0]
        for changed in (replace(first, salt=tag("new-salt")),
                        replace(first, ciphertext_digest=tag("new-transcript"))):
            manipulated = replace(fixture, records=(changed,) + fixture.records[1:])
            with self.assertRaises(Rejected):
                BeaconRound(manipulated, EXEC_PLAN)

    def test_23_certificate_rejects_changed_context_and_order(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        cert = certificate(fixture, model)
        for changed in (replace(cert, qualification_root=tag("new-Q")),
                        replace(cert, freeze_id=tag("new-freeze")),
                        replace(cert, round_id=tag("new-round")),
                        replace(cert, dealer_shares=tuple(reversed(cert.dealer_shares)))):
            with self.assertRaises(Rejected):
                model.verify_certificate(changed)

    def test_24_public_certificate_release_gate_and_same_output(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        cert_a = certificate(fixture, model, (1, 2, 3, 4, 5))
        cert_b = certificate(fixture, model, (3, 4, 5, 6, 7))
        with self.assertRaises(Rejected):
            model.accept_certificate(cert_a, 14)
        result_a = model.accept_certificate(cert_a, 15)
        result_b = model.accept_certificate(cert_b, 15)
        self.assertEqual(result_a, result_b)
        self.assertEqual(model.status, Status.READY)

    def test_25_threshold_collusion_demonstration_not_prevented(self):
        fixture, _, _ = execution()
        # T compromised holders can privately reconstruct before any release gate.
        colluding_indices = (1, 2, 3)
        revealed = tuple(interpolate_at_zero(tuple(shares[i - 1] for i in colluding_indices))
                         for shares in fixture.private_shares)
        self.assertEqual(revealed, fixture.private_secrets)
        self.assertEqual(T, F + 1)

    def test_26_known_exec_hash_is_predictable_not_future_randomness(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        feed(fixture, model)
        published_exec = model.result()
        proposed_bad_panel = digest(b"PANEL", published_exec)
        adversary_prediction = digest(b"PANEL", published_exec)
        self.assertEqual(proposed_bad_panel, adversary_prediction)
        pfixture, pmodel, pcontext = panel()
        pmodel.bind_freeze(pcontext, 32, True)
        feed(pfixture, pmodel, now=35)
        self.assertNotEqual(pmodel.result(), proposed_bad_panel)

    def test_27_two_private_points_alone_allow_different_secrets(self):
        fixture, _, _ = execution()
        shares = fixture.private_shares[0]
        # Add g(x)=(x-1)(x-2): same first F=2 points, different constant.
        alternative = tuple(replace(s, value=(s.value + (s.holder_index - 1)
                                             * (s.holder_index - 2)) % FIELD) for s in shares)
        self.assertEqual(shares[0].value, alternative[0].value)
        self.assertEqual(shares[1].value, alternative[1].value)
        self.assertNotEqual(interpolate_at_zero(shares[:T]), interpolate_at_zero(alternative[:T]))
        # This algebraic demonstration does not prove privacy of a transcript.

    def test_28_success_remains_success_but_late_release_is_rejected(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        feed(fixture, model)
        result = model.result()
        self.assertEqual(model.tick(30), Status.READY)
        self.assertEqual(model.result(), result)
        with self.assertRaises(Rejected):
            model.submit_share(fixture.private_shares[0][5], 30)
        with self.assertRaises(Rejected):
            model.tick(True)

    def test_29_full_vector_hash_does_not_collapse_equal_modular_sums(self):
        fixture, model, context = execution()
        bind_exec(model, context)
        original = tuple(zip(model.q, fixture.private_secrets))
        changed = list(original)
        changed[0] = (changed[0][0], (changed[0][1] + 1) % FIELD)
        changed[1] = (changed[1][0], (changed[1][1] - 1) % FIELD)
        changed = tuple(changed)
        self.assertEqual(sum(s for _, s in original) % FIELD, sum(s for _, s in changed) % FIELD)
        self.assertNotEqual(canonical_beacon_preimage(model.descriptor, model.qroot,
            model.freeze_id, original), canonical_beacon_preimage(model.descriptor,
            model.qroot, model.freeze_id, changed))
        self.assertNotEqual(derive_beacon_seed(model.descriptor, model.qroot,
            model.freeze_id, original), derive_beacon_seed(model.descriptor,
            model.qroot, model.freeze_id, changed))
        for malformed in (tuple(reversed(original)), original[:-1],
                          ((True, original[0][1]),) + original[1:],
                          ((original[0][0], float(original[0][1])),) + original[1:]):
            with self.assertRaises(Rejected):
                canonical_beacon_preimage(model.descriptor, model.qroot, model.freeze_id, malformed)


def export_fixture_vector(fixture, model, context, *, bind_step, release_step):
    model.bind_freeze(context, bind_step, True)
    feed(fixture, model, now=release_step)
    values = tuple(zip(model.q, fixture.private_secrets))
    preimage = canonical_beacon_preimage(model.descriptor, model.qroot, model.freeze_id, values)
    return {
        "warning": "ALL SECRETS AND SHARES ARE PUBLIC TEST FIXTURES; NOT PRODUCTION KEYS OR ENTROPY",
        "purpose": model.descriptor.purpose,
        "round_descriptor_encoding_hex": model.descriptor.encode().hex(),
        "round_id_hex": model.descriptor.identity.hex(),
        "qualification_root_hex": model.qroot.hex(),
        "freeze_context_encoding_hex": context.encode().hex(),
        "freeze_id_hex": model.freeze_id.hex(),
        "timeline": [{"step": 0, "status": "SETUP"},
                     {"step": bind_step, "status": "FROZEN"},
                     {"step": release_step, "status": "READY"}],
        "public_certificate_holder_indices": [1, 2, 3, 4, 5],
        "ordered_dealer_secrets": [{"dealer_id": d, "secret_hex": s.to_bytes(16, "big").hex()}
                                  for d, s in values],
        "dealer_fixtures": [{
            "dealer_id": record.dealer_id,
            "qualification_id_hex": record.qualification_id.hex(),
            "salt_hex": record.salt.hex(),
            "placeholder_ciphertext_digest_hex": record.ciphertext_digest.hex(),
            "secret_commitment_hex": record.secret_commitment.hex(),
            "share_commitments_hex": [h.hex() for h in record.share_commitments],
            "public_test_shares": [{"index": s.holder_index,
                                    "value_hex": s.value.to_bytes(16, "big").hex()}
                                   for s in shares]
        } for record, shares in zip(fixture.records, fixture.private_shares)],
        "output_preimage_hex": preimage.hex(),
        "output_algorithm": "SHAKE256(preimage,32)",
        "seed_hex": model.result().hex(),
        "independently_recomputed_seed_hex": hashlib.shake_256(preimage).digest(32).hex(),
    }


class RecordingResult(unittest.TextTestResult):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.passed = []

    def addSuccess(self, test):
        super().addSuccess(test)
        self.passed.append(test.id())


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(BeaconModelTests)
    result = unittest.TextTestRunner(verbosity=2, resultclass=RecordingResult).run(suite)
    root = Path(__file__).resolve().parent
    report = {
        "scope": "protocol_state_machine_and_algebraic_fixture_only",
        "active_crypto_backend": "BGWVSS_EXTERNAL_UNIMPLEMENTED",
        "parameters": {"n": N, "f": F, "t_private_recovery": T,
                       "q_public_certificate": PUBLIC_QUORUM, "field": str(FIELD),
                       "qualified_dealers_in_default_fixture": 5},
        "tests_run": result.testsRun,
        "success": result.wasSuccessful(),
        "failures": len(result.failures), "errors": len(result.errors),
        "skipped": len(result.skipped), "passed_tests": result.passed,
        "exhaustive_private_recovery_subsets": 175,
        "public_subset_certificates_compared": 21,
        "output_derivation": "SHAKE256(canonical_descriptor_and_entire_ordered_dealer_secret_vector,32)",
        "limitations": [
            "No BGW VSS network protocol, DKG, private channels, signatures, PQ cryptography, or native consensus implemented.",
            "Qualification and share authentication are trusted fixtures built with complete secret-polynomial knowledge.",
            "Stable freeze and logical time are native-state fixtures, not a common-prefix proof.",
            "Seed generation uses deterministic Python random for repeatable tests; not production entropy.",
            "Threshold collusion and known-seed rehash predictability are demonstrated, not prevented.",
            "Test success is not a security, liveness, performance, or deployment proof."
        ],
        "source_sha256": {name: hashlib.sha256((root / name).read_bytes()).hexdigest()
                          for name in ("beacon_model.py", "test_beacon_model.py")}
    }
    efixture, emodel, econtext = execution()
    pfixture, pmodel, pcontext = panel()
    vectors = {
        "status": "PUBLIC_SECRET_FIXTURES_ONLY",
        "active_crypto_backend": "BGWVSS_EXTERNAL_UNIMPLEMENTED",
        "n": N, "f": F, "t_private_recovery": T, "q_public_certificate": PUBLIC_QUORUM,
        "vectors": [export_fixture_vector(efixture, emodel, econtext, bind_step=12, release_step=15),
                    export_fixture_vector(pfixture, pmodel, pcontext, bind_step=32, release_step=35)]
    }
    vector_bytes = (json.dumps(vectors, ensure_ascii=False, indent=2) + "\n").encode()
    (root / "test_vectors.json").write_bytes(vector_bytes)
    report["public_test_vectors_sha256"] = hashlib.sha256(vector_bytes).hexdigest()
    (root / "results.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({k: report[k] for k in ("tests_run", "success", "failures", "errors")}, indent=2))
    sys.exit(0 if result.wasSuccessful() else 1)

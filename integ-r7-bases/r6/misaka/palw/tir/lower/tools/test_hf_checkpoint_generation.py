"""Own preallocation and independent scoring checks; no model/Torch is loaded."""
import copy
import math
import unittest

from hf_checkpoint_generation import protocol_check
from score_checkpoint_generation import score


def policy():
    return {"schema": "misaka.rfc02.greedy-task-protocol.v1", "repository": "test/model", "revision": "a"*40,
            "source_manifest_sha256": "b"*64, "artifact_sha256": "c"*64, "tokenizer_sha256": "d"*64,
            "context": 4, "vocab": 4, "max_source_bytes": 1000, "max_model_parameter_bytes": 2000,
            "max_total_positions": 4, "max_new_tokens": 3, "eos_token_ids": [3],
            "task_jobs": [{"id": "job", "task": "arithmetic", "prompt_text": "prompt", "prompt_tokens": [1, 2], "expected_answer": "2"}],
            "token_agreement_min": 1.0, "exact_job_fraction_min": 1.0, "source_and_integer_task_success_min": 1.0}


class Checks(unittest.TestCase):
    def check(self, p):
        return protocol_check(p, {"vocab_size": 4, "max_position_embeddings": 4})

    def test_actual_fed_extent_includes_own_generated_feedback(self):
        p = policy()
        self.check(p)
        for key, value in [("context", 3), ("max_total_positions", 3), ("max_new_tokens", 4)]:
            q = copy.deepcopy(p); q[key] = value
            with self.assertRaises(ValueError): self.check(q)

    def test_every_scope_pin_and_policy_field_is_fixed(self):
        p = policy()
        for key, value in [("revision", "A"*40), ("tokenizer_sha256", "0"*63), ("extra", True),
                           ("token_agreement_min", math.nan), ("max_source_bytes", True), ("vocab", 5)]:
            q = copy.deepcopy(p); q[key] = value
            with self.assertRaises(ValueError): self.check(q)

    def test_task_token_eos_and_duplicate_job_bounds_precede_execution(self):
        p = policy()
        for values in ([True], [-1], [4], []):
            q = copy.deepcopy(p); q["task_jobs"][0]["prompt_tokens"] = values
            with self.assertRaises(ValueError): self.check(q)
        for values in ([3, 3], [4], []):
            q = copy.deepcopy(p); q["eos_token_ids"] = values
            with self.assertRaises(ValueError): self.check(q)
        q = copy.deepcopy(p); q["task_jobs"] *= 2
        with self.assertRaises(ValueError): self.check(q)
        q = copy.deepcopy(p); q["task_jobs"][0]["task"] = "ignored-video"
        with self.assertRaises(ValueError): self.check(q)

    def test_independent_count_and_decode_reject_fabricated_aggregate(self):
        p = policy()
        src = {"schema": "misaka.rfc02.independent-greedy-source.v1", "jobs": [{"id": "job", "task": "arithmetic",
               "prompt_tokens": [1,2], "generated_tokens": [2,3,0], "first_eos_index": 1,
               "answer_text": "2", "predeclared_task_answer_passed": True}]}
        own = {"schema": "misaka.rfc02.independent-greedy-integer.v1", "jobs": [{"id": "job", "task": "arithmetic",
               "prompt_tokens": [1,2], "generated_tokens": [2,3,0], "source_generated_tokens": [2,3,0]}],
               "token_agreement": 1.0, "exact_job_fraction": 1.0, "predeclared_free_running_fidelity_passed": True}
        class Decoder:
            def decode(self, tokens, skip_special_tokens): return "".join(str(t) for t in tokens)
        self.assertTrue(score(p, src, own, Decoder())["predeclared_generation_and_answer_thresholds_met"])
        bad = copy.deepcopy(own); bad["jobs"][0]["generated_tokens"][0] = 1
        with self.assertRaises(ValueError): score(p, src, bad, Decoder())
        bad = copy.deepcopy(src); bad["jobs"][0]["answer_text"] = "made-up answer"
        with self.assertRaises(ValueError): score(p, bad, own, Decoder())


if __name__ == "__main__":
    unittest.main()

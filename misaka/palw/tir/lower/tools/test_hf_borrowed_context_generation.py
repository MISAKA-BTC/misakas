import copy
import json
from pathlib import Path
import unittest

import hf_borrowed_context_generation as task


class CompleteTaskControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.protocol = json.loads((Path(__file__).resolve().parents[2] / "docs/design/palw/tir/evidence/qwen25-borrowed-context128-task-protocol.json").read_text())
        cls.config = {"vocab_size": 151936, "max_position_embeddings": 32768}

    def test_policy_binds_full_extents_before_models(self):
        self.assertEqual(len(task.protocol_check(self.protocol, self.config)), 3)
        for updates in ({"max_total_positions": 1}, {"context": 32}, {"token_agreement_min": float("nan")}, {"eos_token_ids": [151645, 151645]}):
            with self.assertRaises(ValueError):
                task.protocol_check({**self.protocol, **updates}, self.config)

    def test_foreign_rule_and_out_of_range_prompt_refused(self):
        for change in ("foreign", 151936):
            p = copy.deepcopy(self.protocol)
            if isinstance(change, str):
                p["task_jobs"][0]["answer_rule"] = change
            else:
                p["task_jobs"][0]["prompt_tokens"][0] = change
            with self.assertRaises(ValueError):
                task.protocol_check(p, self.config)

    def test_code_accepts_complete_function_and_pure_fence(self):
        j = self.protocol["task_jobs"][2]
        for text in ("def add(a,b):\n return a+b", "```python\ndef add(a, b):\n    return a + b\n```", "```\ndef add(a, b):\n return a+b\n```"):
            self.assertTrue(task.answer_passed(j, text))

    def test_code_rejects_truncation_extra_actions_and_changes(self):
        j = self.protocol["task_jobs"][2]
        for text in ("```python\ndef add(a,b):\n return a+b", "def add(a,b):\n return a-b", "def add(a,b):\n return b+a", "import os\ndef add(a,b):\n return a+b", "@run\ndef add(a,b):\n return a+b", "def add(a,b=3):\n return a+b", "def add(a,b):\n print(a)\n return a+b", "def add(a,b):\n return a+b\n#done\nprint('extra')"):
            self.assertFalse(task.answer_passed(j, text))

    def test_greeting_punctuation_rule_is_exact(self):
        j = self.protocol["task_jobs"][1]
        for good in ("Hello", "Hello.", " Hello.\n"):
            self.assertTrue(task.answer_passed(j, good))
        for bad in ("Hello!", "hello", "Hello. More words."):
            self.assertFalse(task.answer_passed(j, bad))

    def test_eos_is_required_and_cannot_have_tail(self):
        class Tokenizer:
            def decode(self, tokens, skip_special_tokens):
                return "42"
        j = self.protocol["task_jobs"][0]
        self.assertTrue(task.image(self.protocol, j, [42, 151645], Tokenizer())["task_answer_passed"])
        incomplete = task.image(self.protocol, j, [42] * 32, Tokenizer())
        self.assertFalse(incomplete["completed"])
        self.assertFalse(incomplete["task_answer_passed"])
        for bad in ([], [42], [42, 151645, 1], [42] * 33):
            with self.assertRaises(ValueError):
                task.image(self.protocol, j, bad, Tokenizer())


if __name__ == "__main__":
    unittest.main()

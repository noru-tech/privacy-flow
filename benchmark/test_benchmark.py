#!/usr/bin/env python3
"""Tests for the benchmark scripts' arithmetic and sampling (no network, no corpus needed).

    python3 -m unittest benchmark/test_benchmark.py
"""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import candidates  # noqa: E402
import score  # noqa: E402
import sheets  # noqa: E402


class Statistics(unittest.TestCase):
    def test_wilson_matches_published_values(self):
        # Newcombe (1998), Table I: 81/263 → 0.2553 to 0.3662.
        w = score.wilson(81, 263)
        self.assertAlmostEqual(w["lo"], 0.2553, places=3)
        self.assertAlmostEqual(w["hi"], 0.3662, places=3)
        w = score.wilson(0, 10)
        self.assertEqual(w["lo"], 0.0)
        self.assertAlmostEqual(w["hi"], 0.2775, places=3)
        self.assertIsNone(score.wilson(0, 0))

    def test_kappa(self):
        # 20 agree yes, 15 agree no, 5 + 10 disagree: po = 0.7, pe = 0.5 → κ = 0.4.
        pairs = [("y", "y")] * 20 + [("n", "n")] * 15 + [("y", "n")] * 5 + [("n", "y")] * 10
        k = score.kappa(pairs)
        self.assertAlmostEqual(k["agreement"], 0.7)
        self.assertAlmostEqual(k["kappa"], 0.4, places=4)
        self.assertIsNone(score.kappa([("y", "y")])["kappa"])


class Sampling(unittest.TestCase):
    def test_take_is_deterministic_and_independent_of_input_order(self):
        xs = [f"id-{i}" for i in range(100)]
        a = sheets.take(xs, 8, "app", lambda x: x)
        b = sheets.take(list(reversed(xs)), 8, "app", lambda x: x)
        self.assertEqual(a, b)
        self.assertEqual(len(a), 8)
        self.assertNotEqual(a, sheets.take(xs, 8, "other-app", lambda x: x))

    def test_sites_fill_from_the_other_stratum(self):
        sites = [{"path": "a.py", "line": i, "column": 1, "class": "log", "callee": "log.info"} for i in range(50)]
        sites += [{"path": "b.py", "line": i, "column": 1, "class": "http", "callee": "requests.get"} for i in range(3)]
        chosen, pop = sheets.site_items("app", sites)
        self.assertEqual(len(chosen), sheets.K_SITES)
        self.assertEqual(pop, {"sites:log": 50, "sites:other": 3})
        self.assertEqual(sum(1 for c in chosen.values() if c["item"]["class"] == "http"), 3)


class Candidates(unittest.TestCase):
    def test_classification(self):
        cases = {
            "console.log": "log", "logger.info": "log", "print": "log",
            "requests.post": "http", "fetch": "http", "self.client.get": "http",
            "sentry_sdk.capture_exception": "error_tracking", "posthog.capture": "analytics",
            "openai.chat.completions.create": "llm", "stripe.customers.create": "third_party",
            "localStorage.setItem": "storage", "send_mail": "messaging",
            "res.send": None, "response.json": None, "dict.get": None, "events_table.alias": None,
        }
        for callee, want in cases.items():
            self.assertEqual(candidates.classify(callee, False), want, callee)

    def test_globs_follow_piiflow_semantics(self):
        g = candidates.glob_regex("**/tests/**")
        self.assertTrue(g.match("tests/a.py"))
        self.assertTrue(g.match("x/tests/a.py"))
        self.assertFalse(g.match("x/testsuite/a.py"))
        self.assertTrue(candidates.glob_regex("**/*.test.*").match("src/a.test.ts"))
        self.assertIn("**/node_modules/**", candidates.default_excludes())


if __name__ == "__main__":
    unittest.main()

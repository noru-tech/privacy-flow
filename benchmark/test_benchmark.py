#!/usr/bin/env python3
"""Tests for the benchmark scripts' arithmetic and sampling (no network, no corpus needed).

    python3 -m unittest benchmark/test_benchmark.py
"""
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import json  # noqa: E402
import subprocess  # noqa: E402
import tempfile  # noqa: E402

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


class Rendering(unittest.TestCase):
    def test_collapse_keeps_source_and_sink(self):
        hop = lambda line, col: {"path": "a.py", "line": line, "column": col}
        hops = [hop(1, 1), hop(1, 5), hop(2, 1), hop(2, 9), hop(3, 1), hop(3, 4)]
        self.assertEqual(sheets.collapse(hops), [hop(1, 1), hop(2, 1), hop(3, 1), hop(3, 4)])
        self.assertEqual(sheets.collapse([hop(1, 1), hop(1, 2)]), [hop(1, 1), hop(1, 2)])


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


class Importer(unittest.TestCase):
    """An export from the labelling app round-trips into labels score.py reads."""

    HERE = pathlib.Path(__file__).resolve().parent

    def run_import(self, export, labels_dir, *extra):
        path = pathlib.Path(labels_dir) / "export.json"
        path.write_text(json.dumps(export))
        return subprocess.run([sys.executable, str(self.HERE / "labelling/import_labels.py"), str(path),
                               "--labels-dir", str(labels_dir), *extra], capture_output=True, text=True)

    def test_round_trip_and_refusals(self):
        try:
            import yaml  # noqa: F401  (score.load_labels needs it)
        except ImportError:
            self.skipTest("PyYAML not installed")
        sheet = self.HERE / "sheets/taxonomy.items.jsonl"
        if not sheet.exists():
            self.skipTest("sheets not drawn")
        items = [json.loads(l) for l in sheet.read_text().splitlines()]
        flow = next(i for i in items if i["kind"] == "flow")
        site = next(i for i in items if i["kind"] == "site")
        export = {"format": "privacy-flow-labels/1", "reviewer": "R1", "apps": {"taxonomy": {
            "items": {flow["id"]: {"verdict": "tp", "category_ok": "yes", "citation": "a.ts:1", "note": 'says "hi"'},
                      site["id"]: {"is_sink": "yes", "personal": "yes", "categories": ["user.contact.email"], "source": "b.ts:2"}},
            "added_sites": [{"path": "c.ts", "line": 3, "categories": [], "source": ""}]}}}
        with tempfile.TemporaryDirectory() as tmp:
            r = self.run_import(export, tmp)
            self.assertEqual(r.returncode, 0, r.stderr)
            labels, added = score.load_labels(pathlib.Path(tmp) / "R1", "taxonomy")
            self.assertEqual(labels[flow["id"]]["verdict"], "tp")
            self.assertEqual(labels[flow["id"]]["note"], 'says "hi"')
            self.assertEqual(labels[site["id"]]["categories"], ["user.contact.email"])
            self.assertEqual(len(labels), len(items))
            self.assertEqual(added[0]["line"], 3)
            # A smaller export does not replace more work without --force.
            smaller = json.loads(json.dumps(export))
            del smaller["apps"]["taxonomy"]["items"][site["id"]]
            self.assertNotEqual(self.run_import(smaller, tmp).returncode, 0)
            self.assertEqual(self.run_import(smaller, tmp, "--force").returncode, 0)
            # Values outside the protocol stop the import.
            bad = json.loads(json.dumps(export))
            bad["apps"]["taxonomy"]["items"][flow["id"]]["verdict"] = "yes"
            r = self.run_import(bad, tmp, "--force")
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("verdict", r.stderr)


class HeadToHead(unittest.TestCase):
    """Constructed labels with a known answer must come back as that answer."""

    HERE = pathlib.Path(__file__).resolve().parent

    def test_known_answer(self):
        try:
            import yaml  # noqa: F401
        except ImportError:
            self.skipTest("PyYAML not installed")
        if not (self.HERE.parent / ".benchmark-cache/results/taxonomy/flows.json").exists():
            self.skipTest("benchmark applications not fetched and run")
        sys.path.insert(0, str(self.HERE / "labelling"))
        import import_labels
        with tempfile.TemporaryDirectory() as tmp:
            final = pathlib.Path(tmp) / "final"
            final.mkdir()
            for sheet in sorted((self.HERE / "sheets").glob("*.items.jsonl")):
                app = sheet.name.split(".")[0]
                items = [json.loads(l) for l in sheet.read_text().splitlines()]
                key = json.loads((self.HERE / "sheets/key" / f"{app}.json").read_text())["items"]
                labels = {}
                for it in items:
                    if it["kind"] == "flow":
                        real = key[it["id"]]["tool"] == "piiflow"
                        labels[it["id"]] = {"verdict": "tp" if real else "fp_path", "category_ok": "yes"}
                    elif it["kind"] == "gap":
                        labels[it["id"]] = {"hides_flow": "no"}
                    else:
                        labels[it["id"]] = {"is_sink": "yes", "personal": "yes"}
                (final / f"{app}.yml").write_text(import_labels.render(app, items, labels, []))
            out = pathlib.Path(tmp) / "out"
            r = subprocess.run([sys.executable, str(self.HERE / "score.py"), "--labels", tmp, "--out", str(out)],
                               capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            hh = json.loads((out / "scores.json").read_text())["head_to_head"]
            self.assertEqual(hh["precision"]["piiflow"]["p"], 1.0)
            self.assertEqual(hh["precision"]["privado"]["p"], 0.0)
            self.assertNotIn("polar", hh["applications"])
            self.assertNotIn("redash", hh["applications"])
            self.assertEqual(hh["recall"]["piiflow_found"]["n"], hh["recall"]["privado_found"]["n"])


if __name__ == "__main__":
    unittest.main()

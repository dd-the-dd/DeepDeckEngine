import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "compare-native-oracle-audits.py"
SPEC = importlib.util.spec_from_file_location("compare_native_oracle_audits", SCRIPT)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


def ability(text, status="canonical", recognizer="native", rule=None):
    return {
        "text": text,
        "type": "spellAbility",
        "status": status,
        "recognizer": recognizer,
        "rule": rule,
        "diagnostics": [],
        "unknown": [],
    }


def card(oracle_id, name, status, abilities):
    return {
        "oracle_id": oracle_id,
        "name": name,
        "set": "tst",
        "collector_number": oracle_id,
        "status": status,
        "abilities": abilities,
    }


class CompareNativeOracleAuditsTests(unittest.TestCase):
    def compare(self, before, after):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            before_path = root / "before.jsonl"
            after_path = root / "after.jsonl"
            before_path.write_text(
                "".join(json.dumps(row) + "\n" for row in before), encoding="utf-8"
            )
            after_path.write_text(
                "".join(json.dumps(row) + "\n" for row in after), encoding="utf-8"
            )
            return MODULE.compare(before_path, after_path)

    def test_mixed_card_preserves_native_ability_and_reports_new_gain(self):
        stable = ability("Stable.", rule={"kind": "stable"})
        debt = ability("Debt.", status="unsupported", recognizer="unclaimed")
        gained = ability("Debt.", rule={"kind": "gained"})
        report = self.compare(
            [card("a", "Mixed", "unsupported", [stable, debt])],
            [card("a", "Mixed", "canonical", [stable, gained])],
        )
        self.assertTrue(report["passed"])
        self.assertEqual(report["preservedNativeAbilities"], 1)
        self.assertEqual(len(report["gainedNativeAbilities"]), 1)
        self.assertEqual(
            [row["cardName"] for row in report["gainedFullyNativeCards"]], ["Mixed"]
        )

    def test_native_rule_change_is_a_regression_even_if_card_stays_canonical(self):
        report = self.compare(
            [card("a", "Stable", "canonical", [ability("A.", rule={"kind": "one"})])],
            [card("a", "Stable", "canonical", [ability("A.", rule={"kind": "two"})])],
        )
        self.assertFalse(report["passed"])
        self.assertIn("native-rule-changed", [row["kind"] for row in report["regressions"]])
        self.assertEqual(report["comparedPreviouslyNativeAbilities"], 1)
        self.assertEqual(report["preservedNativeAbilities"], 0)

    def test_ability_reorder_is_detected_by_text_and_rule_identity(self):
        first = ability("First.", rule={"kind": "first"})
        second = ability("Second.", rule={"kind": "second"})
        report = self.compare(
            [card("a", "Ordered", "canonical", [first, second])],
            [card("a", "Ordered", "canonical", [second, first])],
        )
        self.assertFalse(report["passed"])
        self.assertIn("ability-text-changed", [row["kind"] for row in report["regressions"]])

    def test_unclaimed_ability_text_and_order_change_is_a_regression(self):
        first = ability("First debt.", status="unsupported", recognizer="unclaimed")
        second = ability("Second debt.", status="unsupported", recognizer="unclaimed")
        promoted_first = ability("First debt.", rule={"kind": "first"})
        promoted_second = ability("Second debt changed.", rule={"kind": "second"})
        report = self.compare(
            [card("a", "Debt", "unsupported", [first, second])],
            [card("a", "Debt", "canonical", [promoted_second, promoted_first])],
        )
        self.assertFalse(report["passed"])
        self.assertIn("ability-text-changed", [row["kind"] for row in report["regressions"]])

    def test_card_order_change_is_a_regression(self):
        first = card("a", "First", "canonical", [])
        second = card("b", "Second", "canonical", [])
        report = self.compare([first, second], [second, first])
        self.assertFalse(report["passed"])
        self.assertIn("card-order-changed", [row["kind"] for row in report["regressions"]])

    def test_lexical_unknown_reduction_is_reported_without_semantic_regression(self):
        before = ability("Attacking.", rule={"kind": "stable"})
        before["unknown"] = ["Attacking"]
        after = ability("Attacking.", rule={"kind": "stable"})
        report = self.compare(
            [card("a", "Lexical", "canonical", [before])],
            [card("a", "Lexical", "canonical", [after])],
        )
        self.assertTrue(report["passed"])
        self.assertEqual(len(report["nativeLexicalChanges"]), 1)


if __name__ == "__main__":
    unittest.main()

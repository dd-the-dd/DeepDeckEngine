#!/usr/bin/env python3
"""Link one preserved normal-card native emission to complete-source runtime DATA.

This tool executes only the pinned Oracle analyzer. It never executes runtime
fixtures, computes unofficial semantic pins, or certifies migration coverage.
"""
from __future__ import annotations

import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


class LinkError(Exception):
    def __init__(self, code: str, message: str):
        super().__init__(message)
        self.code = code


def require(condition: bool, code: str, message: str) -> None:
    if not condition:
        raise LinkError(code, message)


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def strict_equal(left, right) -> bool:
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(
            strict_equal(value, right[key]) for key, value in left.items()
        )
    if isinstance(left, list):
        return len(left) == len(right) and all(
            strict_equal(a, b) for a, b in zip(left, right)
        )
    return left == right


def unique_members(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, "invalid_json", f"Duplicate JSON key: {key}")
        result[key] = value
    return result


def invalid_constant(value):
    raise LinkError("invalid_json", f"Non-JSON numeric constant: {value}")


def decode(text: str):
    return json.loads(
        text, object_pairs_hook=unique_members, parse_constant=invalid_constant
    )


def read_json(path: Path):
    return decode(path.read_text(encoding="utf-8-sig", errors="strict"))


def write_json(path: Path, value) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, allow_nan=False) + "\n",
        encoding="utf-8",
    )


def fixture_paths(values: list[Path]) -> list[Path]:
    found = []
    for value in values:
        path = value.resolve(strict=True)
        found.extend(sorted(path.rglob("*.json")) if path.is_dir() else [path])
    require(bool(found), "no_fixtures", "No fixture JSON files were selected")
    require(
        len(set(found)) == len(found),
        "duplicate_fixture_path",
        "A fixture path was selected more than once",
    )
    return found


def source_aliases(state, alias: str):
    """Read physical source copies in the existing FixturePlayer zone schema."""
    matches = []
    require(isinstance(state, dict) and isinstance(state.get("players"), list),
            "fixture_schema_unsupported", "initialState.players must be an array")
    for player_index, player in enumerate(state["players"]):
        require(isinstance(player, dict), "fixture_schema_unsupported", "Player must be an object")
        for zone in ("library", "hand", "battlefield", "graveyard", "commandZone"):
            cards = player.get(zone, [])
            require(isinstance(cards, list), "fixture_schema_unsupported", f"{zone} must be an array")
            for index, card in enumerate(cards):
                require(isinstance(card, dict), "fixture_schema_unsupported", "Zone card must be an object")
                if card.get("id") == alias:
                    matches.append((f"$.initialState.players[{player_index}].{zone}[{index}]", card))
    return matches


def actual_rule_index(rule, rules, label: str) -> int:
    indexes = [index for index, actual in enumerate(rules) if strict_equal(rule, actual)]
    require(bool(indexes), "fixture_rule_mismatch", f"{label} is not one complete actual rule")
    return indexes[0]


def link_fixture(fixture, card, rules, alias: str):
    require(
        isinstance(fixture, dict)
        and fixture.get("schemaVersion") == "deepdeck-runtime-fixture/v1",
        "fixture_schema_unsupported",
        "Expected deepdeck-runtime-fixture/v1",
    )
    require(
        isinstance(fixture.get("id"), str) and bool(fixture["id"]),
        "fixture_identity_missing", "Fixture needs a nonempty id",
    )
    result = copy.deepcopy(fixture)
    index = actual_rule_index(result.get("rule"), rules, "fixture.rule")
    result["rule"] = copy.deepcopy(rules[index])
    aliases = source_aliases(result.get("initialState"), alias)
    require(bool(aliases), "source_alias_missing", f"No initialState source has id {alias!r}")
    fields = {"name": "name", "typeLine": "type_line", "manaCost": "mana_cost",
              "power": "power", "toughness": "toughness"}
    for path, source in aliases:
        for fixture_key, card_key in fields.items():
            require(
                strict_equal(source.get(fixture_key), card.get(card_key)),
                "source_metadata_mismatch",
                f"{path}.{fixture_key} differs from the complete real source",
            )
        require(
            strict_equal(source.get("rules"), rules),
            "source_rules_incomplete",
            f"{path}.rules must contain ALL actual rules in order",
        )
        source["rules"] = copy.deepcopy(rules)

    overrides = []
    def link_actions(value, path):
        if isinstance(value, dict):
            if value.get("kind") == "executeRule" and "rule" in value:
                override_index = actual_rule_index(value["rule"], rules, path + ".rule")
                value["rule"] = copy.deepcopy(rules[override_index])
                overrides.append({"path": path + ".rule", "actualRuleIndex": override_index})
            for key, child in value.items():
                link_actions(child, f"{path}.{key}")
        elif isinstance(value, list):
            for number, child in enumerate(value):
                link_actions(child, f"{path}[{number}]")
    link_actions(result.get("actions", []), "$.actions")
    return result, {"actualRuleIndex": index, "sourceAliasPaths": [p for p, _ in aliases],
                    "linkedExecuteRuleOverrides": overrides}


def ensure_unchanged(inputs: dict[Path, str]) -> None:
    for path, original in inputs.items():
        require(
            path.is_file() and digest(path) == original,
            "input_changed", f"Input changed during linking: {path}",
        )


def link(args) -> dict:
    started = time.perf_counter()
    source = args.source_input.resolve(strict=True)
    analyzer = args.analyzer.resolve(strict=True)
    expected_path = args.expected_rules.resolve(strict=True)
    fixtures = fixture_paths(args.fixture)
    output = args.output.resolve()
    require(not output.exists(), "output_exists", "Output must be a new directory")
    require(bool(args.source_instance_id), "source_alias_missing", "Source alias must be nonempty")
    input_hashes = {path: digest(path) for path in (source, analyzer, expected_path, *fixtures)}
    require(
        input_hashes[analyzer] == args.expected_analyzer_sha.upper(),
        "analyzer_hash_mismatch", "Analyzer SHA256 does not match its expected pin",
    )
    if args.expected_source_input_sha:
        require(input_hashes[source] == args.expected_source_input_sha.upper(),
                "source_hash_mismatch", "Source input SHA256 does not match its expected pin")
    cards = read_json(source)
    require(isinstance(cards, list), "source_schema_unsupported", "Source must be a Scryfall card array")
    selected = [card for card in cards if isinstance(card, dict) and card.get("oracle_id") == args.oracle_id]
    require(len(selected) == 1, "source_identity_ambiguous", "Expected exactly one exact oracle_id")
    card = selected[0]
    faces = card.get("card_faces")
    require(faces is None or isinstance(faces, list), "source_structure_metadata_invalid",
            "card_faces must be absent, null or an array; source metadata is malformed")
    require(
        card.get("layout") == "normal" and not card.get("card_faces"),
        "tool_layout_unsupported",
        "LINKER TOOL LIMIT: only normal/single cards without card_faces are supported; this is not a card factory diagnostic",
    )
    require(
        all(isinstance(card.get(key), str) and bool(card[key]) for key in ("id", "oracle_id", "name", "type_line")),
        "source_metadata_missing", "Source identity/name/type metadata is incomplete",
    )
    expected = read_json(expected_path)
    require(
        isinstance(expected, list) and bool(expected)
        and all(isinstance(rule, dict) and isinstance(rule.get("kind"), str) for rule in expected),
        "expected_rules_invalid", "Expected rules must be a nonempty complete ordered array",
    )
    originals = [(path, read_json(path)) for path in fixtures]
    output.mkdir(parents=True)
    selected_path = output / "exact-selected-input.json"
    write_json(selected_path, [card])
    require(strict_equal(read_json(selected_path), [card]), "selected_input_changed", "Selection changed source data")
    selected_hash = digest(selected_path)
    inputs = {**input_hashes, selected_path: selected_hash}
    audit_path = output / "actual-native.jsonl"
    env = os.environ.copy()
    env["DEEPDECK_ORACLE_NATIVE_ONLY"] = "1"
    audit_start = time.perf_counter()
    process = subprocess.run([str(analyzer), str(selected_path), str(audit_path)], env=env, capture_output=True)
    audit_seconds = time.perf_counter() - audit_start
    (output / "analyzer.stdout").write_bytes(process.stdout)
    (output / "analyzer.stderr").write_bytes(process.stderr)
    ensure_unchanged(inputs)
    require(process.returncode == 0, "analyzer_failed", f"Analyzer exited {process.returncode}")
    rows = [decode(line) for line in audit_path.read_text(encoding="utf-8-sig", errors="strict").splitlines() if line.strip()]
    require(len(rows) == 1, "audit_identity_mismatch", "Expected exactly one analyzer row")
    row = rows[0]
    require(isinstance(row, dict), "audit_identity_mismatch", "Analyzer row must be an object")
    for key in ("oracle_id", "name", "set", "collector_number"):
        require(strict_equal(row.get(key), card.get(key)), "audit_identity_mismatch", f"Audit {key} differs")
    abilities = row.get("abilities")
    require(
        row.get("status") == "canonical" and isinstance(abilities, list) and bool(abilities)
        and all(isinstance(a, dict) and a.get("status") == "canonical"
                and a.get("recognizer") == "native" and isinstance(a.get("rule"), dict)
                and not a.get("diagnostics")
                for a in abilities),
        "incomplete_native_card", "Every complete ability must be canonical/native",
    )
    rules = [ability["rule"] for ability in abilities]
    require(strict_equal(expected, rules), "expected_rules_mismatch", "Actual complete rules differ in case/type/structure/order")
    write_json(output / "actual-rules.json", rules)
    entries, fixture_ids = [], set()
    # Validate the whole bounded lot before emitting any linked fixture.
    linked = []
    for path, fixture in originals:
        value, details = link_fixture(fixture, card, rules, args.source_instance_id)
        require(value["id"] not in fixture_ids, "duplicate_fixture_identity", "Duplicate fixture id")
        fixture_ids.add(value["id"])
        linked.append((path, value, details))
    ensure_unchanged(inputs)
    for number, (path, value, details) in enumerate(linked, 1):
        target = output / "runtime" / f"{number:03}-{path.name}"
        write_json(target, value)
        entries.append({"sourcePath": str(path), "sourceSHA256": input_hashes[path],
                        "fixtureId": value["id"], "outputPath": str(target),
                        "outputSHA256": digest(target), **details})
    ensure_unchanged(inputs)
    manifest = {"schemaVersion": "deepdeck-native-fixture-link/v1", "status": "LINKED_NOT_RUNTIME_EXECUTED",
                "linkerSHA256": digest(Path(__file__)),
                "oracleId": card["oracle_id"], "cardName": card["name"], "layout": "normal",
                "sourceAlias": args.source_instance_id, "completeRuleCount": len(rules),
                "sourceInputPath": str(source), "sourceInputSHA256": input_hashes[source],
                "selectedInputPath": str(selected_path), "selectedInputSHA256": selected_hash,
                "analyzerPath": str(analyzer), "analyzerSHA256": input_hashes[analyzer],
                "expectedRulesPath": str(expected_path), "expectedRulesSHA256": input_hashes[expected_path],
                "actualNativeOutputSHA256": digest(audit_path), "actualRulesSHA256": digest(output / "actual-rules.json"),
                "fixtureCount": len(entries), "fixtures": entries, "inputHashesStable": True,
                "caseAndTypeExact": True, "analyzerExecutionSeconds": audit_seconds,
                "totalSeconds": time.perf_counter() - started, "runtimeExecuted": False,
                "coverageCertified": False, "cargoExecuted": False}
    write_json(output / "link-manifest.json", manifest)
    return manifest


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(description=__doc__)
    result.add_argument("--source-input", type=Path, required=True)
    result.add_argument("--oracle-id", required=True)
    result.add_argument("--analyzer", type=Path, required=True)
    result.add_argument("--expected-analyzer-sha", required=True)
    result.add_argument("--expected-rules", type=Path, required=True)
    result.add_argument("--fixture", type=Path, nargs="+", action="extend", required=True)
    result.add_argument("--source-instance-id", required=True)
    result.add_argument("--expected-source-input-sha")
    result.add_argument("--output", type=Path, required=True)
    return result


def main(argv=None) -> int:
    try:
        manifest = link(parser().parse_args(argv))
    except (LinkError, OSError, ValueError, UnicodeError) as error:
        print(json.dumps({"status": "failed", "code": getattr(error, "code", "invalid_input"),
                          "message": str(error), "runtimeExecuted": False}), file=sys.stderr)
        return 1
    print(json.dumps({key: manifest[key] for key in ("status", "oracleId", "completeRuleCount", "fixtureCount", "runtimeExecuted")}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

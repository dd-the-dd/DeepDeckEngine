#!/usr/bin/env python3
"""Compare NativeOnly JSONL audits without hiding mixed-card regressions."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = "deepdeck-native-oracle-audit-comparison/v1"


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest().upper()


def value_sha256(value: Any) -> str:
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode(
        "utf-8"
    )
    return hashlib.sha256(encoded).hexdigest().upper()


def card_key(card: dict[str, Any]) -> str:
    if card.get("oracle_id"):
        return str(card["oracle_id"])
    return ":".join(str(card.get(field) or "") for field in ("set", "collector_number", "name"))


def read_audit(path: Path) -> list[dict[str, Any]]:
    cards: list[dict[str, Any]] = []
    seen: set[str] = set()
    with path.open("r", encoding="utf-8") as stream:
        for line_number, line in enumerate(stream, 1):
            if not line.strip():
                continue
            try:
                card = json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"{path}:{line_number}: invalid JSON: {error}") from error
            key = card_key(card)
            if key in seen:
                raise ValueError(f"{path}:{line_number}: duplicate card key {key!r}")
            seen.add(key)
            cards.append(card)
    return cards


def is_native_ability(ability: dict[str, Any]) -> bool:
    return ability.get("status") == "canonical" and ability.get("recognizer") == "native"


def is_fully_native_card(card: dict[str, Any]) -> bool:
    abilities = card.get("abilities") or []
    return card.get("status") == "canonical" and all(is_native_ability(item) for item in abilities)


def ability_identity(card: dict[str, Any], index: int, ability: dict[str, Any]) -> dict[str, Any]:
    return {
        "cardKey": card_key(card),
        "cardName": card.get("name"),
        "abilityIndex": index,
        "text": ability.get("text"),
        "textSha256": value_sha256(ability.get("text")),
        "ruleSha256": value_sha256(ability.get("rule")),
        "diagnosticsSha256": value_sha256(ability.get("diagnostics") or []),
    }


def compare(before_path: Path, after_path: Path) -> dict[str, Any]:
    before_cards = read_audit(before_path)
    after_cards = read_audit(after_path)
    before_by_key = {card_key(card): card for card in before_cards}
    after_by_key = {card_key(card): card for card in after_cards}
    before_order = [card_key(card) for card in before_cards]
    after_order = [card_key(card) for card in after_cards]
    before_positions = {key: index for index, key in enumerate(before_order)}
    after_positions = {key: index for index, key in enumerate(after_order)}

    regressions: list[dict[str, Any]] = []
    missing_cards = [key for key in before_order if key not in after_by_key]
    added_cards = [key for key in after_order if key not in before_by_key]
    regressions.extend({"kind": "card-missing", "cardKey": key} for key in missing_cards)
    regressions.extend({"kind": "card-added", "cardKey": key} for key in added_cards)
    for key in before_order:
        if key in after_positions and before_positions[key] != after_positions[key]:
            regressions.append(
                {
                    "kind": "card-order-changed",
                    "cardKey": key,
                    "beforeIndex": before_positions[key],
                    "afterIndex": after_positions[key],
                }
            )

    compared_previously_native = 0
    preserved_native = 0
    gained_native: list[dict[str, Any]] = []
    gained_cards: list[dict[str, Any]] = []
    lexical_changes: list[dict[str, Any]] = []
    for key in before_order:
        before_card = before_by_key[key]
        after_card = after_by_key.get(key)
        if after_card is None:
            continue
        before_abilities = before_card.get("abilities") or []
        after_abilities = after_card.get("abilities") or []
        if len(before_abilities) != len(after_abilities):
            regressions.append(
                {
                    "kind": "ability-count-changed",
                    "cardKey": key,
                    "cardName": before_card.get("name"),
                    "before": len(before_abilities),
                    "after": len(after_abilities),
                }
            )

        for index, before_ability in enumerate(before_abilities):
            before_is_native = is_native_ability(before_ability)
            if before_is_native:
                compared_previously_native += 1
            after_ability = after_abilities[index] if index < len(after_abilities) else None
            if after_ability is None:
                continue
            identity = ability_identity(before_card, index, before_ability)
            regression_count_before_ability = len(regressions)
            for field, kind in (("text", "ability-text-changed"),):
                before_value = before_ability.get(field)
                after_value = after_ability.get(field)
                if before_value != after_value:
                    regressions.append(
                        {
                            "kind": kind,
                            **identity,
                            "beforeSha256": value_sha256(before_value),
                            "afterSha256": value_sha256(after_value),
                        }
                    )
            if not before_is_native and is_native_ability(after_ability):
                gained_native.append(ability_identity(after_card, index, after_ability))
            if not before_is_native:
                continue
            if not is_native_ability(after_ability):
                regressions.append(
                    {
                        "kind": "native-ability-lost",
                        **identity,
                        "afterStatus": after_ability.get("status"),
                        "afterRecognizer": after_ability.get("recognizer"),
                    }
                )
                continue
            for field, kind in (
                ("type", "ability-type-changed"),
                ("rule", "native-rule-changed"),
                ("diagnostics", "native-diagnostics-changed"),
            ):
                before_value = before_ability.get(field)
                after_value = after_ability.get(field)
                if before_value != after_value:
                    regressions.append(
                        {
                            "kind": kind,
                            **identity,
                            "beforeSha256": value_sha256(before_value),
                            "afterSha256": value_sha256(after_value),
                        }
                    )
            if len(regressions) == regression_count_before_ability:
                preserved_native += 1
            before_unknown = before_ability.get("unknown") or []
            after_unknown = after_ability.get("unknown") or []
            if before_unknown != after_unknown:
                lexical_changes.append(
                    {
                        **identity,
                        "beforeUnknown": before_unknown,
                        "afterUnknown": after_unknown,
                        "beforeSha256": value_sha256(before_unknown),
                        "afterSha256": value_sha256(after_unknown),
                    }
                )

        if is_fully_native_card(before_card) and not is_fully_native_card(after_card):
            regressions.append(
                {"kind": "fully-native-card-regressed", "cardKey": key, "cardName": before_card.get("name")}
            )
        if not is_fully_native_card(before_card) and is_fully_native_card(after_card):
            gained_cards.append(
                {
                    "cardKey": key,
                    "cardName": after_card.get("name"),
                    "set": after_card.get("set"),
                    "collectorNumber": after_card.get("collector_number"),
                }
            )

    return {
        "schemaVersion": SCHEMA_VERSION,
        "passed": not regressions,
        "before": {
            "path": str(before_path),
            "sha256": file_sha256(before_path),
            "cards": len(before_cards),
            "abilities": sum(len(card.get("abilities") or []) for card in before_cards),
        },
        "after": {
            "path": str(after_path),
            "sha256": file_sha256(after_path),
            "cards": len(after_cards),
            "abilities": sum(len(card.get("abilities") or []) for card in after_cards),
        },
        "corpus": {
            "sameCardOrder": before_order == after_order,
            "missingCards": missing_cards,
            "addedCards": added_cards,
        },
        "comparedPreviouslyNativeAbilities": compared_previously_native,
        "preservedNativeAbilities": preserved_native,
        "nativeLexicalChanges": lexical_changes,
        "gainedNativeAbilities": gained_native,
        "gainedFullyNativeCards": gained_cards,
        "regressions": regressions,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        report = compare(args.before, args.after)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    encoded = json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    if args.output:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    else:
        sys.stdout.write(encoded)
    print(
        f"passed={report['passed']}; "
        f"comparedPreviouslyNativeAbilities={report['comparedPreviouslyNativeAbilities']}; "
        f"preservedNativeAbilities={report['preservedNativeAbilities']}; "
        f"gainedNativeAbilities={len(report['gainedNativeAbilities'])}; "
        f"gainedFullyNativeCards={len(report['gainedFullyNativeCards'])}; "
        f"regressions={len(report['regressions'])}",
        file=sys.stderr,
    )
    return 0 if report["passed"] else 2


if __name__ == "__main__":
    raise SystemExit(main())

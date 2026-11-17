//! Suspend (CR 702.62a): complete card compilation into the typed keyword.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(
    name: &str,
    type_line: &str,
    mana_cost: Option<&str>,
    text: &str,
) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: mana_cost.map(ToOwned::to_owned),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rules(parsed: &OracleCardParseResult) -> Vec<&Value> {
    parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.as_ref())
        .collect()
}

fn suspend(count: i64, cost: &str) -> Value {
    json!({
        "kind": "keywordAbility",
        "source": { "kind": "self" },
        "ability": {
            "kind": "suspend",
            "timeCounters": count,
            "cost": { "kind": "payMana", "manaCost": cost },
        },
    })
}

#[test]
fn keldon_halberdier_suspend_with_its_reminder_compiles_to_the_typed_keyword() {
    let parsed = parse(
        "Keldon Halberdier",
        "Creature \u{2014} Human Warrior",
        Some("{4}{R}"),
        "First strike\nSuspend 4\u{2014}{R} (Rather than cast this card from your hand, you may pay {R} and exile it with four time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost. It has haste.)",
    );
    assert_eq!(parsed.status, "canonical", "{:?}", parsed.abilities);
    let rules = rules(&parsed);
    assert_eq!(rules.len(), 2, "{rules:?}");
    assert_eq!(rules[1], &suspend(4, "{R}"));
}

/// Sibling: a noncreature card without a mana cost and a "pay" reminder
/// combined with an ordinary spell effect on a separate line.
#[test]
fn a_manaless_sorcery_keeps_suspend_beside_its_spell_effect() {
    let parsed = parse(
        "Ancestral Vision",
        "Sorcery",
        None,
        "Suspend 4\u{2014}{U} (Rather than cast this card from your hand, pay {U} and exile it with four time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)\nTarget player draws three cards.",
    );
    let rules = rules(&parsed);
    assert!(rules.contains(&&suspend(4, "{U}")), "{rules:?}");
    assert_eq!(rules.len(), 2, "{rules:?}");
}

#[test]
fn near_misses_surface_explicit_unsupported_grammar() {
    for (type_line, text) in [
        // Variable time counters need a casting-X producer.
        (
            "Creature \u{2014} Spirit",
            "Suspend X\u{2014}{X}{3}{U}. X can't be 0.",
        ),
        // A land can never begin to be cast from hand.
        ("Land", "Suspend 2\u{2014}{G}"),
        // The reminder contradicts the printed count.
        (
            "Sorcery",
            "Suspend 4\u{2014}{U} (Rather than cast this card from your hand, pay {U} and exile it with two time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)",
        ),
    ] {
        let parsed = parse("Near Miss", type_line, None, text);
        assert_ne!(parsed.status, "canonical", "{text}");
        assert!(
            !rules(&parsed)
                .iter()
                .any(|rule| rule["ability"]["kind"] == "suspend"),
            "{text}"
        );
    }
}

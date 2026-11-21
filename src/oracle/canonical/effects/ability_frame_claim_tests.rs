use crate::oracle::{OracleCardParseRequest, ParsedOracleAbility, parse_oracle_card_native};
use serde_json::{Value, json};

fn abilities(name: &str, type_line: &str, text: &str) -> Vec<ParsedOracleAbility> {
    parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
    .abilities
}

fn single(name: &str, type_line: &str, text: &str) -> ParsedOracleAbility {
    let mut parsed = abilities(name, type_line, text);
    assert_eq!(parsed.len(), 1, "{name}: one ability");
    parsed.remove(0)
}

/// Each structural introducer claims an ability that no reader completed and
/// reports where its frame stopped, with the token found there (CR 113.3).
#[test]
fn unclaimed_abilities_are_owned_by_their_structural_frame() {
    let cases: &[(&str, &str, &str, &str, &str)] = &[
        // The colon inside a keyword reminder does not make an activated frame.
        (
            "Mobile Garrison",
            "Artifact — Vehicle",
            "Crew 2 (Tap any number of creatures you control with total power 2 or more: This Vehicle becomes an artifact creature until end of turn.)",
            "ability-frame.keyword.expansion",
            "Crew",
        ),
        (
            "Boon Reflection",
            "Enchantment",
            "If you would gain life, you gain twice that much life instead.",
            "ability-frame.replacement.event",
            "gain",
        ),
        (
            "Furnace of Rath",
            "Enchantment",
            "If a source would deal damage to a permanent or player, it deals double that damage to that permanent or player instead.",
            "ability-frame.replacement.event",
            "deal",
        ),
        (
            "Baloth Gorger",
            "Creature — Beast",
            "If this creature was kicked, it enters with three +1/+1 counters on it.",
            "ability-frame.conditional.body",
            "it",
        ),
        (
            "Security Rhox",
            "Creature — Rhino Warrior",
            "You may pay {R}{G} rather than pay this spell's mana cost.",
            "ability-frame.permission.action",
            "pay",
        ),
        // A known subject with an unsupported predicate keeps the static
        // frame's own diagnostic.
        (
            "High Ground",
            "Enchantment",
            "Each creature you control can block an additional creature each combat.",
            "static.continuous.defender-exception",
            "block",
        ),
    ];
    for (name, type_line, text, state, found) in cases {
        let ability = single(name, type_line, text);
        assert_eq!(ability.status, "unsupported", "{name}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.code, "oracle_machine_failure", "{name}");
        assert_eq!(
            diagnostic.state.as_deref(),
            Some(*state),
            "{name}: {diagnostic:?}"
        );
        assert_eq!(
            diagnostic.found.as_deref(),
            Some(*found),
            "{name}: {diagnostic:?}"
        );
    }
}

/// The frame is read from the structure, not from a phrase list: an unseen
/// ability with the same introducer is claimed the same way.
#[test]
fn unseen_frame_members_are_claimed_without_new_rules() {
    let cases: &[(&str, &str, &str)] = &[
        (
            "Creature — Elemental",
            "Rampage 3 (Whenever this creature becomes blocked, it gets +3/+3 until end of turn for each creature blocking it beyond the first.)",
            "ability-frame.keyword.expansion",
        ),
        (
            "Enchantment",
            "If an opponent would draw a card, that player mills a card instead.",
            "ability-frame.replacement.event",
        ),
        (
            "Artifact",
            "As an additional cost to activate abilities, players pay 1 life.",
            "ability-frame.additional-cost.action",
        ),
    ];
    for (type_line, text, state) in cases {
        let ability = single("Generic Frame", type_line, text);
        let diagnostic = &ability.diagnostics[0];
        assert_ne!(diagnostic.code, "unsupported_oracle_ability", "{text}");
        assert_eq!(
            diagnostic.state.as_deref(),
            Some(*state),
            "{text}: {diagnostic:?}"
        );
    }
}

/// Near-miss: a keyword noun heading a sentence is not a keyword line.
#[test]
fn keyword_noun_subject_is_not_a_keyword_line() {
    let ability = single(
        "Generic Gale",
        "Enchantment",
        "Flying creatures you control can block an additional creature each combat.",
    );
    let state = ability.diagnostics[0].state.clone().unwrap_or_default();
    assert_ne!(state, "ability-frame.keyword.expansion", "{state}");
}

/// Claims never accept: a completed reader keeps its exact rule.
#[test]
fn completed_readers_keep_precedence_over_the_frame_claim() {
    let ability = single("Grizzled Outrider", "Creature — Elf Warrior", "Flying");
    assert_eq!(ability.status, "canonical");
    assert_eq!(
        ability.rule.as_ref().map(|rule| rule["ability"].clone()),
        Some(json!({"kind": "flying"}))
    );
    let _: Option<Value> = ability.rule;
}

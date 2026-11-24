use serde_json::Value;

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Keyword Action Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{G}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn kinds(value: &Value, found: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(kind) = object.get("kind").and_then(Value::as_str) {
                found.push(kind.to_string());
            }
            object.values().for_each(|value| kinds(value, found));
        }
        Value::Array(values) => values.iter().for_each(|value| kinds(value, found)),
        _ => {}
    }
}

fn canonical_kinds(type_line: &str, text: &str) -> Vec<String> {
    let parsed = parse(type_line, text);
    assert_eq!(parsed.status, "canonical", "{text}\n{parsed:#?}");
    let mut found = Vec::new();
    for ability in &parsed.abilities {
        kinds(ability.rule.as_ref().expect("a rule"), &mut found);
    }
    found
}

fn unsupported(type_line: &str, text: &str) -> OracleCardParseResult {
    let parsed = parse(type_line, text);
    assert_ne!(parsed.status, "canonical", "{text}\n{parsed:#?}");
    parsed
}

/// Real cards: each verb reaches its primitive program and never a
/// keyword-action opcode.
#[test]
fn real_keyword_action_cards_lower_to_primitive_programs() {
    for (type_line, text, required) in [
        (
            "Creature - Merfolk Scout",
            "When this creature enters, it explores. (Reveal the top card of your library. Put that card into your hand if it's a land. Otherwise, put a +1/+1 counter on this creature, then put the card back or put it into your graveyard.)",
            &["lookAtTopCards", "revealCards", "emitAbilitySignal"][..],
        ),
        (
            "Creature - Bird Advisor",
            "Flying\nWhenever a player casts their second spell each turn, this creature connives. (Draw a card, then discard a card. If you discarded a nonland card, put a +1/+1 counter on this creature.)",
            &["drawCards", "discardCards", "filterObjects", "emitAbilitySignal"][..],
        ),
        (
            "Creature - Dog Soldier",
            "When this creature enters, it endures 1. (Put a +1/+1 counter on it or create a 1/1 white Spirit creature token.)",
            &["chooseOneEffect", "putCounters", "createTokens"][..],
        ),
        (
            "Creature - Fish Crab",
            "{2}{U}: Adapt 2. (If this creature has no +1/+1 counters on it, put two +1/+1 counters on it.)",
            &["countCounters", "putCounters"][..],
        ),
        (
            "Creature - Snake",
            "Reach\n{6}{G}: Monstrosity 4. (If this creature isn't monstrous, put four +1/+1 counters on it and it becomes monstrous.)",
            &["sourceFlagSet", "setPermanentFlag"][..],
        ),
        (
            "Creature - Construct",
            "Indestructible\nWhen this creature enters, suspect it. (It has menace and can't block.)",
            &["suspectPermanent"][..],
        ),
        (
            "Sorcery",
            "Detain up to two target creatures your opponents control. (Until your next turn, those creatures can't attack or block and their activated abilities can't be activated.)",
            &["forEachObject", "grantKeyword", "prohibitPermanentActivations"][..],
        ),
        (
            "Enchantment",
            "At the beginning of your upkeep, populate. (Create a token that's a copy of a creature token you control.)",
            &["choosePermanents", "createTokens"][..],
        ),
        (
            "Creature - Human Scout",
            "When this creature enters, investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")",
            &["createTokens"][..],
        ),
        (
            "Creature - Kor Knight Ally",
            "When this creature enters, support 3. (Put a +1/+1 counter on each of up to three other target creatures.)",
            &["putCounters"][..],
        ),
        (
            "Sorcery",
            "Discover 4. (Exile cards from the top of your library until you exile a nonland card with mana value 4 or less. Cast it without paying its mana cost or put it into your hand. Put the rest on the bottom in a random order.)",
            &["exileFromTopUntil", "castOneCard", "moveCards"][..],
        ),
        (
            "Sorcery",
            "Manifest the top two cards of your library. (To manifest a card, put it onto the battlefield face down as a 2/2 creature. Turn it face up any time for its mana cost if it's a creature card.)",
            &["libraryTopCards", "moveCards"][..],
        ),
        (
            "Creature - Rat",
            "When this creature dies, manifest dread. (Look at the top two cards of your library. Put one onto the battlefield face down as a 2/2 creature and the other into your graveyard. Turn it face up any time for its mana cost if it's a creature card.)",
            &["lookAtTopCards", "chooseCards", "setDifference"][..],
        ),
        (
            "Creature - Bird Soldier",
            "Flying
When this creature enters, bolster 1. (Choose a creature with the least toughness among creatures you control and put a +1/+1 counter on it.)",
            &["choosePermanents", "putCounters"][..],
        ),
    ] {
        let found = canonical_kinds(type_line, text);
        for kind in required {
            assert!(found.iter().any(|found| found == kind), "{kind} in {text}: {found:?}");
        }
        for opcode in ["explore", "connive", "endure", "adapt", "monstrosity", "populate", "support"] {
            assert!(!found.iter().any(|found| found == opcode), "{opcode} in {text}");
        }
    }
}

/// Sibling combinations: a quantity expression, a declared target subject,
/// a coordinated verb and an unparameterized variant reuse the same children.
#[test]
fn keyword_actions_recombine_with_shared_subjects_and_quantities() {
    let found = canonical_kinds(
        "Creature - Spirit",
        "When this creature enters, target creature you control connives 2.",
    );
    for kind in [
        "chooseTargets",
        "drawCards",
        "discardCards",
        "emitAbilitySignal",
    ] {
        assert!(found.iter().any(|found| found == kind), "{kind}: {found:?}");
    }
    canonical_kinds(
        "Creature - Bat",
        "Whenever this creature attacks, you lose 1 life and this creature endures 2.",
    );
    canonical_kinds(
        "Sorcery",
        "Support 2. (Put a +1/+1 counter on each of up to two target creatures.)\nDraw a card.",
    );
    let parsed = parse(
        "Sorcery",
        "Support 2. (Put a +1/+1 counter on each of up to two target creatures.)",
    );
    let rule = parsed.abilities[0].rule.as_ref().expect("support rule");
    assert!(
        rule["declaration"]["decisions"][0]["candidates"]
            .get("excludeSource")
            .is_none_or(|exclude| exclude == false),
        "a spell's support may target any creature: {rule:#}"
    );
}

/// Near misses keep explicit failures instead of approximations.
#[test]
fn unsupported_keyword_action_variants_fail_explicitly() {
    // Simultaneous explorations need the CR 701.44d ordering choice.
    unsupported("Sorcery", "Each creature you control explores.");
    // Adapt names `this permanent`; a spell has none.
    unsupported("Instant", "Adapt 2.");
    // A support quantity is a literal target cardinality.
    unsupported("Sorcery", "Support X.");
    // Learn needs a tracked outside-the-game movement source.
    unsupported(
        "Creature - Eye Pest",
        "When this creature dies, learn. (You may reveal a Lesson card you own from outside the game and put it into your hand, or discard a card to draw a card.)",
    );
    // Endure requires its quantity.
    unsupported(
        "Creature - Spirit",
        "When this creature enters, it endures.",
    );
    // The exile-until threshold of discover is a literal.
    unsupported(
        "Creature - Dinosaur",
        "When this creature enters, discover X, where X is the number of creatures you control.",
    );
}

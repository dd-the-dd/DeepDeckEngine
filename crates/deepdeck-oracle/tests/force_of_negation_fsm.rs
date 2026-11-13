use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const ALTERNATIVE_COST: &str = "If it's not your turn, you may exile a blue card from your hand rather than pay this spell's mana cost.";
const COUNTER_REPLACEMENT: &str = "Counter target noncreature spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard.";

fn parse_card(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{1}{U}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn exact_card_is_native_and_lowers_to_independent_primitive_rules() {
    let parsed = parse_card(
        "Force of Negation",
        &format!("{ALTERNATIVE_COST}\n{COUNTER_REPLACEMENT}"),
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": {
                "kind": "alternativeCost",
                "condition": { "kind": "notYourTurn" },
                "costs": [{
                    "kind": "exileCard",
                    "zone": {
                        "kind": "hand",
                        "player": {
                            "kind": "controllerOf",
                            "object": { "kind": "self" },
                        },
                    },
                    "where": { "kind": "colorContains", "value": "blue" },
                }],
            },
        }))
    );
    let counter = parsed.abilities[1].rule.as_ref().expect("counter rule");
    assert_eq!(counter["effects"][0]["kind"], "counterSpell");
    assert_eq!(counter["effects"][0]["exileInstead"], true);
    assert_eq!(
        counter["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "not",
            "operand": { "kind": "cardTypeContains", "value": "Creature" },
        })
    );
}

#[test]
fn sibling_colors_and_spell_qualifiers_reuse_the_same_automata() {
    let alternative = parse_card(
        "Sibling Alternative",
        "If it's not your turn, you may exile a red card from your hand rather than pay this spell's mana cost.",
    );
    assert_eq!(
        alternative.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        alternative.abilities[0].rule.as_ref().unwrap()["ability"]["costs"][0]["where"]["value"],
        "red"
    );

    let counter = parse_card(
        "Sibling Counter",
        "Counter target creature spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard.",
    );
    assert_eq!(
        counter.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        counter.abilities[0].rule.as_ref().unwrap()["effects"][0]["exileInstead"],
        true
    );
}

#[test]
fn malformed_family_members_fail_in_the_responsible_native_state() {
    for (oracle_text, automaton, state, found) in [
        (
            "If it's not your turn you may exile a blue card from your hand rather than pay this spell's mana cost.",
            "alternative-mana-cost",
            "condition.not-your-turn.separator",
            "you",
        ),
        (
            "Counter target noncreature spell. If that spell is countered, exile it instead of putting it into its owner's graveyard.",
            "counter-destination-replacement",
            "sequence.counter-replacement.manner",
            ",",
        ),
    ] {
        let parsed = parse_card("Malformed Family Member", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

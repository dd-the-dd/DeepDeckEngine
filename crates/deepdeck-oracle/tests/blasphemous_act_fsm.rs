use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const BLASPHEMOUS_ACT_ORACLE: &str = "This spell costs {1} less to cast for each creature on the battlefield.\nBlasphemous Act deals 13 damage to each creature.";

fn parse_sorcery(card_name: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn blasphemous_act_is_fully_native_and_primitive() {
    let parsed = parse_sorcery("Blasphemous Act", "{8}{R}", BLASPHEMOUS_ACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "stackOrCast" },
            },
            "modifiers": [{
                "kind": "reduceOwnGenericCastingCost",
                "amount": {
                    "kind": "countPermanents",
                    "allPlayers": true,
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [],
            },
            "effects": [{
                "kind": "dealDamage",
                "source": { "kind": "self" },
                "amount": { "kind": "integer", "value": 13 },
                "recipient": {
                    "kind": "eachPermanent",
                    "where": { "kind": "cardTypeContains", "value": "Creature" },
                },
            }],
        }))
    );
}

#[test]
fn amount_selector_and_damage_collection_are_generic() {
    let reduction = parse_sorcery(
        "Test Reduction",
        "{6}",
        "This spell costs {2} less to cast for each artifact on the battlefield.",
    );
    assert_eq!(reduction.status, "canonical");
    assert_eq!(
        reduction.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        reduction.abilities[0].rule.as_ref().unwrap()["modifiers"][0]["amount"],
        json!({
            "kind": "multiply",
            "left": { "kind": "integer", "value": 2 },
            "right": {
                "kind": "countPermanents",
                "allPlayers": true,
                "where": { "kind": "cardTypeContains", "value": "Artifact" },
            },
        })
    );

    let damage = parse_sorcery(
        "Example Blast",
        "{4}{R}",
        "Example Blast deals 4 damage to each planeswalker.",
    );
    assert_eq!(damage.status, "canonical");
    assert_eq!(damage.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        damage.abilities[0].rule.as_ref().unwrap()["effects"][0]["recipient"],
        json!({
            "kind": "eachPermanent",
            "where": { "kind": "cardTypeContains", "value": "Planeswalker" },
        })
    );
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_local_state() {
    for (oracle_text, automaton, state, found) in [
        (
            "This spell costs {R} less to cast for each creature on the battlefield.",
            "permanent-scaled-casting-reduction",
            "static.spell-cost-reduction.cost",
            Some("less"),
        ),
        (
            "This spell costs {1} less to cast for some creature on the battlefield.",
            "permanent-scaled-casting-reduction",
            "casting-reduction.direction",
            Some("some"),
        ),
        (
            "This spell costs {1} less to cast for each creature in your graveyard.",
            "permanent-scaled-casting-reduction",
            "query.zone.domain",
            Some("."),
        ),
        (
            "Example Blast deals 4 damage to each artifact.",
            "plain-damage-effect",
            "effect.damage.collection.validation",
            Some("."),
        ),
        (
            "Example Blast deals 4 damage to every creature.",
            "plain-damage-effect",
            "target-list.slot.start",
            Some("every"),
        ),
    ] {
        let parsed = parse_sorcery("Example Blast", "{4}{R}", oracle_text);
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        assert_eq!(parsed.abilities[0].status, "unsupported");
        let diagnostic = &parsed.abilities[0].diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state), "{oracle_text}");
        assert_eq!(diagnostic.found.as_deref(), found, "{oracle_text}");
    }
    // `every` is the same distributive quantifier as `each` in the shared
    // counted query; it lowers to the identical reduction.
    let each = parse_sorcery(
        "Example Blast",
        "{4}{R}",
        "This spell costs {1} less to cast for each creature on the battlefield.",
    );
    let every = parse_sorcery(
        "Example Blast",
        "{4}{R}",
        "This spell costs {1} less to cast for every creature on the battlefield.",
    );
    assert_eq!(every.status, "canonical");
    assert!(each.abilities[0].rule.is_some());
    assert_eq!(every.abilities[0].rule, each.abilities[0].rule);
}

use deepdeck_oracle::oracle::{OracleCardParseRequest, RecognizerProvenance, parse_oracle_card};
use serde_json::json;

fn parse(card_name: &str, oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Sorcery".to_string(),
        mana_cost: Some("{4}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn current_card_name_at_token_zero_outweighs_its_keyword_homonym() {
    let parsed = parse(
        "Awaken the Inferno",
        "Awaken the Inferno deals 6 damage to target creature or planeswalker an opponent controls. Put a +1/+1 counter on up to one target creature you control.\nBasic landcycling {2} ({2}, Discard this card: Search your library for a basic land card, reveal it, put it into your hand, then shuffle.)",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.ability_type, "spellAbility");
    assert_eq!(ability.recognizer, RecognizerProvenance::Legacy);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    {
                        "id": "targetPermanent",
                        "kind": "chooseTargets",
                        "minimum": 1,
                        "maximum": 1,
                        "candidates": {
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                            },
                            "where": {
                                "kind": "or",
                                "operands": [
                                    { "kind": "cardTypeContains", "value": "Creature" },
                                    { "kind": "cardTypeContains", "value": "Planeswalker" },
                                ],
                            },
                        },
                    },
                    {
                        "id": "targetPermanent2",
                        "kind": "chooseTargets",
                        "minimum": 0,
                        "maximum": 1,
                        "candidates": {
                            "kind": "permanents",
                            "controller": { "kind": "controllerOf", "object": { "kind": "self" } },
                            "where": { "kind": "cardTypeContains", "value": "Creature" },
                        },
                    },
                ],
            },
            "effects": [
                {
                    "kind": "dealDamage",
                    "source": { "kind": "self" },
                    "amount": { "kind": "integer", "value": 6 },
                    "recipient": { "kind": "chosenTarget", "id": "targetPermanent" },
                },
                {
                    "kind": "putCounters",
                    "permanent": { "kind": "chosenTarget", "id": "targetPermanent2" },
                    "counter": "+1/+1",
                    "count": { "kind": "integer", "value": 1 },
                },
            ],
        }))
    );
    assert_eq!(parsed.abilities[1].ability_type, "keywordAbility");
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["ability"]["kind"],
        "typecycling"
    );
}

#[test]
fn another_current_name_beginning_with_awaken_is_a_damage_subject() {
    let parsed = parse(
        "Awaken the Furnace",
        "Awaken the Furnace deals 2 damage to target creature.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.ability_type, "spellAbility");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref().unwrap()["effects"][0]["kind"],
        "dealDamage"
    );
}

#[test]
fn actual_awaken_keyword_without_a_current_name_link_remains_a_keyword() {
    let parsed = parse(
        "Test Spell",
        "Awaken 4—{4}{W} (If you cast this spell for {4}{W}, also put four +1/+1 counters on target land you control and it becomes a 0/0 Elemental creature with haste. It's still a land.)",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.ability_type, "keywordAbility");
    assert_eq!(ability.recognizer, RecognizerProvenance::Legacy);
    assert_eq!(ability.rule.as_ref().unwrap()["ability"]["kind"], "awaken");
}

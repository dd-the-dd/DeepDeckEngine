use serde_json::json;

use super::damage_effect_supported;

#[test]
fn accepts_each_direct_damage_shape_owned_by_the_family() {
    let effects = [
        json!({
            "kind": "dealDamage",
            "amount": { "kind": "integer", "value": 3 },
            "recipient": { "kind": "chosenTarget", "id": "target" },
            "bindExcessAs": "excess",
            "bindRecipientAs": "recipient",
        }),
        json!({
            "kind": "dealDividedDamage",
            "targetsDecisionId": "targets",
            "divisionDecisionId": "division",
        }),
        json!({
            "kind": "dealDamageToEachOpponent",
            "amount": 2,
        }),
        json!({
            "kind": "dealDamageToEachOpponentAndCreaturesTheyControl",
            "amount": 2,
        }),
        json!({
            "kind": "dealDamageToEachOpponentAndPlaneswalker",
            "amount": 2,
        }),
        json!({
            "kind": "dealDamageToEachCreatureExceptControlledTokens",
            "player": { "kind": "abilityController" },
            "amount": 2,
        }),
        json!({
            "kind": "dealDamageByPermanentCountEachPlayer",
            "where": { "kind": "cardTypeContains", "value": "Creature" },
            "factor": 2,
        }),
        json!({
            "kind": "chosenPermanentsDealPowerDamage",
            "sources": { "kind": "chosenObjects", "id": "sources" },
            "recipient": { "kind": "chosenTarget", "id": "recipient" },
        }),
    ];

    for effect in effects {
        assert_eq!(damage_effect_supported(&effect), Some(true), "{effect}");
    }
}

#[test]
fn rejects_recognized_damage_kinds_with_invalid_shapes() {
    let effects = [
        json!({
            "kind": "dealDamage",
            "amount": "three",
            "recipient": { "kind": "chosenTarget", "id": "target" },
        }),
        json!({
            "kind": "dealDividedDamage",
            "targetsDecisionId": "targets",
        }),
        json!({
            "kind": "dealDamageByPermanentCountEachPlayer",
            "where": { "kind": "cardTypeContains", "value": "Creature" },
            "factor": 0,
        }),
    ];

    for effect in effects {
        assert_eq!(damage_effect_supported(&effect), Some(false), "{effect}");
    }
}

#[test]
fn declines_effect_kinds_owned_by_other_validation_families() {
    assert_eq!(
        damage_effect_supported(&json!({
            "kind": "drawCards",
            "player": { "kind": "abilityController" },
            "count": 1,
        })),
        None
    );
}

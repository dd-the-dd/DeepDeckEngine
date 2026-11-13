use serde_json::json;

use super::token_copy_effect_supported;

#[test]
fn accepts_generic_token_copy_primitives() {
    let effects = [
        json!({
            "kind": "createTokenCopy",
            "controller": { "kind": "abilityController" },
            "object": { "kind": "self" },
            "basePower": 3,
            "baseToughness": 3,
        }),
        json!({
            "kind": "createTokenCopyOfPermanent",
            "object": { "kind": "chosenTarget", "id": "permanent" },
            "grantKeywords": ["flying"],
            "exileAtNextEndStep": false,
            "quantity": 2,
            "removeLegendary": true,
        }),
        json!({
            "kind": "createModifiedTokenCopy",
            "object": { "kind": "chosenTarget", "id": "permanent" },
            "basePower": 4,
            "baseToughness": 4,
            "addColors": ["G"],
            "addTypes": ["Artifact"],
            "addSubtypes": ["Food"],
            "grantKeywords": ["trample"],
            "sacrificeAtNextEndStep": true,
            "counters": [{ "counter": "+1/+1", "count": 1 }],
        }),
    ];

    for effect in effects {
        assert_eq!(token_copy_effect_supported(&effect), Some(true), "{effect}");
    }
}

#[test]
fn rejects_invalid_shapes_for_owned_token_primitives() {
    let effects = [
        json!({
            "kind": "createTokenCopy",
            "controller": { "kind": "abilityController" },
            "object": { "kind": "self" },
            "basePower": 3,
        }),
        json!({
            "kind": "createTokenCopyOfPermanent",
            "object": { "kind": "chosenTarget", "id": "permanent" },
        }),
        json!({
            "kind": "createModifiedTokenCopy",
            "object": { "kind": "chosenTarget", "id": "permanent" },
            "grantKeywords": ["notAKeyword"],
        }),
    ];

    for effect in effects {
        assert_eq!(
            token_copy_effect_supported(&effect),
            Some(false),
            "{effect}"
        );
    }
}

#[test]
fn declines_composite_and_card_specific_token_operations() {
    for kind in [
        "createTokens",
        "exileTopCardFaceDownAndCreateLinkedToken",
        "exileTargetCardThenCreateTokenCopy",
        "createDinoDnaToken",
        "resolveIdentityEcho",
    ] {
        assert_eq!(token_copy_effect_supported(&json!({ "kind": kind })), None);
    }
}

use serde_json::json;

use super::token_copy_effect_supported;

fn copy(object: serde_json::Value, except: serde_json::Value) -> serde_json::Value {
    json!({
        "kind": "createTokens",
        "controller": { "kind": "abilityController" },
        "quantity": { "kind": "integer", "value": 1 },
        "token": { "kind": "copyOf", "object": object, "except": except },
    })
}

#[test]
fn accepts_token_copies_of_each_object_form_with_copiable_exceptions() {
    let effects = [
        copy(
            json!({ "kind": "self" }),
            json!([{ "kind": "setBasePowerToughness",
                "power": { "kind": "integer", "value": 1 },
                "toughness": { "kind": "integer", "value": 1 } }]),
        ),
        copy(
            json!({ "kind": "chosenTarget", "id": "permanent" }),
            json!([
                { "kind": "removeSupertype", "supertype": "Legendary" },
                { "kind": "grantKeyword", "keyword": "haste" },
                { "kind": "addSubtypes", "subtypes": ["Nightmare"] },
                { "kind": "addCardType", "cardType": "Artifact" },
                { "kind": "setColors", "colors": ["G"] },
            ]),
        ),
        copy(
            json!({ "kind": "boundObjects", "binding": "copied" }),
            json!([]),
        ),
    ];
    for effect in effects {
        assert_eq!(token_copy_effect_supported(&effect), Some(true), "{effect}");
    }
}

#[test]
fn rejects_invalid_copy_exceptions_and_missing_shapes() {
    let mut no_except = copy(json!({ "kind": "self" }), json!([]));
    no_except["token"].as_object_mut().unwrap().remove("except");
    let effects = [
        copy(
            json!({ "kind": "self" }),
            json!([{ "kind": "grantKeyword", "keyword": "notAKeyword" }]),
        ),
        copy(
            json!({ "kind": "self" }),
            json!([{ "kind": "setColors", "colors": ["purple"] }]),
        ),
        copy(
            json!({ "kind": "self" }),
            json!([{ "kind": "setBasePowerToughness", "power": 1 }]),
        ),
        no_except,
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
fn declines_described_tokens_and_retired_recipe_kinds() {
    assert_eq!(
        token_copy_effect_supported(&json!({
            "kind": "createTokens",
            "token": { "power": 1, "toughness": 1 },
        })),
        None
    );
    // The former recipe kinds are no longer token primitives: the generic
    // validator refuses them as unknown effects.
    for kind in [
        "createTokenCopy",
        "createTokenCopyOfPermanent",
        "createModifiedTokenCopy",
    ] {
        assert_eq!(token_copy_effect_supported(&json!({ "kind": kind })), None);
    }
}

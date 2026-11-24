//! The shared written token description (CR 111.4): optional stats,
//! noncreature subtypes, modifiers after the noun in any order, given names,
//! and coordinated lists of independent descriptions.
use super::*;
use crate::oracle::canonical::{TargetlessDeclarationSupport, lower_explicit_token_creations};
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};
use serde_json::{Value, json};

fn try_create(text: &str) -> (TokenCreationAttempt, Option<String>) {
    let mut tokens = recognize_oracle_entities(text);
    let index = ExactNameTokenIndex::from_names(crate::card_catalog::named_token_names().unwrap());
    resolve_named_token_spans(&mut tokens, &index);
    let mut cursor = OracleTokenCursor::new("test.token.descriptor", &tokens);
    let attempt = try_recognize_token_creation_action(&mut cursor, None);
    (attempt, cursor.peek().map(ToOwned::to_owned))
}

/// The lowered creations of one complete sentence.
fn lowered(text: &str) -> Vec<Value> {
    let (attempt, next) = try_create(text);
    let TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(node)) = attempt
    else {
        panic!("a written description must be recognized: {text}")
    };
    // A sentence ending in a quoted ability closes with the quote's own
    // period, so the description reads to the end of the text.
    let expected_tail = if text.ends_with('"') { None } else { Some(".") };
    assert_eq!(next.as_deref(), expected_tail, "{text}");
    lower_explicit_token_creations(&node, TargetlessDeclarationSupport::XValue)
        .unwrap_or_else(|error| panic!("{text}: {error:?}"))
}

fn failure_state(text: &str) -> String {
    match try_create(text).0 {
        TokenCreationAttempt::FailedCommitted(failure) => failure.state.to_string(),
        TokenCreationAttempt::Recognized(node) => panic!("{text} must fail, got {node:?}"),
        TokenCreationAttempt::NotClaimed => panic!("{text} must be claimed"),
    }
}

#[test]
fn symbolic_stats_are_split_and_read_as_numeric_expressions() {
    let effects = lowered("Create an X/X green Ooze creature token.");
    let power = &effects[0]["token"]["power"];
    assert!(
        power.get("kind").is_some() && power["kind"] != json!("integer"),
        "{power}"
    );
    assert_eq!(&effects[0]["token"]["toughness"], power);
}

#[test]
fn noncreature_descriptions_need_no_stats_and_keep_noncreature_subtypes() {
    let effects = lowered(
        "Create a tapped Planet land token named New Planet with \"{T}: Add one mana of any color.\"",
    );
    let token = &effects[0]["token"];
    assert_eq!(token["types"], json!(["Land"]));
    assert_eq!(token["subtypes"], json!(["Planet"]));
    assert_eq!(token["name"], json!("New Planet"));
    assert!(token.get("power").is_none());
    assert_eq!(effects[0]["tapped"], json!(true));
    assert_eq!(token["abilities"].as_array().map(Vec::len), Some(1));
    let effects = lowered("Create a colorless artifact token named Scrap.");
    assert_eq!(effects[0]["token"]["colors"], json!([]));
    assert_eq!(effects[0]["token"]["name"], json!("Scrap"));
    let effects = lowered("Create a Forest land token.");
    assert_eq!(effects[0]["token"]["subtypes"], json!(["Forest"]));
}

#[test]
fn a_creature_without_stats_requires_its_characteristic_defining_ability() {
    let effects = lowered(
        "Create a green and white Elemental creature token with \"This token's power and toughness are each equal to the number of creatures you control.\"",
    );
    assert!(effects[0]["token"].get("power").is_none());
    assert_eq!(
        effects[0]["token"]["abilities"][0]["characteristicDefining"],
        json!(true)
    );
    assert_eq!(
        failure_state("Create a green Elemental creature token with flying."),
        "token.creation.characteristic-defining-statistics"
    );
}

#[test]
fn modifiers_after_the_noun_compose_in_any_order() {
    // Not written on any card: a recombination of the given name, a keyword
    // list and the attacking entry clause.
    let effects = lowered(
        "Create a 2/2 red Elemental creature token named Ember with haste that's tapped and attacking.",
    );
    assert_eq!(effects[0]["token"]["name"], json!("Ember"));
    assert_eq!(effects[0]["token"]["abilities"], json!([{"kind": "haste"}]));
    assert_eq!(effects[0]["tapped"], json!(true));
    assert_eq!(effects[0]["attacking"], json!(true));
    let effects = lowered(
        "Create a 2/1 colorless Construct artifact creature token with flying named Ballistic Boulder that's tapped and attacking.",
    );
    assert_eq!(effects[0]["token"]["name"], json!("Ballistic Boulder"));
    let effects = lowered("Create a 0/1 red Kobold creature token named Kobolds of Kher Keep.");
    assert_eq!(effects[0]["token"]["name"], json!("Kobolds of Kher Keep"));
}

#[test]
fn a_given_name_colliding_with_a_catalog_identity_stays_a_written_name() {
    let effects = lowered("Create a 1/1 colorless Thopter artifact creature token named Treasure.");
    assert_eq!(effects[0]["token"]["name"], json!("Treasure"));
    assert_eq!(effects[0]["token"]["subtypes"], json!(["Thopter"]));
    assert!(effects[0]["token"].get("kind").is_none());
}

#[test]
fn a_coordinated_list_is_one_creation_per_description() {
    let effects = lowered(
        "Create a 1/1 green Snake creature token, a 2/2 green Wolf creature token, and a 3/3 green Elephant creature token.",
    );
    let stats = effects
        .iter()
        .map(|effect| {
            (
                effect["token"]["subtypes"][0].as_str().unwrap().to_string(),
                effect["token"]["power"]["value"].as_i64().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        stats,
        [
            ("Snake".to_string(), 1),
            ("Wolf".to_string(), 2),
            ("Elephant".to_string(), 3),
        ]
    );
    let effects = lowered(
        "Create a 1/1 white Spirit creature token with flying and a 2/2 white Cat creature token.",
    );
    assert_eq!(effects.len(), 2);
}

#[test]
fn an_unsupported_conjunct_fails_the_whole_list() {
    let (attempt, _) = try_create(
        "Create a 1/1 green Snake creature token, a 2/2 green Wolf creature token, and a 3/3 green Inventedling creature token.",
    );
    assert!(matches!(attempt, TokenCreationAttempt::FailedCommitted(_)));
    // A connector followed by another action is not a conjunct.
    let (attempt, next) = try_create("Create a 1/1 white Soldier creature token and draw a card.");
    assert!(matches!(
        attempt,
        TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(_))
    ));
    assert_eq!(next.as_deref(), Some("and"));
}

#[test]
fn a_catalog_identity_is_not_claimed_by_the_written_description() {
    let (attempt, _) = try_create("Create a Treasure token.");
    assert!(matches!(
        attempt,
        TokenCreationAttempt::Recognized(LocalEffectNode::CreateNamedTokens(_))
    ));
}

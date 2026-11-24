use super::*;
use crate::oracle::canonical::{
    MachineFailure, TargetlessDeclarationSupport, lower_explicit_token_creation,
    lower_named_token_creation,
};
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};
use serde_json::json;

fn create(text: &str) -> Result<serde_json::Value, MachineFailure> {
    let mut tokens = recognize_oracle_entities(text);
    let index = ExactNameTokenIndex::from_names(crate::card_catalog::named_token_names().unwrap());
    resolve_named_token_spans(&mut tokens, &index);
    let mut cursor = OracleTokenCursor::new("test.token.entry-state", &tokens);
    let node = match try_recognize_token_creation_action(&mut cursor, None) {
        TokenCreationAttempt::Recognized(node) => node,
        TokenCreationAttempt::FailedCommitted(failure) => return Err(failure),
        TokenCreationAttempt::NotClaimed => panic!("create must be claimed"),
    };
    cursor.consume(".").expect("sentence terminator remains");
    cursor.finish().expect("phrase fully consumed");
    Ok(match node {
        LocalEffectNode::CreateExplicitTokens(node) => {
            lower_explicit_token_creation(&node, TargetlessDeclarationSupport::None)
                .expect("explicit token lowers")
        }
        LocalEffectNode::CreateNamedTokens(node) => {
            let mut lowering = OracleTokenCursor::new("test.token.lowering", &tokens);
            lower_named_token_creation(
                &mut lowering,
                &node.creation,
                json!({"kind":"controller"}),
                false,
            )
            .expect("named token lowers")
        }
        _ => panic!("unexpected token creation node"),
    })
}

#[test]
fn descriptive_token_carries_tapped_beside_its_characteristics() {
    let effect = create("Create a tapped 2/2 colorless Robot artifact creature token.").unwrap();
    assert_eq!(effect["tapped"], json!(true));
    assert_eq!(effect["token"]["subtypes"], json!(["Robot"]));
    assert_eq!(effect["token"]["types"], json!(["Artifact", "Creature"]));
}

#[test]
fn tapped_composes_with_keywords_and_plural_quantity() {
    let effect =
        create("Create two tapped 1/1 colorless Robot artifact creature tokens with flying.")
            .unwrap();
    assert_eq!(effect["tapped"], json!(true));
    assert_eq!(effect["quantity"], json!({"kind":"integer","value":2}));
    assert_eq!(effect["token"]["abilities"], json!([{"kind":"flying"}]));
}

#[test]
fn tapped_composes_with_named_catalog_tokens() {
    let effect = create("Create a tapped Treasure token.").unwrap();
    assert_eq!(effect["tapped"], json!(true));
    assert_eq!(effect["token"]["name"], json!("Treasure"));
}

#[test]
fn untapped_forms_do_not_emit_the_field() {
    for text in [
        "Create a 2/2 colorless Robot artifact creature token.",
        "Create a Treasure token.",
    ] {
        assert!(create(text).unwrap().get("tapped").is_none(), "{text}");
    }
}

#[test]
fn unsupported_entry_state_combinations_fail_explicitly() {
    for text in [
        "Create a tapped and untapped Treasure token.",
        "Create a tapped and attacking Treasure token.",
        "Create a tapped tapped Treasure token.",
        "Create a tapped 1/1 red Goblin creature token that's attacking.",
        "Create two 1/1 red Goblin creature tokens that's tapped and attacking.",
    ] {
        assert!(create(text).is_err(), "{text}");
    }
}

/// CR 508.4: the attacking entry state is one phrase written before the
/// description or as its relative clause; both lower to the same fields.
#[test]
fn attacking_entry_state_is_the_same_field_before_or_after_the_noun() {
    for (text, tapped) in [
        (
            "Create a tapped and attacking 2/2 colorless Robot artifact creature token.",
            true,
        ),
        (
            "Create a 1/1 red Goblin creature token that's tapped and attacking.",
            true,
        ),
        (
            "Create two 1/1 white Cat creature tokens with lifelink that are tapped and attacking.",
            true,
        ),
        (
            "Create a 1/1 white Cat Soldier creature token with vigilance that's attacking.",
            false,
        ),
    ] {
        let effect = create(text).unwrap();
        assert_eq!(effect["attacking"], json!(true), "{text}");
        assert_eq!(effect.get("tapped").is_some(), tapped, "{text}");
    }
}

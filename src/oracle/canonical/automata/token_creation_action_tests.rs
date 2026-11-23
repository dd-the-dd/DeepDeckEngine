use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

#[test]
fn descriptor_failure_commits_its_typed_family_without_advancing_the_parent() {
    let tokens = recognize_oracle_entities("Create a 1/1 red Inventedling creature token.");
    let mut cursor = OracleTokenCursor::new("test.token.creation", &tokens);
    match try_recognize_token_creation_action(&mut cursor, None) {
        TokenCreationAttempt::FailedCommitted(failure) => {
            assert!(failure.expected.contains("subtype"));
            assert_eq!(failure.actual.as_deref(), Some("Inventedling"));
            assert_eq!(cursor.position(), 0);
        }
        _ => panic!("an introduced explicit descriptor must not become a named-token candidate"),
    }
}

#[test]
fn unrelated_action_is_not_claimed_and_long_descriptor_retains_all_characteristics() {
    let tokens = recognize_oracle_entities("Draw a card.");
    let mut cursor = OracleTokenCursor::new("test.token.unrelated", &tokens);
    assert!(matches!(
        try_recognize_token_creation_action(&mut cursor, None),
        TokenCreationAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), 0);
    let tokens = recognize_oracle_entities(
        "Create two 2/3 red, green, and white Elf Warrior artifact creature tokens with first strike and vigilance, then draw a card.",
    );
    let mut cursor = OracleTokenCursor::new("test.token.long", &tokens);
    let TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(node)) =
        try_recognize_token_creation_action(&mut cursor, None)
    else {
        panic!("explicit descriptor must parse")
    };
    assert_eq!(
        node.body.template.types,
        [ObjectCardType::Artifact, ObjectCardType::Creature]
    );
    assert_eq!(node.body.template.creature.colors.len(), 3);
    assert_eq!(node.body.template.creature.subtypes.len(), 2);
    assert_eq!(node.body.template.creature.power, 2);
    assert_eq!(node.body.template.creature.toughness, 3);
    assert_eq!(
        node.body.keywords,
        [GrantedKeyword::FirstStrike, GrantedKeyword::Vigilance]
    );
    assert_eq!(
        cursor.peek(),
        Some(","),
        "effect parent still owns then/draw"
    );
}

#[test]
fn permanent_type_domain_rejects_instant_and_sorcery_in_both_descriptor_parents() {
    for profile in [
        "sorcery creature",
        "instant creature",
        "creature sorcery",
        "creature instant",
    ] {
        let text = format!("You create a 1/1 red Goblin {profile} token.");
        let tokens = recognize_oracle_entities(&text);
        let mut cursor = OracleTokenCursor::new("test.token.permanent-domain", &tokens);
        match try_recognize_token_creation_action(&mut cursor, None) {
            TokenCreationAttempt::FailedCommitted(failure) => {
                assert_eq!(failure.state, "token.descriptor.permanent-type-domain");
                assert!(matches!(
                    failure.actual.as_deref(),
                    Some("instant" | "sorcery")
                ));
                assert_eq!(cursor.position(), 0);
            }
            _ => panic!("forbidden card type must not be lowered or become a named alternative"),
        }
        let mut old_parent = OracleTokenCursor::new("test.token.existing-body", &tokens);
        assert!(recognize_create_creature_tokens_effect(&mut old_parent, None).is_err());
    }
}

#[test]
fn kindred_is_a_compatible_additional_type_and_artifact_creature_is_preserved() {
    use crate::oracle::characteristic_vocabulary::CardType;
    assert!(!CardType::Kindred.is_permanent_type());
    assert!(CardType::Kindred.may_appear_on_permanent());
    for profile in ["artifact creature", "kindred artifact creature"] {
        let text = format!("Create a 1/1 red Goblin {profile} token.");
        let tokens = recognize_oracle_entities(&text);
        let mut cursor = OracleTokenCursor::new("test.token.compatible-types", &tokens);
        let TokenCreationAttempt::Recognized(LocalEffectNode::CreateExplicitTokens(node)) =
            try_recognize_token_creation_action(&mut cursor, None)
        else {
            panic!("compatible permanent profile must survive")
        };
        assert!(node.body.template.types.contains(&ObjectCardType::Artifact));
        assert!(node.body.template.types.contains(&ObjectCardType::Creature));
        assert_eq!(cursor.peek(), Some("."));
    }
    let tokens = recognize_oracle_entities("You create a 1/1 red Goblin creature token");
    let mut old_parent = OracleTokenCursor::new("test.token.existing-positive", &tokens);
    recognize_create_creature_tokens_effect(&mut old_parent, None).unwrap();
    old_parent.finish().unwrap();
}

#[test]
fn referenced_controller_passes_the_existing_typed_body_without_rereading_or_losing_scope() {
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };
    let tokens = recognize_oracle_entities(
        "Its controller creates three 3/3 green Ape creature tokens with flying.",
    );
    let mut cursor = OracleTokenCursor::new("test.token.snapshot-body", &tokens);
    let TokenCreationAttempt::Recognized(LocalEffectNode::CreateTokens(node)) =
        try_recognize_token_creation_action(&mut cursor, Some(binding))
    else {
        panic!("historical snapshot parent needs its original typed body")
    };
    assert_eq!(node.player, LocalPlayerReference::ControllerOf(binding));
    assert_eq!(node.quantity, NumericAtom::Integer(3));
    assert_eq!(node.token.power, 3);
    assert_eq!(node.token.toughness, 3);
    assert_eq!(
        node.token
            .subtypes
            .iter()
            .map(|kind| kind.ir_name())
            .collect::<Vec<_>>(),
        ["Ape"]
    );
    assert_eq!(
        cursor.peek(),
        Some("with"),
        "the old specialized parent owns its modifier continuation"
    );
}

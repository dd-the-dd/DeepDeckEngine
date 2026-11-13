use crate::oracle::canonical::{
    CardColor, GrantedKeyword, LocalPlayerReference, NumericAtom, ObjectCardType, ObjectKind,
    ObjectPredicate, TargetBinding, TargetSelector,
};
use crate::oracle::syntax::recognize_entities;

use super::{OracleTokenCursor, recognize_counter_descriptive_token_sequence};

const EXACT: &str = "Counter target enchantment, instant, or sorcery spell. Its controller creates a 2/2 blue Bird creature token with flying.";

fn recognize(text: &str) -> super::MachineResult<super::CounterDescriptiveTokenSequenceNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("counter-descriptive-token-test", &tokens);
    recognize_counter_descriptive_token_sequence(&mut cursor)
}

#[test]
fn recognizes_counter_union_controller_anaphora_characteristics_and_keyword() {
    let node = recognize(EXACT).expect("Swan Song grammar parses");

    assert_eq!(node.counter.target.slots.len(), 1);
    assert!(matches!(
        &node.counter.target.slots[0].selector,
        TargetSelector::Object(ObjectPredicate::Intersection(operands))
            if operands.contains(&ObjectPredicate::ObjectKind(ObjectKind::Spell))
                && operands.contains(&ObjectPredicate::Union(vec![
                    ObjectPredicate::CardType(ObjectCardType::Enchantment),
                    ObjectPredicate::CardType(ObjectCardType::Instant),
                    ObjectPredicate::CardType(ObjectCardType::Sorcery),
                ]))
    ));
    assert_eq!(
        node.token_creation.player,
        LocalPlayerReference::ControllerOf(TargetBinding {
            effect_index: 0,
            slot_index: 0,
        })
    );
    assert_eq!(node.token_creation.quantity, NumericAtom::Integer(1));
    assert_eq!(node.token_creation.token.power, 2);
    assert_eq!(node.token_creation.token.toughness, 2);
    assert_eq!(node.token_creation.token.colors, vec![CardColor::Blue]);
    assert_eq!(node.token_creation.token.subtypes[0].ir_name(), "Bird");
    assert_eq!(node.keywords, vec![GrantedKeyword::Flying]);
}

#[test]
fn sibling_reuses_type_selector_token_characteristics_and_keyword_children() {
    let node = recognize(
        "Counter target artifact spell. Its controller creates a 3/3 red Goblin creature token with haste.",
    )
    .expect("generic sibling parses");

    assert_eq!(node.token_creation.token.power, 3);
    assert_eq!(node.token_creation.token.toughness, 3);
    assert_eq!(node.token_creation.token.colors, vec![CardColor::Red]);
    assert_eq!(node.token_creation.token.subtypes[0].ir_name(), "Goblin");
    assert_eq!(node.keywords, vec![GrantedKeyword::Haste]);
}

#[test]
fn owner_anaphora_fails_in_the_shared_player_reference_child() {
    let failure = recognize(
        "Counter target artifact spell. Its owner creates a 3/3 red Goblin creature token with haste.",
    )
    .expect_err("owner is not the controller reference required by this grammar");

    assert_eq!(failure.state, "reference.player.controller-of-object");
    assert_eq!(failure.actual.as_deref(), Some("owner"));
}

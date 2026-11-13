use crate::oracle::canonical::{
    AddManaEffectNode, LocalActivationCostNode, NumericAtom, OracleTokenCursor, PlayerPredicate,
    SacrificeSelfSourceNoun, TargetCardinality, TargetSelector,
    recognize_targeted_create_creature_tokens_with_mana_ability,
};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> super::TargetedCreateCreatureTokensWithManaAbilityNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.targeted-token-with-mana-ability", &tokens);
    let node = recognize_targeted_create_creature_tokens_with_mana_ability(&mut cursor)
        .expect("targeted token creation parses");
    cursor
        .finish()
        .expect("token creation consumes every token");
    node
}

#[test]
fn parses_targeted_variable_spawn_creation_with_primitive_mana_ability() {
    let node = parse(
        "Target player creates X 0/1 colorless Eldrazi Spawn creature tokens with \"Sacrifice this token: Add {C}.\"",
    );

    assert!(matches!(node.target.slots.as_slice(), [slot]
        if slot.cardinality == TargetCardinality::EXACTLY_ONE
            && slot.selector == TargetSelector::Player(PlayerPredicate::Any)));
    assert_eq!(node.quantity, NumericAtom::VariableX);
    assert_eq!(node.token.power, 0);
    assert_eq!(node.token.toughness, 1);
    assert!(node.token.colors.is_empty(), "colorless means no colors");
    assert!(matches!(
        node.ability.costs.costs.as_slice(),
        [LocalActivationCostNode::SacrificeSelf(cost)]
            if cost.source_type == SacrificeSelfSourceNoun::Token
    ));
    let AddManaEffectNode::Fixed(mana) = &node.ability.mana else {
        panic!("expected fixed colorless mana production");
    };
    assert_eq!(mana.mana.as_oracle_cost(), "{C}");
}

#[test]
fn varies_player_role_quantity_subtype_and_mana_without_changing_the_grammar() {
    let node = parse(
        "Target opponent creates two 1/1 colorless Eldrazi Scion creature tokens with \"Sacrifice this token: Add {G}.\"",
    );

    assert!(matches!(node.target.slots.as_slice(), [slot]
        if slot.selector == TargetSelector::Player(PlayerPredicate::Opponent)));
    assert_eq!(node.quantity, NumericAtom::Integer(2));
    let AddManaEffectNode::Fixed(mana) = &node.ability.mana else {
        panic!("expected fixed green mana production");
    };
    assert_eq!(mana.mana.as_oracle_cost(), "{G}");
}

#[test]
fn unknown_quoted_token_ability_fails_at_the_activated_cost_child() {
    let tokens = recognize_entities(
        "Target player creates two 1/1 colorless Eldrazi Scion creature tokens with \"This token sings.\"",
    );
    let mut cursor = OracleTokenCursor::new("test.targeted-token-with-mana-ability", &tokens);

    let failure = recognize_targeted_create_creature_tokens_with_mana_ability(&mut cursor)
        .expect_err("unknown quoted rules must remain visible");

    assert_eq!(failure.state, "activation.compound.cost");
    assert_eq!(failure.actual.as_deref(), Some("This"));
}

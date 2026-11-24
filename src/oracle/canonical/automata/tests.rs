use crate::oracle::syntax::{NameLinkKind, ResolvedNameLink, recognize_entities};

use super::super::OracleTokenCursor;
use super::*;

fn current_card_link(tokens: &[crate::oracle::model::RecognizedEntity]) -> ResolvedNameLink {
    let token_end = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("deals"))
        .expect("damage action follows the source name");
    ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Example Source".to_string(),
        raw: "Example Source".to_string(),
        token_start: 0,
        token_end,
        span_start: tokens[0].span_start,
        span_end: tokens[token_end - 1].span_end,
    }
}

fn one_target(selector: TargetSelector) -> TargetListNode {
    TargetListNode {
        slots: vec![TargetSlot {
            selector,
            cardinality: TargetCardinality::EXACTLY_ONE,
            exclude_source: false,
            count: None,
        }],
    }
}

#[test]
fn linked_source_damage_and_life_are_independent_typed_effects() {
    let tokens =
        recognize_entities("Example Source deals 3 damage to any target and you gain 3 life.");
    let links = [current_card_link(&tokens)];
    let mut cursor = OracleTokenCursor::new("test.effect-sequence", &tokens);

    let sequence = recognize_effect_sequence(&mut cursor, &links, None).expect("sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::And]);
    assert_eq!(
        sequence.effects,
        [
            LocalEffectNode::DealDamage(DamageEffectNode {
                source: crate::oracle::canonical::automata::PermanentOperandNode::Reference(
                    LocalObjectReference::SourceSelf
                ),
                amount: NumericExpression::Atom(NumericAtom::Integer(3)),
                recipient: DamageRecipientNode::Targets(one_target(TargetSelector::Any)),
            }),
            LocalEffectNode::GainLife(GainLifeEffectNode {
                player: LocalPlayerReference::Controller,
                amount: NumericExpression::Atom(NumericAtom::Integer(3)),
            }),
        ]
    );
    assert_eq!(cursor.position(), tokens.len());
    assert!(
        cursor
            .transitions()
            .iter()
            .any(|transition| transition.state == "target-list.start")
    );
    assert!(
        cursor
            .transitions()
            .iter()
            .any(|transition| transition.state == "selector.predicate.start")
    );
}

#[test]
fn any_and_typed_damage_targets_share_target_list_and_selector_children() {
    let tokens = recognize_entities("This spell deals 3 damage to target artifact creature.");
    let mut cursor = OracleTokenCursor::new("test.effect-sequence", &tokens);

    let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect("damage parses");

    assert_eq!(
        sequence.effects,
        [LocalEffectNode::DealDamage(DamageEffectNode {
            source: crate::oracle::canonical::automata::PermanentOperandNode::Reference(
                LocalObjectReference::SourceSelf
            ),
            amount: NumericExpression::Atom(NumericAtom::Integer(3)),
            recipient: DamageRecipientNode::Targets(one_target(TargetSelector::Object(
                SelectorPredicate::Intersection(vec![
                    SelectorPredicate::CardType(ObjectCardType::Artifact),
                    SelectorPredicate::CardType(ObjectCardType::Creature),
                ]),
            ))),
        })]
    );
    for state in ["target-list.start", "selector.predicate.start"] {
        assert!(
            cursor
                .transitions()
                .iter()
                .any(|transition| transition.state == state),
            "missing shared child state {state}"
        );
    }
}

#[test]
fn this_spell_variable_damage_and_word_life_use_the_same_submachines() {
    let tokens =
        recognize_entities("This spell deals X damage to target player then you gain three life.");
    let mut cursor = OracleTokenCursor::new("test.effect-sequence", &tokens);

    let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect("sequence parses");

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    assert_eq!(
        sequence.effects[0],
        LocalEffectNode::DealDamage(DamageEffectNode {
            source: crate::oracle::canonical::automata::PermanentOperandNode::Reference(
                LocalObjectReference::SourceSelf
            ),
            amount: NumericExpression::Atom(NumericAtom::VariableX),
            recipient: DamageRecipientNode::Targets(one_target(TargetSelector::Player(
                PlayerPredicate::Any,
            ))),
        })
    );
    assert_eq!(
        sequence.effects[1],
        LocalEffectNode::GainLife(GainLifeEffectNode {
            player: LocalPlayerReference::Controller,
            amount: NumericExpression::Atom(NumericAtom::Integer(3)),
        })
    );
}

#[test]
fn sentence_sequence_resolves_controller_and_power_against_the_exile_target() {
    let tokens =
        recognize_entities("Exile target creature. Its controller gains life equal to its power.");
    let mut cursor = OracleTokenCursor::new("test.exile-anaphora", &tokens);

    let sequence =
        recognize_effect_sequence(&mut cursor, &[], None).expect("linked sequence parses");
    let binding = TargetBinding {
        effect_index: 0,
        slot_index: 0,
    };

    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    assert!(matches!(
        sequence.effects[0],
        LocalEffectNode::ZoneChange(ZoneChangeEffectNode {
            action: ZoneChangeAction::Exile,
            regeneration: ZoneChangeRegenerationPolicy::Allowed,
            ..
        })
    ));
    // The shared player-action child owns the possessive subject; its target
    // is also the antecedent of the amount's `its`.
    assert_eq!(
        sequence.effects[1],
        LocalEffectNode::PlayerAction(PlayerActionNode {
            player: PlayerEffectSubject::ObjectRelation {
                relation: ObjectPlayerRelation::Controller,
                object: PermanentEffectReference::Target(binding),
            },
            action: SingularPlayerAction::Life(LifeActionNode {
                action: TargetPlayerLifeAction::Gain,
                amount: NumericExpression::PowerOf(binding),
            }),
        })
    );
    assert_eq!(cursor.position(), tokens.len());
}

#[test]
fn missing_damage_amount_fails_where_the_equal_to_amount_is_required() {
    // `deals damage` defers its amount to an `equal to` clause, before or
    // after the recipient; without one the frame reports the missing amount.
    let tokens = recognize_entities("Example Source deals damage to any target.");
    let links = [current_card_link(&tokens)];
    let mut cursor = OracleTokenCursor::new("test.effect-sequence", &tokens);

    let failure =
        recognize_effect_sequence(&mut cursor, &links, None).expect_err("amount is required");

    assert_eq!(failure.state, "effect.damage.postposed-amount");
    assert_eq!(failure.expected, "`equal to` and the amount of damage");
}

#[test]
fn an_unlinked_unknown_source_is_not_guessed() {
    let tokens = recognize_entities("Unknown Source deals 3 damage to any target.");
    let mut cursor = OracleTokenCursor::new("test.effect-sequence", &tokens);

    let failure =
        recognize_effect_sequence(&mut cursor, &[], None).expect_err("name must be linked");

    assert_eq!(failure.state, "effect.start");
    assert_eq!(failure.token_index, 0);
    assert_eq!(failure.expected, "a supported effect subject");
}

#[test]
fn dangling_connector_fails_without_partial_success() {
    let tokens = recognize_entities("You gain 3 life and.");
    let mut cursor = OracleTokenCursor::new("test.effect-sequence", &tokens);

    let failure =
        recognize_effect_sequence(&mut cursor, &[], None).expect_err("effect follows and");

    assert_eq!(failure.state, "sequence.connector.effect");
    assert_eq!(failure.actual.as_deref(), Some("."));
}

fn parse_complete_target_list(text: &str) -> TargetListNode {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("test.target-list", &tokens);
    let target_list = recognize_target_list(&mut cursor).expect("target list parses");
    cursor.finish().expect("target list consumes its phrase");
    target_list
}

fn card_type(card_type: ObjectCardType) -> SelectorPredicate {
    SelectorPredicate::CardType(card_type)
}

#[test]
fn repeated_target_head_opens_two_independent_slots() {
    let target_list = parse_complete_target_list("target creature and target artifact");

    assert_eq!(
        target_list,
        TargetListNode {
            slots: vec![
                TargetSlot {
                    selector: TargetSelector::Object(card_type(ObjectCardType::Creature)),
                    cardinality: TargetCardinality::EXACTLY_ONE,
                    exclude_source: false,
                    count: None,
                },
                TargetSlot {
                    selector: TargetSelector::Object(card_type(ObjectCardType::Artifact)),
                    cardinality: TargetCardinality::EXACTLY_ONE,
                    exclude_source: false,
                    count: None,
                },
            ],
        }
    );
}

#[test]
fn determined_player_noun_opens_a_second_target_domain() {
    let target_list = parse_complete_target_list("target creature and an opponent");

    assert_eq!(target_list.slots.len(), 2);
    assert_eq!(
        target_list.slots[0].selector,
        TargetSelector::Object(card_type(ObjectCardType::Creature))
    );
    assert_eq!(
        target_list.slots[1],
        TargetSlot {
            selector: TargetSelector::Player(PlayerPredicate::Opponent),
            cardinality: TargetCardinality::EXACTLY_ONE,
            exclude_source: false,
            count: None,
        }
    );
}

#[test]
fn consecutive_card_types_form_one_intersection_slot() {
    let target_list = parse_complete_target_list("target artifact creature");

    assert_eq!(target_list.slots.len(), 1);
    assert_eq!(
        target_list.slots[0].selector,
        TargetSelector::Object(SelectorPredicate::Intersection(vec![
            card_type(ObjectCardType::Artifact),
            card_type(ObjectCardType::Creature),
        ]))
    );
}

#[test]
fn selector_or_forms_one_union_slot() {
    let target_list = parse_complete_target_list("target creature or planeswalker");

    assert_eq!(target_list.slots.len(), 1);
    assert_eq!(
        target_list.slots[0].selector,
        TargetSelector::Object(SelectorPredicate::Union(vec![
            card_type(ObjectCardType::Creature),
            card_type(ObjectCardType::Planeswalker),
        ]))
    );
}

#[test]
fn cardinalized_and_or_forms_one_bounded_union_collection() {
    let target_list = parse_complete_target_list("up to two target creatures and/or planeswalkers");

    assert_eq!(target_list.slots.len(), 1);
    assert_eq!(
        target_list.slots[0],
        TargetSlot {
            selector: TargetSelector::Object(SelectorPredicate::Union(vec![
                card_type(ObjectCardType::Creature),
                card_type(ObjectCardType::Planeswalker),
            ])),
            cardinality: TargetCardinality {
                minimum: 0,
                maximum: 2,
            },
            exclude_source: false,
            count: None,
        }
    );
}

#[test]
fn any_target_uses_the_same_one_slot_contract() {
    let target_list = parse_complete_target_list("any target");

    assert_eq!(
        target_list,
        TargetListNode {
            slots: vec![TargetSlot {
                selector: TargetSelector::Any,
                cardinality: TargetCardinality::EXACTLY_ONE,
                exclude_source: false,
                count: None,
            }],
        }
    );
}

#[test]
fn cardinalized_other_target_retains_source_exclusion() {
    let target_list = parse_complete_target_list("up to one other target nonland permanent");

    assert_eq!(target_list.slots.len(), 1);
    assert_eq!(
        target_list.slots[0].cardinality,
        TargetCardinality {
            minimum: 0,
            maximum: 1,
        }
    );
    assert!(target_list.slots[0].exclude_source);
}

#[test]
fn committed_repeated_target_reports_the_missing_selector() {
    let tokens = recognize_entities("target creature and target");
    let mut cursor = OracleTokenCursor::new("test.target-list", &tokens);

    let failure = recognize_target_list(&mut cursor).expect_err("second selector is required");

    assert_eq!(failure.state, "selector.predicate.start");
    assert_eq!(failure.token_index, tokens.len());
    assert_eq!(failure.expected, "an object predicate");
}

#[test]
fn soft_and_probe_rolls_back_when_no_new_target_head_follows() {
    let tokens = recognize_entities("target creature and artifact");
    let mut cursor = OracleTokenCursor::new("test.target-list", &tokens);

    let target_list = recognize_target_list(&mut cursor).expect("first slot remains valid");
    let trailing = cursor
        .finish()
        .expect_err("ambiguous trailing phrase remains visible");

    assert_eq!(target_list.slots.len(), 1);
    assert_eq!(trailing.actual.as_deref(), Some("and"));
    assert!(
        cursor
            .transitions()
            .iter()
            .any(|transition| transition.state == "rollback")
    );
}

#[test]
fn variable_target_cardinality_is_a_structured_failure() {
    let tokens = recognize_entities("up to X target creatures");
    let mut cursor = OracleTokenCursor::new("test.target-list", &tokens);

    let failure = recognize_target_list(&mut cursor).expect_err("X is not a fixed bound");

    assert_eq!(failure.state, "target-list.cardinality.literal-required");
    assert_eq!(failure.actual.as_deref(), Some("X"));
    assert_eq!(failure.span_start, 6);
    assert_eq!(failure.span_end, 7);
}

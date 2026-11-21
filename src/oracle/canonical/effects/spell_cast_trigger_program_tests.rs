use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};

fn program(text: &str, type_line: &str) -> SpellCastTriggerProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    // Match the production lexical preparation before invoking this parent:
    // catalog-backed names such as Treasure are resolved on the same token
    // stream, not supplied as unknown words to the action recognizer.
    let mut tokens = recognize_oracle_entities(text);
    let named_tokens = ExactNameTokenIndex::from_names(
        crate::card_catalog::named_token_names().expect("local named-token catalog"),
    );
    resolve_named_token_spans(&mut tokens, &named_tokens);
    try_parse_promoted_spell_cast_trigger_program(&AbilityInput {
        face_name: "Synthetic cast watcher",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn shared_query_and_sequence_preserve_predicate_and_effect_order() {
    let SpellCastTriggerProgramParseAttempt::Parsed(draft) = program(
        "Whenever you cast an artifact or enchantment spell, draw two cards and gain 1 life.",
        "Enchantment",
    ) else {
        panic!("the existing query and actions must compose");
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"spellCast","player":controller(),"where":{"kind":"or","operands":[card_type("Artifact"),card_type("Enchantment")]}})
    );
    assert_eq!(draft.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draft.rule["effects"][0]["count"], integer(2));
    assert_eq!(draft.rule["effects"][1]["kind"], "gainLife");
    assert!(draft.rule.get("declaration").is_none());
}

#[test]
fn any_player_event_does_not_change_the_effects_recipient() {
    let SpellCastTriggerProgramParseAttempt::Parsed(draft) = program(
        "Whenever a player casts their second spell each turn, you lose 1 life and create a Treasure token.",
        "Creature",
    ) else {
        panic!("an any-player event must keep the body's explicit controller");
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"spellCast","anyPlayer":true,"where":null,"spellCastOrdinal":2})
    );
    assert_eq!(draft.rule["effects"][0]["kind"], "loseLife");
    assert_eq!(draft.rule["effects"][0]["player"], controller());
    assert_eq!(draft.rule["effects"][1]["kind"], "createTokens");
    assert_eq!(draft.rule["effects"][1]["controller"], controller());
}

#[test]
fn unsupported_source_condition_and_body_bindings_keep_failures() {
    for (text, type_line, state) in [
        (
            "Whenever you cast a spell, draw a card.",
            "Instant",
            "trigger.spell-cast.permanent-face",
        ),
        (
            "Whenever you cast a spell, if you control a creature, draw a card.",
            "Creature",
            "trigger.spell-cast.intervening-condition",
        ),
    ] {
        let SpellCastTriggerProgramParseAttempt::Unpromoted(failure) = program(text, type_line)
        else {
            panic!("must preserve this support boundary: {text}");
        };
        assert_eq!(failure.state, state);
    }
    assert!(matches!(
        program("Whenever you cast a spell, draw X cards.", "Creature"),
        SpellCastTriggerProgramParseAttempt::Unpromoted(_)
    ));
}

#[test]
fn a_suffix_cannot_be_dropped_from_the_event_or_program() {
    for text in [
        "Whenever you cast a spell from your graveyard, draw a card.",
        "Whenever you cast a spell, draw a card. Unsupported instruction.",
    ] {
        assert!(!matches!(
            program(text, "Creature"),
            SpellCastTriggerProgramParseAttempt::Parsed(_)
        ));
    }
}

#[test]
fn any_player_filtered_event_keeps_both_object_filter_and_ability_controller() {
    let SpellCastTriggerProgramParseAttempt::Parsed(draft) = program(
        "Whenever a player casts an instant or sorcery spell, you draw a card.",
        "Creature",
    ) else {
        panic!("the shared player-and-object matcher now supports this event");
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"spellCast","anyPlayer":true,
        "where":{"kind":"or","operands":[card_type("Instant"),card_type("Sorcery")]}})
    );
    assert_eq!(draft.rule["effects"][0]["player"], controller());
    assert_eq!(draft.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draft.rule["effects"][0]["count"], integer(1));
    assert!(draft.rule["event"].get("spellCastOrdinal").is_none());
}

#[test]
fn numeric_spell_characteristics_require_the_context_of_the_observed_stack_spell() {
    for text in [
        "Whenever you cast a spell with mana value 4 or greater, draw a card.",
        "Whenever a player casts a creature spell with power 4 or greater, draw a card.",
    ] {
        let SpellCastTriggerProgramParseAttempt::Unpromoted(failure) = program(text, "Enchantment")
        else {
            panic!("printed characteristics must not silently replace stack context: {text}");
        };
        assert_eq!(failure.state, "trigger.spell-cast.numeric-stack-context");
    }
    let numeric = ObjectPredicate::NumericComparison {
        left: SelectorNumericProperty::ManaValue,
        operator: NumericComparisonOperator::GreaterThanOrEqual,
        right: Box::new(NumericAtom::Integer(4).into()),
    };
    assert!(spell_cast_predicate_requires_numeric_stack_context(
        &ObjectPredicate::Not(Box::new(ObjectPredicate::Union(vec![numeric])))
    ));
    // An ordinal is not a characteristic of the observed spell.
    assert!(matches!(
        program(
            "Whenever a player casts their second spell each turn, you lose 1 life and create a Treasure token.",
            "Creature"
        ),
        SpellCastTriggerProgramParseAttempt::Parsed(_)
    ));
}

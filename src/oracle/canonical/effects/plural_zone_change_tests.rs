use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};

fn zone_change(
    text: &str,
    support: ZoneChangeTargetSupport,
) -> MachineResult<ZoneChangeEffectNode> {
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("plural-zone-test", &tokens);
    let LocalEffectNode::ZoneChange(effect) =
        recognize_zone_change_effect_with_support(&mut cursor, support)?
    else {
        panic!("expected a typed zone-change node");
    };
    Ok(effect)
}

fn sequence(text: &str) -> EffectSequenceParseAttempt {
    let mut tokens = recognize_oracle_entities(text);
    let index = ExactNameTokenIndex::from_names(
        crate::card_catalog::named_token_names().expect("local named-token catalog"),
    );
    resolve_named_token_spans(&mut tokens, &index);
    let mut cursor = OracleTokenCursor::new("plural-zone-sequence-test", &tokens);
    try_recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::for_source(LocalObjectReference::SourceSelf),
    )
}

#[test]
fn collection_support_is_explicit_and_opens_every_zone_action_and_slot() {
    for text in [
        "Destroy two target creatures",
        "Exile two target creatures",
        "Destroy target creature and target artifact",
    ] {
        assert!(
            zone_change(text, ZoneChangeTargetSupport::Scalar).is_err(),
            "{text}"
        );
    }
    // CR 601.2c, 115.3: each slot is its own declaration; the collection
    // parent receives every slot with its own cardinality.
    let exile = zone_change(
        "Exile two target creatures",
        ZoneChangeTargetSupport::TargetCollection,
    )
    .expect("an exile collection is a target collection");
    assert_eq!(exile.target.slots[0].cardinality.maximum, 2);
    let slots = zone_change(
        "Destroy target creature and target artifact",
        ZoneChangeTargetSupport::TargetCollection,
    )
    .expect("two target slots in one instruction");
    assert_eq!(slots.target.slots.len(), 2);
    let returned = zone_change(
        "Return up to two target creatures to their owners' hands",
        ZoneChangeTargetSupport::TargetCollection,
    )
    .expect("a plural return agrees with its plural possessive");
    assert_eq!(returned.target.slots[0].cardinality.minimum, 0);
    // A later `other target` slot is admitted; card composition records its
    // distinctness from the earlier slot.
    let other = zone_change(
        "Destroy target creature and up to one other target creature",
        ZoneChangeTargetSupport::TargetCollection,
    )
    .expect("an `other target` slot");
    assert_eq!(other.target.slots.len(), 2);
    assert_eq!(other.target.slots[1].cardinality.minimum, 0);
    // Near misses: number agreement and a malformed selector.
    for text in [
        "Return two target creatures to its owner's hand",
        "Return target creature to their owners' hands",
        "Destroy two target noncreature",
    ] {
        assert!(
            zone_change(text, ZoneChangeTargetSupport::TargetCollection).is_err(),
            "{text}"
        );
    }
}

#[test]
fn collection_operand_keeps_its_bounds_and_predicate_while_scalar_ir_stays_exact() {
    let collection = zone_change(
        "Destroy up to two target artifacts and/or enchantments",
        ZoneChangeTargetSupport::TargetCollection,
    )
    .expect("the same target-list query accepts a bounded collection");
    let (id, candidates, minimum, maximum) = lower_single_target(&collection.target);
    assert_eq!(id, "targetPermanent");
    assert_eq!((minimum, maximum), (0, 2));
    assert_eq!(
        candidates,
        json!({"kind":"permanents","where":{"kind":"or","operands":[
            card_type("Artifact"),card_type("Enchantment")
        ]}})
    );
    let mut cursor = OracleTokenCursor::new("plural-operand-test", &[]);
    let reference = lower_zone_change_target_operand(
        &mut cursor,
        &collection,
        id,
        ZoneChangeTargetSupport::TargetCollection,
    )
    .unwrap();
    assert_eq!(reference, json!({"kind":"chosenTargets","id":id}));
    assert_eq!(
        lower_targeted_zone_change_node(&mut cursor, &collection, reference).unwrap(),
        json!({"kind":"destroyPermanent","permanent":{"kind":"chosenTargets","id":id}})
    );
    assert!(
        lower_zone_change_target_operand(
            &mut cursor,
            &collection,
            id,
            ZoneChangeTargetSupport::Scalar,
        )
        .is_err()
    );
    let scalar = zone_change("Destroy target creature", ZoneChangeTargetSupport::Scalar).unwrap();
    assert_eq!(
        lower_zone_change_target_operand(
            &mut cursor,
            &scalar,
            id,
            ZoneChangeTargetSupport::TargetCollection,
        )
        .unwrap(),
        chosen_target(id)
    );
}

#[test]
fn a_collection_cannot_become_a_singular_controller_or_relative_target_antecedent() {
    assert!(matches!(
        sequence("Destroy target creature. Its controller creates a Treasure token."),
        EffectSequenceParseAttempt::Parsed(_)
    ));
    assert!(!matches!(
        sequence("Destroy two target creatures. Its controller creates a Treasure token."),
        EffectSequenceParseAttempt::Parsed(_)
    ));
    assert!(!matches!(
        sequence("Destroy two target creatures. It can't be regenerated."),
        EffectSequenceParseAttempt::Parsed(_)
    ));
    let EffectSequenceParseAttempt::Parsed(sequence) =
        sequence("Destroy target creature. Destroy two other target creatures.")
    else {
        panic!("both local nodes must reach the typed lowering support boundary");
    };
    let mut cursor = OracleTokenCursor::new("plural-relative-target-test", &[]);
    let EffectSequenceLoweringAttempt::FailedSupport(failure) =
        try_lower_local_effect_sequence(&mut cursor, &sequence)
    else {
        panic!("the unrepresented relation must be a hard support refusal");
    };
    assert_eq!(failure.state, "lower.zone-change.another-target-relation");
}

fn atomic_claim(text: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    claims_promoted_atomic_zone_change_family(&AbilityInput {
        face_name: "Synthetic plural destroy source",
        face_type_line: "Instant",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn atomic_claim_preserves_conditional_and_multisentence_envelopes() {
    assert!(atomic_claim(
        "Destroy up to two target artifacts and/or enchantments."
    ));
    assert!(!atomic_claim(
        "If you control an artifact, destroy up to two target enchantments."
    ));
    assert!(!atomic_claim(
        "Destroy up to two target creatures. Draw a card."
    ));
}

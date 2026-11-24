use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn production_and_spell_use_predicate_remain_separate_typed_parameters() {
    for text in [
        "Add one mana of any color. Spend this mana only to cast a legendary spell, and that spell can't be countered.",
        "Add {G}. Spend this mana only to cast a creature spell.",
        "Add {R} or {G}. Spend this mana only to cast an artifact spell, and that spell cannot be countered.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("qualified-production", &tokens);
        let node = recognize_add_mana_effect(&mut cursor).unwrap();
        cursor.consume(".").unwrap();
        cursor.finish().unwrap();
        let AddManaEffectNode::Qualified { policy, .. } = &node else {
            panic!("missing typed qualifier: {text}")
        };
        assert!(policy.spells.has_domain(ObjectKind::Spell));
        let ir = crate::oracle::canonical::lower_add_mana_effect(&node);
        assert_eq!(ir["kind"], "addMana");
        assert_eq!(ir["spendRestriction"]["kind"], "castSpell");
    }
}

#[test]
fn qualifier_never_steals_another_sentence_or_accepts_an_unproved_use() {
    for text in [
        "Add {G}. Spend this mana only to activate a legendary permanent's ability.",
        "Add {G}. Spend this mana only to cast a legendary card.",
        "Add {G}. Spend that mana only to cast a legendary spell.",
        "Add {G}. Spend this mana only to cast a legendary spell, and that spell draws a card.",
        "Add {G}. Spend this mana only to cast a spell with mana value X.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("qualified-production-negative", &tokens);
        assert!(recognize_add_mana_effect(&mut cursor).is_err(), "{text}");
    }
    let tokens = recognize_entities("Add {G}. Draw a card.");
    let mut cursor = OracleTokenCursor::new("unqualified-production", &tokens);
    assert!(matches!(
        recognize_add_mana_effect(&mut cursor).unwrap(),
        AddManaEffectNode::Fixed(_)
    ));
    assert_eq!(cursor.peek(), Some("."));
}

#[test]
fn qualifier_is_local_to_its_immediately_preceding_production() {
    let tokens =
        recognize_entities("Add {G}. Add {R}. Spend this mana only to cast a legendary spell.");
    let mut cursor = OracleTokenCursor::new("qualified-production-scope", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None).unwrap();
    cursor.finish().unwrap();
    assert!(matches!(
        &sequence.effects[0],
        LocalEffectNode::AddMana(AddManaEffectNode::Fixed(_))
    ));
    assert!(matches!(
        &sequence.effects[1],
        LocalEffectNode::AddMana(AddManaEffectNode::Qualified { .. })
    ));
}

#[test]
fn boolean_domain_identity_uses_shared_lowering_capability() {
    for text in [
        "Add {G}. Spend this mana only to cast artifact spells or spells.",
        "Add {G}. Spend this mana only to cast spells or artifact spells.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("policy-boolean-domain", &tokens);
        assert!(recognize_add_mana_effect(&mut cursor).is_err(), "{text}");
    }
    for text in [
        "Add {G}. Spend this mana only to cast spells.",
        "Add {G}. Spend this mana only to cast creature spells or artifact spells.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("policy-representable-domain", &tokens);
        recognize_add_mana_effect(&mut cursor).unwrap();
        cursor.consume(".").unwrap();
        cursor.finish().unwrap();
    }
    assert!(
        !crate::oracle::canonical::effects::domain_filter_is_representable(&ObjectPredicate::Not(
            Box::new(ObjectPredicate::ObjectKind(ObjectKind::Spell))
        ))
    );
}

#[test]
fn a_union_of_cast_and_activation_uses_of_the_same_objects_is_one_restriction() {
    for text in [
        "Add {C}{C}. Spend this mana only to cast colorless Eldrazi spells or activate abilities of colorless Eldrazi.",
        "Add one mana of any color. Spend this mana only to cast artifact spells or activate abilities of artifacts.",
        "Add {R}. Spend this mana only to cast a Dragon spell or to activate an ability of a Dragon.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("qualified-production-union", &tokens);
        let node = recognize_add_mana_effect(&mut cursor).unwrap_or_else(|failure| {
            panic!("{text}: {failure}");
        });
        cursor.consume(".").unwrap();
        cursor.finish().unwrap();
        let AddManaEffectNode::Qualified { policy, .. } = &node else {
            panic!("missing typed qualifier: {text}")
        };
        assert_eq!(policy.uses.len(), 2, "{text}");
        assert!(policy.also_activates_abilities(), "{text}");
        let ir = crate::oracle::canonical::lower_add_mana_effect(&node);
        assert_eq!(
            ir["spendRestriction"]["kind"], "castSpellOrActivateAbility",
            "{text}"
        );
    }
}

#[test]
fn a_union_the_runtime_cannot_restrict_fails_at_its_own_state() {
    for text in [
        "Add {C}. Spend this mana only to cast an artifact spell or activate an ability.",
        "Add {C}. Spend this mana only to activate abilities.",
        "Add {C}. Spend this mana only to cast artifact spells or activate abilities of creatures.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("qualified-production-union-negative", &tokens);
        let failure = recognize_add_mana_effect(&mut cursor).expect_err(text);
        assert_eq!(failure.state, "mana.policy.runtime-union", "{text}");
    }
}

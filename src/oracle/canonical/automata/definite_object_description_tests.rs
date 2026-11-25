//! The one definite description reader (`the sacrificed creature`, `the
//! exiled card`, `the creature exiled this way`, `the creature that died`):
//! its producer kinds and their order over hand-built environments, then
//! the relation and numeric operands of real cards, an unused recombination
//! and near misses. Runtime proofs live in
//! `engine/tests/described_object_anchor_runtime.rs`.
use std::sync::Arc;

use serde_json::{Value, json};

use super::super::{
    ActionResultObject, ActionResultReference, ActionResultVerb, BindingEnvironment,
    CostPaidConditionObject, DeclaredTargetAntecedent, EventObjectSlot, GrammaticalNumber,
    ObjectCardType, ObjectKind, ObjectPredicate, PermanentEffectReference, TargetBinding,
    TargetNominal, TargetSelector, TypedObservedEventObject,
};
use super::{
    DESCRIPTION_WITHOUT_PRODUCER, DIED_WITHOUT_EVENT, DefiniteObjectDescription, DescribedObject,
    DescriptionProducer, OracleTokenCursor, THIS_WAY_WITHOUT_PRODUCER,
    definite_object_description_starts, recognize_definite_object_description,
};
use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

const CREATURE: TargetNominal = TargetNominal::CardType(ObjectCardType::Creature);

fn read(text: &str, environment: &BindingEnvironment) -> Result<DefiniteObjectDescription, String> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("definite-description-test", &tokens);
    cursor.replace_binding_environment(Some(Arc::new(environment.clone())));
    let description = recognize_definite_object_description(&mut cursor, environment)
        .map_err(|failure| failure.expected.clone())?;
    cursor
        .finish()
        .map_err(|failure| failure.expected.clone())?;
    Ok(description)
}

/// An environment read inside an effect program (a cost is in scope).
fn program() -> BindingEnvironment {
    BindingEnvironment {
        effect_index: Some(1),
        ..BindingEnvironment::default()
    }
}

fn result(verb: ActionResultVerb, predicate: ObjectPredicate) -> ActionResultObject {
    ActionResultObject {
        reference: ActionResultReference {
            effect_index: 0,
            permanent: false,
        },
        selector: TargetSelector::Object(predicate),
        number: GrammaticalNumber::Singular,
        verb: Some(verb),
    }
}

fn result_reference() -> PermanentEffectReference {
    PermanentEffectReference::ActionResult(ActionResultReference {
        effect_index: 0,
        permanent: false,
    })
}

#[test]
fn the_reader_claims_only_the_three_written_forms() {
    for text in [
        "the sacrificed creature",
        "the exiled card",
        "the creature exiled this way",
        "the card revealed this way",
        "the creature that died",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("definite-description-test", &tokens);
        assert!(definite_object_description_starts(&cursor), "{text}");
    }
    // `the chosen card` names a choice (a name or an object chosen by its
    // own reader), `the card you exiled` the scavenged source card, and a
    // possessive is the genitive of a numeric read.
    for text in [
        "the chosen card",
        "the card you exiled",
        "the sacrificed creature's power",
        "the creature",
        "that creature",
    ] {
        let tokens = recognize_entities(text);
        let cursor = OracleTokenCursor::new("definite-description-test", &tokens);
        assert!(!definite_object_description_starts(&cursor), "{text}");
    }
}

/// Producer order: an earlier action of the program, else a cost of the
/// ability, else (for an exile) the linked exile; each kind is typed.
#[test]
fn each_producer_kind_is_resolved_in_lexical_order() {
    let mut environment = program();
    environment.result_object = Some(result(
        ActionResultVerb::Sacrificed,
        ObjectPredicate::CardType(ObjectCardType::Creature),
    ));
    let action = read("the sacrificed creature", &environment).expect("action result");
    assert_eq!(action.object, DescribedObject::Bound(result_reference()));
    assert_eq!(action.producer, DescriptionProducer::ActionResult);
    assert_eq!(action.nominal, CREATURE);
    // The same restatement written with `this way`.
    assert_eq!(
        read("the creature sacrificed this way", &environment)
            .expect("this way")
            .object,
        DescribedObject::Bound(result_reference())
    );

    // No action: the cost of the ability.
    let environment = program();
    let cost = read("the sacrificed creature", &environment).expect("cost-paid permanent");
    assert_eq!(
        cost.object,
        DescribedObject::CostPaid(CostPaidConditionObject::SacrificedPermanent)
    );
    assert_eq!(cost.producer, DescriptionProducer::CostPaid);
    assert_eq!(
        read("the discarded card", &environment)
            .expect("cost-paid card")
            .object,
        DescribedObject::CostPaid(CostPaidConditionObject::DiscardedCard)
    );

    // No exile of the program: the linked exile of the source, in a program
    // and in a static ability alike.
    for environment in [program(), BindingEnvironment::default()] {
        let linked = read("the exiled card", &environment).expect("linked exile");
        assert_eq!(linked.object, DescribedObject::LinkedExile);
        assert_eq!(linked.producer, DescriptionProducer::LinkedExile);
    }
}

#[test]
fn a_departed_event_object_is_the_creature_that_died() {
    let mut environment = program();
    environment.frame.event_object = Some(EventObjectSlot {
        reference: PermanentEffectReference::Observed(TypedObservedEventObject::EventPermanent),
        predicate: ObjectPredicate::CardType(ObjectCardType::Creature),
        subject: true,
        departed: true,
        controller: None,
    });
    let died = read("the creature that died", &environment).expect("event object");
    assert_eq!(died.producer, DescriptionProducer::EventObject);
    assert_eq!(
        died.object,
        DescribedObject::Bound(PermanentEffectReference::Observed(
            TypedObservedEventObject::EventPermanent
        ))
    );
    // An object still on the battlefield did not die.
    if let Some(slot) = environment.frame.event_object.as_mut() {
        slot.departed = false;
    }
    assert_eq!(
        read("the creature that died", &environment),
        Err(DIED_WITHOUT_EVENT.to_string())
    );
}

/// Distinct dependency diagnostics: a description no producer makes is
/// never read as another object.
#[test]
fn a_description_without_a_producer_keeps_its_own_diagnostic() {
    // `this way` names an instruction or a cost of this ability, never a
    // linked exile of another ability.
    assert_eq!(
        read("the card exiled this way", &program()),
        Err(THIS_WAY_WITHOUT_PRODUCER.to_string())
    );
    // A destroyed or returned object has no cost producer.
    assert_eq!(
        read("the destroyed creature", &program()),
        Err(DESCRIPTION_WITHOUT_PRODUCER.to_string())
    );
    // Outside an effect program no cost is in scope.
    assert_eq!(
        read("the sacrificed creature", &BindingEnvironment::default()),
        Err(DESCRIPTION_WITHOUT_PRODUCER.to_string())
    );
    // A discard pays with a card: `the discarded creature` is no cost object.
    assert!(read("the discarded creature", &program()).is_err());
    // An action result whose selection does not guarantee the nominal is
    // not restated by it.
    let mut environment = program();
    environment.result_object = Some(result(
        ActionResultVerb::Discarded,
        ObjectPredicate::ObjectKind(ObjectKind::Card),
    ));
    assert_eq!(
        read("the discarded card", &environment)
            .expect("the discarded card result")
            .object,
        DescribedObject::Bound(result_reference())
    );
    // `the creature discarded this way` names neither that card nor a
    // discard cost's card.
    assert!(read("the creature discarded this way", &environment).is_err());
    // A declared card target of a card zone change may be the exiled card:
    // it is not read as a linked exile.
    let mut environment = program();
    environment.card_target = Some(DeclaredTargetAntecedent {
        binding: TargetBinding {
            effect_index: 0,
            slot_index: 0,
        },
        selector: TargetSelector::Object(ObjectPredicate::ObjectKind(ObjectKind::Card)),
    });
    assert!(read("the exiled card", &environment).is_err());
}

fn parse(type_line: &str, oracle: &str) -> (String, Vec<Value>, Vec<String>) {
    let request = OracleCardParseRequest {
        card_name: "Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    let states = parsed
        .abilities
        .iter()
        .flat_map(|ability| ability.diagnostics.iter())
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect();
    let rules = parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .collect();
    (parsed.status, rules, states)
}

/// Every `sharesCharacteristic` relation anchor of the rules.
fn share_anchors(rules: &[Value]) -> Vec<Value> {
    fn collect(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(map) => {
                if map.get("kind").and_then(Value::as_str) == Some("sharesCharacteristic") {
                    found.push(map["object"].clone());
                }
                map.values().for_each(|child| collect(child, found));
            }
            Value::Array(items) => items.iter().for_each(|child| collect(child, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    rules.iter().for_each(|rule| collect(rule, &mut found));
    found
}

/// The relation anchor names the producer's record for each producer kind:
/// a cost record (Endemic Plague, Sanguine Praetor), the linked exile
/// (Ixalan's Binding) and an unused recombination of a cost-paid card with
/// a counters consumer.
#[test]
fn relation_anchors_name_each_producer_record() {
    let sacrificed = json!({"kind":"decisionResult","decisionId":"sacrificedPermanentObject"});
    let discarded = json!({"kind":"decisionResult","decisionId":"discardedCardObject"});
    let linked = json!({"kind":"cardsExiledWithSource","link":"pair"});
    for (type_line, oracle, anchor) in [
        (
            "Sorcery",
            "As an additional cost to cast this spell, sacrifice a creature.\nDestroy all creatures that share a creature type with the sacrificed creature. They can't be regenerated.",
            &sacrificed,
        ),
        (
            "Creature \u{2014} Phyrexian Praetor",
            "{B}, Sacrifice a creature: Destroy each creature with the same mana value as the sacrificed creature.",
            &sacrificed,
        ),
        (
            "Enchantment",
            "{2}, Discard a card: Put a +1/+1 counter on each creature you control that shares a color with the discarded card.",
            &discarded,
        ),
        (
            "Enchantment",
            "When this enchantment enters, exile target nonland permanent an opponent controls until this enchantment leaves the battlefield.\nYour opponents can't cast spells with the same name as the exiled card.",
            &linked,
        ),
    ] {
        let (status, rules, states) = parse(type_line, oracle);
        assert_eq!(status, "canonical", "{oracle}: {states:?}");
        assert_eq!(share_anchors(&rules), vec![anchor.clone()], "{oracle}");
    }
}

/// The cost record is installed on exactly the producing cost.
#[test]
fn a_cost_record_anchor_installs_its_producing_cost() {
    let (status, rules, states) = parse(
        "Creature \u{2014} Phyrexian Praetor",
        "{B}, Sacrifice a creature: Destroy each creature with the same mana value as the sacrificed creature.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    let costs = rules[0]["costs"].as_array().cloned().unwrap_or_default();
    assert!(
        costs.iter().any(|cost| cost["kind"] == "sacrificePermanent"
            && cost["bindObjectAs"] == "sacrificedPermanentObject"),
        "{costs:?}"
    );
}

/// Numeric operands through the same reader: the event's departed object
/// (Death's Presence), an earlier action written `this way`, and a cost
/// written `this way`.
#[test]
fn numeric_postposed_genitives_read_the_described_object() {
    for (type_line, oracle) in [
        (
            "Enchantment",
            "Whenever a creature you control dies, put X +1/+1 counters on target creature you control, where X is the power of the creature that died.",
        ),
        (
            "Sorcery",
            "Target player sacrifices a creature. You gain life equal to the toughness of the creature sacrificed this way.",
        ),
        (
            "Artifact Creature \u{2014} Construct",
            "Tap an untapped creature you control: This creature gets +X/+0 until end of turn, where X is the power of the creature tapped this way.",
        ),
    ] {
        let (status, _, states) = parse(type_line, oracle);
        assert_eq!(status, "canonical", "{oracle}: {states:?}");
    }
}

#[test]
fn near_misses_keep_distinct_dependency_failures() {
    for (type_line, oracle, state) in [
        (
            "Sorcery",
            "Draw a card. Destroy all creatures that share a color with the creature exiled this way.",
            "reference.definite-description.producer",
        ),
        (
            "Enchantment",
            "Whenever a creature you control dies, destroy all creatures that share a color with the creature that died.",
            "selector.relation.anchor.description",
        ),
        (
            "Enchantment",
            "Whenever a creature you control attacks, you gain life equal to the power of the creature that died.",
            "reference.definite-description.producer",
        ),
        (
            "Enchantment",
            "Players can't cast spells with the same name as the exiled card.",
            "binding.linked-exile.producer",
        ),
    ] {
        let (status, _, states) = parse(type_line, oracle);
        assert_ne!(status, "canonical", "{oracle}");
        assert!(
            states.iter().any(|actual| actual == state),
            "{oracle}: {states:?}"
        );
    }
}

//! Zone of function is one typed value for statics and triggers.
use super::*;

fn static_in(zone: &str) -> Value {
    json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":zone}},
        "modifiers":[{"kind":"prohibitPlayerAction",
            "players":{"kind":"opponentsOf","player":{"kind":"abilityController"}},
            "action":"gainLife"}]})
}

#[test]
fn static_and_trigger_zones_share_one_classifier() {
    assert_eq!(
        static_zone_of_function(&static_in("battlefield")),
        Some(ZoneOfFunction::Battlefield)
    );
    assert_eq!(
        static_zone_of_function(&static_in("graveyard")),
        Some(ZoneOfFunction::Graveyard)
    );
    // Near miss: a non-self object is not a zone of function.
    let mut other = static_in("graveyard");
    other["activeWhile"]["object"] = json!({"kind":"enchantedPermanent"});
    assert_eq!(static_zone_of_function(&other), None);

    let trigger = json!({"kind":"triggeredAbility","triggerZone":"commandZoneOrBattlefield"});
    let zone = trigger_zone_of_function(&trigger);
    assert!(zone.functions_in("commandZone") && zone.functions_in("battlefield"));
    assert!(!zone.functions_in("hand"));
    let printed = trigger_zone_of_function(&json!({"kind":"triggeredAbility"}));
    assert!(printed.functions_in("battlefield") && !printed.functions_in("exile"));
}

#[test]
fn card_zone_statics_reuse_the_battlefield_modifier_vocabulary() {
    assert!(card_zone_static_rule_supported(&static_in("graveyard")));
    assert!(card_zone_static_rule_supported(&static_in("hand")));
    // The battlefield and stack keep their own collectors.
    assert!(!card_zone_static_rule_supported(&static_in("battlefield")));
    assert!(!card_zone_static_rule_supported(&static_in("stack")));
    let mut unknown = static_in("graveyard");
    unknown["modifiers"][0]["kind"] = json!("notAModifier");
    assert!(!card_zone_static_rule_supported(&unknown));
}

fn graveyard_trigger(event: Value) -> Value {
    json!({"kind":"triggeredAbility","source":{"kind":"self"},"triggerZone":"graveyard",
        "event":event,"effects":[{"kind":"moveAbilitySourceToHand"}]})
}

/// A graveyard rule is admitted only for event alternatives a graveyard
/// card observes (CR 113.6); a spell's own cast and a look-back departure
/// are not observed from a graveyard.
#[test]
fn graveyard_triggers_are_admitted_only_for_observed_events() {
    let controller = json!({"kind":"controllerOf","object":{"kind":"self"}});
    let step = json!({"kind":"stepBegan","step":"upkeep","player":controller.clone()});
    assert!(trigger_zone_observed(&graveyard_trigger(step.clone())));
    assert!(graveyard_event_observed(
        &json!({"kind":"spellCast","player":controller.clone(),"where":null})
    ));
    assert!(!graveyard_event_observed(
        &json!({"kind":"spellCast","sourceIsSelf":true,"where":null})
    ));
    let departure = json!({"kind":"permanentLeftBattlefield","object":{"kind":"self"}});
    assert!(!graveyard_event_observed(&departure));
    let arrival = json!({"kind":"eventPattern","lemma":"move",
        "subject":{"object":{"kind":"cards","owner":controller.clone()}},
        "from":"battlefield","to":"graveyard"});
    assert!(graveyard_event_observed(&arrival));
    // Every alternative of a list must be observed.
    assert!(!trigger_zone_observed(&graveyard_trigger(
        json!({"kind":"oneOf","events":[step.clone(), departure]})
    )));
    // A battlefield rule is unaffected; a graveyard rule does not function
    // on a permanent.
    let mut battlefield = graveyard_trigger(step);
    assert!(functions_only_off_battlefield(&battlefield));
    battlefield.as_object_mut().unwrap().remove("triggerZone");
    assert!(trigger_zone_observed(&battlefield));
    assert!(!functions_only_off_battlefield(&battlefield));
}

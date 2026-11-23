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

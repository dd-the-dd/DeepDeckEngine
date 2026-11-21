use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_entities, resolve_counter_name_spans},
};

fn with_input<T>(text: &str, type_line: &str, operation: impl FnOnce(&AbilityInput<'_>) -> T) -> T {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_entities(text);
    resolve_counter_name_spans(&mut tokens);
    operation(&AbilityInput {
        face_name: "Synthetic damage replacement",
        face_type_line: type_line,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn modifier(text: &str, type_line: &str) -> Value {
    with_input(
        text,
        type_line,
        |input| match try_parse_damage_replacement(input) {
            DamageReplacementParseAttempt::Parsed(draft) => draft.rule["modifiers"][0].clone(),
            DamageReplacementParseAttempt::Failed(failure) => {
                panic!("{text}: {failure:?}")
            }
            DamageReplacementParseAttempt::NotClaimed => panic!("{text}: not claimed"),
        },
    )
}

fn failure_state(text: &str, type_line: &str) -> String {
    with_input(
        text,
        type_line,
        |input| match try_parse_damage_replacement(input) {
            DamageReplacementParseAttempt::Failed(failure) => failure.state.to_string(),
            DamageReplacementParseAttempt::Parsed(draft) => panic!("{text}: parsed {}", draft.rule),
            DamageReplacementParseAttempt::NotClaimed => panic!("{text}: not claimed"),
        },
    )
}

fn event_quantity() -> Value {
    json!({ "kind": "eventQuantity" })
}

fn integer(value: i64) -> Value {
    json!({ "kind": "integer", "value": value })
}

#[test]
fn source_query_recipients_and_doubling_compose_one_replacement() {
    // Torbran, Thane of Red Fell: a characteristic before the event-supplied
    // `source` head, a controller relation and two recipient alternatives.
    let torbran = modifier(
        "If a red source you control would deal damage to an opponent or a permanent an opponent controls, it deals that much damage plus 2 instead.",
        "Legendary Creature — Dwarf Noble",
    );
    assert_eq!(
        torbran,
        json!({
            "kind": "replaceDamage",
            "event": {
                "damage": "any",
                "source": {
                    "kind": "objects",
                    "where": { "kind": "colorContains", "value": "Red" },
                    "controller": controller(),
                },
                "recipients": [
                    { "kind": "player", "player": { "kind": "opponentsOf", "player": controller() } },
                    {
                        "kind": "permanents",
                        "controller": { "kind": "opponentsOf", "player": controller() },
                    },
                ],
            },
            "replacement": {
                "kind": "modifyAmount",
                "quantity": { "kind": "add", "left": event_quantity(), "right": integer(2) },
            },
        })
    );
    // Furnace of Rath: any source, `a permanent or player` with an elided
    // article, a scaled amount and the restating `to that permanent or player`.
    let furnace = modifier(
        "If a source would deal damage to a permanent or player, it deals double that damage to that permanent or player instead.",
        "Enchantment",
    );
    assert_eq!(furnace["event"]["source"], json!({ "kind": "objects" }));
    assert_eq!(
        furnace["event"]["recipients"],
        json!([
            { "kind": "permanents" },
            { "kind": "player", "player": { "kind": "eachPlayer" } },
        ])
    );
    assert_eq!(
        furnace["replacement"]["quantity"],
        json!({ "kind": "multiply", "left": integer(2), "right": event_quantity() })
    );
}

#[test]
fn prevention_amounts_are_event_arithmetic() {
    for (text, amount) in [
        (
            "If a black source would deal damage to you, prevent 2 of that damage.",
            integer(2),
        ),
        (
            "If a source would deal 3 or less damage to this creature, prevent that damage.",
            event_quantity(),
        ),
        (
            "If a source would deal damage to another Dinosaur you control, prevent all but 1 of that damage.",
            json!({ "kind": "subtract", "left": event_quantity(), "right": integer(1) }),
        ),
        (
            "If a source would deal damage to you or a permanent you control, prevent half that damage, rounded up.",
            json!({ "kind": "divide", "left": event_quantity(), "right": integer(2), "round": "up" }),
        ),
    ] {
        let modifier = modifier(text, "Creature — Dinosaur");
        assert_eq!(modifier["replacement"]["kind"], "prevent", "{text}");
        assert_eq!(modifier["replacement"]["amount"], amount, "{text}");
    }
    let callous = modifier(
        "If a source would deal 3 or less damage to this creature, prevent that damage.",
        "Creature — Giant",
    );
    assert_eq!(callous["event"]["maximum"], 3);
    assert_eq!(callous["event"]["recipients"], json!([self_ref()]));
}

#[test]
fn unused_combination_needs_no_new_production() {
    // Not printed on any card: an excluded-source artifact query with a
    // combat-only guard, a planeswalker recipient and a halved amount.
    let modifier = modifier(
        "If another artifact you control would deal 2 or more combat damage to a planeswalker an opponent controls, it deals half that damage, rounded down, to that permanent instead.",
        "Artifact",
    );
    assert_eq!(
        modifier["event"],
        json!({
            "damage": "combat",
            "minimum": 2,
            "source": {
                "kind": "objects",
                "where": { "kind": "cardTypeContains", "value": "Artifact" },
                "controller": controller(),
                "excludeSource": true,
            },
            "recipients": [{
                "kind": "permanents",
                "where": { "kind": "cardTypeContains", "value": "Planeswalker" },
                "controller": { "kind": "opponentsOf", "player": controller() },
            }],
        })
    );
}

#[test]
fn attachment_and_spell_sources_keep_their_domains() {
    let flail = modifier(
        "If equipped creature would deal combat damage, it deals double that damage instead.",
        "Artifact — Equipment",
    );
    assert_eq!(
        flail["event"]["source"],
        json!({ "kind": "attachedPermanent", "attachment": self_ref() })
    );
    let aura = modifier(
        "If enchanted creature would deal combat damage to a permanent or player, it deals double that damage instead.",
        "Enchantment — Aura",
    );
    assert_eq!(aura["event"]["damage"], "combat");
    let servant = modifier(
        "If a red instant or sorcery spell you control would deal damage, it deals double that damage instead.",
        "Creature — Elemental",
    );
    assert_eq!(servant["event"]["source"]["domain"], "spell");
    assert!(servant["event"].get("recipients").is_none());
}

#[test]
fn near_misses_fail_inside_the_committed_frame() {
    for (text, state) in [
        // A different replacing action is not a damage amount.
        (
            "If damage would be dealt to this creature, put that many +1/+1 counters on it instead.",
            "replacement.damage-modification.subject",
        ),
        // A second action after the prevention is not this modification.
        (
            "If a source would deal damage to you, prevent that damage and put an incarnation counter on this enchantment.",
            "replacement.damage-event.terminator",
        ),
        // A trailing state condition is not part of the event pattern.
        (
            "If damage would be dealt to this creature while it has a +1/+1 counter on it, prevent that damage.",
            "replacement.damage-event.separator",
        ),
        // The restated recipient must be one the event admits.
        (
            "If a source would deal damage to an opponent, it deals double that damage to that permanent instead.",
            "replacement.damage-modification.recipient-anaphor",
        ),
        // An amount defined elsewhere (`X`) has no event-quantity producer.
        (
            "If a source would deal damage to you, prevent X of that damage.",
            "replacement.damage-prevention.literal",
        ),
    ] {
        assert_eq!(failure_state(text, "Enchantment"), state, "{text}");
    }
    // A nonpermanent face cannot own a battlefield-active replacement.
    assert_eq!(
        failure_state(
            "If a source would deal damage to you, prevent 1 of that damage.",
            "Instant"
        ),
        "replacement.damage-event.source-domain"
    );
}

#[test]
fn unrelated_conditional_envelopes_are_not_claimed() {
    for text in [
        "If you control a creature, draw a card.",
        "If this creature would die, exile it instead.",
        "If a creature dealt damage this way would die this turn, exile it instead.",
        "If you would gain life, you gain twice that much life instead.",
    ] {
        with_input(text, "Enchantment", |input| {
            assert!(
                matches!(
                    try_parse_damage_replacement(input),
                    DamageReplacementParseAttempt::NotClaimed
                ),
                "{text}"
            );
        });
    }
}

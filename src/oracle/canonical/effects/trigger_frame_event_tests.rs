//! Trigger frames claimed by their introducer: each event atom lowers to the
//! runtime matcher that observes it, and unread events fail inside the frame.
use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn program(text: &str, face_type: &str) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic trigger frame",
        face_type_line: face_type,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn event(text: &str, face_type: &str) -> Value {
    match program(text, face_type) {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule["event"].clone(),
        GenericTriggerProgramAttempt::FailedCommitted(failure)
        | GenericTriggerProgramAttempt::RejectedCandidate(failure) => {
            panic!("{text}: {failure}")
        }
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not applicable"),
    }
}

fn committed_state(text: &str) -> &'static str {
    match program(text, "Creature") {
        GenericTriggerProgramAttempt::FailedCommitted(failure) => failure.state,
        GenericTriggerProgramAttempt::Parsed(_) => panic!("{text}: unexpectedly parsed"),
        GenericTriggerProgramAttempt::RejectedCandidate(failure) => {
            panic!("{text}: claimed trigger was only rejected: {failure}")
        }
        GenericTriggerProgramAttempt::NotApplicable => panic!("{text}: not claimed"),
    }
}

fn step(step: &str, player: Value) -> Value {
    json!({"kind": "stepBegan", "step": step, "player": player})
}

#[test]
fn turn_owner_and_step_name_compose_independently() {
    let opponents = json!({"kind": "opponentsOf", "player": controller()});
    let each = json!({"kind": "eachPlayer"});
    for (text, expected) in [
        (
            "At the beginning of each player's upkeep, draw a card.",
            step("upkeep", each.clone()),
        ),
        (
            "At the beginning of each opponent's end step, draw a card.",
            step("endStep", opponents.clone()),
        ),
        (
            "At the beginning of your opponents' upkeep, draw a card.",
            step("upkeep", opponents.clone()),
        ),
        // CR 505.1a: the second main phase is the postcombat main phase.
        (
            "At the beginning of your second main phase, draw a card.",
            step("postcombatMain", controller()),
        ),
        (
            "At the beginning of each of your postcombat main phases, draw a card.",
            step("postcombatMain", controller()),
        ),
        (
            "At the beginning of combat on each opponent's turn, draw a card.",
            step("beginCombat", opponents.clone()),
        ),
        // Unused combination: another owner with a renamed main phase.
        (
            "At the beginning of each player's first main phase, draw a card.",
            step("precombatMain", each.clone()),
        ),
    ] {
        assert_eq!(event(text, "Creature"), expected, "{text}");
    }
    assert_eq!(
        event(
            "At the beginning of the upkeep of enchanted creature's controller, draw a card.",
            "Enchantment — Aura",
        ),
        step(
            "upkeep",
            json!({"kind": "controllerOfAttachedPermanent", "attachment": self_ref()})
        )
    );
}

#[test]
fn source_events_lower_to_self_matchers_in_written_order() {
    let object = self_ref();
    for (text, face, expected) in [
        (
            "When this creature leaves the battlefield, draw a card.",
            "Creature",
            json!({"kind": "permanentLeftBattlefield", "object": object}),
        ),
        (
            "When this artifact is put into a graveyard from the battlefield, draw a card.",
            "Artifact",
            json!({"kind": "permanentLeftBattlefield", "object": object, "destination": "graveyard"}),
        ),
        (
            "Whenever this creature attacks or blocks, draw a card.",
            "Creature",
            json!({"kind": "oneOf", "events": [
                {"kind": "declaredAttacker", "object": object},
                {"kind": "declaredBlocker", "object": object},
            ]}),
        ),
        (
            "Whenever this creature blocks or becomes blocked, draw a card.",
            "Creature",
            json!({"kind": "oneOf", "events": [
                {"kind": "declaredBlocker", "object": object},
                {"kind": "permanentBecameBlocked", "object": object},
            ]}),
        ),
        // Unused combination: entry and departure of the same source.
        (
            "Whenever this enchantment enters or leaves the battlefield, draw a card.",
            "Enchantment",
            json!({"kind": "oneOf", "events": [
                {"kind": "enterBattlefield", "object": object},
                {"kind": "permanentLeftBattlefield", "object": object},
            ]}),
        ),
        // A lone attack keeps the established source-attack atom.
        (
            "Whenever this creature attacks, draw a card.",
            "Creature",
            json!({"kind": "declaredAttacker", "object": object}),
        ),
    ] {
        assert_eq!(event(text, face), expected, "{text}");
    }
}

#[test]
fn an_attached_noun_restates_the_attachment_and_admits_tapping() {
    // Psychic Venom's frame; the Aura's attachment fixes the object.
    for noun in ["land", "artifact", "creature"] {
        let text = format!("Whenever enchanted {noun} becomes tapped, draw a card.");
        assert_eq!(
            event(&text, "Enchantment — Aura"),
            json!({"kind": "attachedPermanentTapped", "attachment": self_ref()}),
            "{text}"
        );
    }
    // Only a creature takes part in combat.
    let GenericTriggerProgramAttempt::FailedCommitted(failure) = program(
        "Whenever enchanted land attacks, draw a card.",
        "Enchantment — Aura",
    ) else {
        panic!("an attached land event is claimed and rejected")
    };
    assert_eq!(failure.state, "event.attached-object.action");
}

#[test]
fn cycling_and_ordinal_draw_events_reuse_existing_matchers() {
    // Krosan Tusker's frame on a nonpermanent face: CR 702.29 cycling
    // triggers function while the card is cycled from the hand.
    assert_eq!(
        event("When you cycle this card, draw a card.", "Instant"),
        json!({"kind": "sourceCycled", "object": self_ref()})
    );
    // Mystic Skyfish: `each turn` and `in a turn` name the same period.
    for text in [
        "Whenever you draw your second card each turn, draw a card.",
        "Whenever you draw your second card in a turn, draw a card.",
    ] {
        assert_eq!(
            event(text, "Creature"),
            json!({"kind": "cardDrawn", "player": controller(), "drawOrdinal": 2}),
            "{text}"
        );
    }
}

#[test]
fn claimed_frames_report_the_furthest_event_failure() {
    for (text, state) in [
        // No self matcher observes a source becoming untapped.
        (
            "Whenever this creature becomes untapped, draw a card.",
            "trigger.source-event.verb-support",
        ),
        // A counterpart with a numeric characteristic is a typed attribute
        // the runtime matcher cannot evaluate: an engine gap, not grammar.
        (
            "Whenever this creature blocks or becomes blocked by a creature with power 1 or less, draw a card.",
            "trigger.event.engine-gap",
        ),
        (
            "When this creature is put into your graveyard from the battlefield, draw a card.",
            "event.source.put-into-graveyard.zone",
        ),
        (
            "At end of combat, draw a card.",
            "trigger.envelope.step-prefix",
        ),
        (
            "At the beginning of enchanted player's upkeep, draw a card.",
            "event.step.scope",
        ),
        // A label naming a mechanic is not a rules-free flavor word (CR 207.2c).
        (
            "Exile Cannon \u{2014} When this creature enters, draw a card.",
            "trigger.envelope.label",
        ),
    ] {
        assert_eq!(committed_state(text), state, "{text}");
    }
    // An unknown verb is located at the verb token, whichever actor-verb
    // alternative reported it.
    let GenericTriggerProgramAttempt::FailedCommitted(failure) =
        program("Whenever you frobnicate, draw a card.", "Creature")
    else {
        panic!("the trigger word claims an unknown event")
    };
    assert_eq!(failure.token_index, 2);
    // A permanent-observed event still needs a permanent source face.
    let GenericTriggerProgramAttempt::FailedCommitted(failure) = program(
        "When this creature leaves the battlefield, draw a card.",
        "Instant",
    ) else {
        panic!("a battlefield observer on an instant face must fail inside the frame")
    };
    assert_eq!(failure.state, "trigger.program.permanent-face");
}

#[test]
fn controlled_object_combat_and_departure_verbs_reuse_controlled_matchers() {
    for (text, kind, exclude_source, destination) in [
        (
            "Whenever a creature you control becomes blocked, draw a card.",
            "controlledCreatureBecameBlocked",
            None,
            None,
        ),
        // Unused combination: a compound type predicate with the block verb.
        (
            "Whenever an artifact creature you control blocks, draw a card.",
            "controlledCreatureDeclaredBlocker",
            None,
            None,
        ),
        (
            "Whenever another creature you control leaves the battlefield, draw a card.",
            "controlledPermanentLeftBattlefield",
            Some(true),
            None,
        ),
        (
            "Whenever an enchantment you control is put into a graveyard from the battlefield, draw a card.",
            "controlledPermanentLeftBattlefield",
            Some(false),
            Some("graveyard"),
        ),
    ] {
        let lowered = event(text, "Creature");
        assert_eq!(lowered["kind"], kind, "{text}");
        assert_eq!(lowered["player"], controller(), "{text}");
        assert!(lowered["where"].is_object(), "{text}");
        assert_eq!(lowered["excludeSource"].as_bool(), exclude_source, "{text}");
        assert_eq!(lowered["destination"].as_str(), destination, "{text}");
    }
    for (text, state) in [
        // A one-or-more departure batch has no observer (any controller's
        // single departure is the generic move pattern, tested below).
        (
            "Whenever one or more artifacts are put into a graveyard from the battlefield, draw a card.",
            "trigger.object-event.departure-support",
        ),
        (
            "Whenever a creature an opponent controls becomes tapped, draw a card.",
            "event.object.complement",
        ),
        (
            "Whenever one or more creatures you control become blocked, draw a card.",
            "trigger.object-event.verb-support",
        ),
    ] {
        assert_eq!(committed_state(text), state, "{text}");
    }
}

fn source_pattern(lemma: &str, role: &str) -> Value {
    json!({"kind": "eventPattern", "lemma": lemma,
        "subject": {"role": role, "object": {"kind": "self"}}})
}

#[test]
fn combat_modifiers_lower_to_generic_event_patterns() {
    let mut alone = source_pattern("attack", "attacker");
    alone["attackingAlone"] = json!(true);
    let mut unblocked = source_pattern("becomeUnblocked", "attacker");
    unblocked["declaredAttacker"] = json!(true);
    let mut first_attack = source_pattern("attack", "attacker");
    first_attack["turnOrdinal"] = json!(1);
    let mut first_block = source_pattern("block", "blocker");
    first_block["turnOrdinal"] = json!(1);
    let mut alone_first = alone.clone();
    alone_first["turnOrdinal"] = json!(1);
    for (text, expected) in [
        // Rogue Kavu, Eternal of Harsh Truths, Godo's ordinal.
        (
            "Whenever this creature attacks alone, draw a card.",
            alone.clone(),
        ),
        (
            "Whenever this creature attacks and isn't blocked, draw a card.",
            unblocked,
        ),
        (
            "Whenever this creature attacks for the first time each turn, draw a card.",
            first_attack,
        ),
        // Siblings: the ordinal of a block, two modifiers together, and an
        // unmodified verb of a modified union keeping its historical matcher.
        (
            "Whenever this creature blocks for the first time each turn, draw a card.",
            first_block,
        ),
        (
            "Whenever this creature attacks alone for the first time each turn, draw a card.",
            alone_first,
        ),
        (
            "Whenever this creature attacks alone or blocks, draw a card.",
            json!({"kind": "oneOf", "events": [
                alone,
                {"kind": "declaredBlocker", "object": {"kind": "self"}},
            ]}),
        ),
    ] {
        assert_eq!(event(text, "Creature"), expected, "{text}");
    }
}

#[test]
fn block_counterparts_are_typed_queries_of_the_other_combatant() {
    // Netcaster Spider: the attacker it blocks.
    let spider = event(
        "Whenever this creature blocks a creature with flying, draw a card.",
        "Creature",
    );
    assert_eq!(spider["subject"]["role"], "blocker");
    assert_eq!(spider["counterpart"]["role"], "attacker");
    assert!(!spider["counterpart"]["where"].is_null());
    assert!(spider["counterpart"].get("oneOrMore").is_none());
    // `by a creature` is one event per blocker (CR 509.3d), not `becomes
    // blocked` once per combat.
    let wolverines = event(
        "Whenever this creature becomes blocked by a creature, draw a card.",
        "Creature",
    );
    assert_eq!(wolverines["lemma"], "block");
    assert_eq!(wolverines["subject"]["role"], "attacker");
    assert_eq!(wolverines["counterpart"]["role"], "blocker");
    assert_eq!(
        event(
            "Whenever this creature becomes blocked, draw a card.",
            "Creature"
        ),
        json!({"kind": "permanentBecameBlocked", "object": {"kind": "self"}})
    );
    // Dwarven Soldier: a trailing `one or more` counterpart is shared by the
    // bare `blocks` of the union.
    let soldier = event(
        "Whenever this creature blocks or becomes blocked by one or more Orcs, draw a card.",
        "Creature",
    );
    let branches = soldier["events"].as_array().expect("union");
    assert_eq!(branches.len(), 2);
    assert_eq!(branches[0]["subject"]["role"], "blocker");
    assert_eq!(branches[1]["subject"]["role"], "attacker");
    for branch in branches {
        assert_eq!(branch["counterpart"]["oneOrMore"], true);
        assert_eq!(
            branch["counterpart"]["where"],
            branches[0]["counterpart"]["where"]
        );
    }
}

#[test]
fn combat_modifiers_without_a_runtime_matcher_remain_engine_gaps() {
    for text in [
        // No per-turn ordinal is recorded for a blocked attacker or tapping.
        "Whenever this creature becomes blocked for the first time each turn, draw a card.",
        "Whenever this creature becomes tapped for the first time each turn, draw a card.",
        // `and isn't blocked` completes one attack, not a union.
        "Whenever this creature blocks or attacks and isn't blocked, draw a card.",
        // Arrogant Bloodlord: a numeric counterpart characteristic.
        "Whenever this creature becomes blocked by a creature with power 1 or less, draw a card.",
    ] {
        assert_eq!(committed_state(text), "trigger.event.engine-gap", "{text}");
    }
}

#[test]
fn that_creature_after_a_block_counterpart_is_the_other_combatant() {
    let rule = |text: &str| match program(text, "Creature") {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule,
        _ => panic!("{text}: not parsed"),
    };
    // Ogre Leadfoot, Phyrexian Slayer, and a union sibling.
    for text in [
        "Whenever this creature becomes blocked by an artifact creature, destroy that creature.",
        "Whenever this creature becomes blocked by a white creature, destroy that creature. It can't be regenerated.",
        "Whenever this creature blocks or becomes blocked by a creature, destroy that creature.",
    ] {
        assert_eq!(
            rule(text)["effects"][0]["permanent"],
            json!({"kind": "combatCounterpart"}),
            "{text}"
        );
    }
    // Near misses: a branch without counterpart, or no counterpart at all.
    for text in [
        "Whenever this creature attacks or becomes blocked by a creature, destroy that creature.",
        "Whenever this creature becomes blocked, destroy that creature.",
    ] {
        assert!(
            !matches!(
                program(text, "Creature"),
                GenericTriggerProgramAttempt::Parsed(_)
            ),
            "{text}"
        );
    }
}

#[test]
fn deals_damage_without_recipient_is_the_aggregated_damage_pattern() {
    // Phantom Nishoba, Armadillo Cloak, and a combat-only sibling.
    for (text, face, source, kind) in [
        (
            "Whenever this creature deals damage, you gain that much life.",
            "Creature",
            json!({"kind": "self"}),
            None,
        ),
        (
            "Whenever enchanted creature deals damage, you gain that much life.",
            "Enchantment — Aura",
            json!({"kind": "attachedPermanent", "attachment": {"kind": "self"}}),
            None,
        ),
        (
            "Whenever this creature deals combat damage, you gain that much life.",
            "Creature",
            json!({"kind": "self"}),
            Some("combatOnly"),
        ),
    ] {
        let mut expected = json!({"kind": "eventPattern", "lemma": "dealDamage", "source": source});
        if let Some(flag) = kind {
            expected[flag] = json!(true);
        }
        assert_eq!(event(text, face), expected, "{text}");
    }
    // A named recipient keeps the historical per-recipient kind.
    assert_eq!(
        event(
            "Whenever this creature deals damage to a player, you gain that much life.",
            "Creature"
        )["kind"],
        "damageDealtBySource"
    );
    // Near miss: a one-or-more source batch is still unproved.
    assert_eq!(
        committed_state("Whenever one or more creatures you control deal damage, you gain 1 life."),
        "trigger.dealt-damage.source-support"
    );
}

#[test]
fn zone_changes_without_an_established_observer_lower_to_move_patterns() {
    // Dingus Egg, Disciple of the Vault, an opponent sibling, Ulamog.
    for (text, expected) in [
        (
            "Whenever a land is put into a graveyard from the battlefield, draw a card.",
            json!({"kind": "eventPattern", "lemma": "move",
                "subject": {"object": {"kind": "permanents",
                    "where": lower_permanent_object_predicate(
                        &ObjectPredicate::CardType(ObjectCardType::Land))}},
                "from": "battlefield", "to": "graveyard"}),
        ),
        (
            "Whenever a creature an opponent controls leaves the battlefield, draw a card.",
            json!({"kind": "eventPattern", "lemma": "move",
                "subject": {"object": {"kind": "permanents",
                    "controller": {"kind": "opponentsOf", "player": controller()},
                    "where": lower_permanent_object_predicate(
                        &ObjectPredicate::CardType(ObjectCardType::Creature))}},
                "from": "battlefield"}),
        ),
        (
            "When this creature is put into a graveyard from anywhere, draw a card.",
            json!({"kind": "eventPattern", "lemma": "move",
                "subject": {"object": {"kind": "self"}}, "to": "graveyard"}),
        ),
    ] {
        assert_eq!(event(text, "Creature"), expected, "{text}");
    }
    // A permanent the ability controller controls keeps its historical kind.
    assert_eq!(
        event(
            "Whenever a creature you control leaves the battlefield, draw a card.",
            "Creature"
        )["kind"],
        "controlledPermanentLeftBattlefield"
    );
    // Near misses: a one-or-more departure batch, and an owner relation on
    // the destination of a `from anywhere` move.
    for (text, state) in [
        (
            "Whenever one or more lands are put into a graveyard from the battlefield, draw a card.",
            "trigger.object-event.departure-support",
        ),
        (
            "When this creature is put into your graveyard from anywhere, draw a card.",
            "event.source.put-into-graveyard.zone",
        ),
    ] {
        assert_eq!(committed_state(text), state, "{text}");
    }
}

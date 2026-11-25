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
        (
            "At the beginning of your postcombat main phase, draw a card.",
            step("postcombatMain", controller()),
        ),
        // CR 505.1a: the second main phase is the postcombat main phase with
        // main-phase ordinal 2, not every postcombat main phase.
        (
            "At the beginning of your second main phase, draw a card.",
            {
                let mut second = step("postcombatMain", controller());
                second["mainPhaseOrdinal"] = json!(2);
                second
            },
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
        // No per-turn ordinal is recorded for untapping.
        (
            "Whenever this creature becomes untapped for the first time each turn, draw a card.",
            "engine.gap.event-matcher",
        ),
        // A counterpart with a numeric characteristic is a typed attribute
        // the runtime matcher cannot evaluate: an engine gap, not grammar.
        (
            "Whenever this creature blocks or becomes blocked by a creature with power 1 or less, draw a card.",
            "engine.gap.event-matcher",
        ),
        (
            "When this creature is put into your graveyard from the battlefield, draw a card.",
            "event.source.put-into-graveyard.origin",
        ),
        (
            "At end of turn, draw a card.",
            "trigger.envelope.step-prefix",
        ),
        (
            "At the beginning of enchanted player's upkeep, draw a card.",
            "event.step.scope",
        ),
        // A label naming a mechanic is not a rules-free flavor word (CR 207.2c).
        (
            "Max speed \u{2014} When this creature enters, draw a card.",
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
    // A controlled object's only `become` frame with a runtime matcher is
    // `the target of`, which claims only that relation; `becomes tapped`
    // fails at the object event's own complement.
    for (text, state) in [(
        "Whenever a creature an opponent controls becomes tapped, draw a card.",
        "event.object.complement",
    )] {
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
    // The blocked attacker's per-turn ordinal is recorded with the block.
    let mut first_blocked = source_pattern("block", "attacker");
    first_blocked["turnOrdinal"] = json!(1);
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
            "Whenever this creature becomes blocked for the first time each turn, draw a card.",
            first_blocked,
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
        // No per-turn ordinal is recorded for tapping.
        "Whenever this creature becomes tapped for the first time each turn, draw a card.",
        // `and isn't blocked` completes one attack, not a union.
        "Whenever this creature blocks or attacks and isn't blocked, draw a card.",
        // Arrogant Bloodlord: a numeric counterpart characteristic.
        "Whenever this creature becomes blocked by a creature with power 1 or less, draw a card.",
    ] {
        assert_eq!(committed_state(text), "engine.gap.event-matcher", "{text}");
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
    // Near miss: an owner relation on the destination of a source move.
    for (text, state) in [(
        "When this creature is put into your graveyard from the battlefield, draw a card.",
        "event.source.put-into-graveyard.origin",
    )] {
        assert_eq!(committed_state(text), state, "{text}");
    }
}

#[test]
fn sacrificed_object_predicates_read_the_departure_snapshot() {
    // Gleaming Geardrake, Tireless Tracker, Savra, Jenny Flint.
    for text in [
        "Whenever you sacrifice an artifact, draw a card.",
        "Whenever you sacrifice a Clue, draw a card.",
        "Whenever you sacrifice a black creature, draw a card.",
        "Whenever you sacrifice a Clue or Food, draw a card.",
        "Whenever a player sacrifices another creature, draw a card.",
    ] {
        let lowered = event(text, "Creature");
        assert_eq!(lowered["kind"], "permanentSacrificed", "{text}");
        assert!(lowered["where"].is_object(), "{text}");
    }
    // Near miss: a numeric characteristic is not captured at departure.
    assert_eq!(
        committed_state("Whenever you sacrifice a creature with power 3 or greater, draw a card."),
        "trigger.sacrifice.numeric-snapshot-support"
    );
}

#[test]
fn cast_patterns_read_the_source_spell_copies_and_matching_ordinals() {
    // `When you cast this spell` (Eldrazi): the source spell itself.
    let own = event(
        "When you cast this spell, draw a card.",
        "Creature — Eldrazi",
    );
    assert_eq!(own["kind"], "spellCast");
    assert_eq!(own["player"], controller());
    assert_eq!(own["sourceIsSelf"], true);
    // Magecraft: cast or copy, one predicate for both events.
    let magecraft = event(
        "Whenever you cast or copy an instant or sorcery spell, draw a card.",
        "Creature",
    );
    assert_eq!(magecraft["kind"], "oneOf");
    assert_eq!(magecraft["events"][0]["kind"], "spellCast");
    assert_eq!(magecraft["events"][1]["kind"], "spellCopied");
    assert_eq!(
        magecraft["events"][0]["where"],
        magecraft["events"][1]["where"]
    );
    // A matching-spell ordinal (sibling: an opponent's first).
    for text in [
        "Whenever you cast your first instant or sorcery spell each turn, draw a card.",
        "Whenever an opponent casts their first noncreature spell each turn, draw a card.",
    ] {
        let lowered = event(text, "Creature");
        assert_eq!(lowered["spellCastOrdinal"], 1, "{text}");
        assert!(!lowered["where"].is_null(), "{text}");
    }
    // Near misses: an ordinal over a color the spell history does not
    // record, and an opponent's copy (the copy observer's player is yours).
    for (text, state) in [
        (
            "Whenever you cast your second red spell each turn, draw a card.",
            "trigger.spell-cast.ordinal-basis-support",
        ),
        (
            "Whenever an opponent casts or copies an instant spell, draw a card.",
            "trigger.spell-cast.copy-support",
        ),
    ] {
        assert_eq!(committed_state(text), state, "{text}");
    }
}

#[test]
fn one_attacking_permanent_is_the_generic_attack_pattern() {
    let subject = |object: Value| {
        json!({"kind": "eventPattern", "lemma": "attack",
            "subject": {"role": "attacker", "object": object}})
    };
    let creature =
        lower_permanent_object_predicate(&ObjectPredicate::CardType(ObjectCardType::Creature));
    for (text, object) in [
        (
            "Whenever a creature you control attacks, draw a card.",
            json!({"kind": "permanents", "controller": controller(), "where": creature}),
        ),
        (
            "Whenever a creature attacks, draw a card.",
            json!({"kind": "permanents", "where": creature}),
        ),
        (
            "Whenever another creature an opponent controls attacks, draw a card.",
            json!({"kind": "permanents",
                "controller": {"kind": "opponentsOf", "player": controller()},
                "where": creature, "excludeSource": true}),
        ),
    ] {
        assert_eq!(event(text, "Enchantment"), subject(object), "{text}");
    }
    // The one-or-more batch keeps its own observer.
    assert_eq!(
        event(
            "Whenever one or more creatures you control attack, draw a card.",
            "Enchantment"
        )["kind"],
        "controlledCreaturesAttacked"
    );
}

#[test]
fn a_spell_that_targets_reads_the_shared_target_alternatives() {
    // Heroic, Repartee, and a player-or-permanent sibling.
    let heroic = event(
        "Whenever you cast a spell that targets this creature, draw a card.",
        "Creature",
    );
    assert_eq!(
        heroic["targets"],
        json!({"only": false, "anyOf": [{"kind": "self"}]})
    );
    let repartee = event(
        "Whenever you cast an instant or sorcery spell that targets a creature, draw a card.",
        "Creature",
    );
    assert_eq!(repartee["targets"]["anyOf"][0]["kind"], "permanents");
    assert!(repartee["targets"]["anyOf"][0]["where"].is_object());
    let sibling = event(
        "Whenever an opponent casts a spell that targets only you or a creature you control, draw a card.",
        "Creature",
    );
    assert_eq!(sibling["targets"]["only"], true);
    assert_eq!(
        sibling["targets"]["anyOf"][0],
        json!({"kind": "player", "player": controller()})
    );
    assert_eq!(sibling["targets"]["anyOf"][1]["controller"], controller());
    // Near miss: a target predicate with a numeric characteristic.
    assert_eq!(
        committed_state(
            "Whenever you cast a spell that targets a creature with power 3 or greater, draw a card."
        ),
        "trigger.spell-cast.target-support"
    );
}

#[test]
fn the_cast_origin_zone_and_turn_relation_are_pattern_modifiers() {
    for (text, field, value) in [
        (
            "Whenever you cast a spell from exile, draw a card.",
            "fromZone",
            json!("exile"),
        ),
        (
            "Whenever you cast a spell from your graveyard, draw a card.",
            "fromOwnZone",
            json!(true),
        ),
        (
            "Whenever you cast a spell from anywhere other than your hand, draw a card.",
            "fromZoneNot",
            json!("hand"),
        ),
        (
            "Whenever you cast a spell during an opponent's turn, draw a card.",
            "duringOpponentTurn",
            json!(true),
        ),
        (
            "Whenever an opponent casts a spell during your turn, draw a card.",
            "duringControllerTurn",
            json!(true),
        ),
    ] {
        assert_eq!(event(text, "Creature")[field], value, "{text}");
    }
    // Near miss: a zone the cast record does not carry.
    assert_eq!(
        committed_state("Whenever you cast a spell from the command zone, draw a card."),
        "event.spell-cast.origin-zone"
    );
}

#[test]
fn numeric_spell_characteristics_are_read_on_the_cast_record() {
    let mana_value = event(
        "Whenever you cast a spell with mana value 4 or greater, draw a card.",
        "Creature",
    );
    assert_eq!(mana_value["where"]["kind"], "compare");
    assert_eq!(mana_value["where"]["left"]["kind"], "manaValueOf");
    assert_eq!(
        mana_value["where"]["right"],
        json!({"kind": "integer", "value": 4})
    );
    let power = event(
        "Whenever a player casts a creature spell with power 4 or greater, draw a card.",
        "Creature",
    );
    assert_eq!(power["where"]["kind"], "and");
}

#[test]
fn one_or_more_moves_are_aggregated_move_patterns() {
    // Graveyard arrivals: owner relation, `from anywhere`, one or more.
    let arrivals = event(
        "Whenever one or more creature cards are put into your graveyard from anywhere, draw a card.",
        "Enchantment",
    );
    assert_eq!(arrivals["lemma"], "move");
    assert_eq!(arrivals["to"], "graveyard");
    assert_eq!(arrivals["aggregation"], "oneOrMore");
    assert_eq!(arrivals["subject"]["object"]["kind"], "cards");
    assert_eq!(arrivals["subject"]["object"]["owner"], controller());
    let opponents = event(
        "Whenever a land card is put into an opponent's graveyard, draw a card.",
        "Enchantment",
    );
    assert!(opponents.get("aggregation").is_none());
    assert_eq!(
        opponents["subject"]["object"]["owner"],
        json!({"kind": "opponentsOf", "player": controller()})
    );
    // Departures: one or more permanents you control.
    let departures = event(
        "Whenever one or more creatures you control leave the battlefield, draw a card.",
        "Enchantment",
    );
    assert_eq!(departures["from"], "battlefield");
    assert_eq!(departures["aggregation"], "oneOrMore");
    assert_eq!(departures["subject"]["object"]["controller"], controller());
    // Near miss: an origin the arrival record does not carry.
    assert_eq!(
        committed_state(
            "Whenever one or more cards are put into your graveyard from your library or hand, draw a card."
        ),
        "event.object.graveyard-origin"
    );
}

#[test]
fn a_transition_union_lowers_both_transitions_of_the_same_subjects() {
    let union = event(
        "Whenever another creature you control enters or dies, draw a card.",
        "Enchantment",
    );
    assert_eq!(union["kind"], "oneOf");
    assert_eq!(union["events"][0]["kind"], "permanentEntered");
    assert_eq!(union["events"][1]["kind"], "permanentDied");
    assert_eq!(union["events"][0]["where"], union["events"][1]["where"]);
    let reversed = event(
        "Whenever a creature you control dies or enters, draw a card.",
        "Enchantment",
    );
    assert_eq!(reversed["events"][0]["kind"], "permanentDied");
    // A third verb of the same subject is an occurrence union of the shared
    // subject's frames (CR 603.2), not part of the transition union.
    let occurrences = event(
        "Whenever a creature you control enters or attacks, draw a card.",
        "Enchantment",
    );
    assert_eq!(occurrences["kind"], "oneOf");
    assert_eq!(occurrences["events"][0]["kind"], "permanentEntered");
    assert_eq!(occurrences["events"][1]["lemma"], "attack");
    // Near miss: a verb no subject frame reads stays an explicit failure.
    assert!(
        !committed_state("Whenever a creature you control enters or explores, draw a card.")
            .is_empty()
    );
}

#[test]
fn arrival_origins_and_exile_departures_are_typed() {
    let source = event(
        "When this card is put into your graveyard from your library, draw a card.",
        "Creature",
    );
    assert_eq!(
        source,
        json!({"kind": "eventPattern", "lemma": "move",
            "subject": {"object": {"kind": "self"}}, "from": "library", "to": "graveyard"})
    );
    let cards = event(
        "Whenever a creature card is put into your graveyard from your library, draw a card.",
        "Enchantment",
    );
    assert_eq!(cards["from"], "library");
    assert_eq!(cards["subject"]["object"]["owner"], controller());
    let exile = event(
        "When this creature dies or is put into exile from the battlefield, draw a card.",
        "Creature",
    );
    assert_eq!(exile["events"][0]["kind"], "permanentDied");
    assert_eq!(
        exile["events"][1],
        json!({"kind": "permanentLeftBattlefield", "object": {"kind": "self"}, "destination": "exile"})
    );
    // Near misses: two origins, and an owner relation from the battlefield.
    for (text, state) in [
        (
            "Whenever a creature card is put into your graveyard from your library or hand, draw a card.",
            "event.object.graveyard-origin",
        ),
        (
            "When this creature is put into your graveyard from the battlefield, draw a card.",
            "event.source.put-into-graveyard.origin",
        ),
    ] {
        assert_eq!(committed_state(text), state, "{text}");
    }
}

#[test]
fn one_or_more_block_events_are_aggregated_block_patterns() {
    let blocked = event(
        "Whenever one or more creatures you control become blocked, draw a card.",
        "Enchantment",
    );
    assert_eq!(blocked["lemma"], "block");
    assert_eq!(blocked["subject"]["role"], "attacker");
    assert_eq!(blocked["subject"]["object"]["controller"], controller());
    assert_eq!(blocked["aggregation"], "oneOrMore");
    let blocking = event(
        "Whenever one or more creatures an opponent controls block, draw a card.",
        "Enchantment",
    );
    assert_eq!(blocking["subject"]["role"], "blocker");
    // The singular form keeps its historical kind.
    assert_eq!(
        event(
            "Whenever a creature you control becomes blocked, draw a card.",
            "Enchantment"
        )["kind"],
        "controlledCreatureBecameBlocked"
    );
}

#[test]
fn a_spell_that_has_a_keyword_adds_the_keyword_predicate() {
    let convoke = event(
        "Whenever you cast a spell that has convoke, draw a card.",
        "Creature",
    );
    assert_eq!(convoke["kind"], "spellCast");
    assert_eq!(convoke["where"]["kind"], "hasKeyword");
    let flying = event(
        "Whenever you cast a creature spell that has flying, draw a card.",
        "Creature",
    );
    assert_eq!(flying["where"]["kind"], "and");
    // Near miss: `of the chosen color` reads a symbolic choice the ability
    // does not produce; with no `As ... enters, choose a color` on the face,
    // the card-level binding rejects it instead of borrowing a value.
    let request = crate::oracle::OracleCardParseRequest {
        card_name: "Chosen Color Probe".to_string(),
        type_line: "Creature".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(
            "Whenever you cast a spell of the chosen color, draw a card.".to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || crate::oracle::parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_ne!(parsed.abilities[0].status, "canonical", "{parsed:#?}");
    assert!(
        parsed.abilities[0]
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic
                .state
                .as_deref()
                .is_some_and(|state| state.starts_with("binding.chosen-color"))),
        "{parsed:#?}"
    );
}

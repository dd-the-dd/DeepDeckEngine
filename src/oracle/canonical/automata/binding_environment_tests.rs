//! The shared binding environment: real cards, an unused recombination and
//! near misses for each reference it resolves. Runtime proofs live in
//! `engine/tests/binding_environment_runtime.rs`.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};

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

/// Every `dealDamage` recipient of the card's canonical rules.
fn damage_recipients(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .filter(|effect| effect["kind"] == "dealDamage")
        .map(|effect| effect["recipient"].clone())
        .collect()
}

const ENCHANTMENT: &str = "Enchantment";
const CREATURE: &str = "Creature \u{2014} Goblin";
const INSTANT: &str = "Instant";

#[test]
fn that_player_names_the_player_the_event_records() {
    let triggering = json!({"kind":"triggeringPlayer"});
    // Real cards: Megrim (discard), Underworld Dreams (draw), Eidolon of the
    // Great Revel (cast). Unused recombination: a draw trigger whose damage
    // recipient is followed by a gapped second damage predicate.
    for oracle in [
        "Whenever an opponent discards a card, this enchantment deals 2 damage to that player.",
        "Whenever an opponent draws a card, this enchantment deals 1 damage to that player.",
        "Whenever a player casts a spell with mana value 3 or less, this enchantment deals 2 damage to that player.",
        "Whenever an opponent draws a card, this enchantment deals 1 damage to that player and 1 damage to you.",
    ] {
        let recipients = damage_recipients(ENCHANTMENT, oracle);
        assert_eq!(recipients.first(), Some(&triggering), "{oracle}");
    }
}

#[test]
fn that_player_prefers_the_player_declared_earlier_in_the_program() {
    // `that player` after `target player` is the declared target, never the
    // event player.
    let recipients = damage_recipients(
        INSTANT,
        "Target player discards a card. This spell deals 2 damage to that player.",
    );
    assert_eq!(recipients[0]["kind"], "chosenTarget", "{recipients:?}");
}

#[test]
fn possessive_player_relation_reads_the_declared_target() {
    // Real cards: Consign to the Pit, Ionize; unused recombination: an
    // activated tap followed by damage to the tapped creature's owner.
    for (type_line, oracle, relation) in [
        (
            INSTANT,
            "Destroy target creature. This spell deals 2 damage to that creature's controller.",
            "controllerOf",
        ),
        (
            CREATURE,
            "{T}: Tap target creature. This creature deals 1 damage to that creature's owner.",
            "ownerOf",
        ),
    ] {
        let recipients = damage_recipients(type_line, oracle);
        let recipient = recipients.last().expect("a damage recipient");
        assert_eq!(recipient["kind"], relation, "{oracle}: {recipients:?}");
        assert_eq!(recipient["object"]["kind"], "chosenTarget", "{oracle}");
    }
}

#[test]
fn object_references_name_the_bound_object() {
    // Real cards: Abyssal Hunter (declared target), Ornery Goblin (the other
    // combatant of a block event).
    let recipients = damage_recipients(
        CREATURE,
        "{B}, {T}: Tap target creature. This creature deals damage equal to its power to that creature.",
    );
    assert_eq!(recipients[0]["kind"], "chosenTarget", "{recipients:?}");
    let recipients = damage_recipients(
        CREATURE,
        "Whenever this creature blocks or becomes blocked by a creature, this creature deals 1 damage to that creature.",
    );
    assert_eq!(recipients[0], json!({"kind":"combatCounterpart"}));
}

#[test]
fn references_without_a_binding_fail_explicitly() {
    for (type_line, oracle, state) in [
        // No event records a player and no player was declared.
        (
            CREATURE,
            "{T}: This creature deals 1 damage to that player.",
            "player-subject.bound.resolve",
        ),
        // The nominal does not follow from the declared selector.
        (
            INSTANT,
            "Destroy target artifact. This spell deals 2 damage to that creature's controller.",
            "player-subject.object-relation.antecedent",
        ),
        // `defending player` outside an attack trigger.
        (
            CREATURE,
            "{T}: This creature deals 1 damage to defending player.",
            "player-subject.defending-player",
        ),
    ] {
        let (status, _, states) = parse(type_line, oracle);
        assert_ne!(status, "canonical", "{oracle}");
        assert!(
            states.iter().any(|candidate| candidate == state),
            "{oracle}: {states:?}"
        );
    }
}

/// Every effect object reference (`permanent`/`object`) of the card's rules.
fn effect_objects(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .filter_map(|effect| {
            effect
                .get("permanent")
                .or_else(|| effect.get("object"))
                .cloned()
        })
        .collect()
}

#[test]
fn that_noun_names_the_object_the_event_record_binds() {
    let event_permanent = json!({"kind":"triggeringPermanent"});
    // Real cards: Stinkweed Imp (the damaged creature), Anointer of Valor's
    // attacker shape, Serra Redeemer (the entering creature).
    for (oracle, expected) in [
        (
            "Whenever this creature deals combat damage to a creature, destroy that creature.",
            event_permanent.clone(),
        ),
        (
            "Whenever a creature attacks, put a +1/+1 counter on that creature.",
            event_permanent.clone(),
        ),
        (
            "Whenever another creature you control with power 2 or less enters, put two +1/+1 counters on that creature.",
            json!({"kind":"observedEventObject","event":"permanentEntered"}),
        ),
    ] {
        let objects = effect_objects(CREATURE, oracle);
        assert_eq!(objects.first(), Some(&expected), "{oracle}: {objects:?}");
    }
}

#[test]
fn possessive_relation_reads_the_event_object() {
    // Real cards: Dingus Staff (the dying creature), Zo-Zu the Punisher (the
    // entering land). Unused recombination: a damaged creature's owner.
    for (type_line, oracle, relation, object) in [
        (
            "Artifact",
            "Whenever a creature dies, this artifact deals 2 damage to that creature's controller.",
            "controllerOf",
            "triggeringPermanent",
        ),
        (
            CREATURE,
            "Whenever a land enters, this creature deals 2 damage to that land's controller.",
            "controllerOf",
            "observedEventObject",
        ),
        (
            CREATURE,
            "Whenever this creature deals combat damage to a creature, this creature deals 1 damage to that creature's owner.",
            "ownerOf",
            "triggeringPermanent",
        ),
    ] {
        let recipients = damage_recipients(type_line, oracle);
        let recipient = recipients.last().expect("a damage recipient");
        assert_eq!(recipient["kind"], relation, "{oracle}: {recipients:?}");
        assert_eq!(recipient["object"]["kind"], object, "{oracle}");
    }
}

#[test]
fn event_object_nominal_must_follow_from_the_event_predicate() {
    // Near misses: the event wrote a land, not a creature; the dying object
    // is a creature, not necessarily an artifact.
    for oracle in [
        "Whenever a land enters, destroy that creature.",
        "Whenever a creature dies, this creature deals 2 damage to that artifact's controller.",
    ] {
        let (status, _, _) = parse(CREATURE, oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

#[test]
fn it_names_the_object_that_is_the_event_subject() {
    let event_permanent = json!({"kind":"triggeringPermanent"});
    // Real cards: Fervent Charge (an attacker), Atarka (a Dragon attacker).
    // Unused recombination: a dying creature's controller through `its`.
    for oracle in [
        "Whenever a creature you control attacks, it gets +2/+2 until end of turn.",
        "Whenever a Dragon you control attacks, it gains double strike until end of turn.",
    ] {
        let objects = effect_objects(ENCHANTMENT, oracle);
        assert_eq!(
            objects.first(),
            Some(&event_permanent),
            "{oracle}: {objects:?}"
        );
    }
    let (status, rules, states) = parse(
        ENCHANTMENT,
        "Whenever a creature dies, its controller loses 1 life.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    assert_eq!(
        rules[0]["effects"][0]["player"],
        json!({"kind":"controllerOf","object":event_permanent})
    );
    // Near miss: a damaged recipient is not the event's subject; a bare `it`
    // does not name it.
    let (status, _, _) = parse(
        ENCHANTMENT,
        "Whenever a creature you control deals combat damage to a creature, tap it.",
    );
    assert_ne!(status, "canonical");
}

/// Every numeric leaf of the rules whose kind is a property read.
fn property_reads(type_line: &str, oracle: &str) -> Vec<Value> {
    fn collect(value: &Value, found: &mut Vec<Value>) {
        match value {
            Value::Object(map) => {
                if matches!(
                    map.get("kind").and_then(Value::as_str),
                    Some("powerOf" | "toughnessOf" | "manaValueOf")
                ) {
                    found.push(value.clone());
                }
                map.values().for_each(|child| collect(child, found));
            }
            Value::Array(items) => items.iter().for_each(|child| collect(child, found)),
            _ => {}
        }
    }
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    let mut found = Vec::new();
    rules.iter().for_each(|rule| collect(rule, &mut found));
    found
}

#[test]
fn genitive_property_reads_the_named_object() {
    // Real cards: Kresh the Bloodbraided (the dying creature), Hamletback
    // Goliath (the entering creature).
    // Unused recombination: a damaged creature's toughness as a life gain.
    for (type_line, oracle, kind, object) in [
        (
            CREATURE,
            "Whenever another creature dies, you may put X +1/+1 counters on this creature, where X is that creature's power.",
            "powerOf",
            "triggeringPermanent",
        ),
        (
            CREATURE,
            "Whenever another creature enters, you may put X +1/+1 counters on this creature, where X is that creature's power.",
            "powerOf",
            "observedEventObject",
        ),
        (
            CREATURE,
            "Whenever this creature deals combat damage to a creature, you gain life equal to that creature's toughness.",
            "toughnessOf",
            "triggeringPermanent",
        ),
    ] {
        let reads = property_reads(type_line, oracle);
        assert!(
            reads
                .iter()
                .any(|read| read["kind"] == kind && read["object"]["kind"] == object),
            "{oracle}: {reads:?}"
        );
    }
    // Near miss: the event wrote a land; `that creature` names nothing.
    let (status, _, _) = parse(
        CREATURE,
        "Whenever a land enters, you gain life equal to that creature's power.",
    );
    assert_ne!(status, "canonical");
}

#[test]
fn spell_genitive_reads_the_cast_record() {
    // Real card: Manaplasm. Unused recombination: the cast spell's mana
    // value as a life gain.
    for oracle in [
        "Whenever you cast a spell, this creature gets +X/+X until end of turn, where X is that spell's mana value.",
        "Whenever an opponent casts a spell, you gain life equal to that spell's mana value.",
    ] {
        let (status, rules, states) = parse(CREATURE, oracle);
        assert_eq!(status, "canonical", "{oracle}: {states:?}");
        assert!(
            rules[0].to_string().contains("triggeringSpellManaValue"),
            "{oracle}: {rules:?}"
        );
    }
    // Near miss: a copied spell has no cast record to read.
    let (status, _, _) = parse(
        CREATURE,
        "Whenever you cast or copy an instant or sorcery spell, you gain life equal to that spell's mana value.",
    );
    assert_ne!(status, "canonical");
}

/// Every `moveCards` effect of the card's canonical rules.
fn card_moves(type_line: &str, oracle: &str) -> Vec<Value> {
    let (status, rules, states) = parse(type_line, oracle);
    assert_eq!(status, "canonical", "{oracle}: {states:?}");
    rules
        .iter()
        .flat_map(|rule| rule["effects"].as_array().cloned().unwrap_or_default())
        .filter(|effect| effect["kind"] == "moveCards")
        .collect()
}

#[test]
fn it_names_the_card_a_death_put_into_a_graveyard() {
    // Real cards: Endless Cockroaches (the source), The Cauldron of Eternity
    // (an observed creature), Demonic Vigor (the enchanted creature).
    // Unused recombination: the dying creature exiled by an artifact.
    for (type_line, oracle, zone) in [
        (
            CREATURE,
            "When this creature dies, return it to its owner's hand.",
            "hand",
        ),
        (
            ENCHANTMENT,
            "Whenever a creature you control dies, put it on the bottom of its owner's library.",
            "library",
        ),
        (
            "Enchantment \u{2014} Aura",
            "Enchant creature\nWhen enchanted creature dies, return that card to the battlefield under your control.",
            "battlefield",
        ),
        (
            "Artifact",
            "Whenever a creature an opponent controls dies, exile it.",
            "exile",
        ),
    ] {
        let moves = card_moves(type_line, oracle);
        assert!(
            moves
                .iter()
                .any(|effect| effect["cards"] == json!({"kind":"triggeringCard"})
                    && effect["to"]["kind"] == zone),
            "{oracle}: {moves:?}"
        );
    }
    // Near miss: a departure to an unwritten zone binds no card.
    let (status, _, _) = parse(
        ENCHANTMENT,
        "Whenever a creature you control leaves the battlefield, return that card to its owner's hand.",
    );
    assert_ne!(status, "canonical");
}

#[test]
fn the_exiled_card_is_linked_to_the_exiling_ability() {
    // Real card: Journey to Nowhere's shape. The exile of the other ability
    // records the link; the reader returns the linked set (CR 607.2a).
    let (status, rules, states) = parse(
        ENCHANTMENT,
        "When this enchantment enters, exile target creature.\nWhen this enchantment leaves the battlefield, return the exiled card to the battlefield under its owner's control.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    let text = serde_json::to_string(&rules).unwrap();
    assert!(text.contains("\"linkToSource\":true"), "{text}");
    assert!(text.contains("cardsExiledWithSource"), "{text}");
    // Unused recombination: `each card exiled with this` into hands.
    let (status, _, states) = parse(
        ENCHANTMENT,
        "When this enchantment enters, exile target artifact.\n{2}, Sacrifice this enchantment: Return each card exiled with this enchantment to its owner's hand.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    // Near miss: nothing on the face exiles; the set has no producer.
    let (status, _, states) = parse(
        CREATURE,
        "When this creature leaves the battlefield, return the exiled card to its owner's hand.",
    );
    assert_ne!(status, "canonical");
    assert!(
        states
            .iter()
            .any(|state| state == "binding.linked-exile.producer"),
        "{states:?}"
    );
}

#[test]
fn widened_slots_bind_prevention_targets_attachments_and_the_cast_spell() {
    // Samite Alchemist: the protected target is `that creature`.
    let (status, _, states) = parse(
        CREATURE,
        "{W}{W}, {T}: Prevent the next 4 damage that would be dealt this turn to target creature you control. Tap that creature.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    // Grave Strength's shape: milling moves no permanent, the target stays.
    let (status, _, states) = parse(
        "Sorcery",
        "Choose target creature. Mill three cards, then put a +1/+1 counter on that creature.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    // Unstable Mutation's trigger: `that creature` is the enchanted creature.
    let objects = effect_objects(
        "Enchantment \u{2014} Aura",
        "Enchant creature\nAt the beginning of the upkeep of enchanted creature's controller, put a -1/-1 counter on that creature.",
    );
    assert!(
        objects
            .iter()
            .any(|object| object["kind"] == "attachedPermanent"),
        "{objects:?}"
    );
    // `counter that spell` after a cast trigger (Ertai's Familiar shape).
    let (status, rules, states) = parse(
        ENCHANTMENT,
        "Whenever an opponent casts a spell, you may pay {2}. If you do, counter that spell.",
    );
    assert_eq!(status, "canonical", "{states:?}");
    assert!(
        rules[0].to_string().contains("triggeringStackObject"),
        "{rules:?}"
    );
    // Near misses: no cast event binds a spell; the enchanted permanent is
    // a land, not a creature.
    for (type_line, oracle) in [
        (CREATURE, "{T}: Counter that spell."),
        (
            "Enchantment \u{2014} Aura",
            "Enchant land\nAt the beginning of the upkeep of enchanted land's controller, put a -1/-1 counter on that creature.",
        ),
    ] {
        let (status, _, _) = parse(type_line, oracle);
        assert_ne!(status, "canonical", "{oracle}");
    }
}

#[test]
fn where_x_reads_the_declared_target_property() {
    // Death Mutation: the destroyed target's mana value.
    let reads = property_reads(
        "Sorcery",
        "Destroy target nonblack creature. It can't be regenerated. Create X 1/1 green Saproling creature tokens, where X is that creature's mana value.",
    );
    assert!(
        reads
            .iter()
            .any(|read| read["kind"] == "manaValueOf" && read["object"]["kind"] == "chosenTarget"),
        "{reads:?}"
    );
}

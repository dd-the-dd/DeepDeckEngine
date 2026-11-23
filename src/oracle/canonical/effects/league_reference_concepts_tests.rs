//! Shared concepts restored for the league reference cohort: quantified
//! opponent count comparisons, declared non-`player` life subjects, the
//! sacrificed source as `it`, shortened self names, mode flavor words, typed
//! battlefield destinations, exiled-object bindings, `all` permanent
//! selections, `Activate only if`, permanent-only subtypes and event players.
//! Each concept has a real card, an unused sibling combination and a near miss.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes")
}

/// Every ability of the complete card is canonical; returns their rules.
fn canonical_rules(name: &str, type_line: &str, text: &str) -> Vec<Value> {
    let parsed = parse(name, type_line, text);
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    parsed
        .abilities
        .iter()
        .filter(|ability| ability.status != "empty")
        .map(|ability| ability.rule.clone().expect("canonical rule"))
        .collect()
}

/// The single ability of a one-line text is not canonical.
fn rejected(name: &str, type_line: &str, text: &str) {
    let parsed = parse(name, type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}: {:#?}", ability.rule);
}

/// Every JSON object nested in `value` (depth first).
fn objects(value: &Value) -> Vec<&Value> {
    let mut found = Vec::new();
    let mut stack = vec![value];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(map) => {
                found.push(value);
                stack.extend(map.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    found
}

fn find_kind<'a>(value: &'a Value, kind: &str) -> &'a Value {
    objects(value)
        .into_iter()
        .find(|object| object["kind"] == kind)
        .unwrap_or_else(|| panic!("a {kind} node in {value:#}"))
}

fn has_kind(value: &Value, kind: &str) -> bool {
    objects(value)
        .into_iter()
        .any(|object| object["kind"] == kind)
}

fn opponent_exceeds(filter: Value) -> Value {
    json!({
        "kind": "compare",
        "operator": ">",
        "left": {"kind": "greatestOpponentPermanentCount", "player": {"kind": "controllerOf", "object": {"kind": "self"}}, "where": filter.clone()},
        "right": {"kind": "countPermanents", "player": {"kind": "controllerOf", "object": {"kind": "self"}}, "where": filter},
    })
}

#[test]
fn opponent_count_comparison_is_one_atom_for_guards_sentences_and_activations() {
    let rules = canonical_rules(
        "Land Tax",
        "Enchantment",
        "At the beginning of your upkeep, if an opponent controls more lands than you, you may search your library for up to three basic land cards, reveal them, put them into your hand, then shuffle.",
    );
    let compare = find_kind(&rules[0], "compare");
    let filter = compare["left"]["where"].clone();
    assert_eq!(compare, &opponent_exceeds(filter));
    // The same atom in a spell's sentence conditional.
    let rules = canonical_rules(
        "Gift of Estates",
        "Sorcery",
        "If an opponent controls more lands than you, search your library for up to three Plains cards, reveal them, put them into your hand, then shuffle.",
    );
    assert_eq!(
        find_kind(&rules[0], "compare")["left"]["kind"],
        "greatestOpponentPermanentCount"
    );
    // Unused combinations: the converse subject in a trigger guard, and the
    // universal relation as an activation restriction.
    let rules = canonical_rules(
        "Probe",
        "Creature — Soldier",
        "At the beginning of your upkeep, if you control fewer creatures than an opponent, draw a card.",
    );
    assert_eq!(
        find_kind(&rules[0], "compare")["left"]["kind"],
        "greatestOpponentPermanentCount"
    );
    let rules = canonical_rules(
        "Probe",
        "Artifact",
        "{T}: Draw a card. Activate only if you control more artifacts than each opponent.",
    );
    let condition = &rules[0]["activationCondition"];
    assert_eq!(condition["operator"], ">");
    assert_eq!(condition["left"]["kind"], "countPermanents");
    assert_eq!(condition["right"]["kind"], "greatestOpponentPermanentCount");
    // Least-count relations have no evaluator and stay explicit failures.
    rejected(
        "Probe",
        "Sorcery",
        "If an opponent controls fewer lands than you, draw a card.",
    );
    rejected(
        "Probe",
        "Sorcery",
        "If you control more lands than an opponent, draw a card.",
    );
}

#[test]
fn activation_condition_composes_with_timing() {
    let rules = canonical_rules(
        "Weathered Wayfarer",
        "Creature — Human Nomad Cleric",
        "{W}, {T}: Search your library for a land card, reveal it, put it into your hand, then shuffle. Activate only if an opponent controls more lands than you.",
    );
    assert_eq!(
        rules[0]["activationCondition"]["left"]["kind"],
        "greatestOpponentPermanentCount"
    );
    let rules = canonical_rules(
        "Probe",
        "Artifact",
        "{T}: Draw a card. Activate only if you control two or more artifacts and only as a sorcery.",
    );
    let condition = &rules[0]["activationCondition"];
    assert_eq!(condition["kind"], "and");
    assert_eq!(condition["operands"][0], json!({"kind": "sorceryTiming"}));
    // Turn ownership is a shared static atom, so the restriction composes.
    let rules = canonical_rules(
        "Probe",
        "Artifact",
        "{T}: Draw a card. Activate only if it's your turn.",
    );
    assert_eq!(
        rules[0]["activationCondition"],
        json!({"kind": "duringControllerTurn",
            "player": {"kind": "controllerOf", "object": {"kind": "self"}}})
    );
    rejected(
        "Probe",
        "Artifact",
        "{T}: Draw a card. Activate only if it's your upkeep.",
    );
}

#[test]
fn declared_opponent_life_subject_uses_the_shared_player_reader() {
    let rules = canonical_rules(
        "Diregraf Captain",
        "Creature — Zombie Soldier",
        "Deathtouch\nOther Zombie creatures you control get +1/+1.\nWhenever another Zombie you control dies, target opponent loses 1 life.",
    );
    let trigger = rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .unwrap();
    let loss = find_kind(trigger, "loseLife");
    assert_eq!(loss["player"]["kind"], "chosenTarget");
    assert_eq!(
        trigger["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "isOpponentOf"
    );
    // Unused combination: a declared opponent gaining life in a spell.
    let rules = canonical_rules(
        "Probe",
        "Instant",
        "Target opponent gains 3 life. Draw a card.",
    );
    assert_eq!(
        find_kind(&rules[0], "gainLife")["player"]["kind"],
        "chosenTarget"
    );
    rejected("Probe", "Instant", "Target opponent loses life.");
}

#[test]
fn sacrificed_source_is_the_antecedent_of_it() {
    let rules = canonical_rules(
        "Pyrite Spellbomb",
        "Artifact",
        "{R}, Sacrifice this artifact: It deals 2 damage to any target.\n{1}, Sacrifice this artifact: Draw a card.",
    );
    assert!(has_kind(&rules[0], "dealDamage"));
    // Unused combination: a sacrificed creature as a player-set damage source.
    canonical_rules(
        "Probe",
        "Creature — Goblin",
        "Sacrifice this creature: It deals 1 damage to each opponent.",
    );
    // Without a cost naming the source, `It` has no antecedent.
    rejected("Probe", "Artifact", "{T}: It deals 1 damage to any target.");
}

#[test]
fn shortened_self_names_link_leading_proper_nouns_only() {
    let rules = canonical_rules(
        "Arcanis the Omnipotent",
        "Legendary Creature — Wizard",
        "{2}{U}{U}: Return Arcanis to its owner's hand.",
    );
    assert!(rules[0].to_string().contains("\"self\""), "{:#}", rules[0]);
    // Unused combination: the leading word of a two-word name as a damage source.
    canonical_rules(
        "Legolas Greenleaf",
        "Legendary Creature — Elf Archer",
        "{T}: Legolas deals 1 damage to any target.",
    );
    // Only a legendary name is shortened to a leading proper noun.
    rejected(
        "Legolas Greenleaf",
        "Creature — Elf Archer",
        "{T}: Legolas deals 1 damage to any target.",
    );
    // A leading ordinary word is never a shortened name.
    rejected(
        "Forest Spirit",
        "Creature — Spirit",
        "{T}: Forest deals 1 damage to any target.",
    );
}

#[test]
fn mode_flavor_words_label_modes_without_meaning() {
    let rules = canonical_rules(
        "Black Market Connections",
        "Enchantment",
        "At the beginning of your first main phase, choose one or more —\n• Sell Contraband — Create a Treasure token. You lose 1 life.\n• Buy Information — Draw a card. You lose 2 life.\n• Hire a Mercenary — Create a 3/2 colorless Shapeshifter creature token with changeling. You lose 3 life.",
    );
    assert_eq!(rules.len(), 1);
    assert!(!rules[0].to_string().contains("Contraband"));
    // Unused combination: labeled and unlabeled modes in one choice.
    canonical_rules(
        "Probe",
        "Sorcery",
        "Choose one —\n• Big Bonk — Destroy target artifact.\n• Draw two cards.",
    );
    rejected(
        "Probe",
        "Sorcery",
        "Choose one —\n• Bonk, Again — Destroy target artifact.\n• Draw two cards.",
    );
}

#[test]
fn return_to_the_battlefield_is_a_typed_destination_for_every_card_operand() {
    let rules = canonical_rules(
        "Splendid Reclamation",
        "Sorcery",
        "Return all land cards from your graveyard to the battlefield tapped.",
    );
    let movement = find_kind(&rules[0], "moveCards");
    assert_eq!(movement["to"]["kind"], "battlefield");
    assert_eq!(movement["to"]["tapped"], true);
    // Unused combination: an owner-controlled entry followed by another action.
    let rules = canonical_rules(
        "Probe",
        "Sorcery",
        "Return all creature cards with mana value 2 or less from your graveyard to the battlefield under their owners' control, then draw a card.",
    );
    assert_eq!(
        find_kind(&rules[0], "moveCards")["to"]["player"]["kind"],
        "ownerOf"
    );
    rejected(
        "Probe",
        "Sorcery",
        "Return all land cards from your graveyard into the battlefield.",
    );
}

#[test]
fn an_exiled_permanent_is_the_card_antecedent_of_its_return() {
    let rules = canonical_rules(
        "Restoration Angel",
        "Creature — Angel",
        "Flash\nFlying\nWhen this creature enters, you may exile target non-Angel creature you control, then return that card to the battlefield under your control.",
    );
    let trigger = rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .unwrap();
    let exile = find_kind(trigger, "exilePermanent");
    let binding = exile["bind"].as_str().expect("the exiled object is bound");
    assert_eq!(
        find_kind(trigger, "moveCards")["cards"],
        json!({"kind": "boundObjects", "binding": binding})
    );
    // Unused combination: `it` and the owner's control in an activation.
    let rules = canonical_rules(
        "Probe",
        "Creature — Spirit",
        "{3}: Exile another target creature you control, then return it to the battlefield under its owner's control.",
    );
    assert_eq!(
        find_kind(&rules[0], "moveCards")["to"]["player"]["kind"],
        "ownerOf"
    );
    // A reference without an exiled antecedent is not invented.
    rejected("Probe", "Sorcery", "Return that card to the battlefield.");
}

#[test]
fn all_permanents_return_to_their_owners_hands_without_a_choice() {
    let rules = canonical_rules(
        "Aetherize",
        "Instant",
        "Return all attacking creatures to their owner's hand.",
    );
    let bounce = find_kind(&rules[0], "returnToOwnersHand");
    assert_eq!(bounce["object"]["kind"], "eachPermanent");
    assert!(!has_kind(&rules[0], "choosePermanents"));
    // Unused combination: a controlled collection followed by a draw.
    canonical_rules(
        "Probe",
        "Sorcery",
        "Return all artifacts you control to their owners' hands, then draw a card.",
    );
    rejected("Probe", "Sorcery", "Return all creatures to your hand.");
}

#[test]
fn permanent_only_subtypes_establish_the_permanent_domain() {
    let rules = canonical_rules(
        "Emeria, the Sky Ruin",
        "Land",
        "This land enters tapped.\nAt the beginning of your upkeep, if you control seven or more Plains, you may return target creature card from your graveyard to the battlefield.\n{T}: Add {W}.",
    );
    assert!(rules.iter().any(|rule| has_kind(rule, "countPermanents")));
    // Unused combination: an artifact subtype count in an activation restriction.
    canonical_rules(
        "Probe",
        "Artifact",
        "{T}: Draw a card. Activate only if you control two or more Equipment.",
    );
    // CR 109.2a: a creature subtype without `card` or `spell` also names
    // permanents (a Kindred spell on the stack is not an Elf permanent).
    let elves = canonical_rules(
        "Probe",
        "Sorcery",
        "If you control seven or more Elves, draw a card.",
    );
    assert!(elves.iter().any(|rule| has_kind(rule, "countPermanents")));
}

#[test]
fn that_player_after_combat_damage_is_the_event_player() {
    let rules = canonical_rules(
        "Nightveil Probe",
        "Creature — Specter",
        "Flying\nWhenever this creature deals combat damage to a player, that player discards a card.",
    );
    let trigger = rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .unwrap();
    assert_eq!(
        find_kind(trigger, "discardCards")["player"],
        json!({"kind": "triggeringPlayer"})
    );
    // Unused combination: an Equipment event with a life action.
    let rules = canonical_rules(
        "Probe",
        "Artifact — Equipment",
        "Whenever equipped creature deals combat damage to a player, that player loses 2 life.\nEquip {2}",
    );
    let trigger = rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .unwrap();
    assert_eq!(
        find_kind(trigger, "loseLife")["player"],
        json!({"kind": "triggeringPlayer"})
    );
    // An event without a player gives `that player` no referent.
    rejected(
        "Probe",
        "Creature — Specter",
        "Whenever this creature attacks, that player discards a card.",
    );
}

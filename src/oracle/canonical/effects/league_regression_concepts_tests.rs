//! Shared concepts restored for the league regression cohort: the cycling
//! keyword family, mana color distribution, count/presence entering-state
//! exemptions, parameterless keyword vocabulary and attached-object events.
//! Each concept has a real card, an unused sibling combination and a near miss.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, mana_cost: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
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
fn canonical_rules(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Vec<Value> {
    let parsed = parse(name, type_line, mana_cost, text);
    assert_eq!(parsed.status, "canonical", "{text}: {parsed:#?}");
    parsed
        .abilities
        .iter()
        .filter(|ability| ability.status != "empty")
        .map(|ability| {
            assert_eq!(ability.status, "canonical", "{text}: {ability:#?}");
            ability.rule.clone().expect("canonical rule")
        })
        .collect()
}

/// The single ability of a one-line text is not canonical.
fn rejected(type_line: &str, text: &str) {
    let parsed = parse("Probe", type_line, "{2}", text);
    let ability = parsed
        .abilities
        .iter()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}: {:#?}", ability.rule);
}

fn rule_of_kind<'a>(rules: &'a [Value], kind: &str) -> &'a Value {
    rules
        .iter()
        .find(|rule| rule["kind"] == kind)
        .unwrap_or_else(|| panic!("a {kind} rule in {rules:#?}"))
}

#[test]
fn plain_cycling_expands_to_a_hand_discard_draw_and_composes_with_other_abilities() {
    let rules = canonical_rules(
        "Raffine's Tower",
        "Land — Plains Island Swamp",
        "",
        "({T}: Add {W}, {U}, or {B}.)\nThis land enters tapped.\nCycling {3} ({3}, Discard this card: Draw a card.)",
    );
    let cycling = rules
        .iter()
        .find(|rule| rule["ability"]["kind"] == "cycling")
        .expect("cycling keyword rule");
    assert_eq!(
        cycling["ability"],
        json!({
            "kind": "cycling",
            "cost": {"kind": "payMana", "manaCost": "{3}"},
            "activationZone": "hand",
            "costs": [{"kind": "discardCard", "card": {"kind": "self"}}],
            "effects": [{
                "kind": "drawCards",
                "player": {"kind": "controllerOf", "object": {"kind": "self"}},
                "count": {"kind": "integer", "value": 1},
            }],
        })
    );
    // Unused combination: a colored multi-symbol cost and the bare keyword
    // on a creature beside another keyword.
    let rules = canonical_rules(
        "Probe Cycler",
        "Creature — Beast",
        "{4}{G}",
        "Trample\nCycling {1}{G}{G}",
    );
    let cycling = rules
        .iter()
        .find(|rule| rule["ability"]["kind"] == "cycling")
        .expect("bare cycling");
    assert_eq!(cycling["ability"]["cost"]["manaCost"], "{1}{G}{G}");
    // Typecycling keeps its own expansion.
    let rules = canonical_rules("Probe", "Creature — Beast", "{5}", "Swampcycling {2}");
    assert_eq!(rules[0]["ability"]["kind"], "typecycling");
}

#[test]
fn cycling_reminder_and_cost_contradictions_fail_explicitly() {
    for text in [
        "Cycling {3} ({3}, Discard this card: Draw two cards.)",
        "Cycling {3} ({2}, Discard this card: Draw a card.)",
        "Cycling {3} ({3}: Draw a card.)",
        "Cycling—Pay 2 life.",
        "Cycling {3}. Draw a card.",
    ] {
        rejected("Land", text);
    }
}

#[test]
fn mana_color_distribution_is_a_typed_choice_of_the_shared_any_color_child() {
    let rules = canonical_rules(
        "Gilded Lotus",
        "Artifact",
        "{5}",
        "{T}: Add three mana of any one color.",
    );
    assert_eq!(
        rules[0]["effects"][0]["mana"],
        json!({"kind": "chooseColor", "amount": 3})
    );
    let rules = canonical_rules(
        "Manamorphose",
        "Instant",
        "{1}{R/G}",
        "Add two mana in any combination of colors.\nDraw a card.",
    );
    let add = &rules[0]["effects"][0];
    assert_eq!(add["kind"], "addMana");
    assert_eq!(add["mana"], json!({"kind": "chooseColors", "amount": 2}));
    // Unused combination: a combination production with a spend restriction.
    let rules = canonical_rules(
        "Probe Hall",
        "Land",
        "",
        "{1}, {T}: Add four mana in any combination of colors. Spend this mana only to cast creature spells.",
    );
    let add = &rules[0]["effects"][0];
    assert_eq!(add["mana"], json!({"kind": "chooseColors", "amount": 4}));
    assert_eq!(add["spendRestriction"]["kind"], "castSpell");
}

#[test]
fn malformed_color_distributions_fail_explicitly() {
    for text in [
        "{T}: Add three mana of any one colors.",
        "{T}: Add two mana in any combination of color.",
        "{T}: Add two mana in any combination colors.",
        "{T}: Add three mana of any two color.",
        "{T}: Add two mana in any combination of colors that a land an opponent controls could produce.",
    ] {
        rejected("Artifact", text);
    }
}

fn entry_condition(rules: &[Value]) -> &Value {
    &rule_of_kind(rules, "replacementEffect")["condition"]
}

#[test]
fn entering_tapped_exemptions_reuse_shared_count_and_presence_conditions() {
    // Battle land: no `other`; the entering land is not yet on the battlefield.
    let rules = canonical_rules(
        "Canopy Vista",
        "Land — Forest Plains",
        "",
        "({T}: Add {G} or {W}.)\nThis land enters tapped unless you control two or more basic lands.",
    );
    let condition = entry_condition(&rules);
    assert_eq!(condition["kind"], "not");
    assert_eq!(condition["operand"]["operator"], ">=");
    assert_eq!(condition["operand"]["left"]["kind"], "countPermanents");
    assert_eq!(
        condition["operand"]["right"],
        json!({"kind":"integer","value":2})
    );
    // Opponent-count exemption.
    let rules = canonical_rules(
        "Sea of Clouds",
        "Land",
        "",
        "This land enters tapped unless you have two or more opponents.\n{T}: Add {W} or {U}.",
    );
    let condition = entry_condition(&rules);
    assert_eq!(
        condition["operand"]["left"],
        json!({"kind": "countOpponents", "player": {"kind": "controllerOf", "object": {"kind": "self"}}})
    );
    assert_eq!(condition["operand"]["operator"], ">=");
    // Name-referenced source with a presence condition.
    let rules = canonical_rules(
        "Minas Tirith",
        "Legendary Land",
        "",
        "Minas Tirith enters tapped unless you control a legendary creature.\n{T}: Add {W}.",
    );
    assert_eq!(
        entry_condition(&rules)["operand"]["kind"],
        "controlsPermanent"
    );
    // Unused combinations: a creature source with a fewer-comparison of
    // opponents, and a named artifact source with a count of creatures.
    let rules = canonical_rules(
        "Probe Sentry",
        "Creature — Soldier",
        "{2}{W}",
        "This creature enters tapped unless you have one or fewer opponents.",
    );
    assert_eq!(entry_condition(&rules)["operand"]["operator"], "<=");
    let rules = canonical_rules(
        "Probe Engine",
        "Artifact",
        "{3}",
        "Probe Engine enters tapped unless you control three or more creatures.",
    );
    assert_eq!(
        entry_condition(&rules)["operand"]["right"],
        json!({"kind":"integer","value":3})
    );
}

#[test]
fn entering_tapped_exemption_near_misses_fail_explicitly() {
    for text in [
        "This land enters tapped unless you have two or more players.",
        "This land enters tapped unless you have two or more cards.",
        "This land enters tapped unless you have two opponents.",
        "This land enters tapped unless you have two or more opponents and a Forest.",
        "This land enters tapped unless an opponent has two or more opponents.",
    ] {
        rejected("Land", text);
    }
}

#[test]
fn devoid_and_umbra_armor_join_the_shared_keyword_vocabulary() {
    let rules = canonical_rules(
        "Hyena Umbra",
        "Enchantment — Aura",
        "{W}",
        "Enchant creature\nEnchanted creature gets +1/+1 and has first strike.\nUmbra armor (If enchanted creature would be destroyed, instead remove all damage from it and destroy this Aura.)",
    );
    assert!(
        rules
            .iter()
            .any(|rule| rule["ability"] == json!({"kind": "umbraArmor"}))
    );
    let rules = canonical_rules(
        "Probe Drone",
        "Creature — Eldrazi Drone",
        "{2}{B}",
        "Devoid (This card has no color.)",
    );
    assert_eq!(rules[0]["ability"], json!({"kind": "devoid"}));
    // Unused combination: the same atoms inside a keyword list.
    let rules = canonical_rules(
        "Probe Drone",
        "Creature — Eldrazi Drone",
        "{2}{B}",
        "Devoid, flying, trample",
    );
    assert_eq!(rules[0]["kind"], "keywordAbilityGroup");
    assert_eq!(rules[0]["abilities"][0], json!({"kind": "devoid"}));
}

#[test]
fn keyword_reminder_mismatches_fail_explicitly() {
    for text in [
        "Devoid (This card is colorless.)",
        "Umbra armor (If enchanted creature would be destroyed, instead destroy this Aura.)",
        "Umbra",
    ] {
        rejected("Enchantment — Aura", text);
    }
}

#[test]
fn attached_object_events_bind_the_attachment_not_the_source() {
    let rules = canonical_rules(
        "Skullclamp",
        "Artifact — Equipment",
        "{1}",
        "Equipped creature gets +1/-1.\nWhenever equipped creature dies, draw two cards.\nEquip {1}",
    );
    let trigger = rule_of_kind(&rules, "triggeredAbility");
    assert_eq!(trigger["event"], json!({"kind": "attachedPermanentDied"}));
    assert_eq!(trigger["effects"][0]["kind"], "drawCards");
    // Unused combinations: an Aura subject with an attack event and a
    // combat-damage event with a token-creating body.
    let rules = canonical_rules(
        "Probe Aura",
        "Enchantment — Aura",
        "{1}{W}",
        "Enchant creature\nWhenever enchanted creature attacks, you gain 2 life.",
    );
    assert_eq!(
        rule_of_kind(&rules, "triggeredAbility")["event"],
        json!({"kind": "attachedPermanentDeclaredAttacker"})
    );
    let rules = canonical_rules(
        "Probe Blade",
        "Artifact — Equipment",
        "{2}",
        "Whenever equipped creature deals combat damage to a player, draw a card.\nEquip {2}",
    );
    assert_eq!(
        rule_of_kind(&rules, "triggeredAbility")["event"],
        json!({"kind": "attachedPermanentCombatDamageToPlayer"})
    );
}

#[test]
fn attached_object_event_near_misses_fail_explicitly() {
    for text in [
        "Whenever equipped artifact dies, draw a card.",
        // No aggregation over every recipient of one damage event is proved.
        "Whenever equipped creature deals damage, draw a card.",
        "Whenever equipped creature attacks or blocks, draw a card.",
        "Whenever equipped creature dies draw a card.",
    ] {
        rejected("Artifact — Equipment", text);
    }
}

#[test]
fn sequence_earthbend_declares_its_land_target_in_any_ability_envelope() {
    let rules = canonical_rules(
        "Earthbending Student",
        "Creature — Human Warrior Ally",
        "{2}{G}",
        "When this creature enters, earthbend 2. (Target land you control becomes a 0/0 creature with haste that's still a land. Put two +1/+1 counters on it. When it dies or is exiled, return it to the battlefield tapped.)\nLand creatures you control have vigilance. (Attacking doesn't cause them to tap.)",
    );
    let trigger = rule_of_kind(&rules, "triggeredAbility");
    let decision = &trigger["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(
        decision["candidates"]["where"],
        json!({"kind": "cardTypeContains", "value": "Land"})
    );
    let kinds: Vec<_> = trigger["effects"]
        .as_array()
        .unwrap()
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "becomeCreature",
            "grantKeyword",
            "putCounters",
            "installDelayedZoneChangeTrigger",
            "emitAbilitySignal"
        ]
    );
    // Unused combinations: a life-gain trigger followed by a second action,
    // and a spell that earthbends twice (two independent target declarations).
    let rules = canonical_rules(
        "Probe Geomancer",
        "Creature — Human",
        "{2}{G}",
        "Whenever you gain life, earthbend 1, then draw a card.",
    );
    let trigger = rule_of_kind(&rules, "triggeredAbility");
    assert_eq!(
        trigger["effects"].as_array().unwrap().last().unwrap()["kind"],
        "drawCards"
    );
    let rules = canonical_rules(
        "Probe Quake",
        "Sorcery",
        "{3}{G}",
        "Earthbend 2, then earthbend 2.",
    );
    let decisions = rules[0]["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2);
    assert_ne!(decisions[0]["id"], decisions[1]["id"]);
}

#[test]
fn sequence_earthbend_without_a_bound_quantity_fails_explicitly() {
    rejected(
        "Creature — Human",
        "When this creature enters, earthbend X.",
    );
}

#[test]
fn token_characteristic_sentence_adds_abilities_to_the_created_tokens() {
    let rules = canonical_rules(
        "Brood Monitor",
        "Creature — Eldrazi Drone",
        "{4}{G}{G}",
        "Devoid (This card has no color.)\nWhen this creature enters, create three 1/1 colorless Eldrazi Scion creature tokens. They have \"Sacrifice this token: Add {C}.\"",
    );
    let with_list = canonical_rules(
        "Probe",
        "Creature — Eldrazi Drone",
        "{4}{G}{G}",
        "When this creature enters, create three 1/1 colorless Eldrazi Scion creature tokens with \"Sacrifice this token: Add {C}.\"",
    );
    assert_eq!(
        rule_of_kind(&rules, "triggeredAbility")["effects"],
        with_list[0]["effects"],
        "the sentence and the `with` list define the same tokens"
    );
    // Unused combination: a singular token with a keyword in the sentence.
    let rules = canonical_rules(
        "Probe Muster",
        "Sorcery",
        "{2}{W}",
        "Create a 2/2 white Knight creature token. It has vigilance.",
    );
    assert_eq!(rules[0]["effects"][0]["kind"], "createTokens");
}

#[test]
fn token_characteristic_sentence_near_misses_fail_explicitly() {
    for text in [
        "Create two 1/1 white Soldier creature tokens. It has flying.",
        "Create a 1/1 white Soldier creature token. They have flying.",
        "Create two 1/1 white Soldier creature tokens. They have flying until end of turn.",
    ] {
        rejected("Sorcery", text);
    }
}

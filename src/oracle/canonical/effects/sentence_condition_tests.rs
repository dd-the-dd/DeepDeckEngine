//! Sentence conditional `If CONDITION, EFFECTS.` / `Otherwise, EFFECTS.`
//! (CR 608.2c) through real cards: the condition reuses the shared atoms and
//! governs only its sentence; the `Otherwise` sentence is its else-branch.
use serde_json::{Value, json};

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(type_line: &str, mana_cost: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: "Test Card".to_string(),
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

/// The canonical rule of the last ability (earlier ones are keywords).
fn effects(type_line: &str, mana_cost: &str, text: &str) -> Value {
    let parsed = parse(type_line, mana_cost, text);
    let ability = parsed.abilities.last().expect("one ability");
    assert_eq!(
        ability.status,
        "canonical",
        "{text}: {:?}",
        ability
            .diagnostics
            .first()
            .map(|d| (&d.state, &d.expected, &d.found))
    );
    ability.rule.as_ref().expect("canonical rule")["effects"].clone()
}

fn failure(type_line: &str, text: &str) -> (String, String) {
    let parsed = parse(type_line, "{1}", text);
    let ability = parsed.abilities.last().expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}");
    let diagnostic = ability.diagnostics.first().expect("a diagnostic");
    (
        diagnostic.state.clone().unwrap_or_default(),
        diagnostic.expected.clone().unwrap_or_default(),
    )
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn draw(count: i64) -> Value {
    json!({ "kind": "drawCards", "player": controller(), "count": { "kind": "integer", "value": count } })
}

fn target(id: &str) -> Value {
    json!({ "kind": "chosenTarget", "id": id })
}

#[test]
fn controller_presence_atom_guards_only_the_following_sentence() {
    // Chilling Trap.
    let effects = effects(
        "Instant",
        "{1}{U}",
        "Target creature gets -4/-0 until end of turn. If you control a Wizard, draw a card.",
    );
    assert_eq!(effects[0]["kind"], "modifyPowerToughness");
    assert_eq!(
        effects[1],
        json!({
            "kind": "conditional",
            "condition": {
                "kind": "compare", "operator": ">=",
                "left": { "kind": "countPermanents", "player": controller(),
                          "where": { "kind": "subtypeContains", "value": "Wizard" },
                          "excludeSource": false },
                "right": { "kind": "integer", "value": 1 },
            },
            "then": [draw(1)],
            "else": [],
        })
    );
}

#[test]
fn referenced_object_atoms_bind_the_proven_antecedent() {
    // Cemetery Recruitment: a declared card target, read after its move.
    let recruitment = effects(
        "Sorcery",
        "{1}{B}",
        "Return target creature card from your graveyard to your hand. If it's a Zombie card, draw a card.",
    );
    assert_eq!(
        recruitment[1]["condition"],
        json!({ "kind": "objectMatchesFilter", "object": target("targetCard"),
                "where": { "kind": "subtypeContains", "value": "Zombie" } })
    );
    // Built to Last: a permanent target with a coordinated card type.
    let built = effects(
        "Instant",
        "{W}",
        "Target creature gets +2/+2 until end of turn. If it's an artifact creature, it gains indestructible until end of turn.",
    );
    assert_eq!(
        built[1]["condition"],
        json!({ "kind": "objectMatchesFilter", "object": target("targetPermanent"),
                "where": { "kind": "and", "operands": [
                    { "kind": "cardTypeContains", "value": "Artifact" },
                    { "kind": "cardTypeContains", "value": "Creature" } ] } })
    );
    // Dryad Greenseeker: the looked-at card of the collection scope.
    let greenseeker = effects(
        "Creature \u{2014} Dryad",
        "{1}{G}",
        "{T}: Look at the top card of your library. If it's a land card, you may reveal it and put it into your hand.",
    );
    assert_eq!(
        greenseeker[1]["condition"],
        json!({ "kind": "objectMatchesFilter",
                "object": { "kind": "boundObjects", "binding": "lookedCards" },
                "where": { "kind": "cardTypeContains", "value": "Land" } })
    );
    assert_eq!(greenseeker[1]["then"][0]["kind"], "optionalEffects");
    // Aunt May: the object observed by the entry event.
    let aunt_may = effects(
        "Legendary Creature \u{2014} Human Citizen",
        "{W}",
        "Whenever another creature you control enters, you gain 1 life. If it's a Spider, put a +1/+1 counter on it.",
    );
    assert_eq!(
        aunt_may[1]["condition"]["object"],
        json!({ "kind": "observedEventObject", "event": "permanentEntered" })
    );
}

#[test]
fn numeric_properties_and_states_reuse_the_shared_comparison() {
    // Sibling: a possessive numeric property, then a nominal anaphor's state.
    let mana_value = effects(
        "Instant",
        "{1}{B}",
        "Target creature gets -2/-2 until end of turn. If its mana value is 2 or less, you gain 2 life.",
    );
    assert_eq!(
        mana_value[1]["condition"]["where"],
        json!({ "kind": "compare", "operator": "<=",
                "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                "right": { "kind": "integer", "value": 2 } })
    );
    let attacking = effects(
        "Instant",
        "{R}",
        "Target creature gets +2/+0 until end of turn. If that creature is attacking, it gains trample until end of turn.",
    );
    assert_eq!(
        attacking[1]["condition"]["where"],
        json!({ "kind": "isAttacking" })
    );
    let negated = effects(
        "Instant",
        "{G}",
        "Target creature gets +1/+1 until end of turn. If it isn't legendary, draw a card.",
    );
    assert_eq!(
        negated[1]["condition"]["where"],
        json!({ "kind": "not", "operand": { "kind": "isLegendary" } })
    );
}

#[test]
fn cast_facts_and_turn_history_lower_to_existing_conditions() {
    // Blink of an Eye.
    let blink = effects(
        "Instant",
        "{1}{U}",
        "Kicker {1}{U} (You may pay an additional {1}{U} as you cast this spell.)\nReturn target nonland permanent to its owner's hand. If this spell was kicked, draw a card.",
    );
    assert_eq!(
        blink[1]["condition"],
        json!({ "kind": "wasKicked", "spell": { "kind": "self" } })
    );
    // Sibling of Antiquities on the Loose: `Then if` and a negated cast zone.
    let loose = effects(
        "Sorcery",
        "{1}{R}{W}",
        "Create two 2/2 red and white Spirit creature tokens. Then if this spell was cast from anywhere other than your hand, draw a card.",
    );
    assert_eq!(
        loose[1]["condition"],
        json!({ "kind": "not", "operand": { "kind": "wasCastFromHand", "object": { "kind": "self" } } })
    );
    // Fated Intervention: turn ownership.
    let fated = effects(
        "Instant",
        "{2}{G}{G}{G}",
        "Create two 3/3 green Centaur enchantment creature tokens. If it's your turn, scry 2.",
    );
    assert_eq!(
        fated[1]["condition"],
        json!({ "kind": "duringControllerTurn", "player": controller() })
    );
    // Sibling: this-turn history from the static-condition atoms.
    let history = effects(
        "Instant",
        "{W}",
        "You gain 2 life. If you gained life this turn, draw a card.",
    );
    assert_eq!(
        history[1]["condition"]["left"],
        json!({ "kind": "lifeGainedThisTurn", "player": controller() })
    );
}

#[test]
fn a_new_combination_and_an_otherwise_branch_need_no_new_production() {
    // Not printed: shared presence and turn atoms under one conjunction.
    let combined = effects(
        "Instant",
        "{U}",
        "Draw a card. If you control a Wizard and it's your turn, draw a card.",
    );
    assert_eq!(combined[1]["condition"]["kind"], "and");
    assert_eq!(
        combined[1]["condition"]["operands"][1],
        json!({ "kind": "duringControllerTurn", "player": controller() })
    );
    assert_eq!(combined[1]["then"], json!([draw(1)]));
    // Not printed: the else-branch reads the same looked-at card.
    let otherwise = effects(
        "Sorcery",
        "{2}{G}",
        "Look at the top card of your library. If it's a creature card, put it into your hand. Otherwise, put it into your graveyard.",
    );
    let looked = json!({ "kind": "boundObjects", "binding": "lookedCards" });
    assert_eq!(otherwise.as_array().map(Vec::len), Some(2));
    assert_eq!(otherwise[1]["then"][0]["cards"], looked);
    assert_eq!(otherwise[1]["else"][0]["cards"], looked);
    assert_eq!(otherwise[1]["else"][0]["to"]["kind"], "graveyard");
}

#[test]
fn unsupported_or_unproven_conditions_fail_explicitly() {
    // No antecedent for `it`.
    let (state, _) = failure("Instant", "If it's a creature, draw a card.");
    assert_eq!(state, "condition.sentence.object.antecedent");
    // `instead` replaces an earlier instruction: out of this frame.
    let (state, _) = failure(
        "Instant",
        "Kicker {2}\nDestroy target creature with mana value 2 or less. If this spell was kicked, instead destroy target creature.",
    );
    assert_eq!(state, "condition.sentence.instead");
    let (state, _) = failure(
        "Instant",
        "Put a +1/+1 counter on target creature. If you control a Human, put two +1/+1 counters on it instead.",
    );
    assert_eq!(state, "sequence.conditional.instead");
    // A player comparison is not a supported atom of the shared count.
    failure(
        "Sorcery",
        "If you control more creatures than each other player, draw a card.",
    );
    // CR 608.2h: a destroyed permanent needs last-known information.
    let (state, expected) = failure(
        "Instant",
        "Destroy target creature. If its mana value was 3 or less, you gain 3 life.",
    );
    assert_eq!(state, "condition.sentence.object.last-known-information");
    assert!(expected.contains("last-known information"));
    // `Otherwise` without a preceding conditional sentence.
    let (state, _) = failure("Sorcery", "Draw a card. Otherwise, discard a card.");
    assert_eq!(state, "sequence.conditional.otherwise");
}

#[test]
fn an_intervening_trigger_condition_is_not_a_sentence_conditional() {
    // The `if` after the trigger comma belongs to the trigger parent (CR
    // 603.4); the sequence never reinterprets it as a resolution condition.
    let parsed = parse(
        "Creature \u{2014} Bird",
        "{1}{W}",
        "When this creature enters, if it's your turn, draw a card.",
    );
    let ability = parsed.abilities.last().expect("one ability");
    if let Some(rule) = &ability.rule {
        assert!(
            !rule.to_string().contains("\"kind\":\"conditional\""),
            "{rule}"
        );
    }
}

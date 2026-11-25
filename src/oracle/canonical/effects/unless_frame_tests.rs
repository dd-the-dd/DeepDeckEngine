//! `EFFECT unless PAYER COST` (CR 118.12) through every program parent that
//! lowers the shared sequence: one `tryPayCost` by the bound payer, then the
//! guarded actions when it was not paid.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Unless Witness".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rule(type_line: &str, text: &str) -> Value {
    let parsed = parse(type_line, text);
    assert_eq!(
        parsed.status, "canonical",
        "{text}: {:#?}",
        parsed.abilities
    );
    parsed.abilities[0].rule.clone().expect("canonical rule")
}

fn self_controller() -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "self" } })
}

fn target_controller(id: &str) -> Value {
    json!({ "kind": "controllerOf", "object": { "kind": "chosenTarget", "id": id } })
}

/// The two steps an `unless` payment frame lowers to.
fn unless_payment(id: &str, player: Value, cost: Value, guarded: Value) -> [Value; 2] {
    [
        json!({ "kind": "tryPayCost", "id": id, "player": player, "cost": cost }),
        json!({
            "kind": "conditional",
            "condition": {
                "kind": "costWasPaid", "decisionId": id, "value": false, "required": true,
            },
            "then": guarded,
            "else": [],
        }),
    ]
}

#[test]
fn the_controller_pays_with_ordinary_payment_actions() {
    // Archway Commons and the upkeep-tax creatures: payer `you`.
    for (type_line, text, cost) in [
        (
            "Land",
            "When this land enters, sacrifice it unless you pay {1}.",
            json!({ "kind": "payMana", "manaCost": "{1}" }),
        ),
        (
            "Creature — Spirit",
            "At the beginning of your upkeep, sacrifice this creature unless you discard a card.",
            json!({ "kind": "discardCard", "where": null }),
        ),
        (
            "Creature — Elemental",
            "At the beginning of your upkeep, sacrifice this creature unless you sacrifice a land.",
            json!({
                "kind": "sacrificePermanent",
                "where": { "kind": "cardTypeContains", "value": "Land" },
            }),
        ),
    ] {
        let rule = rule(type_line, text);
        assert_eq!(
            rule["effects"],
            json!(unless_payment(
                "unlessPayment0",
                self_controller(),
                cost,
                json!([{ "kind": "sacrificePermanent", "permanent": { "kind": "self" } }]),
            )),
            "{text}"
        );
    }
}

#[test]
fn the_guarded_action_target_controller_pays_in_every_ability_context() {
    // Activated ability (Daring Apprentice family): `its controller`.
    let activated = rule(
        "Creature — Wizard",
        "{T}: Counter target spell unless its controller pays {1}.",
    );
    assert_eq!(
        activated["effects"],
        json!(unless_payment(
            "unlessPayment0",
            target_controller("targetSpell"),
            json!({ "kind": "payMana", "manaCost": "{1}" }),
            json!([{ "kind": "counterSpell", "spell": { "kind": "chosenTarget", "id": "targetSpell" } }]),
        ))
    );
    // A tapped permanent target (Icy Manipulator-style tax).
    let tap = rule(
        "Artifact",
        "{1}, {T}: Tap target creature unless its controller pays 2 life.",
    );
    assert_eq!(
        tap["effects"][0]["player"],
        target_controller("targetPermanent")
    );
    assert_eq!(
        tap["effects"][0]["cost"],
        json!({ "kind": "payLife", "amount": { "kind": "integer", "value": 2 } })
    );
    // Modal spell: the payment result id is local to its mode.
    let modal = rule(
        "Instant",
        "Choose one —\n• Counter target spell unless its controller pays {4}.\n• Draw two cards.",
    );
    let mode = &modal["effects"][0]["then"];
    assert_eq!(mode[0]["id"], "mode1:unlessPayment0");
    assert_eq!(mode[1]["condition"]["decisionId"], "mode1:unlessPayment0");
    assert_eq!(mode[0]["player"], target_controller("mode1:targetSpell"));
}

#[test]
fn a_later_sentence_and_a_postposed_x_compose_with_the_frame() {
    // Dissolve-like spell with a tax: the following sentence is unguarded.
    let spell = rule(
        "Instant",
        "Counter target spell unless its controller pays {1}. Scry 1.",
    );
    let effects = spell["effects"].as_array().expect("effects");
    assert_eq!(effects[0]["kind"], "tryPayCost");
    assert_eq!(effects[1]["then"][0]["kind"], "counterSpell");
    assert_eq!(effects[2]["kind"], "lookAtTopCards");
    // `where X` defines the payment amount before the attempt (CR 107.3).
    let tax = rule(
        "Creature — Wizard",
        "Sacrifice this creature: Counter target instant or sorcery spell unless its controller pays {X}, where X is this creature's power.",
    );
    assert_eq!(tax["effects"][0]["kind"], "bind");
    assert_eq!(
        tax["effects"][1]["cost"],
        json!({ "kind": "payMana", "genericAmount": { "kind": "boundValue", "id": "whereX" } })
    );
    // Alpharael: the frame guards only its own sentence's action.
    let discard = rule(
        "Creature — Angel",
        "When this creature enters, draw two cards. Then discard two cards unless you discard an artifact card.",
    );
    let effects = discard["effects"].as_array().expect("effects");
    assert_eq!(effects[0]["kind"], "drawCards");
    assert_eq!(effects[1]["id"], "unlessPayment1");
    assert_eq!(
        effects[1]["cost"],
        json!({ "kind": "discardCard", "where": { "kind": "cardTypeContains", "value": "Artifact" } })
    );
    assert_eq!(effects[2]["then"][0]["kind"], "discardCards");
}

#[test]
fn new_payer_and_cost_combinations_need_no_new_production() {
    // A third-person sacrifice by the controller of a destroyed target.
    let destroy = rule(
        "Sorcery",
        "Destroy target artifact unless its controller sacrifices a creature of their choice.",
    );
    assert_eq!(
        destroy["effects"],
        json!(unless_payment(
            "unlessPayment0",
            target_controller("targetPermanent"),
            json!({
                "kind": "sacrificePermanent",
                "where": { "kind": "cardTypeContains", "value": "Creature" },
            }),
            json!([{ "kind": "destroyPermanent", "permanent": { "kind": "chosenTarget", "id": "targetPermanent" } }]),
        ))
    );
    // A bounce taxed with a discard by the returned creature's controller.
    let bounce = rule(
        "Instant",
        "Return target creature to its owner's hand unless its controller discards a card.",
    );
    assert_eq!(
        bounce["effects"][0]["player"],
        target_controller("targetPermanent")
    );
    assert_eq!(bounce["effects"][0]["cost"]["kind"], "discardCard");
}

#[test]
fn an_unless_condition_negates_the_shared_controller_guard() {
    let pirate = rule(
        "Creature — Pirate",
        "When this creature enters, you lose 2 life unless you control another Pirate.",
    );
    let effects = pirate["effects"].as_array().expect("effects");
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0]["kind"], "conditional");
    assert_eq!(effects[0]["condition"]["kind"], "not");
    assert_eq!(effects[0]["then"][0]["kind"], "loseLife");
}

#[test]
fn the_single_sentence_counter_tax_keeps_its_promoted_ir() {
    let rule = rule(
        "Instant",
        "Counter target spell unless its controller pays {3}.",
    );
    assert_eq!(rule["effects"][0]["id"], "resolutionPayment0");
    assert_eq!(rule["effects"][1]["then"][0]["kind"], "counterStackObject");
}

#[test]
fn unsupported_unless_shapes_stay_explicit_failures() {
    for (type_line, text, state) in [
        (
            "Enchantment",
            "At the beginning of your upkeep, sacrifice this enchantment unless any player pays {3}.",
            "sequence.unless.payer.indefinite-player",
        ),
        (
            "Creature — Zombie",
            "When this creature enters, sacrifice it unless it's attacking.",
            // Read as a state condition; its `it` has no proven antecedent.
            // (`unless it escaped` reads the source's cast record and is
            // supported: `escape_cast_record_guards_the_entry_sacrifice`.)
            "condition.sentence.object.antecedent",
        ),
        // `that player` is the triggering player, not a player declared by
        // the guarded action; never guessed.
        (
            "Enchantment",
            "Whenever an opponent casts a noncreature spell, you may draw a card unless that player pays {4}.",
            "sequence.unless.payer.bound-player",
        ),
        (
            "Creature — Beast",
            "At the beginning of your upkeep, put a wind counter on this creature, then sacrifice this creature unless you pay {G} for each wind counter on it.",
            "sequence.unless.sentence-end",
        ),
    ] {
        let parsed = parse(type_line, text);
        assert_ne!(parsed.status, "canonical", "{text}");
        assert!(
            parsed.abilities[0]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.state.as_deref() == Some(state)),
            "{text}: {:#?}",
            parsed.abilities[0].diagnostics
        );
    }
}

/// Escape creatures (`When this creature enters, sacrifice it unless it
/// escaped.`, CR 702.138c): the guarded sacrifice is skipped when the
/// source's cast record says it escaped.
#[test]
fn escape_cast_record_guards_the_entry_sacrifice() {
    let rule = rule(
        "Creature \u{2014} Zombie",
        "When this creature enters, sacrifice it unless it escaped.",
    );
    let text = rule.to_string();
    assert!(text.contains("sacrifice"), "{rule:#}");
    assert!(text.contains("scape"), "{rule:#}");
}

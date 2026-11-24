//! The shared `choose DOMAIN` instruction frame: target declarations (CR
//! 115.1, 601.2c), a creature type, a color and a player chosen on
//! resolution (CR 608.2d) read by later consumers through their bindings,
//! and explicit failures for the domains whose binding consumers are not
//! executable yet.
use serde_json::Value;

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{3}{U}".to_string()),
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

/// The rule of the last non-empty ability (the effect-bearing one).
fn canonical_rule(name: &str, type_line: &str, text: &str) -> Value {
    let parsed = parse(name, type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_eq!(ability.status, "canonical", "{text}: {parsed:#?}");
    ability.rule.clone().expect("canonical rule")
}

fn failure_states(name: &str, type_line: &str, text: &str) -> Vec<String> {
    let parsed = parse(name, type_line, text);
    let ability = parsed
        .abilities
        .iter()
        .rev()
        .find(|ability| ability.status != "empty")
        .expect("one ability");
    assert_ne!(ability.status, "canonical", "{text}");
    ability
        .diagnostics
        .iter()
        .filter_map(|diagnostic| diagnostic.state.clone())
        .collect()
}

#[test]
fn chosen_creature_type_is_produced_once_then_read_by_a_count() {
    // Real cards: a spell and an enters trigger share the same frame.
    for (name, type_line, text, action) in [
        (
            "Distant Melody",
            "Sorcery",
            "Choose a creature type. Draw a card for each permanent you control of that type.",
            "drawCards",
        ),
        (
            "Luminescent Rain",
            "Instant",
            "Choose a creature type. You gain 2 life for each permanent you control of that type.",
            "gainLife",
        ),
        (
            "Pol Jamaar, Illusionist",
            "Legendary Creature — Human Illusion Wizard",
            "Flying\nWhen Pol Jamaar enters, choose a creature type. Draw a card for each creature you control of that type.",
            "drawCards",
        ),
    ] {
        let rule = canonical_rule(name, type_line, text);
        let effects = rule["effects"].as_array().expect("effect program");
        assert_eq!(effects.len(), 2, "{name}");
        assert_eq!(effects[0]["kind"], "chooseCreatureType");
        assert_eq!(effects[0]["id"], "resolutionCreatureType");
        assert_eq!(effects[0]["player"]["kind"], "controllerOf");
        assert_eq!(effects[1]["kind"], action);
        let rendered = effects[1].to_string();
        assert!(
            rendered.contains("\"countPermanents\""),
            "{name}: {rendered}"
        );
        assert!(
            rendered.contains("\"decisionId\":\"resolutionCreatureType\""),
            "{name}: {rendered}"
        );
    }
}

#[test]
fn chosen_type_recombines_with_a_target_player_and_the_long_reference_form() {
    // New combination, not used to design the frame: a targeted player action
    // scaled by `the chosen creature type` count.
    let rule = canonical_rule(
        "Generalized Tribal Gift",
        "Sorcery",
        "Choose a creature type. Target player draws a card for each creature you control of the chosen creature type.",
    );
    assert_eq!(rule["effects"][0]["kind"], "chooseCreatureType");
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
    assert_eq!(rule["effects"][1]["player"]["kind"], "chosenTarget");
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseTargets");
}

#[test]
fn choose_target_declares_the_slot_that_later_references_bind() {
    // Real card: the chosen target is `it` in both conditional branches.
    let rule = canonical_rule(
        "Shackle Slinger",
        "Creature — Human Soldier",
        "Whenever you cast your second spell each turn, choose target creature an opponent controls. If it's tapped, put a stun counter on it. Otherwise, tap it. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
    );
    let rendered = rule.to_string();
    assert!(rendered.contains("\"chooseTargets\""), "{rendered}");
    assert!(!rendered.contains("\"chooseCreatureType\""));

    // Permanent target with a nominal anaphor and a preposed duration.
    let rule = canonical_rule(
        "Generalized Chosen Champion",
        "Instant",
        "Choose target creature you control. Until end of turn, that creature gets +2/+0 and gains trample.",
    );
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    let id = decisions[0]["id"].clone();
    for effect in rule["effects"].as_array().unwrap() {
        assert_eq!(effect["object"]["kind"], "chosenTarget");
        assert_eq!(effect["object"]["id"], id);
    }

    // Player target bound by `that player`.
    let rule = canonical_rule(
        "Generalized Chosen Rival",
        "Sorcery",
        "Choose target opponent. That player loses 2 life.",
    );
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(decision["candidates"]["where"]["kind"], "isOpponentOf");
    assert_eq!(rule["effects"][0]["player"]["id"], decision["id"]);
    assert_eq!(rule["effects"].as_array().unwrap().len(), 1);
}

#[test]
fn unsupported_choices_and_unproved_bindings_fail_at_their_own_state() {
    for (text, state) in [
        // A reference without its producing choice is never guessed.
        (
            "Draw a card for each permanent you control of that type.",
            "binding.chosen-creature-type.source",
        ),
        // A battlefield count or a one-shot battlefield action reads the
        // choice; a lasting effect's collection is not an executable reader.
        (
            "Choose a creature type. Creatures you control of the chosen type get +1/+0 until end of turn.",
            "binding.chosen-creature-type.consumer",
        ),
        // Two choices of one domain would make `that type` ambiguous.
        (
            "Choose a creature type. Choose a creature type. Draw a card for each permanent you control of that type.",
            "lower.choose.creature-type",
        ),
        (
            "Choose a creature type other than Wall. Draw a card for each permanent you control of that type.",
            "choose.domain.creature-type.exclusion",
        ),
        // A restricted color set has no producer yet.
        (
            "Choose a color other than white. Creatures of the chosen color get +1/+1 until end of turn.",
            "choose.domain.color.exclusion",
        ),
        // A chosen color that nothing reads is refused, not a no-op choice.
        (
            "Choose a color. Draw a card.",
            "binding.chosen-color.consumer",
        ),
        // A chosen-color reference without its producer is never guessed.
        (
            "Creatures of the chosen color get +1/+1 until end of turn.",
            "binding.chosen-color.source",
        ),
        // A chosen card name that nothing reads is refused, not a no-op.
        (
            "Choose a card name. Target opponent reveals their hand.",
            "binding.chosen-card-name.consumer",
        ),
        (
            "Choose a nonartifact, nonland card name. Draw a card.",
            "binding.chosen-card-name.consumer",
        ),
        // A qualified name choice is not widened to every card name.
        (
            "Choose a card name that hasn't been chosen. Draw a card.",
            "choose.domain.card-name.boundary",
        ),
        // A qualified player population is not widened to every opponent.
        (
            "Choose an opponent who controls a creature. That player loses 2 life.",
            "choose.domain.player.boundary",
        ),
        // A chosen player that no instruction reads is refused.
        (
            "Choose an opponent. Draw a card.",
            "lower.choose.player.consumer",
        ),
        // An unbounded number has no decision protocol.
        (
            "Choose a number. Draw a card.",
            "choose.domain.number.range",
        ),
        (
            "Choose any target. Draw a card.",
            "choose.domain.target.kind",
        ),
        (
            "Choose target creature card in your graveyard. Return it to your hand.",
            "choose.domain.target.kind",
        ),
    ] {
        let states = failure_states("Near Miss", "Sorcery", text);
        assert!(
            states.iter().any(|actual| actual == state),
            "{text}: {states:?}"
        );
    }
    // A plural choice binds no singular pronoun.
    assert!(
        !failure_states(
            "Near Miss",
            "Sorcery",
            "Choose two target creatures. Tap them."
        )
        .is_empty()
    );
}

#[test]
fn a_color_chosen_on_resolution_is_read_by_a_group_predicate() {
    // Sibling of the creature-type frame: no color-specific consumer exists;
    // the shared object query reads `of the chosen color`.
    for text in [
        "Choose a color. Creatures of the chosen color get +1/+1 until end of turn.",
        "Choose a color. Creatures you control of the chosen color get +1/+0 until end of turn.",
    ] {
        let rule = canonical_rule("Generalized Color Rally", "Instant", text);
        let effects = rule["effects"].as_array().expect("effect program");
        assert_eq!(effects.len(), 2, "{text}");
        assert_eq!(effects[0]["kind"], "chooseColor");
        assert_eq!(effects[0]["id"], "resolutionColor");
        assert_eq!(effects[0]["player"]["kind"], "controllerOf");
        let rendered = effects[1].to_string();
        assert!(rendered.contains("\"chosenColor\""), "{text}: {rendered}");
        assert!(
            rendered.contains("\"decisionId\":\"resolutionColor\""),
            "{text}: {rendered}"
        );
    }
}

#[test]
fn a_player_chosen_on_resolution_is_the_antecedent_of_that_player() {
    let rule = canonical_rule(
        "Generalized Chosen Rival",
        "Sorcery",
        "Choose an opponent. That player loses 2 life.",
    );
    let effects = rule["effects"].as_array().expect("effect program");
    assert_eq!(effects[0]["kind"], "choosePlayers");
    assert_eq!(effects[0]["id"], "resolutionPlayer0");
    assert_eq!(effects[0]["bindAs"], "resolutionPlayer0");
    assert_eq!(effects[0]["candidates"]["kind"], "opponentsOf");
    assert!(effects[0].get("selection").is_none());
    // Not a target: no declaration, and the later reference reads the
    // bound value the choice writes.
    assert!(
        rule["declaration"]["decisions"]
            .as_array()
            .is_none_or(Vec::is_empty),
        "{rule:#}"
    );
    assert_eq!(effects[1]["kind"], "loseLife");
    assert_eq!(effects[1]["player"]["kind"], "boundValue");
    assert_eq!(effects[1]["player"]["id"], "resolutionPlayer0");
}

#[test]
fn a_random_player_choice_recombines_with_a_count_of_absent_counters() {
    // Previously unused combination: random player choice, `that player`,
    // and a count of permanents with no counters on them.
    let rule = canonical_rule(
        "Generalized Random Reckoning",
        "Sorcery",
        "Choose an opponent at random. That player loses life equal to the number of creatures you control with no counters on them.",
    );
    let effects = rule["effects"].as_array().expect("effect program");
    assert_eq!(effects[0]["kind"], "choosePlayers");
    assert_eq!(effects[0]["selection"]["kind"], "random");
    assert_eq!(effects[1]["kind"], "loseLife");
    assert_eq!(effects[1]["player"]["kind"], "boundValue");
    let amount = effects[1]["amount"].to_string();
    assert!(amount.contains("\"countPermanents\""), "{amount}");
    assert!(amount.contains("\"hasCounter\""), "{amount}");
}

#[test]
fn a_stored_entry_choice_binds_the_static_reference_instead_of_a_resolution_one() {
    // The `As ... enters` replacement stores its own decision; a static
    // ability reading `of the chosen type` has no resolution producer, so the
    // card-level pass binds it to the stored entry decision (CR 614.12),
    // never to the resolution-time choice id.
    let parsed = parse(
        "Generalized Lord",
        "Creature — Elf",
        "As this creature enters, choose a creature type.\nOther creatures you control of the chosen type get +1/+1.",
    );
    assert_eq!(parsed.abilities[0].status, "canonical");
    let entry = parsed.abilities[0].rule.as_ref().expect("entry rule");
    let stored = entry["decisions"][0]["id"]
        .as_str()
        .expect("stored entry decision")
        .to_string();
    let lord = &parsed.abilities[1];
    assert_eq!(lord.status, "canonical", "{parsed:#?}");
    let text = lord.rule.as_ref().expect("lord rule").to_string();
    assert!(text.contains(&format!("\"{stored}\"")), "{text}");
    assert!(!text.contains("resolutionCreatureType"), "{text}");
}

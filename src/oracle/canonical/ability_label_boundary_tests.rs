//! CR 207.2c-d: an ability word or flavor word labels an ability and has no
//! rules meaning. The ability boundary strips it once, so a labeled ability
//! compiles exactly like its unlabeled body; labels with rules stay.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Label Boundary Probe".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn assert_same_as_body(type_line: &str, label: &str, body: &str) {
    let labeled = parse(type_line, &format!("{label} \u{2014} {body}"));
    let unlabeled = parse(type_line, body);
    assert_eq!(
        unlabeled.status, "canonical",
        "{body}: {:#?}",
        unlabeled.diagnostics
    );
    assert_eq!(
        labeled.status, "canonical",
        "{label}: {:#?}",
        labeled.diagnostics
    );
    assert_eq!(
        labeled.abilities[0].rule, unlabeled.abilities[0].rule,
        "{label} \u{2014} {body}"
    );
}

#[test]
fn fateful_hour_spell_body_compiles_like_its_unlabeled_body() {
    // Spell Snuff, real Oracle text: the second ability is a registered
    // ability word over a conditional spell instruction, which the spell
    // effect parent used to read from the label (`effect.start`, token 0).
    let card = parse(
        "Instant",
        "Counter target spell.\nFateful hour \u{2014} If you have 5 or less life, draw a card.",
    );
    assert_eq!(card.status, "canonical", "{:#?}", card.diagnostics);
    assert_same_as_body(
        "Instant",
        "Fateful hour",
        "If you have 5 or less life, draw a card.",
    );
}

#[test]
fn ability_words_and_flavor_words_share_the_boundary_for_every_frame() {
    for (type_line, label, body) in [
        ("Instant", "Radiance", "Draw a card."),
        (
            "Creature — Elf",
            "Landfall",
            "Whenever a land you control enters, you gain 1 life.",
        ),
        (
            "Creature — Elf",
            "Channel",
            "{1}{G}, Discard this card: Draw a card.",
        ),
        // A flavor word (CR 207.2d) that is no registered ability word.
        ("Instant", "Teetering Peaks", "Draw a card."),
    ] {
        assert_same_as_body(type_line, label, body);
    }
}

#[test]
fn a_body_failure_is_reported_at_its_position_in_the_whole_ability() {
    let body = "Frobnicate target creature.";
    let unlabeled = parse("Instant", body);
    let labeled = parse("Instant", &format!("Domain \u{2014} {body}"));
    assert_eq!(labeled.status, "unsupported");
    let (bare, shifted) = (&unlabeled.diagnostics[0], &labeled.diagnostics[0]);
    assert_eq!(shifted.state, bare.state);
    // Label words plus the em dash precede the body.
    assert_eq!(shifted.token_index, bare.token_index.map(|index| index + 2));
    let offset = "Domain \u{2014} ".len();
    assert_eq!(
        shifted.span_start,
        bare.span_start.map(|start| start + offset)
    );
}

#[test]
fn labels_with_rules_are_never_stripped() {
    // Exhaust is a keyword ability (CR 702.177): its label is not rules-free.
    // The label keeps its rule: the ability is classified and limited to
    // one activation per object, unlike the stripped body alone.
    let exhausted = parse("Creature — Human", "Exhaust \u{2014} {2}: Draw a card.");
    assert_eq!(
        exhausted.status, "canonical",
        "{:#?}",
        exhausted.diagnostics
    );
    let rule = exhausted.abilities[0].rule.as_ref().expect("exhaust rule");
    assert_eq!(rule["abilityClass"], "exhaust");
    assert_eq!(rule["activationLimit"]["kind"], "oncePerGameObjectAbility");
    // Saga chapter symbols number chapter abilities (CR 714.2b).
    let saga = parse(
        "Enchantment — Saga",
        "(As this Saga enters and after your draw step, add a lore counter.)\nI \u{2014} This Saga gains \"{T}: Add {C}.\"",
    );
    assert_eq!(saga.status, "canonical", "{:#?}", saga.diagnostics);
    assert!(
        saga.abilities[saga.abilities.len() - 1]
            .rule
            .as_ref()
            .is_some_and(|rule| rule.to_string().contains("chapter")),
        "{:#?}",
        saga.abilities[saga.abilities.len() - 1].rule
    );
    // A modal header is not a label.
    let modal = parse(
        "Instant",
        "Choose one \u{2014}\n• Draw a card.\n• You gain 3 life.",
    );
    assert_eq!(modal.status, "canonical", "{:#?}", modal.diagnostics);
}

use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

const REMINDER: &str = "(Put two loyalty counters on a Jace token you control. If you don't control one, first create a blue Jace planeswalker token with \"[\u{2212}1]: Surveil 1\" and \"[\u{2212}3]: Draw a card.\")";

fn parse(name: &str, type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn gate_state(parsed: &OracleCardParseResult) -> Option<&str> {
    let ability = &parsed.abilities[0];
    (ability.status != "canonical").then_some(())?;
    ability
        .diagnostics
        .first()
        .and_then(|diagnostic| diagnostic.state.as_deref())
}

/// The Jace token's printed loyalty abilities do not compile yet, so creating
/// it would drop abilities. Every shape of the action must fail at the same
/// explicit state until the shared loyalty-ability grammar covers them.
#[test]
fn every_shape_composes_once_the_token_definition_is_complete() {
    for (name, type_line, text) in [
        (
            "Academic Ascent",
            "Sorcery",
            format!("Empower Jace 2. {REMINDER}"),
        ),
        (
            "Keeper of the Quiet Hour",
            "Creature \u{2014} Human Wizard",
            format!("When this creature enters, empower Jace 2. {REMINDER}"),
        ),
        (
            "Activated Mentor",
            "Artifact",
            format!("{{6}}: Empower Jace 2. {REMINDER}"),
        ),
        (
            "Reminderless Draw",
            "Instant",
            "Draw a card. Empower Jace 1.".to_string(),
        ),
        (
            "Variable Enforcer",
            "Creature \u{2014} Construct",
            "Whenever this creature attacks, empower Jace X, where X is the number of creatures you control.".to_string(),
        ),
    ] {
        let parsed = parse(name, type_line, &text);
        // `where X is EXPR` composes through the shared numeric definition.
        assert_eq!(parsed.status, "canonical", "{name}: {:?}", gate_state(&parsed));
    }
}

#[test]
fn other_planeswalker_names_are_not_this_keyword_action() {
    let other = parse("Other Walker", "Sorcery", "Empower Chandra 2.");
    assert_ne!(gate_state(&other), Some("empower.token.definition"));
    assert_ne!(other.abilities[0].status, "canonical");
}

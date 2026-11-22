//! Template keywords (CR 702) compile their rules definition through the
//! ordinary ability parser: Oracle keyword line -> instantiated definition ->
//! the same IR the printed definition compiles to, with keyword provenance.
use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, keyword_expansion_rules,
    parse_oracle_card,
};
use serde_json::{Value, json};

fn parse(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{1}{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread")
        .join()
        .expect("parser")
}

/// The rules the printed definition text compiles to on the same face.
fn printed_rules(type_line: &str, definition: &str) -> Vec<Value> {
    let parsed = parse("Definition Probe", type_line, definition);
    assert_eq!(
        parsed.status, "canonical",
        "{definition}: {:#?}",
        parsed.diagnostics
    );
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("canonical rule"))
        .collect()
}

fn single_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let [ability] = parsed.abilities.as_slice() else {
        panic!("one ability: {:#?}", parsed.abilities);
    };
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    ability.rule.as_ref().expect("canonical rule")
}

#[test]
fn accorder_paladin_battle_cry_compiles_its_cr_definition_with_provenance() {
    const TYPE_LINE: &str = "Creature \u{2014} Human Knight";
    let parsed = parse(
        "Accorder Paladin",
        TYPE_LINE,
        "Battle cry (Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn.)",
    );
    let rule = single_rule(&parsed);
    let definition = printed_rules(
        TYPE_LINE,
        "Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn.",
    );
    assert_eq!(rule["kind"], "keywordExpansion");
    assert_eq!(rule["keyword"], "battle-cry");
    assert_eq!(rule["parameters"], json!({}));
    assert_eq!(rule["definition"]["rulesSection"], "702.91a");
    assert_eq!(rule["definition"]["rulesClause"], "battle-cry.cr-702-91a");
    assert_eq!(rule["rules"], Value::Array(definition.clone()));

    // Flattened, the playable rule is the definition's rule plus provenance.
    let playable = keyword_expansion_rules(rule);
    let [flattened] = playable.as_slice() else {
        panic!("one playable rule: {playable:#?}");
    };
    let mut expected = definition[0].clone();
    expected["keywordProvenance"] = json!({
        "keyword": "battle-cry",
        "parameters": {},
        "rulesSection": "702.91a",
        "templateVersion": 1,
        "part": 0,
    });
    assert_eq!(flattened, &expected);
}

#[test]
fn a_bare_keyword_line_and_an_unrelated_rule_flatten_independently() {
    // Goblin Wardriver prints no reminder on some printings' reference text.
    let parsed = parse(
        "Goblin Wardriver",
        "Creature \u{2014} Goblin Warrior",
        "Battle cry\nFlying",
    );
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    let expansion = parsed.abilities[0].rule.as_ref().unwrap();
    let flying = parsed.abilities[1].rule.as_ref().unwrap();
    assert_eq!(expansion["kind"], "keywordExpansion");
    assert_eq!(keyword_expansion_rules(flying), vec![flying.clone()]);
}

#[test]
fn a_foreign_reminder_fails_explicitly_instead_of_being_ignored() {
    let parsed = parse(
        "Near Miss",
        "Creature \u{2014} Human Knight",
        "Battle cry (Whenever this creature attacks, it gets +1/+0 until end of turn.)",
    );
    assert_eq!(parsed.status, "unsupported");
    let diagnostic = &parsed.abilities[0].diagnostics[0];
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("keyword.template.reminder.body")
    );
    assert_eq!(diagnostic.automaton.as_deref(), Some("keyword-template"));
}

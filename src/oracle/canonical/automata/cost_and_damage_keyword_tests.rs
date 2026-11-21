use crate::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use serde_json::{Value, json};

fn single_rule(name: &str, type_line: &str, text: &str) -> Result<Value, String> {
    let result = parse_oracle_card_native(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let [ability] = result.abilities.as_slice() else {
        panic!("{name}: expected one ability");
    };
    ability.rule.clone().ok_or_else(|| {
        ability.diagnostics[0]
            .state
            .clone()
            .unwrap_or_else(|| ability.diagnostics[0].code.clone())
    })
}

/// Real keyword lines with their validated reminders (CR 702.25, 702.51,
/// 702.80, 702.126) lower to the engine's keyword kinds.
#[test]
fn payment_and_damage_keywords_lower_with_validated_reminders() {
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "Chord of Calling",
            "Instant",
            "Convoke (Your creatures can help cast this spell. Each creature you tap while casting this spell pays for {1} or one mana of that creature's color.)",
            "convoke",
        ),
        (
            "Reverse Engineer",
            "Sorcery",
            "Improvise (Your artifacts can help cast this spell. Each artifact you tap after you're done activating mana abilities pays for {1}.)",
            "improvise",
        ),
        (
            "Boggart Ram-Gang",
            "Creature — Goblin Warrior",
            "Wither (This deals damage to creatures in the form of -1/-1 counters.)",
            "wither",
        ),
        (
            "Knight of the Holy Nimbus",
            "Creature — Human Rebel Knight",
            "Flanking (Whenever a creature without flanking blocks this creature, the blocking creature gets -1/-1 until end of turn.)",
            "flanking",
        ),
    ];
    for (name, type_line, text, kind) in cases {
        let rule =
            single_rule(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
        assert_eq!(rule["kind"], "keywordAbility", "{name}");
        assert_eq!(rule["ability"], json!({ "kind": kind }), "{name}");
    }
}

/// An unused list combination composes without new rules; an altered
/// reminder is a failure inside the reminder schema, never dropped.
#[test]
fn keyword_list_combination_and_reminder_near_miss() {
    let rule =
        single_rule("Generic Raider", "Creature — Elf", "Flanking, wither").expect("keyword list");
    assert_eq!(
        rule["abilities"],
        json!([{ "kind": "flanking" }, { "kind": "wither" }])
    );
    let state = single_rule(
        "Generic Blight",
        "Creature — Elf",
        "Wither (This deals damage to players in the form of poison counters.)",
    )
    .expect_err("altered reminder");
    assert_eq!(state, "keyword.reminder.body");
}

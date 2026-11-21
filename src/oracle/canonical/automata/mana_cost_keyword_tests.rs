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

fn pay(cost: &str) -> Value {
    json!({ "kind": "payMana", "manaCost": cost })
}

/// Real keyword lines whose reminders are validated against the printed or
/// rules-fixed cost (CR 702.37, 702.109, 702.170, 702.185).
#[test]
fn mana_cost_keywords_lower_to_engine_keyword_kinds() {
    let cases: &[(&str, &str, &str, Value)] = &[
        (
            "Exalted Angel",
            "Creature — Angel",
            "Morph {2}{W}{W} (You may cast this card face down as a 2/2 creature for {3}. Turn it face up any time for its morph cost.)",
            json!({"kind": "morph", "cost": pay("{2}{W}{W}"), "putCounterWhenTurnedFaceUp": false}),
        ),
        (
            "Den Protector",
            "Creature — Human Warrior",
            "Megamorph {1}{G} (You may cast this card face down as a 2/2 creature for {3}. Turn it face up any time for its megamorph cost and put a +1/+1 counter on it.)",
            json!({"kind": "morph", "cost": pay("{1}{G}"), "putCounterWhenTurnedFaceUp": true}),
        ),
        (
            "Goblin Heelcutter",
            "Creature — Goblin Berserker",
            "Dash {2}{R} (You may cast this spell for its dash cost. If you do, it gains haste, and it's returned from the battlefield to its owner's hand at the beginning of the next end step.)",
            json!({"kind": "dash", "cost": pay("{2}{R}")}),
        ),
        (
            "Aven Interrupter",
            "Creature — Bird Rogue",
            "Plot {2}{W} (You may pay {2}{W} and exile this card from your hand. Cast it as a sorcery on a later turn without paying its mana cost. Plot only as a sorcery.)",
            json!({"kind": "plot", "cost": pay("{2}{W}")}),
        ),
        (
            "Starfield Shepherd",
            "Creature — Angel",
            "Warp {1}{W} (You may cast this card from your hand for its warp cost. Exile this creature at the beginning of the next end step, then you may cast it from exile on a later turn.)",
            json!({"kind": "warp", "cost": pay("{1}{W}")}),
        ),
    ];
    for (name, type_line, text, expected) in cases {
        let rule =
            single_rule(name, type_line, text).unwrap_or_else(|state| panic!("{name}: {state}"));
        assert_eq!(rule["kind"], "keywordAbility", "{name}");
        assert_eq!(&rule["ability"], expected, "{name}");
    }
}

/// An unused combination: a reminder-free mana-cost keyword in a list.
/// Near-misses: a reminder cost that differs from the printed one, and a
/// non-mana cost, fail inside the atom instead of being dropped.
#[test]
fn mana_cost_keyword_composition_and_near_misses() {
    let rule = single_rule("Generic Scout", "Creature — Human", "Flying, dash {1}{U}")
        .expect("keyword list");
    assert_eq!(
        rule["abilities"],
        json!([{ "kind": "flying" }, { "kind": "dash", "cost": pay("{1}{U}") }])
    );
    let mismatch = single_rule(
        "Generic Outlaw",
        "Creature — Human Rogue",
        "Plot {2}{W} (You may pay {1}{W} and exile this card from your hand. Cast it as a sorcery on a later turn without paying its mana cost. Plot only as a sorcery.)",
    )
    .expect_err("reminder cost mismatch");
    // Plot owns its reminder (two sentences); the mismatch fails there.
    assert_eq!(mismatch, "keyword.plot.reminder");
    let nonmana = single_rule(
        "Generic Stalker",
        "Creature — Horror",
        "Warp {B}, Pay 2 life.",
    )
    .expect_err("a non-mana warp cost");
    assert!(
        nonmana.starts_with("keyword") || nonmana.starts_with("ability-frame"),
        "{nonmana}"
    );
}

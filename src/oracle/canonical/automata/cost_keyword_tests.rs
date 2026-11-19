use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};
use serde_json::{Value, json};

use super::{CostKeyword, CostKeywordParameter, OracleTokenCursor, recognize_cost_keyword};

fn recognize(text: &str) -> super::super::super::MachineResult<super::CostKeywordNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("cost-keyword-test", &tokens);
    recognize_cost_keyword(&mut cursor)
}

fn first_rule(name: &str, type_line: &str, text: &str) -> Value {
    let result = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    result.abilities[0]
        .rule
        .clone()
        .unwrap_or_else(|| panic!("{name}: {:?}", result.abilities[0].diagnostics))
}

#[test]
fn real_cards_lower_to_existing_keyword_primitives() {
    for (name, type_line, text, ability) in [
        (
            "Beastbond Outcaster",
            "Creature — Human Ranger",
            "Plot {1}{G} (You may pay {1}{G} and exile this card from your hand. Cast it as a sorcery on a later turn without paying its mana cost. Plot only as a sorcery.)",
            json!({ "kind": "plot", "cost": { "kind": "payMana", "manaCost": "{1}{G}" } }),
        ),
        (
            "Doomskar",
            "Sorcery",
            "Foretell {1}{W}{W} (During your turn, you may pay {2} and exile this card from your hand face down. Cast it on a later turn for its foretell cost.)",
            json!({
                "kind": "foretell",
                "cost": { "kind": "payMana", "manaCost": "{1}{W}{W}" },
                "exileCost": { "kind": "payMana", "manaCost": "{2}" },
            }),
        ),
        (
            "Life from the Loam",
            "Sorcery",
            "Dredge 3 (If you would draw a card, you may mill three cards instead. If you do, return this card from your graveyard to your hand.)",
            json!({ "kind": "dredge", "count": { "kind": "integer", "value": 3 } }),
        ),
        (
            "Desperate Ritual",
            "Instant — Arcane",
            "Splice onto Arcane {1}{R} (As you cast an Arcane spell, you may reveal this card from your hand and pay its splice cost. If you do, add this card's effects to that spell.)",
            json!({
                "kind": "splice",
                "onto": { "kind": "subtypeContains", "value": "Arcane" },
                "cost": { "kind": "payMana", "manaCost": "{1}{R}" },
            }),
        ),
    ] {
        let rule = first_rule(name, type_line, text);
        assert_eq!(rule["kind"], "keywordAbility", "{name}");
        assert_eq!(rule["ability"], ability, "{name}");
    }
}

#[test]
fn unused_parameter_combinations_reuse_the_same_children() {
    // Dredge 1 restates `a card` (number agreement), splice onto a card-type
    // union, and a bare keyword without its reminder: no production change.
    let dredge = recognize(
        "Dredge 1 (If you would draw a card, you may mill a card instead. If you do, return this card from your graveyard to your hand.)",
    )
    .expect("dredge 1");
    assert_eq!(dredge.parameter, CostKeywordParameter::Count(1));

    let splice = recognize(
        "Splice onto instant or sorcery {2}{U} (As you cast an instant or sorcery spell, you may reveal this card from your hand and pay its splice cost. If you do, add this card's effects to that spell.)",
    )
    .expect("splice onto a type union");
    assert_eq!(splice.keyword, CostKeyword::Splice);
    let rule = first_rule(
        "Splice Probe",
        "Instant",
        "Splice onto instant or sorcery {2}{U}",
    );
    assert_eq!(rule["ability"]["onto"]["kind"], "or");

    let bare = recognize("Plot {3}{R}{R}").expect("bare plot");
    assert_eq!(bare.keyword, CostKeyword::Plot);
}

#[test]
fn near_misses_fail_explicitly() {
    let mismatched = recognize(
        "Plot {1}{G} (You may pay {2}{G} and exile this card from your hand. Cast it as a sorcery on a later turn without paying its mana cost. Plot only as a sorcery.)",
    )
    .expect_err("reminder cost differs from the printed cost");
    assert_eq!(mismatched.state, "keyword.plot.reminder");

    let count = recognize(
        "Dredge 2 (If you would draw a card, you may mill three cards instead. If you do, return this card from your graveyard to your hand.)",
    )
    .expect_err("reminder count differs");
    assert_eq!(count.state, "keyword.dredge.reminder");

    let quality = recognize(
        "Splice onto Arcane {1}{R} (As you cast an instant spell, you may reveal this card from your hand and pay its splice cost. If you do, add this card's effects to that spell.)",
    )
    .expect_err("reminder quality differs");
    assert_eq!(quality.state, "keyword.splice.reminder");

    let nonmana = recognize("Splice onto Arcane—Sacrifice two Mountains.")
        .expect_err("a nonmana splice cost has no runtime proof here");
    assert_eq!(nonmana.state, "keyword.splice.cost");

    let variable = recognize("Foretell {X}{U}").expect_err("variable foretell cost");
    assert_eq!(variable.state, "keyword.foretell.parameter");
}

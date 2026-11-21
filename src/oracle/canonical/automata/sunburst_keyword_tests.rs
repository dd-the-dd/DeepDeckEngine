use crate::oracle::syntax::recognize_entities;
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

use super::super::{GrantedKeyword, KeywordAtomNode};
use super::{OracleTokenCursor, recognize_standalone_keyword_ability};

fn recognize(text: &str) -> super::super::super::MachineResult<Vec<KeywordAtomNode>> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("sunburst-keyword-test", &tokens);
    recognize_standalone_keyword_ability(&mut cursor).map(|list| list.keywords)
}

#[test]
fn sunburst_reminder_counter_follows_the_named_permanent_type() {
    for text in [
        "Sunburst",
        "Sunburst (This artifact enters with a charge counter on it for each color of mana spent to cast it.)",
        "Sunburst (This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.)",
        "Sunburst (This enchantment enters with a charge counter on it for each color of mana spent to cast it.)",
    ] {
        assert_eq!(
            recognize(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}")),
            vec![KeywordAtomNode::Parameterless(GrantedKeyword::Sunburst)],
            "{text}"
        );
    }
    assert_eq!(GrantedKeyword::Sunburst.canonical_name(), "sunburst");
}

#[test]
fn sunburst_composes_in_a_keyword_list_with_existing_atoms() {
    assert_eq!(
        recognize("Flying, sunburst").expect("bare list parses"),
        vec![
            KeywordAtomNode::Parameterless(GrantedKeyword::Flying),
            KeywordAtomNode::Parameterless(GrantedKeyword::Sunburst),
        ]
    );
}

#[test]
fn sunburst_reminder_rejects_a_counter_that_contradicts_its_subject() {
    let creature_with_charge = recognize(
        "Sunburst (This creature enters with a charge counter on it for each color of mana spent to cast it.)",
    )
    .expect_err("a creature subject names +1/+1 counters");
    assert_eq!(
        creature_with_charge.state,
        "keyword.sunburst.reminder.counter"
    );

    let artifact_with_plus = recognize(
        "Sunburst (This artifact enters with a +1/+1 counter on it for each color of mana spent to cast it.)",
    )
    .expect_err("a noncreature subject names charge counters");
    assert_eq!(
        artifact_with_plus.state,
        "keyword.sunburst.reminder.counter"
    );

    let altered_count = recognize(
        "Sunburst (This artifact enters with a charge counter on it for each mana spent to cast it.)",
    )
    .expect_err("the count is the number of colors spent");
    assert_eq!(altered_count.state, "keyword.sunburst.reminder.count");
}

#[test]
fn sunburst_cards_lower_to_the_singular_keyword_primitive() {
    for (name, type_line, text) in [
        (
            "Engineered Explosives",
            "Artifact",
            "Sunburst (This artifact enters with a charge counter on it for each color of mana spent to cast it.)",
        ),
        (
            "Skyreach Manta",
            "Artifact Creature — Fish",
            "Sunburst (This creature enters with a +1/+1 counter on it for each color of mana spent to cast it.)\nFlying",
        ),
    ] {
        let result = parse_oracle_card(OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
            mana_cost: Some("{5}".to_string()),
            oracle_text: Some(text.to_string()),
            layout: None,
            faces: Vec::new(),
        });
        let rule = result.abilities[0]
            .rule
            .as_ref()
            .unwrap_or_else(|| panic!("{name}: {:?}", result.abilities[0].diagnostics));
        assert_eq!(rule["kind"], "keywordAbility", "{name}");
        assert_eq!(rule["ability"]["kind"], "sunburst", "{name}");
    }
}

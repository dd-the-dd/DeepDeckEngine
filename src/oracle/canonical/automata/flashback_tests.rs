use super::*;
use crate::oracle::syntax::recognize_oracle_entities;
fn parse(text: &str) -> MachineResult<FlashbackKeywordNode> {
    let tokens = recognize_oracle_entities(text);
    recognize_flashback_keyword(&mut OracleTokenCursor::new("test.flashback", &tokens))
}
#[test]
fn fixed_flashback_cost_borrows_shared_mana_symbols_and_optional_reminder() {
    for cost in ["{2}{R}", "{0}", "{S}{W/U}", "{3}{B/P}"] {
        for suffix in [
            "",
            " (You may cast this card from your graveyard for its flashback cost. Then exile it.)",
        ] {
            let text = format!("Flashback {cost}{suffix}");
            assert_eq!(parse(&text).unwrap().cost.as_oracle_cost(), cost);
        }
    }
}
#[test]
fn unsupported_costs_and_hostile_reminders_never_become_prefix_coverage() {
    for text in [
        "Flashback {X}{R}",
        "Flashback {1}, Pay 3 life.",
        "Flashback Pay 3 life.",
        "Flashback Sacrifice a creature.",
        "Flashback {2}{R} (You may cast this card from your hand for its flashback cost. Then exile it.)",
        "Flashback {2}{R} (You may cast this card from your graveyard for its flashback cost. Then return it.)",
        "Flashback {2}{R}. Draw a card.",
        "Flashback {2}{R} (Then exile it.)",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

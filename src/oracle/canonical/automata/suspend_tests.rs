use super::*;
use crate::oracle::syntax::recognize_oracle_entities;

fn parse(text: &str) -> MachineResult<SuspendKeywordNode> {
    let tokens = recognize_oracle_entities(text);
    recognize_suspend_keyword(&mut OracleTokenCursor::new("test.suspend", &tokens))
}

#[test]
fn printed_count_and_fixed_cost_are_retained_with_or_without_the_reminder() {
    for (text, count, cost, haste) in [
        ("Suspend 4\u{2014}{1}{U}", 4, "{1}{U}", false),
        (
            "Suspend 4\u{2014}{1}{U} (Rather than cast this card from your hand, you may pay {1}{U} and exile it with four time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)",
            4,
            "{1}{U}",
            false,
        ),
        (
            "Suspend 1\u{2014}{2}{G}{G} (Rather than cast this card from your hand, you may pay {2}{G}{G} and exile it with a time counter on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost. It has haste.)",
            1,
            "{2}{G}{G}",
            true,
        ),
        (
            "Suspend 3\u{2014}{0} (Rather than cast this card from your hand, pay {0} and exile it with three time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)",
            3,
            "{0}",
            false,
        ),
        (
            "Suspend 17 \u{2014} {0} (Rather than cast this card from your hand, you may pay {0} and exile it with seventeen time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)",
            17,
            "{0}",
            false,
        ),
    ] {
        let node = parse(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert_eq!(node.time_counters, count, "{text}");
        assert_eq!(node.cost.as_oracle_cost(), cost, "{text}");
        assert_eq!(node.reminder_mentions_haste, haste, "{text}");
    }
}

#[test]
fn variable_or_contradicted_parameters_never_become_prefix_coverage() {
    for text in [
        // A variable count needs a casting-X producer and its own runtime proof.
        "Suspend X\u{2014}{X}{3}{U}. X can't be 0.",
        "Suspend 0\u{2014}{1}{U}",
        "Suspend 2\u{2014}{X}{R}",
        // The reminder must restate the printed parameters exactly.
        "Suspend 4\u{2014}{1}{U} (Rather than cast this card from your hand, you may pay {1}{U} and exile it with three time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)",
        "Suspend 4\u{2014}{1}{U} (Rather than cast this card from your hand, you may pay {U} and exile it with four time counters on it. At the beginning of your upkeep, remove a time counter. When the last is removed, you may cast it without paying its mana cost.)",
        "Suspend 2\u{2014}{2} (Deep Dish Pizza has no mana cost and must be suspended.)",
        // Trailing or nonmana cost text is not part of the keyword.
        "Suspend 4\u{2014}{1}{U}. Draw a card.",
        "Suspend 4\u{2014}Pay 3 life.",
        "Suspend 4 {1}{U}",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

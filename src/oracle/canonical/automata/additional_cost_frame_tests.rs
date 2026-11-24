use super::*;
use crate::oracle::syntax::recognize_entities;

fn frame(text: &str) -> MachineResult<AdditionalCostFrameNode> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("additional-cost-frame-test", &tokens);
    recognize_additional_cost_frame(&mut cursor)
}

fn kinds(frame: &AdditionalCostFrameNode) -> Vec<Vec<&'static str>> {
    frame
        .branches
        .iter()
        .map(|branch| {
            branch
                .actions
                .iter()
                .map(AdditionalCostActionNode::mode_name)
                .collect()
        })
        .collect()
}

/// An `or` inside an object query belongs to the query; the next `or`
/// separates alternatives (`sacrifice an artifact or creature or pay {4}`).
#[test]
fn query_disjunction_is_not_a_cost_alternative() {
    let parsed = frame(
        "As an additional cost to cast this spell, sacrifice an artifact or creature or pay {4}.",
    )
    .expect("two alternatives");
    assert_eq!(kinds(&parsed), vec![vec!["sacrifice"], vec!["payMana"]]);
    let parsed = frame(
        "As an additional cost to cast this spell, pay 5 life or sacrifice a creature or enchantment.",
    )
    .expect("two alternatives");
    assert_eq!(kinds(&parsed), vec![vec!["payLife"], vec!["sacrifice"]]);
}

/// `and` joins the actions of one alternative (CR 118.3: all are paid).
#[test]
fn conjunction_is_one_branch() {
    let parsed =
        frame("As an additional cost to cast this spell, discard a card and sacrifice a creature.")
            .expect("one branch");
    assert!(!parsed.optional);
    assert_eq!(kinds(&parsed), vec![vec!["discard", "sacrifice"]]);
}

/// A Blight reminder must restate the printed amount.
#[test]
fn blight_reminder_restates_its_amount() {
    let parsed = frame(
        "As an additional cost to cast this spell, blight 2 or pay {1}. (To blight 2, put two -1/-1 counters on a creature you control.)",
    )
    .expect("blight reminder");
    assert_eq!(kinds(&parsed), vec![vec!["blight"], vec!["payMana"]]);
    let failure = frame(
        "As an additional cost to cast this spell, blight 2 or pay {1}. (To blight 2, put a -1/-1 counter on a creature you control.)",
    )
    .expect_err("a mismatched reminder");
    assert_eq!(failure.state, "additional-cost.reminder");
}

/// Ninjutsu-like costs that act on the spell itself, and payment actions
/// with no casting payer, fail at their own states.
#[test]
fn unpayable_actions_fail_explicitly() {
    for (text, state) in [
        (
            "As an additional cost to cast this spell, mill two cards.",
            "engine.gap.additional-cost-action",
        ),
        (
            "As an additional cost to cast this spell, exile X cards from your graveyard.",
            "engine.gap.additional-cost-action",
        ),
        (
            "As an additional cost to cast this spell, discard this card.",
            "additional-cost.action.source",
        ),
    ] {
        let failure = frame(text).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
}

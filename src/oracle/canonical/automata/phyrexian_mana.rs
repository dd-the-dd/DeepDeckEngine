use super::super::{MachineResult, OracleTokenCursor};
use super::{ManaCostNode, NumericAtom, recognize_mana_cost, recognize_numeric_atom};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PhyrexianManaReminderNode {
    pub symbol: ManaCostNode,
    pub mana_alternative: ManaCostNode,
    pub life_alternative: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PhyrexianManaReminderState {
    Symbol,
    Modal,
    Payment,
    Choice,
    ManaAlternative,
    Connector,
    LifeAmount,
    LifeResource,
    Terminator,
    Complete,
}

fn phyrexian_component(cost: &ManaCostNode) -> Option<String> {
    let raw = cost.as_oracle_cost();
    let inner = raw.strip_prefix('{')?.strip_suffix('}')?;
    let (color, marker) = inner.split_once('/')?;
    (marker.eq_ignore_ascii_case("P") && matches!(color, "W" | "U" | "B" | "R" | "G"))
        .then(|| color.to_string())
}

fn fixed_color_component(cost: &ManaCostNode) -> Option<String> {
    let raw = cost.as_oracle_cost();
    let inner = raw.strip_prefix('{')?.strip_suffix('}')?;
    matches!(inner, "W" | "U" | "B" | "R" | "G").then(|| inner.to_string())
}

/// Recognizes the rules reminder attached to one Phyrexian mana symbol.
/// The child validates that the fixed-mana alternative has the same color as
/// the printed Phyrexian symbol instead of merely preserving reminder prose.
pub(in crate::oracle::canonical) fn recognize_phyrexian_mana_reminder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PhyrexianManaReminderNode> {
    let mut state = PhyrexianManaReminderState::Symbol;
    let mut symbol = None;
    let mut mana_alternative = None;
    let mut life_alternative = None;

    loop {
        state = match state {
            PhyrexianManaReminderState::Symbol => {
                cursor.enter("reminder.phyrexian-mana.symbol");
                let parsed = recognize_mana_cost(cursor)?;
                cursor.require(
                    phyrexian_component(&parsed).is_some(),
                    "one colored Phyrexian mana symbol",
                )?;
                symbol = Some(parsed);
                PhyrexianManaReminderState::Modal
            }
            PhyrexianManaReminderState::Modal => {
                cursor.enter("reminder.phyrexian-mana.modal");
                cursor.consume("can")?;
                PhyrexianManaReminderState::Payment
            }
            PhyrexianManaReminderState::Payment => {
                cursor.enter("reminder.phyrexian-mana.payment");
                cursor.consume_phrase(&["be", "paid", "with"])?;
                PhyrexianManaReminderState::Choice
            }
            PhyrexianManaReminderState::Choice => {
                cursor.enter("reminder.phyrexian-mana.choice");
                cursor.consume("either")?;
                PhyrexianManaReminderState::ManaAlternative
            }
            PhyrexianManaReminderState::ManaAlternative => {
                cursor.enter("reminder.phyrexian-mana.mana-alternative");
                let parsed = recognize_mana_cost(cursor)?;
                let printed_color = symbol.as_ref().and_then(phyrexian_component);
                let alternative_color = fixed_color_component(&parsed);
                cursor.enter("reminder.phyrexian-mana.mana-alternative");
                cursor.require(
                    printed_color.is_some() && printed_color == alternative_color,
                    "a fixed-mana alternative matching the Phyrexian symbol's color",
                )?;
                mana_alternative = Some(parsed);
                PhyrexianManaReminderState::Connector
            }
            PhyrexianManaReminderState::Connector => {
                cursor.enter("reminder.phyrexian-mana.connector");
                cursor.consume("or")?;
                PhyrexianManaReminderState::LifeAmount
            }
            PhyrexianManaReminderState::LifeAmount => {
                cursor.enter("reminder.phyrexian-mana.life-amount");
                let NumericAtom::Integer(amount) = recognize_numeric_atom(cursor)? else {
                    return Err(cursor.fail("a positive literal life payment"));
                };
                cursor.enter("reminder.phyrexian-mana.life-amount");
                cursor.require(amount > 0, "a positive literal life payment")?;
                life_alternative = Some(amount);
                PhyrexianManaReminderState::LifeResource
            }
            PhyrexianManaReminderState::LifeResource => {
                cursor.enter("reminder.phyrexian-mana.life-resource");
                cursor.consume("life")?;
                PhyrexianManaReminderState::Terminator
            }
            PhyrexianManaReminderState::Terminator => {
                cursor.enter("reminder.phyrexian-mana.terminator");
                cursor.consume(".")?;
                PhyrexianManaReminderState::Complete
            }
            PhyrexianManaReminderState::Complete => {
                cursor.enter("reminder.phyrexian-mana.complete");
                cursor.finish()?;
                return Ok(PhyrexianManaReminderNode {
                    symbol: cursor.require_value(symbol, "a Phyrexian mana symbol")?,
                    mana_alternative: cursor
                        .require_value(mana_alternative, "a matching mana alternative")?,
                    life_alternative: cursor
                        .require_value(life_alternative, "a life alternative")?,
                });
            }
        };
    }
}

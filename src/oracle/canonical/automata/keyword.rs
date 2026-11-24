use super::super::{MachineResult, OracleTokenCursor};
use super::{
    GrantedKeyword, ManaCostNode, ProtectionQuality, parameterless_keyword_from_oracle_word,
    recognize_mana_cost, recognize_parameterless_keyword, recognize_protection_quality,
};
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type};

/// A keyword line ending with `hexproof from QUALITIES` (CR 702.11d):
///
/// ```text
/// Line      := (Keyword ",")* "hexproof" "from" Quality (Sep ["from"] Quality)*
///              ["."] [Reminder]
/// Sep       := "and" | "," | "," "and"
/// Quality   := the protection quality reader (a color, `each color`, a
///              plural card type, `everything`)
/// Reminder  := "(" "This" PERMANENT "can't" "be" "the" "target" "of" COLOR
///              "spells" "or" "abilities" "your" "opponents" "control" "." ")"
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct HexproofFromLineNode {
    /// The parameterless keywords printed before it on the line.
    pub keywords: Vec<GrantedKeyword>,
    pub qualities: Vec<ProtectionQuality>,
}

fn word_at(tokens: &[crate::oracle::model::RecognizedEntity], index: usize, word: &str) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(word))
}

/// Structural claim: `hexproof from`, first on the line or after a list of
/// single-word parameterless keywords.
pub(in crate::oracle::canonical) fn hexproof_from_line_starts(
    tokens: &[crate::oracle::model::RecognizedEntity],
) -> bool {
    let Some(start) = (0..tokens.len())
        .find(|index| word_at(tokens, *index, "hexproof") && word_at(tokens, index + 1, "from"))
    else {
        return false;
    };
    tokens[..start].iter().enumerate().all(|(index, token)| {
        if index % 2 == 1 {
            token.raw == ","
        } else {
            parameterless_keyword_from_oracle_word(&token.raw).is_some()
        }
    }) && start % 2 == 0
}

pub(in crate::oracle::canonical) fn recognize_hexproof_from_line(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<HexproofFromLineNode> {
    let mut keywords = Vec::new();
    cursor.enter("keyword.hexproof-from.list");
    while !(cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("hexproof"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("from")))
    {
        keywords.push(recognize_parameterless_keyword(cursor)?);
        cursor.enter("keyword.hexproof-from.list-separator");
        cursor.consume(",")?;
    }
    cursor.enter("keyword.hexproof-from.keyword");
    cursor.consume_phrase(&["hexproof", "from"])?;
    let mut qualities = vec![recognize_protection_quality(cursor)?];
    loop {
        cursor.enter("keyword.hexproof-from.separator");
        let checkpoint = cursor.checkpoint();
        let mut separated = false;
        if cursor.peek() == Some(",") {
            cursor.consume(",")?;
            separated = true;
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        {
            cursor.consume("and")?;
            separated = true;
        }
        if !separated {
            break;
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
        {
            cursor.consume("from")?;
        }
        match recognize_protection_quality(cursor) {
            Ok(quality) => qualities.push(quality),
            Err(_) => {
                cursor.rollback(checkpoint);
                break;
            }
        }
    }
    cursor.enter("keyword.hexproof-from.distinct");
    cursor.require(
        qualities
            .iter()
            .enumerate()
            .all(|(index, quality)| !qualities[..index].contains(quality)),
        "distinct hexproof qualities",
    )?;
    cursor.enter("keyword.hexproof-from.reminder-or-complete");
    if cursor.peek() == Some(".") && cursor.peek_ahead(1).is_none_or(|raw| raw == "(") {
        cursor.consume(".")?;
    }
    if cursor.peek() == Some("(") {
        recognize_hexproof_from_reminder(cursor, &qualities)?;
    }
    cursor.enter("keyword.hexproof-from.complete");
    cursor.finish()?;
    Ok(HexproofFromLineNode {
        keywords,
        qualities,
    })
}

/// `(This PERMANENT can't be the target of COLOR spells or abilities your
/// opponents control.)` for one printed color quality.
fn recognize_hexproof_from_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    qualities: &[ProtectionQuality],
) -> MachineResult<()> {
    cursor.enter("keyword.hexproof-from.reminder.subject");
    cursor.consume_phrase(&["(", "this"])?;
    let subject = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a singular permanent type in the hexproof reminder"))?;
    cursor.require(
        lookup_permanent_card_type(subject, AcceptedForms::SingularOnly).is_some(),
        "a singular permanent type in the hexproof reminder",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("keyword.hexproof-from.reminder.restriction");
    cursor.consume_phrase(&["can't", "be", "the", "target", "of"])?;
    cursor.enter("keyword.hexproof-from.reminder.quality");
    let [ProtectionQuality::Color(color)] = qualities else {
        return Err(cursor.fail("a reminder for one color hexproof quality"));
    };
    cursor.consume(color.oracle_name())?;
    cursor.enter("keyword.hexproof-from.reminder.objects");
    cursor.consume_phrase(&[
        "spells",
        "or",
        "abilities",
        "your",
        "opponents",
        "control",
        ".",
        ")",
    ])
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct KickerKeywordNode {
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KickerState {
    Keyword,
    Cost,
    ReminderOrComplete,
    ReminderOpen,
    ReminderPermission,
    ReminderCost,
    ReminderCostMatch,
    ReminderTiming,
    ReminderTerminator,
    ReminderClose,
    Complete,
}

#[derive(Default)]
struct KickerMemory {
    printed_cost: Option<ManaCostNode>,
    reminder_cost: Option<ManaCostNode>,
}

/// Recognizes a mana Kicker keyword and structurally validates its optional
/// canonical reminder sentence.
pub(in crate::oracle::canonical) fn recognize_mana_kicker(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<KickerKeywordNode> {
    let mut state = KickerState::Keyword;
    let mut memory = KickerMemory::default();
    loop {
        state = match state {
            KickerState::Keyword => {
                cursor.enter("keyword.kicker.keyword");
                cursor.consume("kicker")?;
                KickerState::Cost
            }
            KickerState::Cost => {
                cursor.enter("keyword.kicker.cost");
                memory.printed_cost = Some(recognize_mana_cost(cursor)?);
                KickerState::ReminderOrComplete
            }
            KickerState::ReminderOrComplete => {
                cursor.enter("keyword.kicker.reminder-or-complete");
                match cursor.peek() {
                    None => KickerState::Complete,
                    Some(token) if token == "(" => KickerState::ReminderOpen,
                    _ => return Err(cursor.fail("a canonical Kicker reminder or end of input")),
                }
            }
            KickerState::ReminderOpen => {
                cursor.enter("keyword.kicker.reminder.open");
                cursor.consume("(")?;
                KickerState::ReminderPermission
            }
            KickerState::ReminderPermission => {
                cursor.enter("keyword.kicker.reminder.permission");
                cursor.consume_phrase(&["you", "may", "pay", "an", "additional"])?;
                KickerState::ReminderCost
            }
            KickerState::ReminderCost => {
                cursor.enter("keyword.kicker.reminder.cost");
                memory.reminder_cost = Some(recognize_mana_cost(cursor)?);
                KickerState::ReminderCostMatch
            }
            KickerState::ReminderCostMatch => {
                cursor.enter("keyword.kicker.reminder.cost-match");
                let printed = memory
                    .printed_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the printed Kicker cost"))?;
                let reminder = memory
                    .reminder_cost
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the reminder Kicker cost"))?;
                cursor.require(
                    printed.same_symbols(reminder),
                    "a reminder cost equal to the printed Kicker cost",
                )?;
                KickerState::ReminderTiming
            }
            KickerState::ReminderTiming => {
                cursor.enter("keyword.kicker.reminder.timing");
                cursor.consume_phrase(&["as", "you", "cast", "this", "spell"])?;
                KickerState::ReminderTerminator
            }
            KickerState::ReminderTerminator => {
                cursor.enter("keyword.kicker.reminder.terminator");
                cursor.consume(".")?;
                KickerState::ReminderClose
            }
            KickerState::ReminderClose => {
                cursor.enter("keyword.kicker.reminder.close");
                cursor.consume(")")?;
                KickerState::Complete
            }
            KickerState::Complete => {
                cursor.enter("keyword.kicker.complete");
                cursor.finish()?;
                return Ok(KickerKeywordNode {
                    cost: cursor.require_value(memory.printed_cost, "a populated Kicker cost")?,
                });
            }
        };
    }
}

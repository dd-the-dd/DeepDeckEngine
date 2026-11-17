use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor};
use super::{GrantedKeyword, parameterless_keyword_from_oracle_word, recognize_counter_name};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RiotReminderNode {
    pub counter: CounterName,
    pub alternative_keyword: GrantedKeyword,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RiotKeywordNode {
    pub reminder: Option<RiotReminderNode>,
}

fn recognize_riot_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<RiotReminderNode> {
    cursor.enter("keyword.riot.reminder.open");
    cursor.consume("(")?;
    cursor.enter("keyword.riot.reminder.entry");
    cursor.consume_phrase(&["they", "enter", "with", "your", "choice", "of", "a"])?;
    cursor.enter("keyword.riot.reminder.counter");
    let counter = recognize_counter_name(cursor)?;
    cursor.require(
        counter == "+1/+1",
        "the +1/+1 counter defined by the riot keyword",
    )?;
    cursor.consume("counter")?;
    cursor.enter("keyword.riot.reminder.alternative");
    cursor.consume("or")?;
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("the haste keyword"))?
        .to_string();
    let alternative_keyword = parameterless_keyword_from_oracle_word(&raw)
        .filter(|keyword| *keyword == GrantedKeyword::Haste)
        .ok_or_else(|| cursor.fail("the haste keyword"))?;
    cursor.consume(&raw)?;
    cursor.enter("keyword.riot.reminder.close");
    cursor.consume_phrase(&[".", ")"])?;
    Ok(RiotReminderNode {
        counter,
        alternative_keyword,
    })
}

/// Recognizes the Riot keyword and its standardized optional reminder. The
/// reminder is retained as typed counter/keyword choices and is never copied
/// into IR as raw Oracle text.
pub(in crate::oracle::canonical) fn recognize_riot_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RiotKeywordNode> {
    cursor.enter("keyword.riot.name");
    cursor.consume("riot")?;
    cursor.enter("keyword.riot.terminator");
    cursor.consume(".")?;
    let reminder = if cursor.peek().is_some_and(|raw| raw == "(") {
        Some(recognize_riot_reminder(cursor)?)
    } else {
        None
    };
    cursor.enter("keyword.riot.complete");
    Ok(RiotKeywordNode { reminder })
}

#[cfg(test)]
#[path = "riot_tests.rs"]
mod tests;

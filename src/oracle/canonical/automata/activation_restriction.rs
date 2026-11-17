//! Activation restrictions (CR 602.5b, 602.5d): `Activate only CLAUSE
//! (and only CLAUSE)*.` restricts the enclosing activated ability. It is
//! ability metadata, never an effect, and the same reader serves every
//! activated parent instead of each family re-reading the sentence.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationRestrictionsNode {
    pub sorcery_timing: bool,
    pub controller_turn: bool,
    pub once_each_turn: bool,
}

impl ActivationRestrictionsNode {
    pub(in crate::oracle::canonical) fn is_empty(self) -> bool {
        self == Self::default()
    }
}

pub(in crate::oracle::canonical) fn activation_restriction_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("activate"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("only"))
}

/// `This ability triggers only once each turn.` (CR 603.2h-style trigger
/// limit) restricts the enclosing triggered ability: ability metadata owned by
/// the trigger parent, never an effect.
pub(in crate::oracle::canonical) fn trigger_limit_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    ["this", "ability", "triggers", "only"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads `This ability triggers only once each turn.`; returns whether it was
/// present. Any other `triggers only ...` clause is an explicit failure.
pub(in crate::oracle::canonical) fn recognize_optional_trigger_limit(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    if !trigger_limit_starts(cursor) {
        return Ok(false);
    }
    cursor.enter("trigger.limit.head");
    cursor.consume_phrase(&["this", "ability", "triggers", "only"])?;
    cursor.enter("trigger.limit.clause");
    cursor.consume_phrase(&["once", "each", "turn"])?;
    cursor.enter("trigger.limit.terminator");
    cursor.consume(".")?;
    Ok(true)
}

pub(in crate::oracle::canonical) fn recognize_activation_restrictions(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationRestrictionsNode> {
    let mut node = ActivationRestrictionsNode::default();
    cursor.enter("activation.restriction.head");
    cursor.consume("activate")?;
    loop {
        cursor.enter("activation.restriction.only");
        cursor.consume("only")?;
        cursor.enter("activation.restriction.clause");
        let head = cursor.peek().map(str::to_ascii_lowercase);
        let slot = match head.as_deref() {
            Some("as") => {
                cursor.consume_phrase(&["as", "a", "sorcery"])?;
                &mut node.sorcery_timing
            }
            Some("during") => {
                cursor.consume_phrase(&["during", "your", "turn"])?;
                &mut node.controller_turn
            }
            Some("once") => {
                cursor.consume_phrase(&["once", "each", "turn"])?;
                &mut node.once_each_turn
            }
            _ => {
                return Err(cursor.fail("`as a sorcery`, `during your turn` or `once each turn`"));
            }
        };
        cursor.require(!*slot, "each activation restriction stated once")?;
        *slot = true;
        cursor.enter("activation.restriction.connector");
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
        {
            cursor.consume("and")?;
            continue;
        }
        cursor.consume(".")?;
        return Ok(node);
    }
}

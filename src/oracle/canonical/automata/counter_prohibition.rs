use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalObjectReference, ObjectQueryNode, SpellCastQueryNode, recognize_object_query,
    recognize_spell_cast_collection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterProhibitionSubjectNode {
    SourceSelf(LocalObjectReference),
    ObjectSet(ObjectQueryNode),
    /// `SPELLS [PLAYER cast] can't be countered`: the shared spell
    /// collection with its optional caster relation (CR 601.2, 701.6).
    CastSpells(SpellCastQueryNode),
}

/// `can't` / `cannot` at the cursor.
fn negation_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("cannot"))
}

/// The spell subject: an object set (`spells you control`) or, when that
/// reading does not reach the negation, the spell collection with a cast
/// relation (`creature spells you cast`, `spells`).
fn recognize_spell_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterProhibitionSubjectNode> {
    let mut trial = cursor.clone();
    if let Ok(query) = recognize_object_query(&mut trial)
        && negation_follows(&trial)
    {
        *cursor = trial;
        return Ok(CounterProhibitionSubjectNode::ObjectSet(query));
    }
    recognize_spell_cast_collection(cursor).map(CounterProhibitionSubjectNode::CastSpells)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterProhibitionNode {
    pub subject: CounterProhibitionSubjectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterProhibitionState {
    Subject,
    Negation,
    Passive,
    Action,
    Terminator,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_counter_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterProhibitionNode> {
    let mut state = CounterProhibitionState::Subject;
    let mut subject = None;
    loop {
        state = match state {
            CounterProhibitionState::Subject => {
                cursor.enter("counter-prohibition.subject");
                subject = Some(
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                    {
                        cursor.enter("counter-prohibition.reference.demonstrative");
                        cursor.consume_phrase(&["this", "spell"])?;
                        CounterProhibitionSubjectNode::SourceSelf(LocalObjectReference::SourceSelf)
                    } else {
                        recognize_spell_subject(cursor)?
                    },
                );
                CounterProhibitionState::Negation
            }
            CounterProhibitionState::Negation => {
                cursor.enter("counter-prohibition.negation");
                let negation = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("cannot")
                    })
                    .ok_or_else(|| cursor.fail("`can't` or `cannot`"))?
                    .to_string();
                cursor.consume(&negation)?;
                CounterProhibitionState::Passive
            }
            CounterProhibitionState::Passive => {
                cursor.enter("counter-prohibition.passive");
                cursor.consume("be")?;
                CounterProhibitionState::Action
            }
            CounterProhibitionState::Action => {
                cursor.enter("counter-prohibition.action");
                cursor.consume("countered")?;
                CounterProhibitionState::Terminator
            }
            CounterProhibitionState::Terminator => {
                cursor.enter("counter-prohibition.terminator");
                cursor.consume(".")?;
                // `(This includes by the ward ability.)` restates that a
                // ward trigger's counter is a counter too (CR 702.21a); it
                // adds no rule and only follows the spell's own prohibition.
                if cursor.peek() == Some("(")
                    && matches!(subject, Some(CounterProhibitionSubjectNode::SourceSelf(_)))
                {
                    cursor.enter("counter-prohibition.ward-reminder");
                    cursor.consume_phrase(&[
                        "(", "this", "includes", "by", "the", "ward", "ability", ".", ")",
                    ])?;
                }
                CounterProhibitionState::Complete
            }
            CounterProhibitionState::Complete => {
                cursor.enter("counter-prohibition.complete");
                cursor.finish()?;
                return Ok(CounterProhibitionNode {
                    subject: cursor
                        .require_value(subject, "a populated counter-prohibition subject")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "counter_prohibition_tests.rs"]
mod tests;

use super::super::{MachineResult, OracleTokenCursor};
use super::EffectDurationNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffectDurationState {
    Boundary,
    PointOrPossessor,
    Relation,
    Next,
    Unit,
    Complete,
}

/// Recognizes the canonical duration that expires during the cleanup step of
/// the current turn. Effect-specific parents retain ownership of the modifier
/// or continuous effect to which this duration applies.
pub(in crate::oracle::canonical) fn recognize_effect_duration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EffectDurationNode> {
    let mut state = EffectDurationState::Boundary;
    let mut duration = None;
    loop {
        state = match state {
            EffectDurationState::Boundary => {
                cursor.enter("duration.until-end-of-turn.boundary");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("until") => {
                        cursor.consume("until")?;
                        EffectDurationState::PointOrPossessor
                    }
                    Some("this") => {
                        cursor.consume("this")?;
                        duration = Some(EffectDurationNode::UntilEndOfCurrentTurn);
                        EffectDurationState::Unit
                    }
                    _ => {
                        return Err(cursor.fail("`this turn` or a duration beginning with `until`"));
                    }
                }
            }
            EffectDurationState::PointOrPossessor => {
                cursor.enter("duration.point-or-possessor");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("end") => {
                        cursor.consume("end")?;
                        duration = Some(EffectDurationNode::UntilEndOfCurrentTurn);
                        EffectDurationState::Relation
                    }
                    Some("your") => {
                        cursor.consume("your")?;
                        duration = Some(EffectDurationNode::UntilNextTurn {
                            player: super::LocalPlayerReference::Controller,
                        });
                        EffectDurationState::Next
                    }
                    _ => return Err(cursor.fail("`end` or the controller possessive `your`")),
                }
            }
            EffectDurationState::Relation => {
                cursor.enter("duration.until-end-of-turn.relation");
                cursor.consume("of")?;
                EffectDurationState::Unit
            }
            EffectDurationState::Next => {
                cursor.enter("duration.until-next-turn.next");
                cursor.consume("next")?;
                EffectDurationState::Unit
            }
            EffectDurationState::Unit => {
                cursor.enter("duration.until-end-of-turn.unit");
                cursor.consume("turn")?;
                EffectDurationState::Complete
            }
            EffectDurationState::Complete => {
                cursor.enter("duration.complete");
                return cursor.require_value(duration, "a populated effect duration");
            }
        };
    }
}

/// The permission-window superset of `recognize_effect_duration`: it also reads
/// `until [the] end of your next turn`. A parent that needs the wider window asks
/// for it explicitly; no existing reader changes its accepted language.
pub(in crate::oracle::canonical) fn recognize_play_window_duration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<EffectDurationNode> {
    cursor.enter("duration.play-window.boundary");
    let checkpoint = cursor.checkpoint();
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
    {
        cursor.consume("until")?;
        let article = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"));
        if article {
            cursor.consume("the")?;
        }
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("end"))
        {
            cursor.consume_phrase(&["end", "of"])?;
            cursor.enter("duration.play-window.possessor");
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
            {
                cursor.consume_phrase(&["your", "next", "turn"])?;
                return Ok(EffectDurationNode::UntilEndOfNextTurn {
                    player: super::LocalPlayerReference::Controller,
                });
            }
            cursor.consume("turn")?;
            return Ok(EffectDurationNode::UntilEndOfCurrentTurn);
        }
        cursor.rollback(checkpoint);
    }
    recognize_effect_duration(cursor)
}

/// A continuous-effect action owns exactly one duration. A sentence-level
/// preposed duration (`Until end of turn, X and Y.`, CR 611.2a) is supplied by
/// the enclosing sequence and replaces the trailing reader; a second trailing
/// duration on the same action is a conflict and is rejected.
pub(in crate::oracle::canonical) fn recognize_trailing_or_supplied_duration(
    cursor: &mut OracleTokenCursor<'_>,
    supplied: Option<&EffectDurationNode>,
) -> MachineResult<EffectDurationNode> {
    match supplied {
        Some(duration) => {
            cursor.require(
                !cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("until")),
                "no second duration after a sentence-level preposed duration",
            )?;
            Ok(duration.clone())
        }
        // A clause that ends without a duration states none: the effect
        // lasts indefinitely (CR 611.2a). Each lowering decides whether its
        // primitive has an indefinite form.
        None if cursor
            .peek()
            .is_none_or(|raw| matches!(raw, "." | ";" | "(")) =>
        {
            cursor.enter("duration.indefinite");
            Ok(EffectDurationNode::Indefinite)
        }
        None => recognize_effect_duration(cursor),
    }
}

/// Whether the cursor stands on `for as long as`.
pub(in crate::oracle::canonical) fn source_control_duration_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ["for", "as", "long", "as"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads `for as long as you control SOURCE` (CR 611.2b). The source is
/// the shared current-object reference; other `for as long as` conditions
/// remain unsupported with a structured failure.
pub(in crate::oracle::canonical) fn recognize_source_control_duration(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<EffectDurationNode> {
    cursor.enter("duration.while-source-controlled.introducer");
    cursor.consume_phrase(&["for", "as", "long", "as"])?;
    cursor.enter("duration.while-source-controlled.condition");
    cursor.consume_phrase(&["you", "control"])?;
    cursor.enter("duration.while-source-controlled.source");
    let source = super::recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.require(
        source == super::LocalObjectReference::SourceSelf,
        "the current source object",
    )?;
    Ok(EffectDurationNode::WhileSourceControlled)
}

/// What a `for as long as` duration watches about the ability's own source
/// (CR 611.2b). The effect ends the first time the state stops holding and
/// never starts if it does not hold when the effect would begin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceStateDuration {
    /// `for as long as you control SOURCE`.
    Controlled,
    /// `for as long as SOURCE remains tapped`.
    RemainsTapped,
    /// `for as long as SOURCE remains on the battlefield`.
    RemainsOnBattlefield,
}

/// `for as long as (you control SOURCE | SOURCE remains (tapped | on the
/// battlefield))`. The source is the shared current-object reference; the
/// state is one of the source's own observable states. Any other condition
/// (`that creature has a counter on it`, `it remains exiled`) fails here with
/// a structured state instead of being read as a different duration.
pub(in crate::oracle::canonical) fn recognize_source_state_duration(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<SourceStateDuration> {
    cursor.enter("duration.while-source.introducer");
    cursor.consume_phrase(&["for", "as", "long", "as"])?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        cursor.enter("duration.while-source-controlled.condition");
        cursor.consume_phrase(&["you", "control"])?;
        read_duration_source(cursor, name_links, name_failure)?;
        return Ok(SourceStateDuration::Controlled);
    }
    cursor.enter("duration.while-source.source");
    read_duration_source(cursor, name_links, name_failure)?;
    cursor.enter("duration.while-source.state");
    cursor.consume("remains")?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
    {
        cursor.consume("tapped")?;
        return Ok(SourceStateDuration::RemainsTapped);
    }
    cursor.consume_phrase(&["on", "the", "battlefield"])?;
    Ok(SourceStateDuration::RemainsOnBattlefield)
}

impl SourceStateDuration {
    /// The continuous-effect duration of this window.
    pub(in crate::oracle::canonical) fn effect_duration(self) -> EffectDurationNode {
        match self {
            Self::Controlled => EffectDurationNode::WhileSourceControlled,
            Self::RemainsTapped => EffectDurationNode::WhileSourceTapped,
            Self::RemainsOnBattlefield => EffectDurationNode::WhileSourceOnBattlefield,
        }
    }
}

fn read_duration_source(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[crate::oracle::syntax::ResolvedNameLink],
    name_failure: Option<&crate::oracle::syntax::NameResolutionFailure>,
) -> MachineResult<()> {
    cursor.enter("duration.while-source.source");
    let source = super::recognize_source_reference(cursor, name_links, name_failure)?;
    cursor.require(
        source == super::LocalObjectReference::SourceSelf,
        "the current source object",
    )
}

#[cfg(test)]
#[path = "duration_tests.rs"]
mod tests;

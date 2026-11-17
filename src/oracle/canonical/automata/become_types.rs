//! `OBJECT becomes a [SUBTYPES] [CARD TYPES] [in addition to its other types]`
//! (CR 205.1a/b, layer 4 per CR 613.1d). The subject, the type list, the
//! retention clause, and the duration are independent children; the parent
//! owns only their Oracle order. A bare type-change without `in addition to`
//! replaces the subtypes of its class (CR 205.1b); that meaning is carried as
//! data, never inferred again by a lowering parent.
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_creature_subtype_tokens, lookup_permanent_card_type,
};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CreatureSubtype, EffectDurationNode, ObjectCardType, RecognizedEffectFragment, TargetListNode,
    recognize_trailing_or_supplied_duration,
};

/// Whether the new types supplement or replace the object's existing ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TypeChangeRetention {
    /// No retention clause: new subtypes replace those of their class.
    ReplacesSameClass,
    /// `in addition to its other types`.
    InAdditionToOtherTypes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecomeTypesNode<Subject = TargetListNode> {
    pub objects: Subject,
    pub subtypes: Vec<CreatureSubtype>,
    pub card_types: Vec<ObjectCardType>,
    pub retention: TypeChangeRetention,
    /// `None` is a continuous effect without a stated duration (CR 611.2a).
    pub duration: Option<EffectDurationNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BecomeTypesState {
    Subject,
    Verb,
    Article,
    Types,
    Retention,
    Duration,
    Complete,
}

pub(in crate::oracle::canonical) fn recognize_become_types_subject<'tokens, Subject>(
    cursor: &mut OracleTokenCursor<'tokens>,
    mut subject: impl FnMut(&mut OracleTokenCursor<'tokens>) -> MachineResult<Subject>,
    preposed_duration: Option<&EffectDurationNode>,
) -> MachineResult<RecognizedEffectFragment<BecomeTypesNode<Subject>>> {
    let mut state = BecomeTypesState::Subject;
    let mut objects = None;
    let mut subtypes = Vec::new();
    let mut card_types = Vec::new();
    let mut retention = TypeChangeRetention::ReplacesSameClass;
    let mut duration = None;
    loop {
        state = match state {
            BecomeTypesState::Subject => {
                cursor.enter("effect.become-types.subject");
                objects = Some(subject(cursor)?);
                BecomeTypesState::Verb
            }
            BecomeTypesState::Verb => {
                cursor.enter("effect.become-types.verb");
                cursor.consume("becomes")?;
                BecomeTypesState::Article
            }
            BecomeTypesState::Article => {
                cursor.enter("effect.become-types.article");
                let article = cursor
                    .peek()
                    .map(str::to_ascii_lowercase)
                    .ok_or_else(|| cursor.fail("`a` or `an` before the new types"))?;
                cursor.require(matches!(article.as_str(), "a" | "an"), "`a` or `an`")?;
                cursor.consume(&article)?;
                BecomeTypesState::Types
            }
            BecomeTypesState::Types => {
                cursor.enter("effect.become-types.types");
                loop {
                    let Some(first) = cursor.peek() else { break };
                    if card_types.is_empty()
                        && let Some((subtype, width)) = lookup_creature_subtype_tokens(
                            first,
                            cursor.peek_ahead(1),
                            AcceptedForms::SingularOnly,
                        )
                    {
                        cursor.require(
                            cursor
                                .token_slice(cursor.position(), cursor.position() + width)
                                .iter()
                                .all(|token| token.token_type == "subtypeWord"),
                            "lexical subtypeWord evidence from CR 205.3m",
                        )?;
                        cursor
                            .require(!subtypes.contains(&subtype), "distinct creature subtypes")?;
                        subtypes.push(subtype);
                        cursor.advance_to(cursor.position() + width)?;
                    } else if let Some(card_type) =
                        lookup_permanent_card_type(first, AcceptedForms::SingularOnly)
                    {
                        cursor.require(!card_types.contains(&card_type), "distinct card types")?;
                        card_types.push(card_type);
                        cursor.advance_to(cursor.position() + 1)?;
                    } else {
                        break;
                    }
                }
                cursor.require(
                    !subtypes.is_empty() || !card_types.is_empty(),
                    "a registered creature subtype or permanent card type",
                )?;
                BecomeTypesState::Retention
            }
            BecomeTypesState::Retention => {
                cursor.enter("effect.become-types.retention");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
                {
                    cursor.consume_phrase(&["in", "addition", "to", "its", "other", "types"])?;
                    retention = TypeChangeRetention::InAdditionToOtherTypes;
                }
                BecomeTypesState::Duration
            }
            BecomeTypesState::Duration => {
                cursor.enter("effect.become-types.duration");
                duration = if preposed_duration.is_some()
                    || cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("until"))
                {
                    Some(recognize_trailing_or_supplied_duration(
                        cursor,
                        preposed_duration,
                    )?)
                } else {
                    // A trailing duration after a coordinated verb scopes the
                    // whole chain; it is not propagated backwards here, so an
                    // open-ended type change followed by one is refused.
                    cursor.require(
                        !(cursor
                            .peek()
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                            && cursor.peek_ahead(1).is_some_and(|raw| {
                                ["gain", "gains", "become", "becomes", "get", "gets"]
                                    .iter()
                                    .any(|verb| raw.eq_ignore_ascii_case(verb))
                            })),
                        "a duration for the type change before a coordinated verb",
                    )?;
                    None
                };
                BecomeTypesState::Complete
            }
            BecomeTypesState::Complete => {
                cursor.enter("effect.become-types.complete");
                return Ok(RecognizedEffectFragment {
                    node: BecomeTypesNode {
                        objects: cursor.require_value(objects, "a type-change subject")?,
                        subtypes,
                        card_types,
                        retention,
                        duration,
                    },
                    end_position: cursor.position(),
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "become_types_tests.rs"]
mod tests;

//! Result reuse of an earlier action: `OBJECTS VERBed this way` (CR 608.2c).
//!
//! The child owns only the restated noun phrase and the past-participle verb.
//! It never chooses the producing action: the program parent that owns the
//! earlier actions resolves the antecedent, and lowering binds that action's
//! affected objects under a compiler-assigned identifier. A verb without a
//! producing action of the same kind, or a noun the antecedent does not
//! guarantee, fails explicitly instead of counting zero.
//!
//! ```text
//! ActionResult := ObjectCollection Verb "this" "way"
//! Verb         := "destroyed" | "milled"
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalEffectNode, ObjectKind, ObjectPredicate, PlayerActionNode, SingularPlayerAction,
    ZoneChangeAction, recognize_object_collection,
};

/// The action whose affected objects a `VERBed this way` phrase restates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultVerb {
    /// Permanents put into a graveyard by a destroy action (CR 701.8).
    Destroyed,
    /// Cards put into a graveyard from a library by mill (CR 701.17).
    Milled,
}

impl ActionResultVerb {
    fn from_word(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "destroyed" => Some(Self::Destroyed),
            "milled" => Some(Self::Milled),
            _ => None,
        }
    }

    /// Runtime action kinds whose `bind` field records exactly the objects
    /// this verb restates.
    pub(in crate::oracle::canonical) fn runtime_kinds(self) -> &'static [&'static str] {
        match self {
            Self::Destroyed => &["destroyPermanent"],
            Self::Milled => &["mill"],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActionResultReferenceNode {
    pub predicate: ObjectPredicate,
    pub verb: ActionResultVerb,
}

/// Position of the verb when the cursor is at `OBJECTS VERBed this way`
/// before the end of the clause.
fn verb_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (1..12)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            cursor
                .peek_ahead(*offset)
                .and_then(ActionResultVerb::from_word)
                .is_some()
                && cursor
                    .peek_ahead(offset + 1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                && cursor
                    .peek_ahead(offset + 2)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("way"))
        })
}

pub(in crate::oracle::canonical) fn action_result_reference_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    verb_offset(cursor).is_some()
}

/// Reads `OBJECTS VERBed this way`. The noun phrase is the shared object
/// collection child bounded before the verb; it must name no controller.
pub(in crate::oracle::canonical) fn recognize_action_result_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActionResultReferenceNode> {
    cursor.enter("action-result.objects");
    let offset = verb_offset(cursor)
        .ok_or_else(|| cursor.fail("an object phrase followed by `VERBed this way`"))?;
    let end = cursor.position() + offset;
    let query = cursor.bounded(end, recognize_object_collection)?;
    cursor.require(
        !query.exclude_source && query.controller.is_none() && query.predicate.has_nominal_head(),
        "a nominal object phrase without a controller relation",
    )?;
    cursor.enter("action-result.verb");
    let verb = cursor
        .peek()
        .and_then(ActionResultVerb::from_word)
        .ok_or_else(|| cursor.fail("`destroyed` or `milled`"))?;
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    cursor.consume_phrase(&["this", "way"])?;
    Ok(ActionResultReferenceNode {
        predicate: query.predicate,
        verb,
    })
}

/// A quantity an earlier action of the same program produced
/// (`the total life lost this way`, CR 608.2c, 702.101a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultQuantity {
    /// The life the players actually lost (prevented loss is not lost).
    LifeLost,
}

impl ActionResultQuantity {
    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::LifeLost => "lifeLost",
        }
    }

    /// The runtime action whose `bind` records this quantity.
    pub(in crate::oracle::canonical) const fn runtime_kind(self) -> &'static str {
        match self {
            Self::LifeLost => "loseLife",
        }
    }
}

/// `the [total] [amount of] life lost this way`.
pub(in crate::oracle::canonical) fn action_result_quantity_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let mut offset = 1;
    if word(offset, "total") {
        offset += 1;
    }
    if word(offset, "amount") && word(offset + 1, "of") {
        offset += 2;
    }
    word(0, "the") && word(offset, "life") && word(offset + 1, "lost")
}

/// Reads `the [total] [amount of] life lost this way`. The action being read
/// is the binding environment's; the program parent resolves the producing
/// action among the earlier ones when it lowers the program, and fails
/// explicitly when none produces the quantity.
pub(in crate::oracle::canonical) fn recognize_action_result_quantity(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::NumericExpression> {
    cursor.enter("action-result.quantity");
    cursor.consume("the")?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("total"))
    {
        cursor.consume("total")?;
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("amount"))
    {
        cursor.consume_phrase(&["amount", "of"])?;
    }
    cursor.consume_phrase(&["life", "lost", "this", "way"])?;
    cursor.enter("action-result.quantity.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index);
    let reader = cursor.require_value(
        reader,
        "an action of a program whose earlier actions it restates",
    )?;
    Ok(super::NumericExpression::ActionResultQuantity {
        reader_effect: reader,
        quantity: ActionResultQuantity::LifeLost,
    })
}

/// How a restated noun relates to the producing action's own object set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultFilter {
    /// Every affected object satisfies the noun; count the whole result.
    All,
    /// A card filter applied to the affected cards in their new zone. Only
    /// used where the action moves cards whose characteristics are unchanged.
    Cards(ObjectPredicate),
}

/// The restated noun is guaranteed by the antecedent's own predicate when it
/// is the generic domain noun, the same predicate, or one operand of its
/// intersection (`creature` after `creatures with flying`).
fn guaranteed_by(noun: &ObjectPredicate, antecedent: &ObjectPredicate) -> bool {
    let generic = matches!(
        noun,
        ObjectPredicate::ObjectKind(ObjectKind::Permanent | ObjectKind::Card)
    );
    generic
        || noun == antecedent
        || matches!(antecedent, ObjectPredicate::Intersection(operands) if operands.contains(noun))
}

/// True when `effect` is an action whose affected objects `verb` restates.
pub(in crate::oracle::canonical) fn action_result_verb_produced_by(
    verb: ActionResultVerb,
    effect: &LocalEffectNode,
) -> bool {
    match verb {
        ActionResultVerb::Destroyed => match effect {
            LocalEffectNode::DestroyCollection(_) | LocalEffectNode::DestroyReferencedSet(_) => {
                true
            }
            LocalEffectNode::ZoneChange(node) => node.action == ZoneChangeAction::Destroy,
            _ => false,
        },
        ActionResultVerb::Milled => matches!(
            effect,
            LocalEffectNode::MillCards(_)
                | LocalEffectNode::PlayerAction(PlayerActionNode {
                    action: SingularPlayerAction::Mill(_),
                    ..
                })
        ),
    }
}

/// Resolves the latest earlier action (the last action is the one being
/// scaled) of the restated verb, and the filter its noun needs. A nearer
/// producer of an unsupported form is still the antecedent: it is never
/// skipped in favour of an older action.
pub(in crate::oracle::canonical) fn resolve_action_result_antecedent(
    effects: &[LocalEffectNode],
    reference: &ActionResultReferenceNode,
) -> Option<(usize, ActionResultFilter)> {
    let (_, earlier) = effects.split_last()?;
    let (index, effect) = earlier
        .iter()
        .enumerate()
        .rev()
        .find(|(_, effect)| action_result_verb_produced_by(reference.verb, effect))?;
    let filter = match effect {
        // Destroyed permanents leave the battlefield (tokens cease to exist),
        // so their noun is never re-read in the new zone.
        LocalEffectNode::DestroyCollection(node) => {
            guaranteed_by(&reference.predicate, &node.objects.predicate)
                .then_some(ActionResultFilter::All)?
        }
        // Milled cards keep their characteristics in the graveyard.
        LocalEffectNode::PlayerAction(_) => {
            if reference.predicate == ObjectPredicate::ObjectKind(ObjectKind::Card) {
                ActionResultFilter::All
            } else if reference.predicate.has_domain(ObjectKind::Card)
                && reference.predicate.supports_domain(ObjectKind::Card)
            {
                ActionResultFilter::Cards(reference.predicate.clone())
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some((index, filter))
}

#[cfg(test)]
#[path = "action_result_tests.rs"]
mod tests;

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
//! Verb         := "destroyed" | "milled" | "discarded" | "revealed"
//!               | "exiled" | "sacrificed"
//! Active       := "you" ActiveVerb Threshold ObjectCollection "this" "way"
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CardZoneChangeAction, ConditionAtom, ConditionComparison, ConditionNumber,
    ConditionPlayerSubject, LocalEffectNode, ObjectKind, ObjectPredicate, PlayerActionNode,
    SacrificeEffectNode, SingularPlayerAction, ZoneChangeAction, recognize_count_threshold,
    recognize_object_collection, threshold_comparison,
};

/// The action whose affected objects a `VERBed this way` phrase restates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultVerb {
    /// Permanents put into a graveyard by a destroy action (CR 701.8).
    Destroyed,
    /// Cards put into a graveyard from a library by mill (CR 701.17).
    Milled,
    /// Cards put into a graveyard from a hand by discard (CR 701.9).
    Discarded,
    /// Cards revealed (CR 701.16): they stay where they are.
    Revealed,
    /// Target cards moved to exile from a card zone (CR 701.13a). They keep
    /// their characteristics there; a permanent exiled from the battlefield
    /// (a token ceases to exist) has no producer of this verb yet.
    Exiled,
    /// Permanents put into a graveyard by sacrifice (CR 701.21a).
    Sacrificed,
}

impl ActionResultVerb {
    pub(in crate::oracle::canonical) const ALL: [Self; 6] = [
        Self::Destroyed,
        Self::Milled,
        Self::Discarded,
        Self::Revealed,
        Self::Exiled,
        Self::Sacrificed,
    ];

    pub(in crate::oracle::canonical) fn from_word(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "destroyed" => Some(Self::Destroyed),
            "milled" => Some(Self::Milled),
            "discarded" => Some(Self::Discarded),
            "revealed" => Some(Self::Revealed),
            "exiled" => Some(Self::Exiled),
            "sacrificed" => Some(Self::Sacrificed),
            _ => None,
        }
    }

    /// The active forms a performer restates (`you discard`, `you exiled`):
    /// present, third person and past tense of the same verb.
    pub(in crate::oracle::canonical) fn from_active_word(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "destroy" | "destroys" => Some(Self::Destroyed),
            "mill" | "mills" => Some(Self::Milled),
            "discard" | "discards" => Some(Self::Discarded),
            "reveal" | "reveals" => Some(Self::Revealed),
            "exile" | "exiles" => Some(Self::Exiled),
            "sacrifice" | "sacrifices" => Some(Self::Sacrificed),
            past => Self::from_word(past),
        }
    }

    pub(in crate::oracle::canonical) const fn ir_name(self) -> &'static str {
        match self {
            Self::Destroyed => "destroyed",
            Self::Milled => "milled",
            Self::Discarded => "discarded",
            Self::Revealed => "revealed",
            Self::Exiled => "exiled",
            Self::Sacrificed => "sacrificed",
        }
    }

    pub(in crate::oracle::canonical) fn from_ir_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|verb| verb.ir_name() == name)
    }

    /// Runtime action kinds whose `bind` field records exactly the objects
    /// this verb restates.
    pub(in crate::oracle::canonical) fn runtime_kinds(self) -> &'static [&'static str] {
        match self {
            Self::Destroyed => &["destroyPermanent"],
            Self::Milled => &["mill"],
            Self::Discarded => &["discardCards"],
            Self::Revealed => &["revealCards", "revealFromTopUntilCount"],
            Self::Exiled => &["exileTargetCards"],
            Self::Sacrificed => &["sacrificePermanents", "sacrificePermanent"],
        }
    }

    /// Whether the affected objects keep their characteristics where the
    /// action put them (a card moved between card zones), so a restated
    /// card noun can be re-read on the bound objects. A permanent that left
    /// the battlefield (a token ceases to exist) is never re-read.
    pub(in crate::oracle::canonical) const fn preserves_card_characteristics(self) -> bool {
        matches!(
            self,
            Self::Milled | Self::Discarded | Self::Revealed | Self::Exiled
        )
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
        .ok_or_else(|| cursor.fail("`destroyed`, `milled` or `discarded`"))?;
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    cursor.consume_phrase(&["this", "way"])?;
    Ok(ActionResultReferenceNode {
        predicate: query.predicate,
        verb,
    })
}

/// A quantity an earlier action of the same program produced
/// (`the total life lost this way`, `that many`, `the number of cards
/// revealed this way`, CR 608.2c, 702.101a).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultQuantity {
    /// The life the players actually lost (prevented loss is not lost).
    LifeLost,
    /// `that many` without an event quantity: the number of objects the
    /// nearest earlier recording action affected, or the count it recorded.
    ObjectCount,
    /// `that much` without an event quantity: the amount the nearest earlier
    /// recording action actually produced (life lost).
    Amount,
    /// `the number of OBJECTS VERBed this way`: the objects the nearest
    /// earlier action of the restated verb affected that satisfy the noun.
    Objects(ActionResultReferenceNode),
    /// `the result`, `the flip`, `you win` (a clash): a quantity of the
    /// latest random or contested outcome (`keyword_action.rs`), read from
    /// that outcome's definite binding rather than a producer search.
    RandomResult(super::RandomResultQuantity),
}

/// Where one runtime action records the quantity a reader restates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActionResultField {
    /// `bind` records the affected object ids; the reader counts them.
    Objects,
    /// `bind` records an integer amount; the reader reads it.
    Integer,
    /// `bindCountAs` records how many objects the action affected.
    Count,
}

impl ActionResultField {
    /// The runtime field installed on the producing action.
    pub(in crate::oracle::canonical) const fn ir_field(self) -> &'static str {
        match self {
            Self::Objects | Self::Integer => "bind",
            Self::Count => "bindCountAs",
        }
    }

    /// The runtime numeric expression reading `binding`.
    pub(in crate::oracle::canonical) fn reader(self, binding: &str) -> serde_json::Value {
        match self {
            Self::Objects => serde_json::json!({
                "kind": "countObjects",
                "objects": { "kind": "boundObjects", "binding": binding },
            }),
            Self::Integer | Self::Count => {
                serde_json::json!({ "kind": "boundValue", "id": binding })
            }
        }
    }
}

impl ActionResultQuantity {
    pub(in crate::oracle::canonical) const ALL: [Self; 3] =
        [Self::LifeLost, Self::ObjectCount, Self::Amount];

    pub(in crate::oracle::canonical) const fn ir_name(&self) -> &'static str {
        match self {
            Self::LifeLost => "lifeLost",
            Self::ObjectCount => "objectCount",
            Self::Amount => "amount",
            Self::Objects(_) => "objects",
            Self::RandomResult(_) => "randomResult",
        }
    }

    /// `that many|much`: its antecedent is the immediately preceding action.
    pub(in crate::oracle::canonical) const fn is_anaphor(&self) -> bool {
        matches!(self, Self::ObjectCount | Self::Amount)
    }

    pub(in crate::oracle::canonical) fn from_ir_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|quantity| quantity.ir_name() == name)
    }

    /// The field through which one lowered runtime action records this
    /// quantity, or `None` when that action records no such quantity. The
    /// table lists only runtime actions whose recorded value is exactly the
    /// affected objects (or their number) in every branch the engine runs.
    pub(in crate::oracle::canonical) fn produced_by(
        &self,
        runtime_kind: &str,
    ) -> Option<ActionResultField> {
        match (self, runtime_kind) {
            (Self::LifeLost | Self::Amount, "loseLife") => Some(ActionResultField::Integer),
            // A number chosen on resolution (CR 608.2d) is the amount or the
            // count a later `that much` / `that many` restates.
            (Self::Amount | Self::ObjectCount, "chooseNumber") => Some(ActionResultField::Integer),
            // A die roll's result (CR 706.2) is the amount or the count a
            // later `that much` / `that many` restates.
            (Self::Amount | Self::ObjectCount, "rollDice") => Some(ActionResultField::Integer),
            (
                Self::ObjectCount,
                "discardCards"
                | "drawCards"
                | "mill"
                | "moveCards"
                | "exilePermanent"
                | "sacrificePermanent"
                | "sacrificePermanents"
                | "createTokens",
            ) => Some(ActionResultField::Objects),
            (Self::ObjectCount, "discardHand") => Some(ActionResultField::Count),
            _ => None,
        }
    }
}

/// `that many|much` once no event quantity applies: the nearest earlier
/// action of the enclosing program is the antecedent. The first action of a
/// program has none, so the reader fails explicitly instead of counting zero.
pub(in crate::oracle::canonical) fn action_result_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    much: bool,
) -> MachineResult<super::NumericExpression> {
    // The failure keeps the anaphor's resolve state: no antecedent at all.
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "a numeric value bound by the triggering event or an earlier action of this program",
    )?;
    Ok(super::NumericExpression::ActionResultQuantity {
        reader_effect: reader,
        quantity: if much {
            ActionResultQuantity::Amount
        } else {
            ActionResultQuantity::ObjectCount
        },
    })
}

/// `OBJECTS VERBed this way` as a count read inside an action's arguments
/// (`the number of cards revealed this way`). The reader is the action being
/// read; the program lowering binds the nearest earlier action of the verb,
/// or fails explicitly. The first action of a program has no antecedent.
pub(in crate::oracle::canonical) fn recognize_action_result_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<super::NumericExpression> {
    let reference = recognize_action_result_reference(cursor)?;
    cursor.enter("action-result.count.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "an earlier action of this program whose affected objects the phrase restates",
    )?;
    Ok(super::NumericExpression::ActionResultQuantity {
        reader_effect: reader,
        quantity: ActionResultQuantity::Objects(reference),
    })
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
        ActionResultVerb::Revealed => matches!(
            effect,
            LocalEffectNode::CardAction(
                super::CardActionNode::Reveal(_) | super::CardActionNode::RevealUntil { .. }
            )
        ),
        ActionResultVerb::Discarded => matches!(
            effect,
            LocalEffectNode::DiscardCards(_)
                | LocalEffectNode::PlayerAction(PlayerActionNode {
                    action: SingularPlayerAction::Discard(_),
                    ..
                })
        ),
        ActionResultVerb::Exiled => matches!(
            effect,
            LocalEffectNode::TargetCardZoneChange(node)
                if node.action == CardZoneChangeAction::Exile
        ),
        ActionResultVerb::Sacrificed => matches!(
            effect,
            LocalEffectNode::Sacrifice(SacrificeEffectNode::Selected { .. })
                | LocalEffectNode::PlayerAction(PlayerActionNode {
                    action: SingularPlayerAction::Sacrifice(_),
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
        // Sacrificed permanents left the battlefield; only the noun their
        // own selection guarantees is counted.
        LocalEffectNode::Sacrifice(SacrificeEffectNode::Selected { object, .. }) => {
            guaranteed_by(&reference.predicate, &object.predicate)
                .then_some(ActionResultFilter::All)?
        }
        LocalEffectNode::PlayerAction(PlayerActionNode {
            action: SingularPlayerAction::Sacrifice(sacrifice),
            ..
        }) => guaranteed_by(&reference.predicate, &sacrifice.object.predicate)
            .then_some(ActionResultFilter::All)?,
        // Destroyed targets left the battlefield as well; the noun must be
        // one every declared slot's own predicate guarantees.
        LocalEffectNode::ZoneChange(node) => node
            .target
            .slots
            .iter()
            .all(|slot| match &slot.selector {
                super::TargetSelector::Object(predicate)
                | super::TargetSelector::ControlledObject { predicate, .. } => {
                    guaranteed_by(&reference.predicate, predicate)
                }
                _ => false,
            })
            .then_some(ActionResultFilter::All)?,
        // Exiled target cards keep their characteristics in exile.
        LocalEffectNode::TargetCardZoneChange(_) => {
            if reference.predicate == ObjectPredicate::ObjectKind(ObjectKind::Card) {
                ActionResultFilter::All
            } else if reference.predicate.is_in_domain(ObjectKind::Card) {
                ActionResultFilter::Cards(reference.predicate.clone())
            } else {
                return None;
            }
        }
        // Milled and discarded cards keep their characteristics in the
        // graveyard.
        LocalEffectNode::PlayerAction(_)
        | LocalEffectNode::DiscardCards(_)
        | LocalEffectNode::CardAction(_) => {
            if reference.predicate == ObjectPredicate::ObjectKind(ObjectKind::Card) {
                ActionResultFilter::All
            } else if reference.predicate.is_in_domain(ObjectKind::Card) {
                ActionResultFilter::Cards(reference.predicate.clone())
            } else {
                return None;
            }
        }
        _ => return None,
    };
    Some((index, filter))
}

/// Lexical lookahead for the active voice of an action-result condition:
/// `you VERB QUANTITY NOUN this way` before the end of the clause
/// (`If you discard a creature card this way`, `If you exiled a land card
/// this way`, `If you sacrifice an Island this way`).
pub(in crate::oracle::canonical) fn active_action_result_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "you")
        && cursor
            .peek_ahead(1)
            .and_then(ActionResultVerb::from_active_word)
            .is_some()
        && active_result_end(cursor).is_some()
}

/// Offset of `this` in `... this way` before the end of the clause.
fn active_result_end(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (2..14)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            cursor
                .peek_ahead(*offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
                && cursor
                    .peek_ahead(offset + 1)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("way"))
        })
}

/// `you VERB QUANTITY NOUN this way` (CR 608.2c): the same comparison as the
/// passive `QUANTITY NOUN was VERBed this way`, whose performer the active
/// subject restates. The program lowering binds the nearest earlier action
/// of the verb, or fails when there is none.
pub(in crate::oracle::canonical) fn recognize_active_action_result_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.action-result.active.subject");
    let end = active_result_end(cursor)
        .map(|offset| cursor.position() + offset)
        .ok_or_else(|| cursor.fail("an active `you VERB OBJECTS this way` clause"))?;
    cursor.consume("you")?;
    cursor.enter("condition.action-result.active.verb");
    let verb = cursor
        .peek()
        .and_then(ActionResultVerb::from_active_word)
        .ok_or_else(|| {
            cursor.fail("`discard`, `exile`, `sacrifice`, `mill`, `reveal` or `destroy`")
        })?;
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    cursor.enter("condition.action-result.active.quantity");
    let threshold = recognize_count_threshold(cursor, true)?;
    cursor.enter("condition.action-result.active.objects");
    let query = cursor.bounded(end, recognize_object_collection)?;
    cursor.require(
        !query.exclude_source && query.controller.is_none() && query.predicate.has_nominal_head(),
        "a nominal object phrase without a controller relation",
    )?;
    cursor.consume_phrase(&["this", "way"])?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::ActionResultObjects(ActionResultReferenceNode {
            predicate: query.predicate,
            verb,
        }),
        operator,
        right: ConditionNumber::Integer(value),
    }))
}

#[cfg(test)]
#[path = "action_result_tests.rs"]
mod tests;

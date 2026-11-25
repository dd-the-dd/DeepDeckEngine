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
    /// Permanents that became tapped (CR 701.26a). They stay on the
    /// battlefield; only those the action actually tapped are recorded.
    Tapped,
    /// Permanents returned from the battlefield to their owners' hands
    /// (CR 400.7: each card is a new object in the hand).
    Returned,
    /// Permanents a damage action actually dealt damage to (CR 120.3;
    /// prevented damage is not dealt, CR 615.1). They stay on the
    /// battlefield while the program resolves; only the two-word participle
    /// `dealt damage` names them (`action_result_participle_at`).
    DealtDamage,
}

impl ActionResultVerb {
    pub(in crate::oracle::canonical) const ALL: [Self; 9] = [
        Self::Destroyed,
        Self::Milled,
        Self::Discarded,
        Self::Revealed,
        Self::Exiled,
        Self::Sacrificed,
        Self::Tapped,
        Self::Returned,
        Self::DealtDamage,
    ];

    pub(in crate::oracle::canonical) fn from_word(word: &str) -> Option<Self> {
        match word.to_ascii_lowercase().as_str() {
            "destroyed" => Some(Self::Destroyed),
            "milled" => Some(Self::Milled),
            "discarded" => Some(Self::Discarded),
            "revealed" => Some(Self::Revealed),
            "exiled" => Some(Self::Exiled),
            "sacrificed" => Some(Self::Sacrificed),
            "tapped" => Some(Self::Tapped),
            "returned" => Some(Self::Returned),
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
            "tap" | "taps" => Some(Self::Tapped),
            "return" | "returns" => Some(Self::Returned),
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
            Self::Tapped => "tapped",
            Self::Returned => "returned",
            Self::DealtDamage => "dealtDamage",
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
            // `bind` records the permanents this tap actually tapped.
            Self::Tapped => &["tapPermanent"],
            // `bind` records the objects this return put into a hand.
            Self::Returned => &["returnToOwnersHand"],
            // `bind` records the permanents its damage events show it
            // dealt damage to.
            Self::DealtDamage => &["dealDamage"],
        }
    }

    /// True when the lowered runtime action `effect` runs this verb and its
    /// `bind` records exactly the objects it affected: a kind of
    /// `runtime_kinds`, or a card move whose destination is exile (`exile
    /// the top two cards of your library`, `exile target player's
    /// graveyard`, CR 701.13a), whose `bind` records the moved cards.
    pub(in crate::oracle::canonical) fn records_result(self, effect: &serde_json::Value) -> bool {
        match effect["kind"].as_str() {
            Some(kind) if self.runtime_kinds().contains(&kind) => true,
            Some("moveCards") => self == Self::Exiled && effect["to"]["kind"] == "exile",
            _ => false,
        }
    }

    /// Whether the affected objects keep their characteristics where the
    /// action put them (a card moved between card zones), so a restated
    /// card noun can be re-read on the bound objects. A permanent that left
    /// the battlefield (a token ceases to exist) is never re-read.
    pub(in crate::oracle::canonical) const fn preserves_card_characteristics(self) -> bool {
        matches!(
            self,
            Self::Milled | Self::Discarded | Self::Revealed | Self::Exiled | Self::Returned
        )
    }

    /// Whether the affected objects are still the same permanents on the
    /// battlefield (a tap changes no zone, CR 701.26a), so a restated
    /// permanent noun can be re-read on the bound permanents.
    pub(in crate::oracle::canonical) const fn preserves_permanents(self) -> bool {
        matches!(self, Self::Tapped | Self::DealtDamage)
    }
}

/// The past participle of a restated action at `offset` (`destroyed`,
/// `milled`, the two-word `dealt damage`), with its width in tokens. A
/// passive `OBJECT is dealt damage this way` names the damage action's
/// recipients; the bare `dealt` of `the damage dealt this way` is not one.
pub(in crate::oracle::canonical) fn action_result_participle_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(ActionResultVerb, usize)> {
    let word = cursor.peek_ahead(offset)?;
    if word.eq_ignore_ascii_case("dealt") {
        return cursor
            .peek_ahead(offset + 1)
            .is_some_and(|next| next.eq_ignore_ascii_case("damage"))
            .then_some((ActionResultVerb::DealtDamage, 2));
    }
    ActionResultVerb::from_word(word).map(|verb| (verb, 1))
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
    /// `the excess damage dealt this way` (CR 120.4a): the damage beyond
    /// lethal the nearest earlier damage action dealt to a permanent.
    ExcessDamage,
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
    /// `bindExcessAs` records the excess damage dealt to a permanent.
    Excess,
}

impl ActionResultField {
    /// The runtime field installed on the producing action.
    pub(in crate::oracle::canonical) const fn ir_field(self) -> &'static str {
        match self {
            Self::Objects | Self::Integer => "bind",
            Self::Count => "bindCountAs",
            Self::Excess => "bindExcessAs",
        }
    }

    /// The runtime numeric expression reading `binding`.
    pub(in crate::oracle::canonical) fn reader(self, binding: &str) -> serde_json::Value {
        match self {
            Self::Objects => serde_json::json!({
                "kind": "countObjects",
                "objects": { "kind": "boundObjects", "binding": binding },
            }),
            Self::Integer | Self::Count | Self::Excess => {
                serde_json::json!({ "kind": "boundValue", "id": binding })
            }
        }
    }
}

impl ActionResultQuantity {
    pub(in crate::oracle::canonical) const ALL: [Self; 4] = [
        Self::LifeLost,
        Self::ObjectCount,
        Self::Amount,
        Self::ExcessDamage,
    ];

    pub(in crate::oracle::canonical) const fn ir_name(&self) -> &'static str {
        match self {
            Self::LifeLost => "lifeLost",
            Self::ObjectCount => "objectCount",
            Self::Amount => "amount",
            Self::Objects(_) => "objects",
            Self::ExcessDamage => "excessDamage",
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
            (Self::ExcessDamage, "dealDamage") => Some(ActionResultField::Excess),
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
        ActionResultVerb::Exiled => match effect {
            LocalEffectNode::TargetCardZoneChange(node) => {
                node.action == CardZoneChangeAction::Exile
            }
            // `exile the top two cards of your library`, `exile all creature
            // cards from target player's graveyard`: a card move to exile.
            LocalEffectNode::CardAction(super::CardActionNode::Move { destination, .. }) => {
                destination.zone == super::CardZoneName::Exile
            }
            _ => false,
        },
        ActionResultVerb::Sacrificed => matches!(
            effect,
            LocalEffectNode::Sacrifice(SacrificeEffectNode::Selected { .. })
                | LocalEffectNode::PlayerAction(PlayerActionNode {
                    action: SingularPlayerAction::Sacrifice(_),
                    ..
                })
        ),
        // A chosen set the tap action taps (`tap any number of untapped
        // creatures you control`).
        ActionResultVerb::Tapped => matches!(
            effect,
            LocalEffectNode::SelectedPermanents(node)
                if node.action == super::SelectedPermanentAction::Tap
        ),
        // A chosen set returned to its owners' hands (`return any number of
        // Mountains you control to their owner's hand`).
        ActionResultVerb::Returned => matches!(
            effect,
            LocalEffectNode::SelectedPermanents(node)
                if node.action == super::SelectedPermanentAction::ReturnToOwnerHand
        ),
        // A damage action (`deals 3 damage to any target`); its recipients
        // are read from the damage it actually dealt.
        ActionResultVerb::DealtDamage => matches!(effect, LocalEffectNode::DealDamage(_)),
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
        // Tapped permanents stay on the battlefield and returned ones left
        // it; either way only the noun the selection's own query guarantees
        // is counted, never a characteristic re-read afterwards.
        LocalEffectNode::SelectedPermanents(node) => {
            guaranteed_by(&reference.predicate, &node.operand.query.predicate)
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

/// `When you VERB a|an|one or more NOUN this way,` / `When a|an|one or more
/// NOUN is|are VERBed this way,`: a reflexive triggered ability (CR 603.12)
/// whose event is the earlier instruction's own action, restricted to the
/// objects that action affected that satisfy the noun. It triggers once,
/// when at least one such object exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReflexiveResultNode {
    pub verb: ActionResultVerb,
    pub predicate: ObjectPredicate,
    /// `one or more`: one trigger for any number of objects. `a`/`an`: the
    /// singular event, which a single affected object makes unambiguous.
    pub one_or_more: bool,
}

/// Lexical: the width (with its comma) of a reflexive result connector at
/// `offset`, or `None`. It commits nothing.
pub(in crate::oracle::canonical) fn reflexive_result_connector_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<usize> {
    let word = |index: usize| cursor.peek_ahead(offset + index);
    let is = |index: usize, expected: &str| {
        word(index).is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !is(0, "when") {
        return None;
    }
    let this = (1..20)
        .take_while(|index| word(*index).is_some_and(|raw| raw != "." && raw != ","))
        .find(|index| is(*index, "this") && is(index + 1, "way") && is(index + 2, ","))?;
    let active = is(1, "you")
        && word(2)
            .and_then(ActionResultVerb::from_active_word)
            .is_some();
    let passive = this >= 3
        && word(this - 1)
            .and_then(ActionResultVerb::from_word)
            .is_some()
        && (is(this - 2, "is") || is(this - 2, "are"));
    (active || passive).then_some(this + 3)
}

/// Reads a reflexive result connector (`reflexive_result_connector_at`),
/// its comma included. The noun is the shared object collection bounded
/// before `this way` (active) or the copula (passive); it names no
/// controller, and the verb is a restated action verb.
pub(in crate::oracle::canonical) fn recognize_reflexive_result_connector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ReflexiveResultNode> {
    cursor.enter("reflexive-result.connector");
    let width = reflexive_result_connector_at(cursor, 0).ok_or_else(|| {
        cursor.fail("`When you VERB OBJECTS this way,` or `When OBJECTS are VERBed this way,`")
    })?;
    let this = cursor.position() + width - 3;
    cursor.consume("when")?;
    let active = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"));
    let mut verb = None;
    if active {
        cursor.enter("reflexive-result.active.verb");
        cursor.consume("you")?;
        let raw = cursor.peek().unwrap_or_default().to_owned();
        verb = Some(
            ActionResultVerb::from_active_word(&raw)
                .ok_or_else(|| cursor.fail("a restated action verb"))?,
        );
        cursor.consume(&raw)?;
    }
    cursor.enter("reflexive-result.quantity");
    let one_or_more = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("one"))
    {
        cursor.consume_phrase(&["one", "or", "more"])?;
        true
    } else {
        let article = cursor
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
            .ok_or_else(|| cursor.fail("`a`, `an` or `one or more`"))?
            .to_owned();
        cursor.consume(&article)?;
        false
    };
    cursor.enter("reflexive-result.objects");
    let noun_end = if active { this } else { this - 2 };
    let query = cursor.bounded(noun_end, recognize_object_collection)?;
    cursor.require(
        !query.exclude_source && query.controller.is_none() && query.predicate.has_nominal_head(),
        "a nominal object phrase without a controller relation",
    )?;
    if !active {
        cursor.enter("reflexive-result.passive.verb");
        let copula = if one_or_more { "are" } else { "is" };
        cursor.consume(copula)?;
        let raw = cursor.peek().unwrap_or_default().to_owned();
        verb = Some(
            ActionResultVerb::from_word(&raw)
                .ok_or_else(|| cursor.fail("a restated past participle"))?,
        );
        cursor.consume(&raw)?;
    }
    cursor.enter("reflexive-result.this-way");
    cursor.consume_phrase(&["this", "way"])?;
    cursor.consume(",")?;
    Ok(ReflexiveResultNode {
        verb: verb.expect("the active or passive verb was read"),
        predicate: query.predicate,
        one_or_more,
    })
}

/// Lexical lookahead for `excess damage was|is dealt ... this way`.
pub(in crate::oracle::canonical) fn excess_damage_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "excess") && word(1, "damage") && (word(2, "was") || word(2, "is")) && word(3, "dealt")
}

/// `excess damage was dealt [to that creature|to it|to a permanent] this
/// way` (CR 120.4a, 608.2c): the damage beyond lethal that the nearest
/// earlier damage action of the program dealt is greater than zero. Excess
/// damage exists only for a permanent recipient, so `a permanent` restates
/// no other filter; a narrower noun (`a creature`) is not proven by the
/// recorded amount and fails explicitly. The program lowering binds the
/// producing action or fails; the first action of a program has none.
pub(in crate::oracle::canonical) fn recognize_excess_damage_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.excess-damage.subject");
    cursor.consume_phrase(&["excess", "damage"])?;
    let copula = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&copula)?;
    cursor.consume("dealt")?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("to"))
    {
        cursor.enter("condition.excess-damage.recipient");
        cursor.consume("to")?;
        let determiner = cursor.peek().unwrap_or_default().to_ascii_lowercase();
        match determiner.as_str() {
            "it" => cursor.consume("it")?,
            "that" => {
                cursor.consume("that")?;
                let noun = cursor.peek().unwrap_or_default().to_owned();
                cursor.require(
                    ["creature", "permanent", "planeswalker"]
                        .iter()
                        .any(|expected| noun.eq_ignore_ascii_case(expected)),
                    "the permanent the damage action dealt damage to",
                )?;
                cursor.consume(&noun)?;
            }
            "a" => {
                cursor.consume("a")?;
                cursor.consume("permanent")?;
            }
            _ => {
                return Err(cursor.fail("`to it`, `to that PERMANENT` or `to a permanent`"));
            }
        }
    }
    cursor.enter("condition.excess-damage.scope");
    cursor.consume_phrase(&["this", "way"])?;
    cursor.enter("condition.excess-damage.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "an earlier damage action of this program whose excess damage the condition reads",
    )?;
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(super::NumericExpression::ActionResultQuantity {
            reader_effect: reader,
            quantity: ActionResultQuantity::ExcessDamage,
        }),
        operator: super::NumericComparisonOperator::GreaterThan,
        right: ConditionNumber::Integer(0),
    }))
}

/// Words a restated object may use in place of the producer's own words:
/// anaphors and articles naming the same object (`the card` for `it`, `a
/// card` for `a Food card`).
const RESTATED_OBJECT_WORDS: &[&str] = &[
    "it", "them", "the", "that", "those", "a", "an", "card", "cards",
];

/// `If you don't VERB RESTATEMENT [this way] ,` at `offset`: the declined
/// outcome of the producer written between the absolute positions
/// `producer_start` and `producer_end`, restating its instruction instead of
/// writing a bare `If you don't` (`If you don't put the card into your hand,`
/// after `you may reveal it and put it into your hand`; `If you don't draw a
/// card this way,` after `draw a card if ...`, CR 608.2c). The restated verb
/// is a verb of the producer, and every other restated word is one of the
/// producer's words after that verb or an anaphor or article naming the
/// same object: `If you don't put it into your graveyard` after `put it into
/// your hand` restates nothing. Returns the connector's width with its comma.
/// Lexical lookahead only; it commits nothing.
pub(in crate::oracle::canonical) fn restated_declined_connector_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
    producer_start: usize,
    producer_end: usize,
) -> Option<usize> {
    let word = |index: usize| cursor.peek_ahead(offset + index);
    let is = |index: usize, expected: &str| {
        word(index).is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if !(is(0, "if")
        && is(1, "you")
        && word(2).is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case("don't")))
    {
        return None;
    }
    let lemma = word(3)
        .and_then(super::lookup_effect_verb)
        .filter(|(_, forms)| forms.contains(&super::VerbForm::Base))
        .map(|(entry, _)| entry.lemma)?;
    // The restatement ends at the connector's comma, within its sentence.
    let comma = (4..24)
        .take_while(|index| word(*index).is_some_and(|raw| raw != "."))
        .find(|index| word(*index) == Some(","))?;
    let mut restated = (4..comma).filter_map(word).collect::<Vec<_>>();
    if restated.len() >= 2
        && restated[restated.len() - 2].eq_ignore_ascii_case("this")
        && restated[restated.len() - 1].eq_ignore_ascii_case("way")
    {
        restated.truncate(restated.len() - 2);
    }
    // The producer's words after its first verb of the same lemma.
    let first = producer_start.max(cursor.position());
    let producer = (first..producer_end)
        .filter_map(|position| cursor.peek_ahead(position - cursor.position()))
        .collect::<Vec<_>>();
    let verb = producer.iter().position(|raw| {
        super::lookup_effect_verb(raw).is_some_and(|(entry, _)| entry.lemma == lemma)
    })?;
    let available = producer[verb + 1..]
        .iter()
        .map(|raw| raw.to_ascii_lowercase())
        .collect::<std::collections::BTreeSet<_>>();
    restated
        .iter()
        .all(|raw| {
            let raw = raw.to_ascii_lowercase();
            available.contains(&raw) || RESTATED_OBJECT_WORDS.contains(&raw.as_str())
        })
        .then_some(comma + 1)
}

#[cfg(test)]
#[path = "action_result_tests.rs"]
mod tests;

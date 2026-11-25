//! A cost is a sequence of ordinary actions (CR 118.1, 601.2f-h, 602.2b).
//!
//! ```text
//! CostSequence  := PaymentAction (Connector PaymentAction)*
//! Connector     := "," | "and"              (the parent chooses the set)
//! PaymentAction := MANA | "{T}"
//!                | Clause[supplied payer, operand frame]  (`clause.rs`): a
//!                  lexicon lemma whose frame the cost reads with its child:
//!                  ResourcePayment (pay MANA | N life | {E}... ),
//!                  ObjectSelection
//!                  (tap, untap, sacrifice, discard, exile, reveal, exert),
//!                  ObjectToZone (return ... to its owner's hand, put ...
//!                  [from your ZONE] on top of|on the bottom of a library),
//!                  CountersOn (put N [KIND] counter(s) on CounterHolder),
//!                  CountersFrom (remove N [KIND] counter(s) from CounterHolder),
//!                  CardCount (mill N card(s)), KeywordAmount (blight N)
//! CounterHolder := SourceRef | ("a" | "an") QUERY | "among" QUERY
//! Selection     := SourceRef
//!                | CostQuantity ["other"] ["untapped"] QUERY ["at" "random"]
//! SourceRef     := "this" NOUN   (shared source reference reader)
//!                | NAME          (the card's own name, linked by the name
//!                                 resolver and installed by the cost parent)
//!                | "it"          (after an earlier action of the same list
//!                                 named the source)
//! Quantity      := "a" | "an" | NUMBER | "X" | "another"
//! CostQuantity  := Quantity | ChosenRange       (`quantity_slot.rs`: `any
//!                  number of`, `one or more`, `up to N`, a number the payer
//!                  chooses while paying, CR 601.2h)
//! ```
//!
//! The quantity slot is the shared numeric atom: a literal or the variable X.
//! X is never bound here; the cost parent validates that its own ability
//! declares X (an activation's `{X}` mana cost) and fails binding otherwise.
//! The source noun names the source through the shared reference reader; the
//! verb's object domain decides which zone that source is paid from.
//!
//! A parent whose payer is not the ability controller states the payer's
//! agreement (`ActingSubject`); the lexicon supplies the surface (`pays`,
//! `sacrifices … [of their choice]`, ...). No verb word is tested here.
//!
//! The verb heads, the mana/life payment, the object query and the card
//! selector are the shared children used by effects. This module only composes
//! them into one typed `PaymentActionNode`; it decides no cost semantics. The
//! cost parent (activation, resolution, additional cost…) fixes the payer,
//! all-or-nothing payment (CR 118.3), choices made while paying (CR 601.2h,
//! not targets) and the source zone implied by `this card` actions. A parent
//! whose engine schema has no representation for an atom fails explicitly in
//! its own projection instead of approximating it.
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, ObjectDomain, lookup_card_type, lookup_object_domain,
};
use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    ActingSubject,
    selection_method::{recognize_acting_player_choice_marker, recognize_postfix_selection_method},
};
use super::{ArgumentFrame, VerbLemma, recognize_operand_verb};
use super::{
    CollectionSelectionMethod, CostConnector, CostQuantity, CostSequenceNode, NumericAtom,
    ObjectControllerScope, ObjectKind, ObjectPredicate, ObjectQueryNode, ResourcePaymentNode,
    positive_literal, positive_quantity, quantity_is_plural, recognize_cost_quantity,
    recognize_counter_name, recognize_mana_cost, recognize_object_query,
    recognize_pay_life_cost_with_verb, recognize_player_counter_payment,
    recognize_search_card_selector, recognize_source_reference, recognize_tap_self_cost,
};

/// One ordinary action read in a cost section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentActionNode {
    /// `{2}{R}`, `pay {2}` or `pay N life`.
    Pay(ResourcePaymentNode),
    /// `{T}` or `tap SELECTION` (permanents the payer controls).
    Tap(PaymentObjectsNode),
    /// `tap any number of QUERY with total power N or greater` (`cost.rs`).
    TapTotalPower(super::TotalPowerTapCostNode),
    /// `sacrifice SELECTION` (permanents the payer controls, CR 701.17a).
    Sacrifice(PaymentObjectsNode),
    /// `discard SELECTION` (cards in the payer's hand).
    Discard(PaymentObjectsNode),
    /// `exile SELECTION [from your ZONE]`.
    Exile {
        objects: PaymentObjectsNode,
        origin: Option<PaymentOriginZone>,
    },
    /// `blight N`: N -1/-1 counters on one creature the payer controls.
    Blight { amount: i64 },
    /// `remove N [KIND] counters from HOLDER`.
    RemoveCounters(RemoveCountersPaymentNode),
    /// `untap SELECTION`.
    Untap(PaymentObjectsNode),
    /// `return SELECTION to its owner's hand` (permanents the payer controls).
    ReturnToHand(PaymentObjectsNode),
    /// `put N KIND counter(s) on HOLDER`.
    PutCounters(PutCountersPaymentNode),
    /// `reveal SELECTION [from your hand]` (cards).
    Reveal {
        objects: PaymentObjectsNode,
        origin: Option<PaymentOriginZone>,
    },
    /// `mill N card(s)`.
    Mill { count: i64 },
    /// `exert SELECTION` (CR 701.43).
    Exert(PaymentObjectsNode),
    /// `put SELECTION [from your ZONE] on top of|on the bottom of your
    /// library` (CR 401.4): the ObjectToZone frame read as a payment.
    PutInLibrary {
        objects: PaymentObjectsNode,
        origin: Option<PaymentOriginZone>,
        position: PaymentLibraryEnd,
    },
}

/// The end of a library a cost puts its objects at (CR 401.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentLibraryEnd {
    Top,
    Bottom,
}

impl PaymentLibraryEnd {
    /// The engine's library position word.
    pub(in crate::oracle::canonical) fn engine_position(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }
}

/// Counters put on a permanent while paying a cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PutCountersPaymentNode {
    pub count: NumericAtom,
    pub counter: CounterName,
    pub holder: CounterHolderNode,
}

/// Counters removed while paying a cost. The counter kind and the holder are
/// independent children; the cost parent decides which combinations its
/// engine schema represents. The count is exact or chosen by the payer
/// (`remove any number of storage counters`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RemoveCountersPaymentNode {
    pub count: CostQuantity,
    /// `None`: counters of any kinds, chosen by the payer while paying.
    pub counter: Option<CounterName>,
    pub holder: CounterHolderNode,
}

/// The permanent(s) whose counters are removed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterHolderNode {
    /// `from this PERMANENT`: the cost's source.
    Source(PaymentSourceNoun),
    /// `from a QUERY`: one permanent the payer controls, chosen while paying.
    One(ObjectQueryNode),
    /// `from among QUERY`: counters spread over permanents the payer controls.
    Among(ObjectQueryNode),
}

/// The objects an action is performed on in a cost.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentObjectsNode {
    /// The source of the cost: `{T}`, `this creature`, `this token`, `this card`.
    Source(PaymentSourceNoun),
    /// Objects the payer chooses while paying (never targets).
    Chosen(ChosenPaymentObjectsNode),
    /// Objects the text determines without a choice: a whole zone, a zone
    /// position or the permanent the source is attached to.
    Designated(DesignatedPaymentObjects),
}

/// Objects a cost names by their place rather than by a choice. The verb
/// frame decides which of them it can perform its action on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DesignatedPaymentObjects {
    /// `your hand`: every card in the payer's hand (CR 701.9a, 402.1).
    Hand,
    /// `the top card of your library` / `the top N cards of your library`
    /// (CR 401.2): a literal number of the payer's top library cards.
    LibraryTop { count: i64 },
    /// `the top [PREDICATE] card of your graveyard` (CR 404.2: the order of
    /// a graveyard is kept): the latest card put there that matches.
    GraveyardTop { filter: ObjectPredicate },
    /// `enchanted NOUN` / `equipped creature`: the permanent the source is
    /// attached to (CR 301.5a, 303.4a).
    Attached(super::AttachedObjectRole),
}

/// The object domain a source reference names. The shared reference reader
/// proves the reference; its noun only decides the domain (CR 109.2): a
/// `card` is the source outside the battlefield, any noun a permanent may
/// carry (card type, subtype, `permanent`, `token`) is the source permanent.
/// A source-only payment acts on the source object itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentSourceNoun {
    TapSymbol,
    /// `this creature`, `this Aura`, `this token`, or `it` after an action
    /// on the source permanent.
    Permanent,
    /// `this card` (a hand, graveyard or library card).
    Card,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenPaymentObjectsNode {
    /// A literal, the variable X, or a number the payer chooses while
    /// paying (`any number of`, `one or more`, `up to N`).
    pub count: CostQuantity,
    /// `another` / `other`: the source cannot be one of the chosen objects.
    pub exclude_source: bool,
    pub filter: PaymentObjectFilter,
    pub method: CollectionSelectionMethod,
}

/// The shared query child selected by the verb's object domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentObjectFilter {
    /// Battlefield permanents (shared object query with its controller scope).
    Permanents(ObjectQueryNode),
    /// Cards in a zone (shared card selector).
    Cards(ObjectPredicate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentOriginZone {
    Hand,
    Graveyard,
}

/// N untapped permanents the payer controls, tapped as a cost (CR 118.3,
/// 701.26a): the context-typed projection read by both cost lowerings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TapObjectsCostNode {
    pub count: i64,
    pub object: ObjectQueryNode,
    /// `another` excludes the ability's source from the tapped permanents.
    pub exclude_source: bool,
}

impl PaymentObjectFilter {
    /// The unrestricted card selector (`a card`).
    pub(in crate::oracle::canonical) fn is_any_card(&self) -> bool {
        matches!(
            self,
            Self::Cards(ObjectPredicate::ObjectKind(ObjectKind::Card))
        )
    }
}

/// Which object domain an action's operand belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OperandDomain {
    Permanent,
    Card,
}

/// The lemmas a cost reads and the operand frames it interprets. Any lexicon
/// lemma with one of these frames is a payment action; the cost children
/// below read its operand.
const COST_LEMMAS: &[VerbLemma] = &[
    VerbLemma::Pay,
    VerbLemma::Tap,
    VerbLemma::Untap,
    VerbLemma::Sacrifice,
    VerbLemma::Discard,
    VerbLemma::Exile,
    VerbLemma::Return,
    VerbLemma::Put,
    VerbLemma::Reveal,
    VerbLemma::Mill,
    VerbLemma::Exert,
    VerbLemma::Remove,
    VerbLemma::Blight,
];

const COST_FRAMES: &[ArgumentFrame] = &[
    ArgumentFrame::ResourcePayment,
    ArgumentFrame::ObjectSelection,
    ArgumentFrame::ObjectToZone,
    ArgumentFrame::CountersOn,
    ArgumentFrame::CountersFrom,
    ArgumentFrame::CardCount,
    ArgumentFrame::KeywordAmount,
];

fn payment_verb_starts(cursor: &OracleTokenCursor<'_>, subject: ActingSubject) -> bool {
    let mut trial = cursor.clone();
    !matches!(
        recognize_operand_verb(&mut trial, subject.agreement, COST_LEMMAS, COST_FRAMES),
        Ok(None)
    )
}

/// Lookahead only: true when the cursor is at a payment action head.
pub(in crate::oracle::canonical) fn payment_action_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "manaSymbol" || token.token_type == "tapSymbol")
        || payment_verb_starts(cursor, ActingSubject::CONTROLLER)
}

/// Lookahead only: a payment verb in the payer's agreement (`pays`,
/// `sacrifices`, `discards`…). Mana and `{T}` heads are imperative.
pub(in crate::oracle::canonical::automata) fn conjugated_payment_action_starts(
    cursor: &OracleTokenCursor<'_>,
    form: ActingSubject,
) -> bool {
    if form.agreement == super::VerbAgreement::Base {
        payment_action_starts(cursor)
    } else {
        payment_verb_starts(cursor, form)
    }
}

/// Reads one payment action by delegating to the shared action children.
pub(in crate::oracle::canonical) fn recognize_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PaymentActionNode> {
    recognize_conjugated_payment_action(cursor, ActingSubject::CONTROLLER)
}

/// The same payment actions with the verb in the agreement of the payer
/// chosen by the cost parent (`unless that player sacrifices a creature`). A
/// third-person object choice may state its chooser (`of their choice`): the
/// payer chooses while paying either way (CR 601.2h), so the marker adds no
/// semantics.
pub(in crate::oracle::canonical::automata) fn recognize_conjugated_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: ActingSubject,
) -> MachineResult<PaymentActionNode> {
    recognize_listed_payment_action(cursor, form, SourceAntecedent::Absent)
}

/// Whether an earlier action of the same cost list named the source, so a
/// later `it` can only be that source (`remove five quest counters from this
/// enchantment and sacrifice it`). The list owns this scope; a single action
/// never invents an antecedent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceAntecedent {
    Absent,
    Named(PaymentSourceNoun),
}

impl PaymentActionNode {
    /// The source object this action names, if any (`{T}` names the source
    /// permanent too, but is not a noun phrase a pronoun can resume).
    fn named_source(&self) -> Option<PaymentSourceNoun> {
        let objects = match self {
            Self::Tap(objects)
            | Self::Sacrifice(objects)
            | Self::Discard(objects)
            | Self::Untap(objects)
            | Self::ReturnToHand(objects)
            | Self::Exert(objects)
            | Self::Exile { objects, .. }
            | Self::PutInLibrary { objects, .. }
            | Self::Reveal { objects, .. } => objects,
            Self::RemoveCounters(RemoveCountersPaymentNode {
                holder: CounterHolderNode::Source(noun),
                ..
            })
            | Self::PutCounters(PutCountersPaymentNode {
                holder: CounterHolderNode::Source(noun),
                ..
            }) => return Some(*noun),
            Self::Pay(_)
            | Self::TapTotalPower(_)
            | Self::Blight { .. }
            | Self::RemoveCounters(_)
            | Self::PutCounters(_)
            | Self::Mill { .. } => return None,
        };
        match objects {
            PaymentObjectsNode::Source(PaymentSourceNoun::TapSymbol)
            | PaymentObjectsNode::Chosen(_)
            | PaymentObjectsNode::Designated(_) => None,
            PaymentObjectsNode::Source(noun) => Some(*noun),
        }
    }
}

fn recognize_listed_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: ActingSubject,
    antecedent: SourceAntecedent,
) -> MachineResult<PaymentActionNode> {
    cursor.enter("cost.payment.verb");
    let Some(token) = cursor.peek_token() else {
        return Err(cursor.fail("a payment action"));
    };
    if form.agreement == super::VerbAgreement::Base {
        if token.token_type == "manaSymbol" {
            return recognize_mana_cost(cursor)
                .map(|mana| PaymentActionNode::Pay(ResourcePaymentNode::Mana(mana)));
        }
        if token.token_type == "tapSymbol" {
            recognize_tap_self_cost(cursor)?;
            return Ok(PaymentActionNode::Tap(PaymentObjectsNode::Source(
                PaymentSourceNoun::TapSymbol,
            )));
        }
    }
    let verb_start = cursor.checkpoint();
    let Some((lemma, frame)) =
        recognize_operand_verb(cursor, form.agreement, COST_LEMMAS, COST_FRAMES)?
    else {
        return Err(cursor.fail(
            "a payment action: mana, `{T}`, or a lexicon action valid as a cost (pay, tap, untap, sacrifice, discard, exile, return, put, reveal, mill, exert, remove, blight)",
        ));
    };
    let chooser_marker = |cursor: &mut OracleTokenCursor<'_>| {
        if form.third_person {
            recognize_acting_player_choice_marker(cursor)?;
        }
        Ok(())
    };
    let objects = |cursor: &mut OracleTokenCursor<'_>, verb: PaymentVerb| {
        cursor.enter(verb.operand_state());
        recognize_payment_objects(cursor, verb, antecedent)
    };
    match (lemma, frame) {
        (VerbLemma::Pay, ArgumentFrame::ResourcePayment) => {
            cursor.enter("cost.payment.pay.resource");
            if cursor
                .peek_token()
                .is_some_and(|token| token.token_type == "manaSymbol")
            {
                cursor.enter("cost.payment.pay.mana");
                recognize_mana_cost(cursor)
                    .map(|mana| PaymentActionNode::Pay(ResourcePaymentNode::Mana(mana)))
            } else if super::player_counter_payment_starts(cursor) {
                // CR 107.14: `{E}` is one energy counter; paying it removes
                // that many energy counters from the payer.
                cursor.enter("cost.payment.pay.player-counters");
                recognize_player_counter_payment(cursor).map(|counters| {
                    PaymentActionNode::Pay(ResourcePaymentNode::PlayerCounters(counters))
                })
            } else {
                // The life child reads its own verb in the payer's agreement.
                cursor.rollback(verb_start);
                cursor.enter("cost.payment.pay.life");
                recognize_pay_life_cost_with_verb(cursor, form)
                    .map(|life| PaymentActionNode::Pay(ResourcePaymentNode::Life(life)))
            }
        }
        (VerbLemma::Tap, _) if super::total_power_tap_starts(cursor) => {
            super::recognize_total_power_tap_cost(cursor).map(PaymentActionNode::TapTotalPower)
        }
        (VerbLemma::Tap, _) => objects(cursor, PaymentVerb::Tap).map(PaymentActionNode::Tap),
        (VerbLemma::Untap, _) => objects(cursor, PaymentVerb::Untap).map(PaymentActionNode::Untap),
        (VerbLemma::Sacrifice, _) => {
            let operand = objects(cursor, PaymentVerb::Sacrifice)?;
            chooser_marker(cursor)?;
            Ok(PaymentActionNode::Sacrifice(operand))
        }
        (VerbLemma::Discard, _) => {
            let operand = objects(cursor, PaymentVerb::Discard)?;
            chooser_marker(cursor)?;
            Ok(PaymentActionNode::Discard(operand))
        }
        (VerbLemma::Exile, _) => {
            let objects = objects(cursor, PaymentVerb::Exile)?;
            if matches!(objects, PaymentObjectsNode::Designated(_)) {
                // A zone position already names its zone (`the top card of
                // your library`): no further origin is read.
                return Ok(PaymentActionNode::Exile {
                    objects,
                    origin: None,
                });
            }
            cursor.enter("cost.payment.exile.origin");
            let origin = recognize_optional_origin_zone(cursor)?;
            cursor.require(
                origin.is_some() || matches!(objects, PaymentObjectsNode::Source(_)),
                "`from your hand` or `from your graveyard` after the exiled cards",
            )?;
            // A source permanent is exiled from the battlefield: a named
            // card zone would contradict its noun.
            cursor.enter("cost.payment.exile.source-domain");
            cursor.require(
                !(origin.is_some()
                    && objects == PaymentObjectsNode::Source(PaymentSourceNoun::Permanent)),
                "`this card` for a source exiled from a card zone",
            )?;
            Ok(PaymentActionNode::Exile { objects, origin })
        }
        (VerbLemma::Reveal, _) => {
            let objects = objects(cursor, PaymentVerb::Reveal)?;
            cursor.enter("cost.payment.reveal.origin");
            let origin = recognize_optional_origin_zone(cursor)?;
            Ok(PaymentActionNode::Reveal { objects, origin })
        }
        (VerbLemma::Exert, _) => objects(cursor, PaymentVerb::Exert).map(PaymentActionNode::Exert),
        (VerbLemma::Return, ArgumentFrame::ObjectToZone) => {
            let objects = objects(cursor, PaymentVerb::Return)?;
            cursor.enter("cost.payment.return.destination");
            recognize_owners_hand(cursor)?;
            Ok(PaymentActionNode::ReturnToHand(objects))
        }
        (VerbLemma::Put, ArgumentFrame::CountersOn) => {
            recognize_counter_placement(cursor, antecedent).map(PaymentActionNode::PutCounters)
        }
        (VerbLemma::Put, ArgumentFrame::ObjectToZone) => {
            let objects = objects(cursor, PaymentVerb::Put)?;
            cursor.enter("cost.payment.put.origin");
            let origin = recognize_optional_origin_zone(cursor)?;
            // Chosen cards name the zone they come from; the source names
            // its own (`this card from your graveyard`, `this creature`).
            cursor.require(
                origin.is_some() || matches!(objects, PaymentObjectsNode::Source(_)),
                "`from your hand` or `from your graveyard` after the cards put into a library",
            )?;
            cursor.enter("cost.payment.put.source-domain");
            cursor.require(
                !(origin.is_some()
                    && objects == PaymentObjectsNode::Source(PaymentSourceNoun::Permanent)),
                "`this card` for a source put from a card zone",
            )?;
            cursor.enter("cost.payment.put.destination");
            let position = recognize_payment_library_end(cursor, &objects)?;
            Ok(PaymentActionNode::PutInLibrary {
                objects,
                origin,
                position,
            })
        }
        (VerbLemma::Remove, ArgumentFrame::CountersFrom) => {
            cursor.enter("action.remove.operand");
            recognize_counter_removal(cursor, antecedent).map(PaymentActionNode::RemoveCounters)
        }
        (VerbLemma::Mill, ArgumentFrame::CardCount) => {
            cursor.enter("cost.payment.mill.count");
            let count = positive_literal(cursor, "`a` or a positive literal number of cards")?;
            cursor.enter("cost.payment.mill.noun");
            cursor.consume(if count == 1 { "card" } else { "cards" })?;
            Ok(PaymentActionNode::Mill { count })
        }
        (VerbLemma::Blight, ArgumentFrame::KeywordAmount) => {
            cursor.enter("cost.payment.blight.amount");
            positive_literal(cursor, "a positive literal Blight amount")
                .map(|amount| PaymentActionNode::Blight { amount })
        }
        _ => {
            cursor.enter("cost.payment.frame");
            Err(cursor.fail(
                "a payment reading of this action's frame (putting an object into a zone has no cost reader)",
            ))
        }
    }
}

/// `on top of|on the bottom of your library`, or `... its owner's library`
/// for the source (CR 401.4: a card goes to its owner's library). A chosen
/// card comes from the payer's own zone, so `your library` is its owner's.
fn recognize_payment_library_end(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &PaymentObjectsNode,
) -> MachineResult<PaymentLibraryEnd> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let position = if word(cursor, 0, "on") && word(cursor, 1, "top") && word(cursor, 2, "of") {
        cursor.consume_phrase(&["on", "top", "of"])?;
        PaymentLibraryEnd::Top
    } else if word(cursor, 0, "on")
        && word(cursor, 1, "the")
        && word(cursor, 2, "bottom")
        && word(cursor, 3, "of")
    {
        cursor.consume_phrase(&["on", "the", "bottom", "of"])?;
        PaymentLibraryEnd::Bottom
    } else {
        return Err(cursor.fail("`on top of` or `on the bottom of` a library"));
    };
    cursor.enter("cost.payment.put.library-owner");
    let owners = cursor
        .peek_ahead(1)
        .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase());
    if word(cursor, 0, "your") {
        cursor.consume("your")?;
    } else if matches!(objects, PaymentObjectsNode::Source(_))
        && (word(cursor, 0, "its") || word(cursor, 0, "their"))
        && owners.as_deref() == Some("owner's")
    {
        cursor.advance_to(cursor.position() + 2)?;
    } else {
        return Err(cursor.fail("`your library`, or `its owner's library` for the source"));
    }
    cursor.enter("cost.payment.put.library");
    cursor.consume("library")?;
    Ok(position)
}

/// `to its owner's hand` / `to their owner's hand` / `to their owners' hands`.
fn recognize_owners_hand(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.consume("to")?;
    let normalized = |cursor: &OracleTokenCursor<'_>, offset: usize| {
        cursor
            .peek_ahead(offset)
            .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase())
    };
    cursor.require(
        matches!(normalized(cursor, 0).as_deref(), Some("its" | "their"))
            && matches!(
                normalized(cursor, 1).as_deref(),
                Some("owner's" | "owners'")
            )
            && matches!(normalized(cursor, 2).as_deref(), Some("hand" | "hands")),
        "`its owner's hand` or `their owner's hand`",
    )?;
    cursor.advance_to(cursor.position() + 3)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaymentVerb {
    Tap,
    Untap,
    Sacrifice,
    Discard,
    Exile,
    Reveal,
    Exert,
    Return,
    Remove,
    Put,
}

impl PaymentVerb {
    fn domain(self) -> OperandDomain {
        match self {
            Self::Tap
            | Self::Untap
            | Self::Sacrifice
            | Self::Remove
            | Self::Exert
            | Self::Return => OperandDomain::Permanent,
            Self::Discard | Self::Exile | Self::Reveal | Self::Put => OperandDomain::Card,
        }
    }

    /// Exile takes a card (`exile this card from your graveyard`) or the
    /// source permanent (`exile this artifact`); other card verbs take cards.
    fn admits_permanent_source(self) -> bool {
        matches!(self, Self::Exile | Self::Put)
    }

    fn operand_state(self) -> &'static str {
        match self {
            Self::Tap => "action.tap.operand",
            Self::Untap => "action.untap.operand",
            Self::Sacrifice => "action.sacrifice.operand",
            Self::Discard => "action.discard.operand",
            Self::Exile => "action.exile.operand",
            Self::Reveal => "action.reveal.operand",
            Self::Exert => "action.exert.operand",
            Self::Return => "action.return.operand",
            Self::Remove => "action.remove.operand",
            Self::Put => "action.put.operand",
        }
    }
}

/// The shared selection operand: a source reference or chosen objects. The
/// object query decides whether a comma continues its own list (`unless they
/// sacrifice a creature, artifact, or land`) or belongs to the cost list.
fn recognize_payment_objects(
    cursor: &mut OracleTokenCursor<'_>,
    verb: PaymentVerb,
    antecedent: SourceAntecedent,
) -> MachineResult<PaymentObjectsNode> {
    cursor.enter("cost.payment.selection");
    if source_object_starts(cursor, antecedent) {
        return recognize_source_object(
            cursor,
            verb.domain(),
            verb.admits_permanent_source(),
            antecedent,
        )
        .map(PaymentObjectsNode::Source);
    }
    if let Some(designated) = recognize_designated_objects(cursor, verb)? {
        return Ok(PaymentObjectsNode::Designated(designated));
    }
    recognize_chosen_objects(cursor, verb).map(PaymentObjectsNode::Chosen)
}

/// Objects named by their place: `your hand`, `the top [N] card(s) of your
/// library`, `the top [PREDICATE] card of your graveyard`, `enchanted NOUN`.
/// `None` consumes nothing: the chosen-object quantity reads the operand.
/// Once its head is read the phrase is committed; the verb decides which
/// place it can act on, so `exile your hand` fails at the verb's operand.
fn recognize_designated_objects(
    cursor: &mut OracleTokenCursor<'_>,
    verb: PaymentVerb,
) -> MachineResult<Option<DesignatedPaymentObjects>> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(cursor, 0, "your") && word(cursor, 1, "hand") {
        cursor.enter("cost.payment.designated.hand");
        cursor.require(
            verb == PaymentVerb::Discard,
            "`discard` for the payer's whole hand",
        )?;
        cursor.consume_phrase(&["your", "hand"])?;
        return Ok(Some(DesignatedPaymentObjects::Hand));
    }
    if word(cursor, 0, "the") && word(cursor, 1, "top") {
        cursor.enter("cost.payment.designated.zone-top");
        cursor.require(
            verb == PaymentVerb::Exile,
            "`exile` for the top cards of a zone",
        )?;
        return recognize_zone_top_objects(cursor).map(Some);
    }
    if super::attached_object_reference_starts(cursor) {
        cursor.enter("cost.payment.designated.attached");
        cursor.require(
            matches!(verb, PaymentVerb::Tap | PaymentVerb::Sacrifice),
            "`tap` or `sacrifice` for the permanent the source is attached to",
        )?;
        let role = if word(cursor, 0, "equipped") {
            super::AttachedObjectRole::Equipped
        } else {
            super::AttachedObjectRole::Enchanted
        };
        // The shared attached-object reader validates the nominal for the
        // role (`equipped creature`, `enchanted permanent|land|creature`).
        super::recognize_attached_object_reference(cursor, Some(role))?;
        return Ok(Some(DesignatedPaymentObjects::Attached(role)));
    }
    Ok(None)
}

/// `the top card of your library`, `the top N cards of your library`, `the
/// top [PREDICATE] card of your graveyard`: the quantity, an optional card
/// predicate and the zone are independent slots; the zone decides which
/// combination it has (a library's top cards are counted, a graveyard's
/// top card is the latest one matching the predicate).
fn recognize_zone_top_objects(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DesignatedPaymentObjects> {
    cursor.consume_phrase(&["the", "top"])?;
    cursor.enter("cost.payment.zone-top.quantity");
    let (count, filter) = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
    {
        cursor.consume("card")?;
        (1, None)
    } else if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("cards"))
    {
        let NumericAtom::Integer(count) = super::recognize_numeric_atom(cursor)? else {
            return Err(cursor.fail("a literal number of top cards"));
        };
        cursor.require(count > 1, "a literal number of top cards above one")?;
        cursor.enter("cost.payment.zone-top.noun");
        cursor.consume("cards")?;
        (count, None)
    } else {
        cursor.enter("cost.payment.zone-top.predicate");
        let filter = recognize_search_card_selector(cursor, false)?;
        cursor.require(
            filter.supports_domain(ObjectKind::Card),
            "a card predicate for the top card of a zone",
        )?;
        (1, Some(filter))
    };
    cursor.enter("cost.payment.zone-top.zone");
    cursor.consume_phrase(&["of", "your"])?;
    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("library") => {
            cursor.consume("library")?;
            cursor.enter("cost.payment.zone-top.library");
            // A library is hidden: its top cards are counted, never matched
            // against a predicate before they are seen.
            cursor.require(
                filter.is_none(),
                "the top cards of a library without a card predicate",
            )?;
            Ok(DesignatedPaymentObjects::LibraryTop { count })
        }
        Some("graveyard") => {
            cursor.consume("graveyard")?;
            cursor.enter("cost.payment.zone-top.graveyard");
            cursor.require(count == 1, "the one top card of a graveyard")?;
            Ok(DesignatedPaymentObjects::GraveyardTop {
                filter: filter.unwrap_or(ObjectPredicate::ObjectKind(ObjectKind::Card)),
            })
        }
        _ => Err(cursor.fail("`library` or `graveyard` after `of your`")),
    }
}

/// The card's own name at the cursor, linked by the name resolver and
/// installed by the cost parent (CR 201.5: a card's name in its text names
/// that object).
fn source_name_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let names = cursor.source_names();
    !names.is_empty() && super::source_reference_starts_at(cursor.position(), None, names, None)
}

fn source_object_starts(cursor: &OracleTokenCursor<'_>, antecedent: SourceAntecedent) -> bool {
    source_name_starts(cursor)
        || cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
        || (antecedent != SourceAntecedent::Absent
            && cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("it")))
}

/// The source of the cost, named by the shared source reference reader
/// (`this NOUN`) or resumed by `it` after an earlier action of the same list
/// named it. The noun decides only the object domain; the verb decides
/// whether that domain is payable (`exile this artifact` exiles the source
/// permanent, `discard this card` the source card).
fn recognize_source_object(
    cursor: &mut OracleTokenCursor<'_>,
    domain: OperandDomain,
    admits_permanent: bool,
    antecedent: SourceAntecedent,
) -> MachineResult<PaymentSourceNoun> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
    {
        cursor.enter("cost.payment.source.pronoun");
        let SourceAntecedent::Named(noun) = antecedent else {
            return Err(cursor.fail("an earlier cost action naming the source for `it`"));
        };
        cursor.consume("it")?;
        return source_noun_in_domain(cursor, noun, domain, admits_permanent);
    }
    if source_name_starts(cursor) {
        cursor.enter("cost.payment.source.name");
        let names = cursor.source_names().to_vec();
        recognize_source_reference(cursor, &names, None)?;
        // A name carries no noun: the verb's object domain names the source.
        // Exile reads the source permanent unless a card zone follows
        // (`Exile NAME from your graveyard`).
        let names_card_zone = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("from"));
        return Ok(match domain {
            OperandDomain::Permanent => PaymentSourceNoun::Permanent,
            OperandDomain::Card if admits_permanent && !names_card_zone => {
                PaymentSourceNoun::Permanent
            }
            OperandDomain::Card => PaymentSourceNoun::Card,
        });
    }
    cursor.enter("cost.payment.source.reference");
    let noun_raw = cursor.peek_ahead(1).map(str::to_owned);
    let reference_start = cursor.checkpoint();
    recognize_source_reference(cursor, &[], None)?;
    cursor.enter("cost.payment.source.noun");
    let noun_raw = cursor.require_value(noun_raw, "a source noun after `this`")?;
    let noun = if lookup_object_domain(&noun_raw, AcceptedForms::SingularOnly)
        == Some(ObjectDomain::Card)
    {
        PaymentSourceNoun::Card
    } else if lookup_object_domain(&noun_raw, AcceptedForms::SingularOnly)
        != Some(ObjectDomain::Spell)
        && !lookup_card_type(&noun_raw, AcceptedForms::SingularOnly)
            .is_some_and(|kind| !kind.may_appear_on_permanent())
    {
        PaymentSourceNoun::Permanent
    } else {
        // A spell or a nonpermanent card type never names a payable
        // battlefield source (CR 110.4a). The refusal is located at the
        // noun the shared reference reader accepted.
        cursor.rollback(reference_start);
        cursor.advance_to(cursor.position() + 1)?;
        cursor.enter("cost.payment.source.noun");
        return Err(cursor.fail("a source noun naming a permanent or `this card`"));
    };
    source_noun_in_domain(cursor, noun, domain, admits_permanent)
}

fn source_noun_in_domain(
    cursor: &mut OracleTokenCursor<'_>,
    noun: PaymentSourceNoun,
    domain: OperandDomain,
    admits_permanent: bool,
) -> MachineResult<PaymentSourceNoun> {
    cursor.enter("cost.payment.source.domain");
    match (noun, domain) {
        (PaymentSourceNoun::Permanent, OperandDomain::Permanent)
        | (PaymentSourceNoun::Card, OperandDomain::Card) => Ok(noun),
        // `exile this artifact`: the verb takes cards and permanents.
        (PaymentSourceNoun::Permanent, OperandDomain::Card) if admits_permanent => Ok(noun),
        (_, OperandDomain::Card) => Err(cursor.fail("`this card` for this card action")),
        (_, OperandDomain::Permanent) => {
            Err(cursor.fail("a source noun naming a permanent for this permanent action"))
        }
    }
}

fn recognize_chosen_objects(
    cursor: &mut OracleTokenCursor<'_>,
    verb: PaymentVerb,
) -> MachineResult<ChosenPaymentObjectsNode> {
    cursor.enter("cost.payment.quantity");
    cursor.require(
        !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target")),
        "objects chosen while paying (a cost never targets, CR 601.2h)",
    )?;
    let another = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("another"));
    let count = if another {
        cursor.consume("another")?;
        CostQuantity::Exactly(NumericAtom::Integer(1))
    } else {
        recognize_cost_quantity(
            cursor,
            "`a`, `an`, `another`, X, a positive quantity or `any number of`",
        )?
    };
    // `two other untapped creatures`: `other` carries the same exclusion.
    let other = !another
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("other"));
    if other {
        cursor.consume("other")?;
    }
    let exclude_source = another || other;
    if verb == PaymentVerb::Tap {
        // Only untapped permanents can be tapped (CR 701.26a): the written
        // status is redundant evidence, never a filter to drop elsewhere.
        // (An untap cost keeps its `tapped` status in the shared query.)
        cursor.enter("cost.payment.tap.status");
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("untapped"))
        {
            cursor.consume("untapped")?;
        }
    }
    let filter =
        match verb.domain() {
            OperandDomain::Permanent => PaymentObjectFilter::Permanents(
                recognize_permanent_operand(cursor, verb, exclude_source)?,
            ),
            OperandDomain::Card => {
                cursor.enter("cost.payment.card-query");
                let filter = recognize_search_card_selector(cursor, count.is_plural())?;
                cursor.require(
                    filter.supports_domain(ObjectKind::Card),
                    "a card-domain payment operand",
                )?;
                PaymentObjectFilter::Cards(filter)
            }
        };
    cursor.enter("cost.payment.selection-method");
    let method = match verb.domain() {
        OperandDomain::Card => recognize_postfix_selection_method(cursor)?,
        OperandDomain::Permanent => CollectionSelectionMethod::Choice,
    };
    Ok(ChosenPaymentObjectsNode {
        count,
        exclude_source,
        filter,
        method,
    })
}

fn recognize_permanent_operand(
    cursor: &mut OracleTokenCursor<'_>,
    verb: PaymentVerb,
    exclude_source: bool,
) -> MachineResult<ObjectQueryNode> {
    cursor.enter("cost.payment.object-query");
    let query_start = cursor.position();
    let object = recognize_object_query(cursor)?;
    // `another X or a Y`: each branch has its own article, so one source
    // exclusion cannot be scoped to the whole query.
    cursor.enter("cost.payment.branch-article");
    let span = cursor.token_slice(query_start, cursor.position());
    let mixed_branch_article = span.windows(2).any(|pair| {
        pair[0].raw.eq_ignore_ascii_case("or")
            && ["a", "an", "another"]
                .iter()
                .any(|word| pair[1].raw.eq_ignore_ascii_case(word))
    });
    cursor.require(
        !(exclude_source && mixed_branch_article),
        "one source exclusion scoped to the whole object query",
    )?;
    match verb {
        PaymentVerb::Tap
        | PaymentVerb::Untap
        | PaymentVerb::Remove
        | PaymentVerb::Return
        | PaymentVerb::Exert => {
            // A tapped-permanent or counter-holder cost states its
            // payer-controlled scope: without `you control` the text names
            // any permanent, which a payer cannot be assumed to own.
            cursor.enter(match verb {
                PaymentVerb::Tap => "cost.payment.tap.domain",
                PaymentVerb::Remove => "cost.payment.remove.holder-domain",
                _ => "cost.payment.controlled.domain",
            });
            cursor.require(
                object.controller == Some(ObjectControllerScope::AbilityController)
                    && object.predicate.supports_domain(ObjectKind::Permanent)
                    && object.predicate.has_nominal_head(),
                "battlefield permanents with a nominal head controlled by the payer (`you control`)",
            )?;
        }
        _ => {
            // A player can sacrifice only permanents they control (CR 701.17a).
            cursor.enter("cost.payment.sacrifice.controller");
            cursor.require(
                matches!(
                    object.controller,
                    None | Some(ObjectControllerScope::AbilityController)
                ),
                "a permanent controlled by the payer",
            )?;
            cursor.enter("cost.payment.sacrifice.permanent-domain");
            cursor.require(
                object.predicate.supports_domain(ObjectKind::Permanent)
                    && object.predicate.has_nominal_head(),
                "a battlefield-permanent predicate with a nominal head",
            )?;
        }
    }
    Ok(object)
}

/// `N KIND counter(s) on CounterHolder` after `put` (the kind is named).
fn recognize_counter_placement(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: SourceAntecedent,
) -> MachineResult<PutCountersPaymentNode> {
    cursor.enter("cost.payment.put.quantity");
    let count = positive_quantity(cursor, "`a`, `an`, X or a positive counter quantity")?;
    cursor.enter("cost.payment.put.kind");
    let counter = recognize_counter_name(cursor)?;
    cursor.enter("cost.payment.put.noun");
    cursor.consume(counter_noun(count))?;
    cursor.enter("cost.payment.put.relation");
    cursor.consume("on")?;
    cursor.enter("cost.payment.put.holder");
    let holder = recognize_counter_holder(cursor, antecedent)?;
    cursor.require(
        !matches!(holder, CounterHolderNode::Among(_)),
        "`this PERMANENT` or `a PERMANENT you control` receiving the counters",
    )?;
    Ok(PutCountersPaymentNode {
        count,
        counter,
        holder,
    })
}

/// `Quantity [COUNTER_NAME] counter(s) from CounterHolder` after `remove`.
fn recognize_counter_removal(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: SourceAntecedent,
) -> MachineResult<RemoveCountersPaymentNode> {
    cursor.enter("cost.payment.remove.quantity");
    let count = recognize_cost_quantity(
        cursor,
        "`a`, `an`, X, a positive counter quantity or `any number of`",
    )?;
    cursor.enter("cost.payment.remove.kind");
    let is_counter_noun =
        |raw: &str| raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters");
    let counter = if cursor.peek().is_some_and(is_counter_noun) {
        None
    } else {
        Some(recognize_counter_name(cursor)?)
    };
    cursor.enter("cost.payment.remove.noun");
    cursor.consume(if count.is_plural() {
        "counters"
    } else {
        "counter"
    })?;
    cursor.enter("cost.payment.remove.relation");
    cursor.consume("from")?;
    cursor.enter("cost.payment.remove.holder");
    let holder = recognize_counter_holder(cursor, antecedent)?;
    Ok(RemoveCountersPaymentNode {
        count,
        counter,
        holder,
    })
}

/// `counter` agrees with a literal one, `counters` with any other quantity.
fn counter_noun(count: NumericAtom) -> &'static str {
    if quantity_is_plural(count) {
        "counters"
    } else {
        "counter"
    }
}

/// `this PERMANENT` | `it` | `a PERMANENT you control` | `among PERMANENTS you control`.
fn recognize_counter_holder(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: SourceAntecedent,
) -> MachineResult<CounterHolderNode> {
    if source_object_starts(cursor, antecedent) {
        return recognize_source_object(cursor, OperandDomain::Permanent, false, antecedent)
            .map(CounterHolderNode::Source);
    }
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a counter holder"))?;
    let holder = match head.as_str() {
        "a" | "an" => {
            cursor.consume(&head)?;
            CounterHolderNode::One(recognize_permanent_operand(
                cursor,
                PaymentVerb::Remove,
                false,
            )?)
        }
        "among" => {
            cursor.consume("among")?;
            CounterHolderNode::Among(recognize_permanent_operand(
                cursor,
                PaymentVerb::Remove,
                false,
            )?)
        }
        _ => {
            return Err(cursor.fail("`this PERMANENT`, `a PERMANENT` or `among PERMANENTS`"));
        }
    };
    Ok(holder)
}

fn recognize_optional_origin_zone(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<PaymentOriginZone>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("from"))
    {
        return Ok(None);
    }
    cursor.consume("from")?;
    cursor.consume("your")?;
    cursor.enter("cost.payment.exile.origin-zone");
    let zone = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("graveyard"))
    {
        PaymentOriginZone::Graveyard
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("hand"))
    {
        PaymentOriginZone::Hand
    } else {
        return Err(cursor.fail("`hand` or `graveyard`"));
    };
    cursor.advance_to(cursor.position() + 1)?;
    Ok(Some(zone))
}

/// Which connectors a cost parent's list grammar admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CostListGrammar {
    pub comma: bool,
    pub and: bool,
}

/// Reads a list of payment actions. A connector always requires another
/// action; the parent owns what may follow the list.
pub(in crate::oracle::canonical) fn recognize_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    grammar: CostListGrammar,
) -> MachineResult<CostSequenceNode<PaymentActionNode>> {
    recognize_conjugated_cost_sequence(cursor, grammar, ActingSubject::CONTROLLER)
}

/// The same list with every verb conjugated for the payer the parent names.
///
/// A conjunct without its own verb is a gapped conjunct of the previous
/// written action (`Sacrifice a Swamp and a Forest`, `pay {1} and 1 life`,
/// `Exile two cards from your graveyard and this creature`): the elided verb
/// is the previous action's lemma, read over the same tokens with that verb
/// written in the connector's slot, so the conjunct is the same ordinary
/// payment action with its own operand (CR 118.3: every part is paid). A
/// serial comma before the final `and` (`A, B, and C`) is one connector.
pub(in crate::oracle::canonical::automata) fn recognize_conjugated_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    grammar: CostListGrammar,
    form: ActingSubject,
) -> MachineResult<CostSequenceNode<PaymentActionNode>> {
    let mut costs: Vec<PaymentActionNode> = Vec::new();
    let mut connectors = Vec::new();
    // The list owns pronoun scope: `it` resumes the latest named source.
    let mut antecedent = SourceAntecedent::Absent;
    // The lemma the latest written verb of the list named; a gapped
    // conjunct elides it. Mana and `{T}` heads write no verb.
    let mut written_lemma: Option<VerbLemma> = None;
    // The checkpoint at the connector token that writes the elided verb.
    let mut gap: Option<super::super::machine::MachineCheckpoint> = None;
    cursor.enter("cost.sequence.first");
    loop {
        let action_start = cursor.position();
        let action = match (gap.take(), written_lemma) {
            (Some(slot), Some(lemma)) => {
                recognize_gapped_payment_action(cursor, form, antecedent, slot, lemma)?
            }
            _ => {
                let action = recognize_listed_payment_action(cursor, form, antecedent)?;
                written_lemma = written_payment_lemma(cursor, action_start);
                action
            }
        };
        if let Some(noun) = action.named_source() {
            antecedent = SourceAntecedent::Named(noun);
        }
        costs.push(action);
        cursor.enter("cost.sequence.connector");
        let serial_and = cursor.peek() == Some(",")
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("and"));
        let (connector, width) = match cursor.peek() {
            Some(",") if grammar.comma && grammar.and && serial_and => (CostConnector::And, 2),
            Some(",") if grammar.comma => (CostConnector::Comma, 1),
            Some(raw) if grammar.and && raw.eq_ignore_ascii_case("and") => (CostConnector::And, 1),
            _ => {
                cursor.enter("cost.sequence.complete");
                return Ok(CostSequenceNode { costs, connectors });
            }
        };
        // The connector's last token is the slot a gapped verb is written in.
        cursor.advance_to(cursor.position() + width - 1)?;
        let slot = cursor.checkpoint();
        cursor.advance_to(cursor.position() + 1)?;
        connectors.push(connector);
        cursor.enter("cost.sequence.next");
        if written_lemma.is_some() && gapped_conjunct_starts(cursor, form) {
            gap = Some(slot);
        }
    }
}

/// The lexicon lemma written at `start` when the action read from there
/// opened with a cost verb (`sacrifice`, `pay`, ...). Mana and `{T}` heads
/// have none.
fn written_payment_lemma(cursor: &OracleTokenCursor<'_>, start: usize) -> Option<VerbLemma> {
    let token = cursor.token_slice(start, start + 1).first()?;
    if token.token_type == "manaSymbol" || token.token_type == "tapSymbol" {
        return None;
    }
    super::lookup_verb(&token.raw, COST_LEMMAS).map(|(entry, _)| entry.lemma)
}

/// Lookahead only: after a connector, an operand without its own verb. A
/// payment head or any other lexicon verb (`and draw a card`) is never a
/// gapped conjunct: its own reader keeps it.
fn gapped_conjunct_starts(cursor: &OracleTokenCursor<'_>, form: ActingSubject) -> bool {
    !conjugated_payment_action_starts(cursor, form)
        && !cursor
            .peek()
            .is_some_and(|raw| super::lookup_effect_verb(raw).is_some())
        && cursor.peek().is_some_and(|raw| {
            raw != "." && raw != "," && raw != ":" && !raw.eq_ignore_ascii_case("then")
        })
}

/// Reads the gapped conjunct after a connector: the shared payment action
/// over a view of the same tokens whose connector slot writes the elided
/// verb in the payer's agreement. Every token keeps its index and span, so
/// the conjunct's diagnostics locate the written text.
fn recognize_gapped_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: ActingSubject,
    antecedent: SourceAntecedent,
    slot: super::super::machine::MachineCheckpoint,
    lemma: VerbLemma,
) -> MachineResult<PaymentActionNode> {
    cursor.enter("cost.sequence.gapped-verb");
    let end = cursor.position();
    let mut view_slot = cursor.clone();
    view_slot.rollback(slot);
    let tokens = super::elided_verb_tokens_in_form(
        &view_slot,
        view_slot.position(),
        lemma,
        form.agreement.form(),
    );
    let mut view = view_slot.with_tokens(&tokens);
    let action = recognize_listed_payment_action(&mut view, form, antecedent)?;
    cursor.require(
        view.position() > end,
        "an operand of the gapped payment verb",
    )?;
    cursor.advance_to(view.position())?;
    Ok(action)
}

#[cfg(test)]
#[path = "payment_action_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "payment_cost_context_tests.rs"]
mod cost_context_tests;

#[cfg(test)]
#[path = "payment_clause_tests.rs"]
mod clause_tests;

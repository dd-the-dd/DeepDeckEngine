//! A cost is a sequence of ordinary actions (CR 118.1, 601.2f-h, 602.2b).
//!
//! ```text
//! CostSequence  := PaymentAction (Connector PaymentAction)*
//! Connector     := "," | "and"              (the parent chooses the set)
//! PaymentAction := MANA                      | "pay" (MANA | N "life")
//!                | "{T}"                     | "tap" Selection
//!                | "sacrifice" Selection     | "discard" Selection
//!                | "exile" Selection ["from" "your" ZONE]
//!                | "blight" N
//!                | "remove" Quantity [COUNTER_NAME] ("counter" | "counters")
//!                  "from" CounterHolder
//! CounterHolder := "this" SOURCE_NOUN | ("a" | "an") QUERY | "among" QUERY
//! Selection     := "this" SOURCE_NOUN
//!                | Quantity ["other"] ["untapped"] QUERY ["at" "random"]
//! Quantity      := "a" | "an" | NUMBER | "another"
//! ```
//!
//! A parent whose payer is not the ability controller reads the same actions
//! conjugated in the third person (`pays`, `sacrifices … [of their choice]`,
//! `discards`, `taps`, `exiles`, `blights`, `removes`).
//!
//! The verb heads, the mana/life payment, the object query and the card
//! selector are the shared children used by effects. This module only composes
//! them into one typed `PaymentActionNode`; it decides no cost semantics. The
//! cost parent (activation, resolution, additional cost…) fixes the payer,
//! all-or-nothing payment (CR 118.3), choices made while paying (CR 601.2h,
//! not targets) and the source zone implied by `this card` actions. A parent
//! whose engine schema has no representation for an atom fails explicitly in
//! its own projection instead of approximating it.
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_permanent_card_type};
use crate::oracle::counter_name::CounterName;

use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::{
    DiscardVerbForm, ImperativeActionVerb, recognize_action_with_form, recognize_discard_action,
    recognize_sacrifice_action_with_form,
    selection_method::{recognize_acting_player_choice_marker, recognize_postfix_selection_method},
};
use super::{
    CollectionSelectionMethod, CostConnector, CostSequenceNode, ObjectCardType,
    ObjectControllerScope, ObjectKind, ObjectPredicate, ObjectQueryNode, ResourcePaymentNode,
    positive_literal, recognize_counter_name, recognize_mana_cost,
    recognize_object_query_before_separator, recognize_pay_life_cost,
    recognize_resource_payment_predicate, recognize_search_card_selector, recognize_tap_self_cost,
};

/// One ordinary action read in a cost section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentActionNode {
    /// `{2}{R}`, `pay {2}` or `pay N life`.
    Pay(ResourcePaymentNode),
    /// `{T}` or `tap SELECTION` (permanents the payer controls).
    Tap(PaymentObjectsNode),
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
}

/// Counters removed while paying a cost. The counter kind and the holder are
/// independent children; the cost parent decides which combinations its
/// engine schema represents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RemoveCountersPaymentNode {
    pub count: i64,
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
}

/// The typed noun confirming the source reference. It is syntax evidence; a
/// source-only payment acts on the source object itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PaymentSourceNoun {
    TapSymbol,
    PermanentType(ObjectCardType),
    Token,
    Card,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ChosenPaymentObjectsNode {
    pub count: i64,
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

const PAYMENT_VERBS: [&str; 7] = [
    "pay",
    "tap",
    "sacrifice",
    "discard",
    "exile",
    "blight",
    "remove",
];

/// Lexical lookahead only: true when the cursor is at a payment action head.
pub(in crate::oracle::canonical) fn payment_action_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek_token().is_some_and(|token| {
        token.token_type == "manaSymbol"
            || token.token_type == "tapSymbol"
            || PAYMENT_VERBS
                .iter()
                .any(|verb| token.raw.eq_ignore_ascii_case(verb))
    })
}

/// Lexical lookahead only: a payment verb conjugated for the payer
/// (`pays`, `sacrifices`, `discards`…). Mana and `{T}` heads are imperative.
pub(in crate::oracle::canonical::automata) fn conjugated_payment_action_starts(
    cursor: &OracleTokenCursor<'_>,
    form: DiscardVerbForm,
) -> bool {
    match form {
        DiscardVerbForm::Imperative => payment_action_starts(cursor),
        DiscardVerbForm::ThirdPersonSingular => cursor.peek().is_some_and(|raw| {
            PAYMENT_VERBS
                .iter()
                .any(|verb| raw.eq_ignore_ascii_case(&format!("{verb}s")))
        }),
    }
}

/// Reads one payment action by delegating to the shared action children.
pub(in crate::oracle::canonical) fn recognize_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PaymentActionNode> {
    recognize_conjugated_payment_action(cursor, DiscardVerbForm::Imperative)
}

/// The same payment actions with the verb conjugated for the payer chosen by
/// the cost parent (`unless that player sacrifices a creature`). A third-person
/// object choice may state its chooser (`of their choice`): the payer chooses
/// while paying either way (CR 601.2h), so the marker adds no semantics.
pub(in crate::oracle::canonical::automata) fn recognize_conjugated_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: DiscardVerbForm,
) -> MachineResult<PaymentActionNode> {
    cursor.enter("cost.payment.verb");
    let Some(token) = cursor.peek_token() else {
        return Err(cursor.fail("a payment action"));
    };
    if form == DiscardVerbForm::ThirdPersonSingular {
        return recognize_third_person_payment_action(cursor);
    }
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
    let verb = token.raw.to_ascii_lowercase();
    let form = DiscardVerbForm::Imperative;
    match verb.as_str() {
        "pay" => {
            let checkpoint = cursor.checkpoint();
            cursor.consume("pay")?;
            if cursor
                .peek_token()
                .is_some_and(|token| token.token_type == "manaSymbol")
            {
                cursor.enter("cost.payment.pay.mana");
                recognize_mana_cost(cursor)
                    .map(|mana| PaymentActionNode::Pay(ResourcePaymentNode::Mana(mana)))
            } else {
                cursor.rollback(checkpoint);
                cursor.enter("cost.payment.pay.life");
                recognize_pay_life_cost(cursor)
                    .map(|life| PaymentActionNode::Pay(ResourcePaymentNode::Life(life)))
            }
        }
        _ => recognize_object_payment_action(cursor, &verb, form).unwrap_or_else(|| {
            Err(cursor.fail(
                "a payment action: mana, `pay`, `{T}`, `tap`, `sacrifice`, `discard`, `exile`, `blight` or `remove`",
            ))
        }),
    }
}

/// `pays MANA | pays N life` or a conjugated object action.
fn recognize_third_person_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PaymentActionNode> {
    let raw = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if raw == "pays" {
        return recognize_resource_payment_predicate(cursor).map(PaymentActionNode::Pay);
    }
    let base = raw.strip_suffix('s').unwrap_or_default().to_string();
    recognize_object_payment_action(cursor, &base, DiscardVerbForm::ThirdPersonSingular)
        .unwrap_or_else(|| {
            Err(cursor.fail(
                "a conjugated payment action: `pays`, `taps`, `sacrifices`, `discards`, `exiles`, `blights` or `removes`",
            ))
        })
}

/// One action on objects or counters, its verb conjugated by `form`. `None`
/// when `verb` names no such payment action.
fn recognize_object_payment_action(
    cursor: &mut OracleTokenCursor<'_>,
    verb: &str,
    form: DiscardVerbForm,
) -> Option<MachineResult<PaymentActionNode>> {
    let chooser_marker = |cursor: &mut OracleTokenCursor<'_>| {
        if form == DiscardVerbForm::ThirdPersonSingular {
            recognize_acting_player_choice_marker(cursor)?;
        }
        Ok(())
    };
    Some(match verb {
        "tap" => recognize_action_with_form(cursor, ImperativeActionVerb::Tap, form, |cursor| {
            recognize_payment_objects(cursor, PaymentVerb::Tap)
        })
        .map(PaymentActionNode::Tap),
        "sacrifice" => recognize_sacrifice_action_with_form(cursor, form, |cursor| {
            let objects = recognize_payment_objects(cursor, PaymentVerb::Sacrifice)?;
            chooser_marker(cursor)?;
            Ok(objects)
        })
        .map(|action| PaymentActionNode::Sacrifice(action.operand)),
        "discard" => recognize_discard_action(cursor, form, |cursor| {
            let objects = recognize_payment_objects(cursor, PaymentVerb::Discard)?;
            chooser_marker(cursor)?;
            Ok(objects)
        })
        .map(|action| PaymentActionNode::Discard(action.operand)),
        "exile" => {
            recognize_action_with_form(cursor, ImperativeActionVerb::Exile, form, |cursor| {
                let objects = recognize_payment_objects(cursor, PaymentVerb::Exile)?;
                cursor.enter("cost.payment.exile.origin");
                let origin = recognize_optional_origin_zone(cursor)?;
                cursor.require(
                    origin.is_some() || matches!(objects, PaymentObjectsNode::Source(_)),
                    "`from your hand` or `from your graveyard` after the exiled cards",
                )?;
                Ok(PaymentActionNode::Exile { objects, origin })
            })
        }
        "blight" => {
            recognize_action_with_form(cursor, ImperativeActionVerb::Blight, form, |cursor| {
                cursor.enter("cost.payment.blight.amount");
                positive_literal(cursor, "a positive literal Blight amount")
            })
            .map(|amount| PaymentActionNode::Blight { amount })
        }
        "remove" => recognize_action_with_form(
            cursor,
            ImperativeActionVerb::Remove,
            form,
            recognize_counter_removal,
        )
        .map(PaymentActionNode::RemoveCounters),
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaymentVerb {
    Tap,
    Sacrifice,
    Discard,
    Exile,
    Remove,
}

impl PaymentVerb {
    fn domain(self) -> OperandDomain {
        match self {
            Self::Tap | Self::Sacrifice | Self::Remove => OperandDomain::Permanent,
            Self::Discard | Self::Exile => OperandDomain::Card,
        }
    }
}

/// The shared selection operand: a source reference or chosen objects.
fn recognize_payment_objects(
    cursor: &mut OracleTokenCursor<'_>,
    verb: PaymentVerb,
) -> MachineResult<PaymentObjectsNode> {
    cursor.enter("cost.payment.selection");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        cursor.consume("this")?;
        cursor.enter("cost.payment.source.noun");
        return recognize_source_noun(cursor, verb.domain()).map(PaymentObjectsNode::Source);
    }
    recognize_chosen_objects(cursor, verb).map(PaymentObjectsNode::Chosen)
}

fn recognize_source_noun(
    cursor: &mut OracleTokenCursor<'_>,
    domain: OperandDomain,
) -> MachineResult<PaymentSourceNoun> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a source noun after `this`"))?;
    let noun = match domain {
        OperandDomain::Card if raw.eq_ignore_ascii_case("card") => PaymentSourceNoun::Card,
        OperandDomain::Card => return Err(cursor.fail("`card` after `this`")),
        OperandDomain::Permanent if raw.eq_ignore_ascii_case("token") => PaymentSourceNoun::Token,
        OperandDomain::Permanent => lookup_permanent_card_type(raw, AcceptedForms::SingularOnly)
            .map(PaymentSourceNoun::PermanentType)
            .ok_or_else(|| cursor.fail("a permanent type or `token` after `this`"))?,
    };
    cursor.advance_to(cursor.position() + 1)?;
    Ok(noun)
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
        1
    } else {
        positive_literal(
            cursor,
            "`a`, `an`, `another` or a positive literal quantity",
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
                let filter = recognize_search_card_selector(cursor, count > 1)?;
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
    let object = recognize_object_query_before_separator(cursor)?;
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
        PaymentVerb::Tap | PaymentVerb::Remove => {
            // A tapped-permanent or counter-holder cost states its
            // payer-controlled scope: without `you control` the text names
            // any permanent, which a payer cannot be assumed to own.
            cursor.enter(if verb == PaymentVerb::Tap {
                "cost.payment.tap.domain"
            } else {
                "cost.payment.remove.holder-domain"
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

/// `Quantity [COUNTER_NAME] counter(s) from CounterHolder` after `remove`.
fn recognize_counter_removal(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RemoveCountersPaymentNode> {
    cursor.enter("cost.payment.remove.quantity");
    let count = positive_literal(cursor, "`a`, `an` or a positive literal counter quantity")?;
    cursor.enter("cost.payment.remove.kind");
    let is_counter_noun =
        |raw: &str| raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters");
    let counter = if cursor.peek().is_some_and(is_counter_noun) {
        None
    } else {
        Some(recognize_counter_name(cursor)?)
    };
    cursor.enter("cost.payment.remove.noun");
    cursor.consume(if count == 1 { "counter" } else { "counters" })?;
    cursor.enter("cost.payment.remove.relation");
    cursor.consume("from")?;
    cursor.enter("cost.payment.remove.holder");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a counter holder"))?;
    let holder = match head.as_str() {
        "this" => {
            cursor.consume("this")?;
            cursor.enter("cost.payment.source.noun");
            CounterHolderNode::Source(recognize_source_noun(cursor, OperandDomain::Permanent)?)
        }
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
    Ok(RemoveCountersPaymentNode {
        count,
        counter,
        holder,
    })
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
    recognize_conjugated_cost_sequence(cursor, grammar, DiscardVerbForm::Imperative)
}

/// The same list with every verb conjugated for the payer the parent names.
pub(in crate::oracle::canonical::automata) fn recognize_conjugated_cost_sequence(
    cursor: &mut OracleTokenCursor<'_>,
    grammar: CostListGrammar,
    form: DiscardVerbForm,
) -> MachineResult<CostSequenceNode<PaymentActionNode>> {
    let mut costs = Vec::new();
    let mut connectors = Vec::new();
    cursor.enter("cost.sequence.first");
    costs.push(recognize_conjugated_payment_action(cursor, form)?);
    loop {
        cursor.enter("cost.sequence.connector");
        let connector = match cursor.peek() {
            Some(",") if grammar.comma => CostConnector::Comma,
            Some(raw) if grammar.and && raw.eq_ignore_ascii_case("and") => CostConnector::And,
            _ => {
                cursor.enter("cost.sequence.complete");
                return Ok(CostSequenceNode { costs, connectors });
            }
        };
        cursor.advance_to(cursor.position() + 1)?;
        connectors.push(connector);
        cursor.enter("cost.sequence.next");
        costs.push(recognize_conjugated_payment_action(cursor, form)?);
    }
}

#[cfg(test)]
#[path = "payment_action_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "payment_cost_context_tests.rs"]
mod cost_context_tests;

//! Cycling family (CR 702.29). `Cycling [cost]` (702.29a) and
//! `[Type]cycling [cost]` (702.29e) share one activated-cost envelope with a
//! hand-zone discard-self cost. Plain cycling expands to the controller drawing
//! one card; typecycling expands to a controller library search whose card
//! predicate comes from the shared search-card selector. The optional reminder
//! is validated against the expansion through the shared draw/search children,
//! never lowered separately.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    CardZoneName, CollectionBinding, DrawCardsEffectNode, LibrarySearchEffectNode,
    LocalActivationCostNode, LocalPlayerReference, ManaCostNode, NumericAtom, NumericExpression,
    ObjectPredicate, SearchDestination, SearchShuffleOrder, recognize_activation_cost_sequence,
    recognize_controller_library_search, recognize_draw_cards_effect,
    recognize_search_card_selector,
};
use crate::oracle::model::RecognizedEntity;
use crate::oracle::syntax::recognize_oracle_entities;

const CYCLING: &str = "cycling";
const MAX_LABEL_WORDS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TypecyclingKeywordNode {
    pub cost: ManaCostNode,
    pub expansion: CyclingExpansion,
}

/// The effect of the activated ability a cycling keyword expands to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CyclingExpansion {
    /// CR 702.29a: draw a card.
    Draw(DrawCardsEffectNode),
    /// CR 702.29e: search for a card of the labelled kind.
    Search(LibrarySearchEffectNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TypecyclingState {
    Label,
    Cost,
    CostSupport,
    Envelope,
    ReminderCost,
    ReminderCostSupport,
    ReminderSeparator,
    ReminderSearch,
    ReminderSearchSupport,
    ReminderTerminator,
    ReminderClose,
    Complete,
}

/// True when a label word ending in `cycling` precedes the cost and names at
/// least one type word (`Cycling` alone is the plain keyword).
pub(in crate::oracle::canonical) fn typecycling_label_starts(tokens: &[RecognizedEntity]) -> bool {
    for (index, token) in tokens.iter().take(MAX_LABEL_WORDS).enumerate() {
        if token.token_type == "manaSymbol" || token.token_type == "separator" {
            return false;
        }
        let raw = token.raw.to_ascii_lowercase();
        if raw.ends_with(CYCLING) {
            return index > 0 || raw.len() > CYCLING.len();
        }
    }
    false
}

/// True for the unlabelled keyword: `Cycling` immediately followed by a mana
/// cost (CR 702.29a). Nonmana cycling costs are not claimed here.
pub(in crate::oracle::canonical) fn plain_cycling_starts(tokens: &[RecognizedEntity]) -> bool {
    matches!(
        tokens,
        [keyword, cost, ..]
            if keyword.raw.eq_ignore_ascii_case(CYCLING) && cost.token_type == "manaSymbol"
    )
}

fn label_failure(cursor: &OracleTokenCursor<'_>, child: MachineFailure) -> MachineFailure {
    cursor.fail(&format!(
        "a card selector for the typecycling label ({})",
        child.expected
    ))
}

pub(in crate::oracle::canonical) fn recognize_typecycling_keyword(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TypecyclingKeywordNode> {
    let mut state = TypecyclingState::Label;
    let mut label_words: Vec<String> = Vec::new();
    let mut label_filter: Option<ObjectPredicate> = None;
    let mut costs = None;
    let mut printed_mana: Option<ManaCostNode> = None;
    let mut reminder_costs = None;
    let mut reminder_search: Option<LibrarySearchEffectNode> = None;
    let mut reminder_draw: Option<DrawCardsEffectNode> = None;
    loop {
        state = match state {
            TypecyclingState::Label => {
                cursor.enter("keyword.typecycling.label");
                loop {
                    let token = cursor
                        .peek_token()
                        .ok_or_else(|| cursor.fail("a type label ending in `cycling`"))?;
                    let raw = token.raw.clone();
                    let token_type = token.token_type.clone();
                    cursor.require(
                        token_type != "manaSymbol"
                            && token_type != "separator"
                            && label_words.len() < MAX_LABEL_WORDS,
                        "a type label ending in `cycling`",
                    )?;
                    cursor.consume(&raw)?;
                    let lowered = raw.to_ascii_lowercase();
                    if let Some(stem) = lowered.strip_suffix(CYCLING) {
                        if !stem.is_empty() {
                            label_words.push(raw[..stem.len()].to_string());
                        }
                        break;
                    }
                    label_words.push(raw);
                }
                if label_words.is_empty() {
                    // Plain cycling (CR 702.29a): no searched-card label.
                    cursor.enter("keyword.cycling.plain");
                    cursor.require(
                        cursor
                            .peek_token()
                            .is_some_and(|token| token.token_type == "manaSymbol"),
                        "a mana cost after `cycling`",
                    )?;
                    state = TypecyclingState::Cost;
                    continue;
                }
                let synthetic =
                    recognize_oracle_entities(&format!("{} card", label_words.join(" ")));
                let mut label =
                    OracleTokenCursor::new("keyword.typecycling.label-selector", &synthetic);
                let filter = recognize_search_card_selector(&mut label, false)
                    .map_err(|child| label_failure(cursor, child))?;
                label
                    .finish()
                    .map_err(|child| label_failure(cursor, child))?;
                label_filter = Some(filter);
                TypecyclingState::Cost
            }
            TypecyclingState::Cost => {
                cursor.enter("keyword.typecycling.cost");
                costs = Some(recognize_activation_cost_sequence(cursor)?);
                TypecyclingState::CostSupport
            }
            TypecyclingState::CostSupport => {
                cursor.enter("keyword.typecycling.cost-support");
                let costs = cursor.require_value(costs.take(), "a shared typed payment cost")?;
                let [LocalActivationCostNode::PayMana(mana)] = costs.costs.as_slice() else {
                    return Err(cursor.fail(
                        "one mana payment; nonmana typecycling costs require their own runtime proof",
                    ));
                };
                mana.fixed_runtime_mana_value()
                    .map_err(|error| cursor.fail(error.expected()))?;
                printed_mana = Some(mana.clone());
                TypecyclingState::Envelope
            }
            TypecyclingState::Envelope => {
                cursor.enter("keyword.typecycling.envelope");
                match cursor.peek() {
                    None => TypecyclingState::Complete,
                    Some("(") => {
                        cursor.consume("(")?;
                        TypecyclingState::ReminderCost
                    }
                    _ => {
                        return Err(cursor.fail(
                            "the canonical typecycling reminder or end of the keyword ability",
                        ));
                    }
                }
            }
            TypecyclingState::ReminderCost => {
                cursor.enter("keyword.typecycling.reminder.cost");
                reminder_costs = Some(recognize_activation_cost_sequence(cursor)?);
                TypecyclingState::ReminderCostSupport
            }
            TypecyclingState::ReminderCostSupport => {
                cursor.enter("keyword.typecycling.reminder.cost-support");
                let costs =
                    cursor.require_value(reminder_costs.take(), "the reminder activation costs")?;
                let printed = cursor.require_value(printed_mana.as_ref(), "the printed cost")?;
                cursor.require(
                    matches!(
                        costs.costs.as_slice(),
                        [LocalActivationCostNode::PayMana(mana), LocalActivationCostNode::DiscardSelf(_)]
                            if mana.same_symbols(printed)
                    ),
                    "the printed cost followed by discarding this card",
                )?;
                TypecyclingState::ReminderSeparator
            }
            TypecyclingState::ReminderSeparator => {
                cursor.enter("keyword.typecycling.reminder.separator");
                cursor.consume(":")?;
                TypecyclingState::ReminderSearch
            }
            TypecyclingState::ReminderSearch => {
                if label_filter.is_none() {
                    cursor.enter("keyword.cycling.reminder.draw");
                    reminder_draw = Some(recognize_draw_cards_effect(cursor)?);
                } else {
                    cursor.enter("keyword.typecycling.reminder.search");
                    reminder_search = Some(recognize_controller_library_search(cursor)?);
                }
                TypecyclingState::ReminderSearchSupport
            }
            TypecyclingState::ReminderSearchSupport if label_filter.is_none() => {
                cursor.enter("keyword.cycling.reminder.draw-support");
                let draw = cursor.require_value(reminder_draw.take(), "the reminder card draw")?;
                cursor.require(
                    draw == plain_expansion_draw(),
                    "a reminder in which you draw one card",
                )?;
                TypecyclingState::ReminderTerminator
            }
            TypecyclingState::ReminderSearchSupport => {
                cursor.enter("keyword.typecycling.reminder.search-support");
                let search =
                    cursor.require_value(reminder_search.take(), "the reminder library search")?;
                let label = cursor.require_value(label_filter.as_ref(), "the label selector")?;
                cursor.require(
                    search.filter == *label,
                    "a reminder searching for the card kind named by the label",
                )?;
                cursor.require(
                    search == expansion_search(label.clone()),
                    "a reminder that reveals one card, puts it into your hand, then shuffles",
                )?;
                TypecyclingState::ReminderTerminator
            }
            TypecyclingState::ReminderTerminator => {
                cursor.enter("keyword.typecycling.reminder.terminator");
                cursor.consume(".")?;
                TypecyclingState::ReminderClose
            }
            TypecyclingState::ReminderClose => {
                cursor.enter("keyword.typecycling.reminder.close");
                cursor.consume(")")?;
                TypecyclingState::Complete
            }
            TypecyclingState::Complete => {
                cursor.enter("keyword.typecycling.complete");
                cursor.finish()?;
                let expansion = match label_filter.take() {
                    Some(filter) => CyclingExpansion::Search(expansion_search(filter)),
                    None => CyclingExpansion::Draw(plain_expansion_draw()),
                };
                return Ok(TypecyclingKeywordNode {
                    cost: cursor.require_value(printed_mana, "the retained mana cost")?,
                    expansion,
                });
            }
        };
    }
}

/// CR 702.29a: the controller draws one card.
fn plain_expansion_draw() -> DrawCardsEffectNode {
    DrawCardsEffectNode {
        player: LocalPlayerReference::Controller,
        count: NumericExpression::Atom(NumericAtom::Integer(1)),
    }
}

/// CR 702.29e: search the library for one such card, reveal it, put it into
/// the hand, then shuffle. A quality-restricted search may find nothing.
fn expansion_search(filter: ObjectPredicate) -> LibrarySearchEffectNode {
    LibrarySearchEffectNode {
        player: LocalPlayerReference::Controller,
        origin: super::SearchOrigin::library(),
        filter,
        slots: Vec::new(),
        minimum: 0,
        maximum: 1,
        count: None,
        where_x: None,
        destination: SearchDestination::Zone(CardZoneName::Hand),
        reveal: true,
        shuffle_order: SearchShuffleOrder::MoveThenShuffle,
        binding: CollectionBinding(0),
    }
}

#[cfg(test)]
#[path = "typecycling_tests.rs"]
mod tests;

use super::super::{MachineResult, OracleTokenCursor};
use super::source_sacrifice::atomic_action::player_subject::recognize_player_effect_subject;
use super::source_sacrifice::atomic_action::{ActingSubject, recognize_discard_action};
pub(in crate::oracle::canonical) use super::source_sacrifice::atomic_action::{
    player_discard::PlayerDiscardEffectNode,
    player_subject::{PlayerBinding, PlayerEffectSubject},
    selection_method::CollectionSelectionMethod,
};
use super::{
    EffectSequenceNode, ObjectPredicate, PlayerPredicate, SelectorPredicate, TargetSelector,
    recognize_effect_sequence,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::{
    characteristic_vocabulary::{AcceptedForms, lookup_card_type},
    syntax::{NameResolutionFailure, ResolvedNameLink},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CardSelectionBinding(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RevealedHandNode {
    pub(in crate::oracle::canonical) target: PlayerPredicate,
    pub(in crate::oracle::canonical) player: PlayerBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct HandCardChoiceNode {
    pub(in crate::oracle::canonical) hand_owner: PlayerBinding,
    pub(in crate::oracle::canonical) filter: ObjectPredicate,
    pub(in crate::oracle::canonical) selection: CardSelectionBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DiscardChosenCardNode {
    pub(in crate::oracle::canonical) player: PlayerBinding,
    pub(in crate::oracle::canonical) card: CardSelectionBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedHandDisruptionNode {
    pub(in crate::oracle::canonical) reveal: RevealedHandNode,
    pub(in crate::oracle::canonical) choice: HandCardChoiceNode,
    pub(in crate::oracle::canonical) discard: DiscardChosenCardNode,
    pub(in crate::oracle::canonical) followup: Option<EffectSequenceNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetedHandDisruptionState {
    Core,
    CoreTerminator,
    FollowupOrComplete,
    Followup,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandDisruptionCoreState {
    Reveal,
    RevealTerminator,
    Choice,
    ChoiceTerminator,
    Discard,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HandDisruptionCoreNode {
    reveal: RevealedHandNode,
    choice: HandCardChoiceNode,
    discard: DiscardChosenCardNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RevealHandState {
    Target,
    Action,
    Possessive,
    Zone,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandChoiceState {
    Chooser,
    Action,
    Determiner,
    Filter,
    CardNoun,
    SourceRelation,
    SourceReference,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DiscardChosenCardState {
    Player,
    Action,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandCardSelectorState {
    FirstAtom,
    Continuation,
    Rhs,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FilterConnector {
    Intersection,
    Union,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct HandCardSelectorFrame {
    union_branches: Vec<SelectorPredicate>,
    intersection: Vec<SelectorPredicate>,
}

impl HandCardSelectorFrame {
    fn finish_intersection(&mut self) -> Option<()> {
        let predicate = match self.intersection.len() {
            0 => return None,
            1 => self.intersection.pop().expect("one hand-card predicate"),
            _ => SelectorPredicate::Intersection(std::mem::take(&mut self.intersection)),
        };
        self.union_branches.push(predicate);
        Some(())
    }

    fn finish(mut self) -> Option<SelectorPredicate> {
        self.finish_intersection()?;
        Some(match self.union_branches.len() {
            1 => self.union_branches.pop().expect("one hand-card branch"),
            _ => SelectorPredicate::Union(self.union_branches),
        })
    }
}

/// Composes the bound reveal/selection/discard core with any trailing local
/// effect sequence. The core owns its anaphoric bindings; the suffix remains a
/// generic sequence of independently recognized effects.
pub(in crate::oracle::canonical) fn recognize_targeted_hand_disruption(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<TargetedHandDisruptionNode> {
    let mut state = TargetedHandDisruptionState::Core;
    let mut core = None;
    let mut followup = None;

    loop {
        state = match state {
            TargetedHandDisruptionState::Core => {
                cursor.enter("hand-disruption.core");
                core = Some(recognize_hand_disruption_core(cursor)?);
                TargetedHandDisruptionState::CoreTerminator
            }
            TargetedHandDisruptionState::CoreTerminator => {
                cursor.enter("hand-disruption.core.terminator");
                cursor.consume(".")?;
                TargetedHandDisruptionState::FollowupOrComplete
            }
            TargetedHandDisruptionState::FollowupOrComplete => {
                cursor.enter("hand-disruption.followup-or-complete");
                if cursor.peek().is_some() {
                    TargetedHandDisruptionState::Followup
                } else {
                    TargetedHandDisruptionState::Complete
                }
            }
            TargetedHandDisruptionState::Followup => {
                cursor.enter("hand-disruption.followup");
                followup = Some(recognize_effect_sequence(cursor, name_links, name_failure)?);
                TargetedHandDisruptionState::Complete
            }
            TargetedHandDisruptionState::Complete => {
                cursor.enter("hand-disruption.complete");
                cursor.finish()?;
                let core = cursor.require_value(core, "a completed hand-disruption core")?;
                return Ok(TargetedHandDisruptionNode {
                    reveal: core.reveal,
                    choice: core.choice,
                    discard: core.discard,
                    followup,
                });
            }
        };
    }
}

/// Recognizes the reveal/selection/discard core and returns with the cursor on
/// its sentence terminator so a parent can decide whether more effects follow.
fn recognize_hand_disruption_core(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<HandDisruptionCoreNode> {
    let targeted_player = PlayerBinding(0);
    let selected_card = CardSelectionBinding(0);
    let mut state = HandDisruptionCoreState::Reveal;
    let mut reveal = None;
    let mut choice = None;
    let mut discard = None;

    loop {
        state = match state {
            HandDisruptionCoreState::Reveal => {
                cursor.enter("hand-disruption.reveal");
                reveal = Some(recognize_revealed_hand(cursor, targeted_player)?);
                HandDisruptionCoreState::RevealTerminator
            }
            HandDisruptionCoreState::RevealTerminator => {
                cursor.enter("hand-disruption.reveal.terminator");
                cursor.consume(".")?;
                HandDisruptionCoreState::Choice
            }
            HandDisruptionCoreState::Choice => {
                cursor.enter("hand-disruption.choice");
                choice = Some(recognize_card_choice_from_revealed_hand(
                    cursor,
                    targeted_player,
                    selected_card,
                )?);
                HandDisruptionCoreState::ChoiceTerminator
            }
            HandDisruptionCoreState::ChoiceTerminator => {
                cursor.enter("hand-disruption.choice.terminator");
                cursor.consume(".")?;
                HandDisruptionCoreState::Discard
            }
            HandDisruptionCoreState::Discard => {
                cursor.enter("hand-disruption.discard");
                discard = Some(recognize_discard_chosen_card(
                    cursor,
                    targeted_player,
                    selected_card,
                )?);
                HandDisruptionCoreState::Complete
            }
            HandDisruptionCoreState::Complete => {
                cursor.enter("hand-disruption.core.complete");
                return Ok(HandDisruptionCoreNode {
                    reveal: cursor.require_value(reveal, "a revealed-hand binding")?,
                    choice: cursor.require_value(choice, "a filtered hand-card selection")?,
                    discard: cursor.require_value(discard, "a discard of the selected card")?,
                });
            }
        };
    }
}

fn recognize_revealed_hand(
    cursor: &mut OracleTokenCursor<'_>,
    player: PlayerBinding,
) -> MachineResult<RevealedHandNode> {
    let mut state = RevealHandState::Target;
    let mut target = None;
    loop {
        state = match state {
            RevealHandState::Target => {
                cursor.enter("hand-disruption.reveal.target");
                let subject = recognize_player_effect_subject(cursor, player, None, false)?;
                cursor.enter("hand-disruption.reveal.target.validation");
                let PlayerEffectSubject::DeclaredTarget {
                    target: targets,
                    binding,
                } = subject
                else {
                    return Err(cursor.fail("one declared player target"));
                };
                cursor.require(binding == player, "the declared reveal-player binding")?;
                let [slot] = targets.slots.as_slice() else {
                    return Err(cursor.fail("exactly one targeted player"));
                };
                cursor.require(
                    slot.cardinality.minimum == 1 && slot.cardinality.maximum == 1,
                    "exactly one targeted player",
                )?;
                let TargetSelector::Player(predicate) = &slot.selector else {
                    return Err(cursor.fail("a player or opponent target"));
                };
                target = Some(*predicate);
                RevealHandState::Action
            }
            RevealHandState::Action => {
                cursor.enter("hand-disruption.reveal.action");
                let action = cursor.consume_type("actionWord")?;
                if !is_verb_form(
                    &action.raw,
                    VerbLemma::Reveal,
                    VerbForm::ThirdPersonSingular,
                ) {
                    return Err(cursor.fail_with_span(
                        "`reveals`",
                        action.index,
                        action.span_start,
                        action.span_end,
                        action.raw.clone(),
                    ));
                }
                RevealHandState::Possessive
            }
            RevealHandState::Possessive => {
                cursor.enter("hand-disruption.reveal.possessive");
                let reference = cursor.consume_type("referenceWord")?;
                cursor.require(reference.raw.eq_ignore_ascii_case("their"), "`their`")?;
                RevealHandState::Zone
            }
            RevealHandState::Zone => {
                cursor.enter("hand-disruption.reveal.zone");
                let zone = cursor.consume_type("zoneWord")?;
                cursor.require(zone.raw.eq_ignore_ascii_case("hand"), "`hand`")?;
                RevealHandState::Complete
            }
            RevealHandState::Complete => {
                cursor.enter("hand-disruption.reveal.complete");
                return Ok(RevealedHandNode {
                    target: cursor.require_value(target, "a targeted player predicate")?,
                    player,
                });
            }
        };
    }
}

fn recognize_card_choice_from_revealed_hand(
    cursor: &mut OracleTokenCursor<'_>,
    hand_owner: PlayerBinding,
    selection: CardSelectionBinding,
) -> MachineResult<HandCardChoiceNode> {
    let mut state = HandChoiceState::Chooser;
    let mut filter = None;
    loop {
        state = match state {
            HandChoiceState::Chooser => {
                cursor.enter("hand-disruption.choice.chooser");
                let chooser = cursor.consume_type("referenceWord")?;
                cursor.require(chooser.raw.eq_ignore_ascii_case("you"), "`you`")?;
                HandChoiceState::Action
            }
            HandChoiceState::Action => {
                cursor.enter("hand-disruption.choice.action");
                let action = cursor.consume_type("actionWord")?;
                if !action.raw.eq_ignore_ascii_case("choose") {
                    return Err(cursor.fail_with_span(
                        "`choose`",
                        action.index,
                        action.span_start,
                        action.span_end,
                        action.raw.clone(),
                    ));
                }
                HandChoiceState::Determiner
            }
            HandChoiceState::Determiner => {
                cursor.enter("hand-disruption.choice.determiner");
                let determiner = cursor.consume_type("quantifierWord")?;
                cursor.require(
                    determiner.raw.eq_ignore_ascii_case("a")
                        || determiner.raw.eq_ignore_ascii_case("an"),
                    "`a` or `an`",
                )?;
                HandChoiceState::Filter
            }
            HandChoiceState::Filter => {
                cursor.enter("hand-disruption.choice.filter");
                filter = Some(recognize_hand_card_selector(cursor)?);
                HandChoiceState::CardNoun
            }
            HandChoiceState::CardNoun => {
                cursor.enter("hand-disruption.choice.card-noun");
                cursor.consume("card")?;
                HandChoiceState::SourceRelation
            }
            HandChoiceState::SourceRelation => {
                cursor.enter("hand-disruption.choice.source-relation");
                cursor.consume("from")?;
                HandChoiceState::SourceReference
            }
            HandChoiceState::SourceReference => {
                cursor.enter("hand-disruption.choice.source-reference");
                let reference = cursor.consume_type("referenceWord")?;
                cursor.require(reference.raw.eq_ignore_ascii_case("it"), "`it`")?;
                HandChoiceState::Complete
            }
            HandChoiceState::Complete => {
                cursor.enter("hand-disruption.choice.complete");
                return Ok(HandCardChoiceNode {
                    hand_owner,
                    filter: cursor.require_value(filter, "a populated hand-card filter")?,
                    selection,
                });
            }
        };
    }
}

fn recognize_discard_chosen_card(
    cursor: &mut OracleTokenCursor<'_>,
    player: PlayerBinding,
    card: CardSelectionBinding,
) -> MachineResult<DiscardChosenCardNode> {
    let mut state = DiscardChosenCardState::Player;
    loop {
        state = match state {
            DiscardChosenCardState::Player => {
                cursor.enter("hand-disruption.discard.player");
                let subject = recognize_player_effect_subject(cursor, player, Some(player), false)?;
                cursor.require(
                    subject == PlayerEffectSubject::Bound(player),
                    "the previously targeted player binding",
                )?;
                DiscardChosenCardState::Action
            }
            DiscardChosenCardState::Action => {
                cursor.enter("hand-disruption.discard.action");
                recognize_discard_action(cursor, ActingSubject::THIRD_SINGULAR, |cursor| {
                    cursor.enter("hand-disruption.discard.card-reference");
                    let reference = cursor.consume_type("referenceWord")?;
                    cursor.require(reference.raw.eq_ignore_ascii_case("that"), "`that`")?;
                    cursor.enter("hand-disruption.discard.card-noun");
                    cursor.consume("card")?;
                    Ok(())
                })?;
                DiscardChosenCardState::Complete
            }
            DiscardChosenCardState::Complete => {
                cursor.enter("hand-disruption.discard.complete");
                return Ok(DiscardChosenCardNode { player, card });
            }
        };
    }
}

fn recognize_hand_card_selector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    let mut state = HandCardSelectorState::FirstAtom;
    let mut frame = HandCardSelectorFrame::default();
    let mut pending_connector = None;

    loop {
        state = match state {
            HandCardSelectorState::FirstAtom => {
                cursor.enter("hand-card-selector.first-atom");
                frame.intersection.push(recognize_filter_atom(cursor)?);
                HandCardSelectorState::Continuation
            }
            HandCardSelectorState::Continuation => {
                cursor.enter("hand-card-selector.continuation");
                if cursor.peek().is_some_and(|raw| raw == ",") {
                    cursor.consume(",")?;
                    pending_connector = Some(FilterConnector::Intersection);
                    HandCardSelectorState::Rhs
                } else if cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("or") || raw.eq_ignore_ascii_case("and/or")
                }) {
                    let connector = cursor.peek().expect("checked union connector").to_string();
                    cursor.consume(&connector)?;
                    frame
                        .finish_intersection()
                        .ok_or_else(|| cursor.fail("a left-hand card-filter branch"))?;
                    pending_connector = Some(FilterConnector::Union);
                    HandCardSelectorState::Rhs
                } else if filter_atom(cursor.peek()).is_some() {
                    pending_connector = Some(FilterConnector::Intersection);
                    HandCardSelectorState::Rhs
                } else {
                    HandCardSelectorState::Complete
                }
            }
            HandCardSelectorState::Rhs => {
                cursor.enter("hand-card-selector.rhs");
                let predicate = recognize_filter_atom(cursor)?;
                match pending_connector.take() {
                    Some(FilterConnector::Intersection | FilterConnector::Union) => {
                        frame.intersection.push(predicate);
                    }
                    None => return Err(cursor.fail("a card-filter connector")),
                }
                HandCardSelectorState::Continuation
            }
            HandCardSelectorState::Complete => {
                cursor.enter("hand-card-selector.complete");
                return frame
                    .finish()
                    .ok_or_else(|| cursor.fail("a populated hand-card selector"));
            }
        };
    }
}

fn recognize_filter_atom(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<SelectorPredicate> {
    let predicate = filter_atom(cursor.peek())
        .ok_or_else(|| cursor.fail("a positive or negative card type"))?;
    cursor.advance_to(cursor.position() + 1)?;
    Ok(predicate)
}

fn filter_atom(raw: Option<&str>) -> Option<SelectorPredicate> {
    let normalized = raw?.to_ascii_lowercase();
    if let Some(positive) = normalized.strip_prefix("non") {
        return positive_card_type(positive)
            .map(|predicate| SelectorPredicate::Not(Box::new(predicate)));
    }
    positive_card_type(&normalized)
}

fn positive_card_type(raw: &str) -> Option<SelectorPredicate> {
    lookup_card_type(raw, AcceptedForms::SingularOrPlural).map(SelectorPredicate::CardType)
}

use crate::oracle::canonical::automata::VerbLemma;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    DamageEffectNode, EffectRecognitionContext, EffectSequenceNode, LocalEffectNode, NumericAtom,
    SelectorPredicate, TargetBinding, TargetCardZoneChangeEffectNode, TargetPlayerScryThenDrawNode,
    TargetSelector, TargetedCreateCreatureTokensWithManaAbilityNode, TemporaryKeywordGrantNode,
    ZoneChangeEffectNode, rebase_machine_failure, rebase_name_failure, rebase_name_link,
    recognize_damage_effect, recognize_effect_sequence_with_context,
    recognize_target_card_zone_change_effect, recognize_target_player_scry_then_draw,
    recognize_targeted_create_creature_tokens_with_mana_ability,
    recognize_temporary_keyword_grant_clause, recognize_zone_change_effect,
    source_reference_starts_at,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EffectSequenceModalNode {
    pub cardinality: ModalCardinalityNode,
    pub modes: Vec<EffectSequenceNode>,
}

/// Recognizes an exact-cardinality modal envelope while delegating every
/// bullet body to the ordinary effect-sequence automaton. The parent owns only
/// the choice header and bullet boundaries.
pub(in crate::oracle::canonical) fn recognize_effect_sequence_modal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EffectSequenceModalNode> {
    recognize_effect_sequence_modal_with_context(
        cursor,
        name_links,
        name_failure,
        &EffectRecognitionContext::default(),
    )
}

/// The modes of a modal ability whose parent owns a recognition context:
/// a triggered ability's modes read the bindings its event publishes (the
/// frame, the observed object, the source attachment), like its program.
pub(in crate::oracle::canonical) fn recognize_effect_sequence_modal_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceModalNode> {
    cursor.enter("modal.effect-sequence.action");
    cursor.consume("choose")?;
    cursor.enter("modal.effect-sequence.cardinality");
    let raw_count = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a positive literal modal cardinality"))?
        .to_string();
    let count = parse_number_word(&raw_count)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| cursor.fail("a positive literal modal cardinality"))?;
    cursor.consume(&raw_count)?;
    // `N or both` / `N or more` widen the upper bound (CR 700.2); the
    // runtime requires at least one chosen mode, so the lower bound stays N.
    cursor.enter("modal.effect-sequence.cardinality-extension");
    let extension = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
    {
        cursor.consume("or")?;
        let word = cursor
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("both") || raw.eq_ignore_ascii_case("more"))
            .ok_or_else(|| cursor.fail("`or both` or `or more`"))?
            .to_ascii_lowercase();
        cursor.consume(&word)?;
        Some(word)
    } else {
        None
    };
    cursor.enter("modal.effect-sequence.header-separator");
    cursor.consume("\u{2014}")?;

    let mut modes = Vec::new();
    while cursor.peek().is_some_and(|raw| raw == "\u{2022}") {
        cursor.enter("modal.effect-sequence.mode-marker");
        cursor.consume("\u{2022}")?;
        // CR 207.2d: a flavor word (`Pray for Protection —`) labels the mode
        // and has no rules meaning; the mode body starts after its dash.
        let label_width =
            flavor_word_label_width(cursor.token_slice(cursor.position(), usize::MAX));
        if label_width > 0 {
            cursor.enter("modal.effect-sequence.mode-flavor-word");
            cursor.advance_to(cursor.position() + label_width + 1)?;
        }
        let mode_start = cursor.position();
        let mut mode_end = mode_start;
        while let Some(raw) = cursor.peek_ahead(mode_end - mode_start) {
            if raw == "\u{2022}" {
                break;
            }
            mode_end += 1;
        }
        cursor.enter("modal.effect-sequence.mode-body");
        cursor.require(mode_end > mode_start, "a nonempty modal effect sequence")?;
        let mode_links = name_links
            .iter()
            .filter_map(|link| rebase_name_link(link, mode_start, mode_end))
            .collect::<Vec<_>>();
        let mode_failure =
            name_failure.and_then(|failure| rebase_name_failure(failure, mode_start, mode_end));
        let mut mode_cursor = OracleTokenCursor::new(
            "modal-effect-sequence-mode",
            cursor.token_slice(mode_start, mode_end),
        );
        // The event's `that many` quantity reaches every mode.
        mode_cursor.set_event_numeric_binding(cursor.event_numeric_binding());
        // Every mode is lowered by the shared modal lowering, which owns
        // its `unless` frames (CR 118.12).
        let mode = recognize_effect_sequence_with_context(
            &mut mode_cursor,
            &mode_links,
            mode_failure.as_ref(),
            &context.clone().with_unless_frames(),
        )
        .map_err(|failure| rebase_machine_failure(failure, mode_start))?;
        modes.push(mode);
        cursor.advance_to(mode_end)?;
    }

    cursor.enter("modal.effect-sequence.complete");
    cursor.require(
        modes.len() >= count as usize,
        "enough distinct modes for the exact modal cardinality",
    )?;
    let maximum = match extension.as_deref() {
        None => count,
        Some("both") => {
            cursor.require(
                count == 1 && modes.len() == 2,
                "`one or both` over exactly two modes",
            )?;
            2
        }
        _ => u32::try_from(modes.len()).map_err(|_| cursor.fail("a bounded mode count"))?,
    };
    cursor.finish()?;
    Ok(EffectSequenceModalNode {
        cardinality: ModalCardinalityNode {
            minimum: count,
            maximum,
            allow_repeated: false,
        },
        modes,
    })
}

/// Width of a mode's leading flavor-word label: a short run of words closed
/// by an em dash before any punctuation of the mode's own text. Zero when the
/// mode starts directly with its effect.
fn flavor_word_label_width(tokens: &[crate::oracle::model::RecognizedEntity]) -> usize {
    const MAXIMUM_LABEL_WORDS: usize = 6;
    let width = tokens
        .iter()
        .take(MAXIMUM_LABEL_WORDS + 1)
        .take_while(|token| token.raw.chars().next().is_some_and(char::is_alphanumeric))
        .count();
    if width > 0
        && width <= MAXIMUM_LABEL_WORDS
        && tokens
            .get(width)
            .is_some_and(|token| token.raw == "\u{2014}")
    {
        width
    } else {
        0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ModalCardinalityNode {
    pub minimum: u32,
    pub maximum: u32,
    pub allow_repeated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ModalPrimitiveActionNode {
    Damage(DamageEffectNode),
    TemporaryKeywordGrant(TemporaryKeywordGrantNode),
    TargetedTokenCreation(TargetedCreateCreatureTokensWithManaAbilityNode),
    TargetPlayerScryThenDraw(TargetPlayerScryThenDrawNode),
    ZoneChange(ZoneChangeEffectNode),
    TargetCardZoneChange(TargetCardZoneChangeEffectNode),
}

impl ModalPrimitiveActionNode {
    pub(in crate::oracle::canonical) fn uses_x(&self) -> bool {
        match self {
            Self::Damage(effect) => effect.amount.uses_x(),
            Self::TargetedTokenCreation(effect) => effect.quantity == NumericAtom::VariableX,
            Self::TargetPlayerScryThenDraw(effect) => effect.scry_count.uses_x(),
            Self::ZoneChange(effect) => effect
                .target
                .slots
                .iter()
                .any(|slot| selector_uses_x(&slot.selector)),
            Self::TargetCardZoneChange(effect) => effect.cardinality.uses_x(),
            Self::TemporaryKeywordGrant(_) => false,
        }
    }
}

fn selector_uses_x(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate_uses_x(predicate)
        }
        TargetSelector::Union(selectors) => selectors.iter().any(selector_uses_x),
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

fn predicate_uses_x(predicate: &SelectorPredicate) -> bool {
    match predicate {
        SelectorPredicate::NumericComparison { right, .. } => right.uses_x(),
        SelectorPredicate::Not(predicate) => predicate_uses_x(predicate),
        SelectorPredicate::Intersection(predicates) | SelectorPredicate::Union(predicates) => {
            predicates.iter().any(predicate_uses_x)
        }
        _ => false,
    }
}

fn current_mode_contains(cursor: &OracleTokenCursor<'_>, expected: &str) -> bool {
    let mut offset = 0;
    while let Some(raw) = cursor.peek_ahead(offset) {
        if raw == "\u{2022}" {
            break;
        }
        if raw.eq_ignore_ascii_case(expected) {
            return true;
        }
        offset += 1;
    }
    false
}

fn current_mode_is_graveyard_card_exile(cursor: &OracleTokenCursor<'_>) -> bool {
    let equals = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let is_cards = |offset: usize| equals(offset, "card") || equals(offset, "cards");
    let is_quantity = |offset: usize| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("x") || parse_number_word(raw).is_some())
    };

    equals(0, "exile")
        && ((equals(1, "target") && is_cards(2) && equals(3, "from"))
            || (is_quantity(1) && equals(2, "target") && is_cards(3) && equals(4, "from"))
            || (equals(1, "up")
                && equals(2, "to")
                && is_quantity(3)
                && equals(4, "target")
                && is_cards(5)
                && equals(6, "from")))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PrimitiveModalNode {
    pub cardinality: ModalCardinalityNode,
    pub modes: Vec<ModalPrimitiveActionNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeState {
    Marker,
    Action,
    Terminator,
    Complete,
}

fn recognize_primitive_modal_mode(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<ModalPrimitiveActionNode> {
    let mut state = ModeState::Marker;
    let mut action = None;
    let mut action_owns_terminator = false;
    loop {
        state = match state {
            ModeState::Marker => {
                cursor.enter("modal.mode.marker");
                cursor.consume("\u{2022}")?;
                ModeState::Action
            }
            ModeState::Action => {
                cursor.enter("modal.mode.action");
                action = Some(
                    if source_reference_starts_at(
                        cursor.position(),
                        cursor.peek(),
                        name_links,
                        name_failure,
                    ) {
                        let effect = recognize_damage_effect(cursor, name_links, name_failure)?;
                        let LocalEffectNode::DealDamage(effect) = effect else {
                            return Err(cursor.fail("a direct damage modal action"));
                        };
                        ModalPrimitiveActionNode::Damage(effect)
                    } else if crate::oracle::canonical::automata::lemma_at(
                        &cursor,
                        VerbLemma::Exile,
                        crate::oracle::canonical::automata::VerbAgreement::Base,
                    ) {
                        if current_mode_is_graveyard_card_exile(cursor) {
                            ModalPrimitiveActionNode::TargetCardZoneChange(
                                recognize_target_card_zone_change_effect(cursor)?,
                            )
                        } else {
                            let effect = recognize_zone_change_effect(cursor)?;
                            let LocalEffectNode::ZoneChange(effect) = effect else {
                                return Err(cursor.fail("an exile modal action"));
                            };
                            ModalPrimitiveActionNode::ZoneChange(effect)
                        }
                    } else if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
                        && cursor.peek_ahead(1).is_some_and(|raw| {
                            raw.eq_ignore_ascii_case("player")
                                || raw.eq_ignore_ascii_case("opponent")
                        })
                        && (current_mode_contains(cursor, "create")
                            || current_mode_contains(cursor, "creates"))
                    {
                        action_owns_terminator = true;
                        ModalPrimitiveActionNode::TargetedTokenCreation(
                            recognize_targeted_create_creature_tokens_with_mana_ability(cursor)?,
                        )
                    } else if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
                        && cursor.peek_ahead(1).is_some_and(|raw| {
                            raw.eq_ignore_ascii_case("player")
                                || raw.eq_ignore_ascii_case("opponent")
                        })
                        && current_mode_contains(cursor, "scries")
                    {
                        ModalPrimitiveActionNode::TargetPlayerScryThenDraw(
                            recognize_target_player_scry_then_draw(
                                cursor,
                                TargetBinding {
                                    effect_index: 0,
                                    slot_index: 0,
                                },
                            )?,
                        )
                    } else if current_mode_contains(cursor, "gain")
                        || current_mode_contains(cursor, "gains")
                    {
                        action_owns_terminator = true;
                        ModalPrimitiveActionNode::TemporaryKeywordGrant(
                            recognize_temporary_keyword_grant_clause(cursor)?,
                        )
                    } else {
                        return Err(cursor.fail(
                            "a damage, keyword-grant, targeted token, targeted scry/draw, battlefield zone-change, or card-zone modal action",
                        ));
                    },
                );
                ModeState::Terminator
            }
            ModeState::Terminator => {
                cursor.enter("modal.mode.terminator");
                if !action_owns_terminator {
                    cursor.consume(".")?;
                }
                ModeState::Complete
            }
            ModeState::Complete => {
                cursor.enter("modal.mode.complete");
                return cursor.require_value(action, "a supported primitive modal action");
            }
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModalState {
    ChoiceAction,
    Cardinality,
    HeaderSeparator,
    Mode,
    Complete,
}

/// Recognizes one exact-cardinality modal envelope. The parent owns choice
/// cardinality and bullet boundaries; child automata own each action.
pub(in crate::oracle::canonical) fn recognize_primitive_modal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<PrimitiveModalNode> {
    let mut state = ModalState::ChoiceAction;
    let mut cardinality = None;
    let mut modes = Vec::new();
    loop {
        state = match state {
            ModalState::ChoiceAction => {
                cursor.enter("modal.exact.action");
                cursor.consume("choose")?;
                ModalState::Cardinality
            }
            ModalState::Cardinality => {
                cursor.enter("modal.exact.cardinality");
                let raw = cursor
                    .peek()
                    .ok_or_else(|| cursor.fail("a positive literal modal cardinality"))?
                    .to_string();
                let count = parse_number_word(&raw)
                    .and_then(|value| u32::try_from(value).ok())
                    .filter(|value| *value > 0)
                    .ok_or_else(|| cursor.fail("a positive literal modal cardinality"))?;
                cursor.consume(&raw)?;
                cardinality = Some(ModalCardinalityNode {
                    minimum: count,
                    maximum: count,
                    allow_repeated: false,
                });
                ModalState::HeaderSeparator
            }
            ModalState::HeaderSeparator => {
                cursor.enter("modal.exact.header-separator");
                cursor.consume("\u{2014}")?;
                ModalState::Mode
            }
            ModalState::Mode => {
                cursor.enter("modal.exact.mode");
                if cursor.peek().is_some_and(|raw| raw == "\u{2022}") {
                    modes.push(recognize_primitive_modal_mode(
                        cursor,
                        name_links,
                        name_failure,
                    )?);
                    ModalState::Mode
                } else {
                    ModalState::Complete
                }
            }
            ModalState::Complete => {
                cursor.enter("modal.exact.complete");
                let cardinality =
                    cursor.require_value(cardinality, "a populated modal cardinality")?;
                cursor.require(
                    modes.len() >= cardinality.maximum as usize,
                    "enough modes for the exact modal cardinality",
                )?;
                cursor.finish()?;
                return Ok(PrimitiveModalNode { cardinality, modes });
            }
        };
    }
}

#[cfg(test)]
#[path = "modal_spell_tests.rs"]
mod tests;

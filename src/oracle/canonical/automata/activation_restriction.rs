//! Activation and cast restrictions (CR 601.3, 602.5b, 602.5d): `Activate
//! only CLAUSE (and only CLAUSE)*.`, `Activate no more than N times each
//! turn.`, `Any player may activate this ability [but only CLAUSE].` and
//! `Cast this spell only CLAUSE.` restrict the enclosing ability or spell.
//! They are ability metadata, never effects: one restriction reader serves
//! every parent, and one typed node carries timing, the turn window, the
//! count limit, the state condition and the activating players.
//!
//! ```text
//! Restriction := "activate" ["this" "ability"] "only" Clauses "."
//!              | "activate" "no" "more" "than" Count "each" "turn" "."
//!              | "activate" "each" LABEL "ability" "only" "once" "."
//!              | "any" "player" "may" "activate" "this" "ability"
//!                    ["but" "only" Clauses] "."
//! Clauses     := Clause (("and" | "," ["and"]) "only" Clause)*
//! Clause      := "as" "a" "sorcery" | "as" "an" "instant"
//!              | "once" ["each" "turn"] | Window | "if" Condition
//! ```
//!
//! Windows are read by the shared turn-window grammar (`turn_step_event.rs`)
//! and conditions by the shared static condition language; this module owns
//! only the clause list, its connectors and the merge of the parts.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    StaticConditionContext, StaticConditionNode, TurnWindowNode, WindowSubject,
    recognize_static_condition, recognize_turn_window, turn_window_starts,
};
use crate::oracle::ability_catalog::match_catalog_mechanic;

/// `as a sorcery` (CR 307.1 timing) or `as an instant`, which restates the
/// default timing of an activated ability (CR 602.2) and restricts nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationTiming {
    Sorcery,
    Instant,
}

/// How often the ability may be activated (CR 602.5b).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationCountLimit {
    /// `only once each turn`, `no more than twice each turn`.
    PerTurn(u32),
    /// `only once`: once for this object (CR 400.7: a new object may
    /// activate it again).
    PerObject,
}

/// A keyword that adds activation rules to the activated ability it labels
/// (`KEYWORD — COST: EFFECT`). Its rules are its definition, not its
/// reminder text: Boast (CR 702.142a), Exhaust (CR 702.177a), Forecast
/// (CR 702.57a-b) and Power-up (CR 702.193a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivationKeyword {
    Boast,
    Exhaust,
    Forecast,
    PowerUp,
}

impl ActivationKeyword {
    pub(in crate::oracle::canonical) fn from_canonical_id(id: &str) -> Option<Self> {
        Some(match id {
            "boast" => Self::Boast,
            "exhaust" => Self::Exhaust,
            "forecast" => Self::Forecast,
            "power-up" => Self::PowerUp,
            _ => return None,
        })
    }

    /// The engine's ability class (`Boast abilities you activate cost ...`).
    pub(in crate::oracle::canonical) fn ability_class(self) -> &'static str {
        match self {
            Self::Boast => "boast",
            Self::Exhaust => "exhaust",
            Self::Forecast => "forecast",
            Self::PowerUp => "powerUp",
        }
    }

    /// The restrictions the keyword's definition adds to its ability.
    pub(in crate::oracle::canonical) fn defined_restrictions(self) -> ActivationRestrictionsNode {
        use super::{StepSet, WindowStep, WindowTurnOwner};
        let mut node = ActivationRestrictionsNode::default();
        match self {
            Self::Boast => {
                node.condition = Some(RestrictionCondition::SourceAttackedThisTurn);
                node.limit = Some(ActivationCountLimit::PerTurn(1));
            }
            Self::Exhaust | Self::PowerUp => node.limit = Some(ActivationCountLimit::PerObject),
            Self::Forecast => {
                node.window = Some(TurnWindowNode {
                    owner: Some(WindowTurnOwner::AbilityPlayer),
                    steps: StepSet::single(WindowStep::Upkeep),
                });
                node.limit = Some(ActivationCountLimit::PerTurn(1));
            }
        }
        node
    }
}

/// `only if CONDITION`. Two propositions the shared condition language does
/// not read yet are typed here, each with its own fixed meaning; every
/// other condition is the shared static condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RestrictionCondition {
    Shared(StaticConditionNode),
    /// `this PERMANENT attacked this turn`: the source's own combat history.
    SourceAttackedThisTurn,
    /// `you've been attacked this step` (CR 506.2): a creature attacks the
    /// player who would activate or cast.
    AbilityPlayerAttacked,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ActivationRestrictionsNode {
    pub timing: Option<ActivationTiming>,
    pub window: Option<TurnWindowNode>,
    pub limit: Option<ActivationCountLimit>,
    /// Evaluated when the ability would be activated (CR 602.5b).
    pub condition: Option<RestrictionCondition>,
    /// `Any player may activate this ability` (CR 602.2: the activating
    /// player controls the ability).
    pub any_player: bool,
    /// The labeling keyword whose definition contributed restrictions.
    pub keyword: Option<ActivationKeyword>,
    /// `Activate each LABEL ability only once`: the reminder names the class
    /// of abilities its once-limit applies to.
    pub class_scope: Option<ActivationKeyword>,
}

impl ActivationRestrictionsNode {
    pub(in crate::oracle::canonical) fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Adds restrictions stated elsewhere for the same ability. A part stated
    /// twice is an explicit failure instead of a silent override; two windows
    /// both apply (CR 602.5b), so they intersect.
    pub(in crate::oracle::canonical) fn merge(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        other: Self,
    ) -> MachineResult<()> {
        fn once<T>(
            cursor: &OracleTokenCursor<'_>,
            slot: &mut Option<T>,
            value: Option<T>,
        ) -> MachineResult<()> {
            if let Some(value) = value {
                cursor.require(slot.is_none(), "each activation restriction stated once")?;
                *slot = Some(value);
            }
            Ok(())
        }
        once(cursor, &mut self.timing, other.timing)?;
        once(cursor, &mut self.limit, other.limit)?;
        once(cursor, &mut self.condition, other.condition)?;
        once(cursor, &mut self.keyword, other.keyword)?;
        once(cursor, &mut self.class_scope, other.class_scope)?;
        if let Some(window) = other.window {
            self.window = Some(match self.window {
                None => window,
                Some(current) => intersect_windows(cursor, current, window)?,
            });
        }
        cursor.require(
            !(self.any_player && other.any_player),
            "one any-player activation permission",
        )?;
        self.any_player |= other.any_player;
        Ok(())
    }
}

fn intersect_windows(
    cursor: &OracleTokenCursor<'_>,
    left: TurnWindowNode,
    right: TurnWindowNode,
) -> MachineResult<TurnWindowNode> {
    let owner = match (left.owner, right.owner) {
        (Some(left), Some(right)) => {
            cursor.require(left == right, "one turn owner across activation windows")?;
            Some(left)
        }
        (left, right) => left.or(right),
    };
    let steps = left.steps.intersect(right.steps);
    cursor.require(
        !steps.is_empty(),
        "activation windows that admit a common step",
    )?;
    Ok(TurnWindowNode { owner, steps })
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Whether a restriction sentence of an activated ability starts here.
pub(in crate::oracle::canonical) fn activation_restriction_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    (word_at(cursor, 0, "activate")
        && (word_at(cursor, 1, "only")
            || (word_at(cursor, 1, "this") && word_at(cursor, 2, "ability"))
            || (word_at(cursor, 1, "no") && word_at(cursor, 2, "more"))
            || word_at(cursor, 1, "each")))
        || (word_at(cursor, 0, "any")
            && word_at(cursor, 1, "player")
            && word_at(cursor, 2, "may")
            && word_at(cursor, 3, "activate"))
}

/// `This ability triggers only once each turn.` (CR 603.2h-style trigger
/// limit) restricts the enclosing triggered ability: ability metadata owned by
/// the trigger parent, never an effect.
pub(in crate::oracle::canonical) fn trigger_limit_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    ["this", "ability", "triggers", "only"]
        .iter()
        .enumerate()
        .all(|(offset, word)| word_at(cursor, offset, word))
}

/// Reads `This ability triggers only once each turn.`; returns whether it was
/// present. Any other `triggers only ...` clause is an explicit failure.
pub(in crate::oracle::canonical) fn recognize_optional_trigger_limit(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<bool> {
    if !trigger_limit_starts(cursor) {
        return Ok(false);
    }
    cursor.enter("trigger.limit.head");
    cursor.consume_phrase(&["this", "ability", "triggers", "only"])?;
    cursor.enter("trigger.limit.clause");
    cursor.consume_phrase(&["once", "each", "turn"])?;
    cursor.enter("trigger.limit.terminator");
    cursor.consume(".")?;
    Ok(true)
}

/// What the restricted action is; it decides which clauses are meaningful.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum RestrictedAction {
    /// An activated ability. `any_player` lets `their` name the activator.
    Activation { any_player: bool },
    /// Casting this spell (CR 601.3): no count limit or activation timing.
    Cast,
}

impl RestrictedAction {
    fn window_subject(self) -> WindowSubject {
        match self {
            Self::Activation { any_player: true } => WindowSubject::AnyPlayer,
            _ => WindowSubject::AbilityPlayer,
        }
    }
}

/// Reads every consecutive restriction sentence of an activated ability and
/// merges them into one node.
pub(in crate::oracle::canonical) fn recognize_activation_restrictions(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivationRestrictionsNode> {
    let mut node = ActivationRestrictionsNode::default();
    let mut first = true;
    while first || activation_restriction_starts(cursor) {
        first = false;
        let sentence = recognize_activation_restriction_sentence(cursor, None)?;
        node.merge(cursor, sentence)?;
    }
    Ok(node)
}

/// One restriction sentence. `class_label` admits the reminder form
/// `Activate each LABEL ability only once.` for that keyword only.
pub(in crate::oracle::canonical) fn recognize_activation_restriction_sentence(
    cursor: &mut OracleTokenCursor<'_>,
    class_label: Option<ActivationKeyword>,
) -> MachineResult<ActivationRestrictionsNode> {
    let mut node = ActivationRestrictionsNode::default();
    if word_at(cursor, 0, "any") {
        cursor.enter("activation.restriction.activator");
        cursor.consume_phrase(&["any", "player", "may", "activate", "this", "ability"])?;
        node.any_player = true;
        cursor.enter("activation.restriction.activator-connector");
        if word_at(cursor, 0, "but") {
            cursor.consume_phrase(&["but", "only"])?;
            recognize_restriction_clauses(
                cursor,
                &mut node,
                RestrictedAction::Activation { any_player: true },
            )?;
        }
        cursor.enter("activation.restriction.terminator");
        cursor.consume(".")?;
        return Ok(node);
    }
    cursor.enter("activation.restriction.head");
    cursor.consume("activate")?;
    if word_at(cursor, 0, "no") {
        cursor.enter("activation.restriction.count-limit");
        cursor.consume_phrase(&["no", "more", "than"])?;
        let count = recognize_turn_count(cursor)?;
        cursor.consume_phrase(&["each", "turn"])?;
        node.limit = Some(ActivationCountLimit::PerTurn(count));
        cursor.enter("activation.restriction.terminator");
        cursor.consume(".")?;
        return Ok(node);
    }
    if word_at(cursor, 0, "each") {
        cursor.enter("activation.restriction.class");
        cursor.consume("each")?;
        let tokens = cursor.token_slice(cursor.position(), usize::MAX);
        let label = match_catalog_mechanic(tokens, 0)
            .and_then(|matched| {
                ActivationKeyword::from_canonical_id(matched.canonical_id)
                    .map(|keyword| (keyword, matched.end_token))
            })
            .filter(|(keyword, _)| Some(*keyword) == class_label);
        let (keyword, width) = cursor.require_value(
            label,
            "the keyword that labels this ability (`each exhaust ability`)",
        )?;
        cursor.advance_to(cursor.position() + width)?;
        cursor.consume_phrase(&["ability", "only", "once"])?;
        node.class_scope = Some(keyword);
        node.limit = Some(ActivationCountLimit::PerObject);
        cursor.enter("activation.restriction.terminator");
        cursor.consume(".")?;
        return Ok(node);
    }
    if word_at(cursor, 0, "this") {
        cursor.consume_phrase(&["this", "ability"])?;
    }
    cursor.enter("activation.restriction.only");
    cursor.consume("only")?;
    recognize_restriction_clauses(
        cursor,
        &mut node,
        RestrictedAction::Activation { any_player: false },
    )?;
    cursor.enter("activation.restriction.terminator");
    cursor.consume(".")?;
    Ok(node)
}

/// `once`, `twice`, `N times`.
fn recognize_turn_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<u32> {
    cursor.enter("activation.restriction.count");
    if word_at(cursor, 0, "once") {
        cursor.consume("once")?;
        return Ok(1);
    }
    if word_at(cursor, 0, "twice") {
        cursor.consume("twice")?;
        return Ok(2);
    }
    let count = cursor
        .peek()
        .and_then(parse_number_word)
        .and_then(|count| u32::try_from(count).ok())
        .filter(|count| *count >= 1);
    let count = cursor.require_value(count, "a fixed positive activation count")?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.consume("times")?;
    Ok(count)
}

/// Whether `and only`, `, only` or `, and only` starts here; returns its width.
fn clause_connector_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    if word_at(cursor, offset, "and") && word_at(cursor, offset + 1, "only") {
        return Some(2);
    }
    if cursor.peek_ahead(offset) == Some(",") {
        if word_at(cursor, offset + 1, "only") {
            return Some(2);
        }
        if word_at(cursor, offset + 1, "and") && word_at(cursor, offset + 2, "only") {
            return Some(3);
        }
    }
    None
}

/// `Clause ((and | , [and]) only Clause)*` after `only`. The sentence
/// terminator stays with the parent.
pub(in crate::oracle::canonical) fn recognize_restriction_clauses(
    cursor: &mut OracleTokenCursor<'_>,
    node: &mut ActivationRestrictionsNode,
    action: RestrictedAction,
) -> MachineResult<()> {
    loop {
        cursor.enter("activation.restriction.clause");
        let head = cursor.peek().map(str::to_ascii_lowercase);
        let mut clause = ActivationRestrictionsNode::default();
        match head.as_deref() {
            Some("as") if matches!(action, RestrictedAction::Activation { .. }) => {
                if word_at(cursor, 1, "an") {
                    cursor.consume_phrase(&["as", "an", "instant"])?;
                    clause.timing = Some(ActivationTiming::Instant);
                } else {
                    cursor.consume_phrase(&["as", "a", "sorcery"])?;
                    clause.timing = Some(ActivationTiming::Sorcery);
                }
            }
            Some("once") if matches!(action, RestrictedAction::Activation { .. }) => {
                cursor.consume("once")?;
                clause.limit = Some(if word_at(cursor, 0, "each") {
                    cursor.consume_phrase(&["each", "turn"])?;
                    ActivationCountLimit::PerTurn(1)
                } else {
                    ActivationCountLimit::PerObject
                });
            }
            Some("if") => {
                cursor.consume("if")?;
                clause.condition = Some(recognize_restriction_condition(cursor)?);
            }
            Some(_) if turn_window_starts(cursor) => {
                cursor.enter("activation.restriction.window");
                clause.window = Some(recognize_turn_window(cursor, action.window_subject())?);
            }
            _ => {
                return Err(cursor.fail(match action {
                    RestrictedAction::Activation { .. } => {
                        "`as a sorcery`, `as an instant`, `once [each turn]`, a turn window (`during`, `before`, `after`) or `if CONDITION`"
                    }
                    RestrictedAction::Cast => {
                        "a turn window (`during`, `before`, `after`) or `if CONDITION`"
                    }
                }));
            }
        }
        node.merge(cursor, clause)?;
        cursor.enter("activation.restriction.connector");
        match clause_connector_width(cursor, 0) {
            Some(width) => cursor.advance_to(cursor.position() + width)?,
            None => return Ok(()),
        }
    }
}

/// The condition ends at the next clause connector or at the sentence end;
/// its own `and` connectives stay inside.
fn recognize_restriction_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<RestrictionCondition> {
    cursor.enter("activation.restriction.condition");
    let start = cursor.position();
    let mut width = 0;
    while let Some(raw) = cursor.peek_ahead(width) {
        if raw == "." || raw == "(" || clause_connector_width(cursor, width).is_some() {
            break;
        }
        width += 1;
    }
    let end = start + width;
    cursor.require(width > 0, "a restriction condition")?;
    if let Some(condition) = recognize_restriction_local_condition(cursor, end) {
        return Ok(condition);
    }
    // No source subject: `it` has no antecedent in a restriction.
    let condition = cursor.bounded(end, |cursor| {
        recognize_static_condition(
            cursor,
            StaticConditionContext {
                source_subject: false,
                name_links: &[],
                name_failure: None,
            },
        )
    })?;
    Ok(RestrictionCondition::Shared(condition))
}

/// The two typed propositions of [`RestrictionCondition`], each only when it
/// spans the whole condition.
fn recognize_restriction_local_condition(
    cursor: &mut OracleTokenCursor<'_>,
    end: usize,
) -> Option<RestrictionCondition> {
    let width = end - cursor.position();
    let words = (0..width)
        .map(|offset| {
            cursor
                .peek_ahead(offset)
                .unwrap_or_default()
                .replace('\u{2019}', "'")
                .to_ascii_lowercase()
        })
        .collect::<Vec<_>>();
    let words = words.iter().map(String::as_str).collect::<Vec<_>>();
    let condition = match words.as_slice() {
        ["this", noun, "attacked", "this", "turn"]
            if ["creature", "permanent", "vehicle"].contains(noun) =>
        {
            RestrictionCondition::SourceAttackedThisTurn
        }
        ["you've", "been", "attacked", "this", "step"] => {
            RestrictionCondition::AbilityPlayerAttacked
        }
        _ => return None,
    };
    cursor.enter("activation.restriction.condition.local");
    cursor.advance_to(end).ok()?;
    Some(condition)
}

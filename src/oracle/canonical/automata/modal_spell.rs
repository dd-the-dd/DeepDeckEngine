//! The modal container (CR 700.2): a header frame that owns the mode count
//! and the bullet boundaries, and one body slot per mode that delegates to
//! the shared effect program.
//!
//! ```text
//! ModalBody   := Header Mode{2,}
//! Header      := "choose" ModeCount ("—" | "." RepeatPermission)
//! ModeCount   := Number                       exactly N
//!              | Number "or" "both"           one or both (two modes)
//!              | Number "or" "more"           N or more
//!              | "up" "to" Number             zero to N
//!              | "any" "number"               zero to every mode
//! RepeatPermission := "you may choose the same mode more than once" "."   (CR 700.2d)
//! Mode        := "•" [FlavorWord "—"] EffectProgram
//!
//! SpreeBody   := "Spree" [Reminder] SpreeMode{2,}                    (CR 702.172)
//! SpreeMode   := "+" ManaCost "—" EffectProgram
//! ```
//!
//! A Spree mode's cost is an additional cost paid only when that mode is
//! chosen (CR 702.172a); the `+` marker has no other rules meaning (CR
//! 702.172b).
//!
//! Commitment is structural: a body is modal only when it starts with
//! `choose` and contains a mode bullet. `a`/`an` are articles, never a mode
//! count (`choose a creature` is an instruction). Mode-history constraints
//! (`that hasn't been chosen`), random choice and conditional count upgrades
//! are explicit failures of the header, never approximated.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    EffectProgramNode, EffectRecognitionContext, ManaCostNode, rebase_machine_failure,
    rebase_name_failure, rebase_name_link, recognize_effect_program_with_context,
    recognize_mana_cost,
};

const MODE_BULLET: &str = "\u{2022}";
const HEADER_DASH: &str = "\u{2014}";

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct EffectSequenceModalNode {
    pub cardinality: ModalCardinalityNode,
    /// Every printed mode is one shared effect program.
    pub modes: Vec<EffectProgramNode>,
    /// Spree (CR 702.172a): the additional cost of each mode, in mode order.
    /// Empty for a bulleted modal ability.
    pub mode_costs: Vec<ManaCostNode>,
}

impl EffectSequenceModalNode {
    pub(in crate::oracle::canonical) fn action_count(&self) -> usize {
        self.modes.iter().map(EffectProgramNode::action_count).sum()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ModalCardinalityNode {
    pub minimum: u32,
    pub maximum: u32,
    pub allow_repeated: bool,
}

/// The printed mode count of a modal header, before the modes are counted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ModeCountNode {
    Exactly(u32),
    OneOrBoth,
    AtLeast(u32),
    UpTo(u32),
    AnyNumber,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ModalHeaderNode {
    pub count: ModeCountNode,
    /// CR 700.2d: `You may choose the same mode more than once.`
    pub allow_repeated: bool,
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Structural markers of a modal body: the `choose` head and at least one
/// mode bullet in the same body. Nothing is consumed.
pub(in crate::oracle::canonical) fn modal_body_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "choose")
        && cursor
            .token_slice(cursor.position(), usize::MAX)
            .iter()
            .any(|token| token.raw == MODE_BULLET)
}

fn recognize_mode_count_number(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<u32> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a positive mode-count number"))?
        .to_string();
    // `a`/`an` are articles; parse_number_word reads them as one.
    cursor.require(
        !raw.eq_ignore_ascii_case("a") && !raw.eq_ignore_ascii_case("an"),
        "a mode-count number word, not an article",
    )?;
    let count = parse_number_word(&raw)
        .and_then(|value| u32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| cursor.fail("a positive mode-count number"))?;
    cursor.consume(&raw)?;
    Ok(count)
}

fn recognize_mode_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ModeCountNode> {
    cursor.enter("modal.header.mode-count");
    if word_at(cursor, 0, "up") {
        cursor.consume("up")?;
        cursor.consume("to")?;
        return Ok(ModeCountNode::UpTo(recognize_mode_count_number(cursor)?));
    }
    if word_at(cursor, 0, "any") {
        cursor.consume("any")?;
        cursor.consume("number")?;
        return Ok(ModeCountNode::AnyNumber);
    }
    let count = recognize_mode_count_number(cursor)?;
    if !word_at(cursor, 0, "or") {
        return Ok(ModeCountNode::Exactly(count));
    }
    cursor.enter("modal.header.mode-count-extension");
    cursor.consume("or")?;
    if word_at(cursor, 0, "both") {
        cursor.consume("both")?;
        cursor.require(count == 1, "`one or both`")?;
        return Ok(ModeCountNode::OneOrBoth);
    }
    if word_at(cursor, 0, "more") {
        cursor.consume("more")?;
        return Ok(ModeCountNode::AtLeast(count));
    }
    Err(cursor.fail("`or both` or `or more`"))
}

/// CR 700.2d's instruction, read word by word after the header's period.
const REPEAT_PERMISSION: [&str; 9] = [
    "you", "may", "choose", "the", "same", "mode", "more", "than", "once",
];

/// Reads `choose MODECOUNT —` (or `choose MODECOUNT. You may choose the same
/// mode more than once.`) up to the first mode bullet.
pub(in crate::oracle::canonical) fn recognize_modal_header(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ModalHeaderNode> {
    cursor.enter("modal.header.action");
    cursor.consume("choose")?;
    let count = recognize_mode_count(cursor)?;
    cursor.enter("modal.header.qualifier");
    cursor.require(
        !word_at(cursor, 0, "that"),
        "a modal header without a mode-history constraint (`that hasn't been chosen` is not lowered)",
    )?;
    cursor.require(
        !(word_at(cursor, 0, "at") && word_at(cursor, 1, "random")),
        "a modal header chosen by a player (`at random` is not lowered)",
    )?;
    cursor.enter("modal.header.terminator");
    let allow_repeated = if cursor.peek() == Some(HEADER_DASH) {
        cursor.consume(HEADER_DASH)?;
        false
    } else if cursor.peek() == Some(".") {
        cursor.consume(".")?;
        cursor.enter("modal.header.repeat-permission");
        cursor.require(
            REPEAT_PERMISSION
                .iter()
                .enumerate()
                .all(|(offset, word)| word_at(cursor, offset, word)),
            "the same-mode permission (CR 700.2d); a conditional mode-count upgrade is not lowered",
        )?;
        cursor.advance_to(cursor.position() + REPEAT_PERMISSION.len())?;
        cursor.consume(".")?;
        true
    } else {
        return Err(cursor.fail("an em dash or period closing the modal header"));
    };
    cursor.enter("modal.header.first-mode");
    cursor.require(
        cursor.peek() == Some(MODE_BULLET),
        "a bulleted mode after the modal header",
    )?;
    Ok(ModalHeaderNode {
        count,
        allow_repeated,
    })
}

/// Resolves the printed count against the printed modes (CR 700.2). A
/// count that cannot be bounded with repetition is an explicit failure.
fn resolve_cardinality(
    cursor: &OracleTokenCursor<'_>,
    header: ModalHeaderNode,
    mode_count: usize,
) -> MachineResult<ModalCardinalityNode> {
    let modes = u32::try_from(mode_count).map_err(|_| cursor.fail("a bounded mode count"))?;
    cursor.require(modes >= 2, "two or more printed modes (CR 700.2)")?;
    let repeated = header.allow_repeated;
    let (minimum, maximum) = match header.count {
        ModeCountNode::Exactly(count) => {
            cursor.require(
                repeated || modes >= count,
                "enough distinct modes for the exact modal cardinality",
            )?;
            (count, count)
        }
        ModeCountNode::OneOrBoth => {
            cursor.require(modes == 2, "`one or both` over exactly two modes")?;
            (1, 2)
        }
        ModeCountNode::UpTo(count) => (0, if repeated { count } else { count.min(modes) }),
        ModeCountNode::AtLeast(count) => {
            cursor.require(modes >= count, "enough modes for `N or more`")?;
            (count, modes)
        }
        ModeCountNode::AnyNumber => (0, modes),
    };
    cursor.require(
        !repeated
            || matches!(
                header.count,
                ModeCountNode::Exactly(_) | ModeCountNode::UpTo(_)
            ),
        "a bounded mode count for repeatable modes",
    )?;
    Ok(ModalCardinalityNode {
        minimum,
        maximum,
        allow_repeated: repeated,
    })
}

/// Recognizes a modal envelope with the default recognition context.
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
/// The parent owns only the header and bullet boundaries; every mode body
/// is the shared effect program.
pub(in crate::oracle::canonical) fn recognize_effect_sequence_modal_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
) -> MachineResult<EffectSequenceModalNode> {
    let header = recognize_modal_header(cursor)?;

    let mut modes = Vec::new();
    while cursor.peek() == Some(MODE_BULLET) {
        cursor.enter("modal.effect-sequence.mode-marker");
        cursor.consume(MODE_BULLET)?;
        // CR 207.2d: a flavor word (`Pray for Protection —`) labels the mode
        // and has no rules meaning; the mode body starts after its dash.
        let label_width =
            flavor_word_label_width(cursor.token_slice(cursor.position(), usize::MAX));
        if label_width > 0 {
            cursor.enter("modal.effect-sequence.mode-flavor-word");
            cursor.advance_to(cursor.position() + label_width + 1)?;
        }
        modes.push(recognize_mode_program(
            cursor,
            name_links,
            name_failure,
            context,
            |tokens, offset| tokens[offset].raw == MODE_BULLET,
        )?);
    }

    cursor.enter("modal.effect-sequence.complete");
    let cardinality = resolve_cardinality(cursor, header, modes.len())?;
    cursor.finish()?;
    Ok(EffectSequenceModalNode {
        cardinality,
        modes,
        mode_costs: Vec::new(),
    })
}

/// Reads one mode body from the cursor up to the next mode marker (as
/// `is_marker` decides) or the end, as one complete shared effect program on
/// a bounded subcursor with rebased name links.
fn recognize_mode_program(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
    context: &EffectRecognitionContext,
    is_marker: impl Fn(&[crate::oracle::model::RecognizedEntity], usize) -> bool,
) -> MachineResult<EffectProgramNode> {
    let mode_start = cursor.position();
    let remaining = cursor.token_slice(mode_start, usize::MAX);
    let mode_end = mode_start
        + (0..remaining.len())
            .find(|offset| is_marker(remaining, *offset))
            .unwrap_or(remaining.len());
    cursor.enter("modal.effect-sequence.mode-body");
    cursor.require(mode_end > mode_start, "a nonempty modal effect program")?;
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
    // Every mode is lowered by the shared modal lowering, which owns its
    // `unless` frames (CR 118.12).
    let mode = recognize_effect_program_with_context(
        &mut mode_cursor,
        &mode_links,
        mode_failure.as_ref(),
        &context.clone().with_unless_frames(),
    )
    .and_then(|mode| {
        mode_cursor.enter("modal.effect-sequence.mode-complete");
        mode_cursor.finish()?;
        Ok(mode)
    })
    .map_err(|failure| rebase_machine_failure(failure, mode_start))?;
    cursor.advance_to(mode_end)?;
    Ok(mode)
}

/// A Spree mode marker: `+` followed by the mode's mana cost.
fn spree_marker_at(tokens: &[crate::oracle::model::RecognizedEntity], offset: usize) -> bool {
    tokens[offset].raw == "+"
        && tokens
            .get(offset + 1)
            .is_some_and(|token| token.token_type == "manaSymbol")
}

/// Structural markers of a Spree body: the `Spree` keyword and a `+ COST`
/// mode marker. Nothing is consumed.
pub(in crate::oracle::canonical) fn spree_body_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let tokens = cursor.token_slice(cursor.position(), usize::MAX);
    word_at(cursor, 0, "spree") && (0..tokens.len()).any(|offset| spree_marker_at(tokens, offset))
}

/// `Spree (reminder) + COST — MODE + COST — MODE ...`: one or more modes
/// (CR 702.172a), each with its own additional cost.
pub(in crate::oracle::canonical) fn recognize_spree_modal(
    cursor: &mut OracleTokenCursor<'_>,
    name_links: &[ResolvedNameLink],
    name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<EffectSequenceModalNode> {
    let context = EffectRecognitionContext::default();
    cursor.enter("modal.spree.keyword");
    cursor.consume("spree")?;
    if cursor.peek() == Some("(") {
        // CR 207.2: the reminder restates CR 702.172a and has no rules meaning.
        cursor.enter("modal.spree.reminder");
        cursor.consume("(")?;
        let mut depth = 1usize;
        while depth > 0 {
            match cursor.peek() {
                None => return Err(cursor.fail("a closing parenthesis for the Spree reminder")),
                Some("(") => depth += 1,
                Some(")") => depth -= 1,
                Some(_) => {}
            }
            cursor.advance_to(cursor.position() + 1)?;
        }
    }
    let mut modes = Vec::new();
    let mut mode_costs = Vec::new();
    while cursor.peek() == Some("+") {
        cursor.enter("modal.spree.mode-marker");
        cursor.consume("+")?;
        mode_costs.push(recognize_mana_cost(cursor)?);
        cursor.enter("modal.spree.mode-dash");
        cursor.consume(HEADER_DASH)?;
        modes.push(recognize_mode_program(
            cursor,
            name_links,
            name_failure,
            &context,
            spree_marker_at,
        )?);
    }
    cursor.enter("modal.spree.complete");
    let cardinality = resolve_cardinality(
        cursor,
        ModalHeaderNode {
            count: ModeCountNode::AtLeast(1),
            allow_repeated: false,
        },
        modes.len(),
    )?;
    cursor.finish()?;
    Ok(EffectSequenceModalNode {
        cardinality,
        modes,
        mode_costs,
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
            .is_some_and(|token| token.raw == HEADER_DASH)
    {
        width
    } else {
        0
    }
}

#[cfg(test)]
#[path = "modal_spell_tests.rs"]
mod tests;

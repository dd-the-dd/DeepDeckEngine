//! Turn-step events after `At the beginning of` (CR 503.1, 504, 505, 507,
//! 513). The event composes two independent concepts:
//!
//! ```text
//! StepEvent  := TurnOwner StepName
//!             | "each" StepName                        -- any player's step
//!             | "each" "of" TurnOwner PluralStepName
//!             | "the" StepName ["of" AttachedController]
//!             | "each" "combat" | "combat" "on" TurnOwner "turn"
//! TurnOwner  := "your" | "your opponents'" | "each player's" | "each opponent's"
//! StepName   := "upkeep" | "draw step" | "end step"
//!             | ("first" | "precombat") "main phase"
//!             | "postcombat" "main phase"
//!             | "second" "main phase"               -- main phase ordinal 2
//! ```
//!
//! The owner names whose turn the step belongs to; the step name is shared by
//! every owner. The temporal prefix and following comma remain parent-owned.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TurnStep {
    Upkeep,
    Draw,
    PrecombatMain,
    BeginCombat,
    PostcombatMain,
    /// `second main phase`: the main phase with ordinal 2 in its turn
    /// (CR 505.1a), one particular postcombat main phase.
    SecondMain,
    EndStep,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum StepPlayerScope {
    AbilityController,
    EachPlayer,
    OpponentsOfAbilityController,
    /// The controller of the permanent the source is attached to
    /// (`the upkeep of enchanted creature's controller`, CR 303.4), with
    /// the nominal the text wrote for that permanent.
    AttachedPermanentController(super::TargetNominal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StepEventNode {
    pub step: TurnStep,
    pub player: StepPlayerScope,
}

fn is_possessive(raw: &str, noun: &str) -> bool {
    raw.eq_ignore_ascii_case(&format!("{noun}'s")) || raw.eq_ignore_ascii_case(&format!("{noun}’s"))
}

fn is_plural_possessive(raw: &str, noun: &str) -> bool {
    raw.eq_ignore_ascii_case(&format!("{noun}'")) || raw.eq_ignore_ascii_case(&format!("{noun}’"))
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.advance_to(cursor.position() + 1)
}

/// Whose turn: `your`, `your opponents'`, `each player's`, `each opponent's`.
fn recognize_turn_owner(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<StepPlayerScope> {
    cursor.enter("event.step.turn-owner");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn owner"))?;
    match head.as_str() {
        "your" => {
            cursor.consume("your")?;
            if cursor
                .peek()
                .is_some_and(|raw| is_plural_possessive(raw, "opponents"))
            {
                consume_current(cursor)?;
                return Ok(StepPlayerScope::OpponentsOfAbilityController);
            }
            Ok(StepPlayerScope::AbilityController)
        }
        "each" => {
            cursor.consume("each")?;
            cursor.enter("event.step.turn-owner.player");
            let owner = match cursor.peek() {
                Some(raw) if is_possessive(raw, "player") => StepPlayerScope::EachPlayer,
                Some(raw) if is_possessive(raw, "opponent") => {
                    StepPlayerScope::OpponentsOfAbilityController
                }
                _ => return Err(cursor.fail("`player's` or `opponent's`")),
            };
            consume_current(cursor)?;
            Ok(owner)
        }
        _ => Err(cursor.fail("`your`, `your opponents'`, `each player's` or `each opponent's`")),
    }
}

/// One step name; `plural` reads the form after `each of`.
fn recognize_step_name(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<TurnStep> {
    cursor.enter("event.step.step");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn step name"))?;
    let (step, words): (TurnStep, &[&str]) = match (head.as_str(), plural) {
        ("upkeep", false) => (TurnStep::Upkeep, &["upkeep"]),
        ("upkeeps", true) => (TurnStep::Upkeep, &["upkeeps"]),
        ("draw", false) => (TurnStep::Draw, &["draw", "step"]),
        ("draw", true) => (TurnStep::Draw, &["draw", "steps"]),
        ("end", false) => (TurnStep::EndStep, &["end", "step"]),
        ("end", true) => (TurnStep::EndStep, &["end", "steps"]),
        ("first" | "precombat", false) => (TurnStep::PrecombatMain, &["main", "phase"]),
        ("first" | "precombat", true) => (TurnStep::PrecombatMain, &["main", "phases"]),
        ("postcombat", false) => (TurnStep::PostcombatMain, &["main", "phase"]),
        ("postcombat", true) => (TurnStep::PostcombatMain, &["main", "phases"]),
        // CR 505.1a: every main phase after the first is a postcombat main
        // phase; `second main phase` names the one with ordinal 2, a typed
        // ordinal rather than any postcombat main phase.
        ("second", false) => (TurnStep::SecondMain, &["main", "phase"]),
        ("second", true) => (TurnStep::SecondMain, &["main", "phases"]),
        _ => {
            return Err(cursor.fail(
                "upkeep, draw step, end step, first/precombat, second or postcombat main phase",
            ));
        }
    };
    if matches!(
        step,
        TurnStep::PrecombatMain | TurnStep::PostcombatMain | TurnStep::SecondMain
    ) {
        consume_current(cursor)?;
    }
    cursor.consume_phrase(words)?;
    Ok(step)
}

/// `of enchanted PERMANENT's controller` after `the STEP`: the turn owner is
/// the controller of the object the source enchants.
fn recognize_attached_controller(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StepPlayerScope> {
    cursor.enter("event.step.attached-controller");
    cursor.consume_phrase(&["of", "enchanted"])?;
    cursor.enter("event.step.attached-controller.object");
    let noun = ["creature", "artifact", "enchantment", "land", "permanent"]
        .into_iter()
        .find(|noun| cursor.peek().is_some_and(|raw| is_possessive(raw, noun)))
        .ok_or_else(|| cursor.fail("an enchanted permanent's possessive"))?;
    let nominal =
        match crate::oracle::characteristic_vocabulary::lookup_object_selector_lexeme(noun) {
            Some(crate::oracle::characteristic_vocabulary::CharacteristicLexeme::CardType(
                value,
            )) => super::TargetNominal::CardType(value),
            Some(crate::oracle::characteristic_vocabulary::CharacteristicLexeme::ObjectDomain(
                value,
            )) => super::TargetNominal::Domain(value),
            _ => return Err(cursor.fail("a registered enchanted permanent nominal")),
        };
    consume_current(cursor)?;
    cursor.enter("event.step.attached-controller.relation");
    cursor.consume("controller")?;
    Ok(StepPlayerScope::AttachedPermanentController(nominal))
}

/// Recognizes the player-owned turn-step phrase after `At the beginning of`.
pub(in crate::oracle::canonical) fn recognize_step_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StepEventNode> {
    cursor.enter("event.step.scope");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn-step owner"))?;
    let node = match head.as_str() {
        "your" => {
            let player = recognize_turn_owner(cursor)?;
            StepEventNode {
                step: recognize_step_name(cursor, false)?,
                player,
            }
        }
        "each" => {
            let next = cursor.peek_ahead(1).map(str::to_ascii_lowercase);
            match next.as_deref() {
                Some("of") => {
                    cursor.consume_phrase(&["each", "of"])?;
                    let player = recognize_turn_owner(cursor)?;
                    StepEventNode {
                        step: recognize_step_name(cursor, true)?,
                        player,
                    }
                }
                Some("combat") => {
                    cursor.consume_phrase(&["each", "combat"])?;
                    StepEventNode {
                        step: TurnStep::BeginCombat,
                        player: StepPlayerScope::EachPlayer,
                    }
                }
                Some(raw) if is_possessive(raw, "player") || is_possessive(raw, "opponent") => {
                    let player = recognize_turn_owner(cursor)?;
                    StepEventNode {
                        step: recognize_step_name(cursor, false)?,
                        player,
                    }
                }
                _ => {
                    cursor.consume("each")?;
                    StepEventNode {
                        step: recognize_step_name(cursor, false)?,
                        player: StepPlayerScope::EachPlayer,
                    }
                }
            }
        }
        "the" => {
            cursor.consume("the")?;
            let step = recognize_step_name(cursor, false)?;
            let player = if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
            {
                recognize_attached_controller(cursor)?
            } else {
                StepPlayerScope::EachPlayer
            };
            StepEventNode { step, player }
        }
        "combat" => {
            cursor.enter("event.step.combat-turn");
            cursor.consume_phrase(&["combat", "on"])?;
            let player = recognize_turn_owner(cursor)?;
            cursor.enter("event.step.combat-turn.noun");
            cursor.consume("turn")?;
            StepEventNode {
                step: TurnStep::BeginCombat,
                player,
            }
        }
        _ => {
            return Err(cursor.fail("`your`, `each`, `the`, or `combat on TURN-OWNER turn`"));
        }
    };
    cursor.enter("event.step.complete");
    Ok(node)
}

// ---------------------------------------------------------------------------
// Turn windows (CR 500.1, 505-513): `during your upkeep`, `during combat
// before blockers are declared`, `during your turn, before attackers are
// declared`, `before the combat damage step`, `after combat`. One window is
// a turn owner and the set of steps of that turn it admits. Every part is an
// independent child: a period narrows the set to its steps, `before`/`after`
// keep the steps on one side of a turn event or period, and `on TURN` names
// the owner. Restriction parents (activation and cast timing) read windows
// through this one grammar; they own `only`, the connectors and the period.
// ---------------------------------------------------------------------------

/// One step of a turn in turn order (CR 500.1). The end of combat step has
/// no engine state yet; a window that only admits it stays explicitly
/// unexecutable rather than being widened or dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::oracle::canonical) enum WindowStep {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginningOfCombat,
    DeclareAttackers,
    DeclareBlockers,
    CombatDamage,
    EndOfCombat,
    PostcombatMain,
    End,
    Cleanup,
}

impl WindowStep {
    pub(in crate::oracle::canonical) const ALL: [Self; 12] = [
        Self::Untap,
        Self::Upkeep,
        Self::Draw,
        Self::PrecombatMain,
        Self::BeginningOfCombat,
        Self::DeclareAttackers,
        Self::DeclareBlockers,
        Self::CombatDamage,
        Self::EndOfCombat,
        Self::PostcombatMain,
        Self::End,
        Self::Cleanup,
    ];

    fn bit(self) -> u16 {
        1 << (self as u16)
    }

    /// The engine's step identifier (`GameStep` serialization).
    pub(in crate::oracle::canonical) fn engine_name(self) -> &'static str {
        match self {
            Self::Untap => "untap",
            Self::Upkeep => "upkeep",
            Self::Draw => "draw",
            Self::PrecombatMain => "precombatMain",
            Self::BeginningOfCombat => "beginningOfCombat",
            Self::DeclareAttackers => "declareAttackers",
            Self::DeclareBlockers => "declareBlockers",
            Self::CombatDamage => "combatDamage",
            Self::EndOfCombat => "endOfCombat",
            Self::PostcombatMain => "postcombatMain",
            Self::End => "endStep",
            Self::Cleanup => "cleanup",
        }
    }
}

/// A set of steps of one turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StepSet(u16);

impl StepSet {
    pub(in crate::oracle::canonical) const EVERY: Self = Self((1 << 12) - 1);

    /// Steps `first..=last` in turn order.
    pub(in crate::oracle::canonical) fn range(first: WindowStep, last: WindowStep) -> Self {
        Self(
            WindowStep::ALL
                .into_iter()
                .filter(|step| *step >= first && *step <= last)
                .fold(0, |bits, step| bits | step.bit()),
        )
    }

    pub(in crate::oracle::canonical) fn single(step: WindowStep) -> Self {
        Self(step.bit())
    }

    /// Every step strictly before `step`.
    fn before(step: WindowStep) -> Self {
        Self(step.bit() - 1)
    }

    /// `step` and every later step.
    fn from_step(step: WindowStep) -> Self {
        Self(Self::EVERY.0 & !(step.bit() - 1))
    }

    /// Every step strictly after `step`.
    fn after(step: WindowStep) -> Self {
        Self(Self::EVERY.0 & !((step.bit() << 1) - 1))
    }

    pub(in crate::oracle::canonical) fn intersect(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub(in crate::oracle::canonical) fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(in crate::oracle::canonical) fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub(in crate::oracle::canonical) fn is_every(self) -> bool {
        self == Self::EVERY
    }

    pub(in crate::oracle::canonical) fn steps(self) -> impl Iterator<Item = WindowStep> {
        WindowStep::ALL
            .into_iter()
            .filter(move |step| self.0 & step.bit() != 0)
    }

    fn first(self) -> Option<WindowStep> {
        self.steps().next()
    }

    fn last(self) -> Option<WindowStep> {
        self.steps().last()
    }
}

/// Whose turn a window belongs to. `AbilityPlayer` is the player who
/// activates or casts (CR 109.5: `your` names the would-be controller), so an
/// any-player activation's `their turn` is that same player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum WindowTurnOwner {
    AbilityPlayer,
    Opponent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TurnWindowNode {
    /// `None` admits every player's turn.
    pub owner: Option<WindowTurnOwner>,
    pub steps: StepSet,
}

/// What a bare `their` may name at the start of a window: only the player
/// performing the restricted action when the subject is `any player`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum WindowSubject {
    AbilityPlayer,
    AnyPlayer,
}

pub(in crate::oracle::canonical) fn turn_window_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        ["during", "before", "after"]
            .iter()
            .any(|head| raw.eq_ignore_ascii_case(head))
    })
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// `your`, `an opponent's`, `their` (bound by the enclosing window or by an
/// any-player subject). Returns `None` when no possessive owner starts here.
fn recognize_window_owner(
    cursor: &mut OracleTokenCursor<'_>,
    current: Option<WindowTurnOwner>,
    subject: WindowSubject,
) -> MachineResult<Option<WindowTurnOwner>> {
    cursor.enter("window.owner");
    if word_at(cursor, 0, "your") {
        cursor.consume("your")?;
        return Ok(Some(WindowTurnOwner::AbilityPlayer));
    }
    if (word_at(cursor, 0, "an") || word_at(cursor, 0, "each"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| is_possessive(raw, "opponent"))
    {
        consume_current(cursor)?;
        consume_current(cursor)?;
        return Ok(Some(WindowTurnOwner::Opponent));
    }
    if word_at(cursor, 0, "their") {
        // `their` is anaphoric: the owner already named by this window, or
        // the acting player of an any-player permission (CR 602.2).
        let owner = match (current, subject) {
            (Some(owner), _) => owner,
            (None, WindowSubject::AnyPlayer) => WindowTurnOwner::AbilityPlayer,
            (None, WindowSubject::AbilityPlayer) => {
                return Err(cursor.fail("`their` with a turn-owner antecedent"));
            }
        };
        cursor.consume("their")?;
        return Ok(Some(owner));
    }
    Ok(None)
}

/// One period name: a step (`upkeep`, `draw step`, `declare attackers step`,
/// `end of combat step`, ...), a main phase or the combat phase (CR 506.1).
fn recognize_window_period_name(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<StepSet> {
    use WindowStep as Step;
    cursor.enter("window.period");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a turn step or phase"))?;
    let (set, words, optional_step): (StepSet, &[&str], bool) = match head.as_str() {
        "untap" => (StepSet::single(Step::Untap), &["untap", "step"], false),
        "upkeep" => (StepSet::single(Step::Upkeep), &["upkeep"], true),
        "draw" => (StepSet::single(Step::Draw), &["draw", "step"], false),
        "cleanup" => (StepSet::single(Step::Cleanup), &["cleanup", "step"], false),
        "beginning" => (
            StepSet::single(Step::BeginningOfCombat),
            &["beginning", "of", "combat"],
            true,
        ),
        "declare" if word_at(cursor, 1, "attackers") => (
            StepSet::single(Step::DeclareAttackers),
            &["declare", "attackers", "step"],
            false,
        ),
        "declare" => (
            StepSet::single(Step::DeclareBlockers),
            &["declare", "blockers", "step"],
            false,
        ),
        "combat" if word_at(cursor, 1, "damage") => (
            StepSet::single(Step::CombatDamage),
            &["combat", "damage", "step"],
            false,
        ),
        "combat" => (
            StepSet::range(Step::BeginningOfCombat, Step::EndOfCombat),
            &["combat"],
            false,
        ),
        "end" if word_at(cursor, 1, "of") => (
            StepSet::single(Step::EndOfCombat),
            &["end", "of", "combat"],
            true,
        ),
        "end" => (StepSet::single(Step::End), &["end", "step"], false),
        "precombat" => (
            StepSet::single(Step::PrecombatMain),
            &["precombat", "main", "phase"],
            false,
        ),
        "first" => (
            StepSet::single(Step::PrecombatMain),
            &["first", "main", "phase"],
            false,
        ),
        "postcombat" => (
            StepSet::single(Step::PostcombatMain),
            &["postcombat", "main", "phase"],
            false,
        ),
        "main" => (
            StepSet::single(Step::PrecombatMain).union(StepSet::single(Step::PostcombatMain)),
            &["main", "phase"],
            false,
        ),
        _ => {
            return Err(cursor.fail(
                "a turn step (upkeep, draw step, declare attackers/blockers step, combat damage step, end of combat step, end step) or phase (combat, main phase)",
            ));
        }
    };
    cursor.consume_phrase(words)?;
    if optional_step && word_at(cursor, 0, "step") {
        cursor.consume("step")?;
    }
    Ok(set)
}

/// Window parts after `during`: `TURN-OWNER turn`, `TURN-OWNER PERIOD`,
/// `the PERIOD`, `any PERIOD` or a bare `combat`.
fn recognize_during_part(
    cursor: &mut OracleTokenCursor<'_>,
    window: &mut TurnWindowNode,
    subject: WindowSubject,
) -> MachineResult<()> {
    cursor.enter("window.during");
    cursor.consume("during")?;
    if let Some(owner) = recognize_window_owner(cursor, window.owner, subject)? {
        merge_window_owner(cursor, window, owner)?;
        if word_at(cursor, 0, "turn") {
            cursor.enter("window.during.turn");
            cursor.consume("turn")?;
            return Ok(());
        }
    } else if word_at(cursor, 0, "the") {
        cursor.consume("the")?;
    } else if word_at(cursor, 0, "any") || word_at(cursor, 0, "each") {
        // `any upkeep step`: the period of every player's turn.
        consume_current(cursor)?;
    }
    window.steps = window
        .steps
        .intersect(recognize_window_period_name(cursor)?);
    Ok(())
}

/// Where a turn event or period sits: an event happens at the start of its
/// step (CR 508.1, 509.1, 510.1), so `before` excludes that step and `after`
/// includes it; a period is excluded on both sides.
enum WindowBoundary {
    Event(WindowStep),
    Period(StepSet),
}

fn recognize_window_boundary(
    cursor: &mut OracleTokenCursor<'_>,
    window: &mut TurnWindowNode,
    subject: WindowSubject,
) -> MachineResult<WindowBoundary> {
    cursor.enter("window.boundary");
    if word_at(cursor, 0, "attackers") {
        cursor.consume_phrase(&["attackers", "are", "declared"])?;
        return Ok(WindowBoundary::Event(WindowStep::DeclareAttackers));
    }
    if word_at(cursor, 0, "blockers") {
        cursor.consume_phrase(&["blockers", "are", "declared"])?;
        return Ok(WindowBoundary::Event(WindowStep::DeclareBlockers));
    }
    if word_at(cursor, 0, "combat") && word_at(cursor, 1, "damage") && !word_at(cursor, 2, "step") {
        cursor.consume_phrase(&["combat", "damage"])?;
        cursor.enter("window.boundary.damage-event");
        if word_at(cursor, 0, "has") {
            cursor.consume_phrase(&["has", "been", "dealt"])?;
        } else {
            cursor.consume_phrase(&["is", "dealt"])?;
        }
        return Ok(WindowBoundary::Event(WindowStep::CombatDamage));
    }
    if let Some(owner) = recognize_window_owner(cursor, window.owner, subject)? {
        merge_window_owner(cursor, window, owner)?;
    } else if word_at(cursor, 0, "the") {
        cursor.consume("the")?;
    }
    Ok(WindowBoundary::Period(recognize_window_period_name(
        cursor,
    )?))
}

fn merge_window_owner(
    cursor: &OracleTokenCursor<'_>,
    window: &mut TurnWindowNode,
    owner: WindowTurnOwner,
) -> MachineResult<()> {
    cursor.require(
        window.owner.is_none_or(|current| current == owner),
        "one turn owner per window",
    )?;
    window.owner = Some(owner);
    Ok(())
}

fn window_continuation_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    ["before", "after", "on"]
        .iter()
        .any(|head| word_at(cursor, offset, head))
}

/// One conjunction of window parts: `during PERIOD`, then any number of
/// `[,] before B`, `[,] after B`, `on TURN-OWNER turn` refinements.
fn recognize_window_conjunction(
    cursor: &mut OracleTokenCursor<'_>,
    subject: WindowSubject,
) -> MachineResult<TurnWindowNode> {
    let mut window = TurnWindowNode {
        owner: None,
        steps: StepSet::EVERY,
    };
    let mut first = true;
    loop {
        cursor.enter("window.part");
        let head = cursor.peek().map(str::to_ascii_lowercase);
        match head.as_deref() {
            Some("during") if first => recognize_during_part(cursor, &mut window, subject)?,
            Some(relation @ ("before" | "after")) => {
                let before = relation == "before";
                consume_current(cursor)?;
                let side = match recognize_window_boundary(cursor, &mut window, subject)? {
                    WindowBoundary::Event(step) if before => StepSet::before(step),
                    WindowBoundary::Event(step) => StepSet::from_step(step),
                    WindowBoundary::Period(period) => {
                        let edge = if before {
                            period.first()
                        } else {
                            period.last()
                        };
                        let edge = cursor.require_value(edge, "a nonempty boundary period")?;
                        if before {
                            StepSet::before(edge)
                        } else {
                            StepSet::after(edge)
                        }
                    }
                };
                window.steps = window.steps.intersect(side);
            }
            Some("on") if !first => {
                cursor.enter("window.on-turn");
                cursor.consume("on")?;
                let owner = recognize_window_owner(cursor, window.owner, subject)?
                    .ok_or_else(|| cursor.fail("a turn owner after `on`"))?;
                merge_window_owner(cursor, &mut window, owner)?;
                cursor.consume("turn")?;
            }
            _ => {
                return Err(cursor.fail(
                    "a window part: `during PERIOD`, `before EVENT`, `after EVENT` or `on TURN-OWNER turn`",
                ));
            }
        }
        first = false;
        if cursor.peek() == Some(",") && window_continuation_at(cursor, 1) {
            cursor.consume(",")?;
        } else if !window_continuation_at(cursor, 0) {
            break;
        }
    }
    cursor.enter("window.complete");
    cursor.require(!window.steps.is_empty(), "a window that admits some step")?;
    Ok(window)
}

/// `WINDOW (or WINDOW)*`: a disjunction of windows of one turn owner, e.g.
/// `before combat or during combat before blockers are declared`.
pub(in crate::oracle::canonical) fn recognize_turn_window(
    cursor: &mut OracleTokenCursor<'_>,
    subject: WindowSubject,
) -> MachineResult<TurnWindowNode> {
    let mut window = recognize_window_conjunction(cursor, subject)?;
    while word_at(cursor, 0, "or")
        && ["during", "before", "after"]
            .iter()
            .any(|head| word_at(cursor, 1, head))
    {
        cursor.enter("window.disjunction");
        cursor.consume("or")?;
        let next = recognize_window_conjunction(cursor, subject)?;
        cursor.require(
            next.owner == window.owner,
            "one turn owner across window alternatives",
        )?;
        window.steps = window.steps.union(next.steps);
    }
    Ok(window)
}

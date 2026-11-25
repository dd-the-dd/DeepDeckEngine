//! The one condition atom reader and the sentence-level conditional
//! instruction `If CONDITION, EFFECTS.` with an optional `Otherwise,
//! EFFECTS.` sentence (CR 608.2c).
//!
//! `recognize_condition_atom` is the only atom grammar of the condition
//! language (`condition.rs`). It reads subject first: a player subject
//! (`you`, `an opponent`, `each player`), an object subject (the source, a
//! referenced object), an object quantity (`a creature`, `no spells`), the
//! current turn (`it's your turn`), or a numeric operand (`X`, `that
//! creature's power`). Every parent supplies a `ConditionContext` whose scope
//! is the allow-list of atom families its evaluation moment can resolve.
//!
//! After a referenced object, one verb phrase grammar reads every predicate
//! of the object: the copula and possession predicates, the relative-clause
//! verb phrases of the shared object query (`shares a color with ...`,
//! `entered this turn`, `is attached to a creature`), a typed zone (`is
//! still on the battlefield`), the source's cast record, and the object's
//! (last known) controller (`you controlled it`). A player copula reads the
//! player designations (`the monarch`, `the starting player`) and poison; an
//! elided `you control` of a coordinated conjunct is restored from the
//! conjunct that states it.
//!
//! The effect-sequence parent owns the introducer position, the sentence
//! boundaries and the branch ranges; this child owns only the Boolean
//! program between `if` and its comma. A referenced object is a symbolic
//! binding resolved from the sequence's proven antecedents; it is never
//! guessed.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::*;
use crate::oracle::characteristic_vocabulary::{
    CharacteristicLexeme, lookup_object_selector_lexeme,
};
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// The object an object-state condition tests. Bindings stay symbolic until
/// lowering resolves them to the declared target or the bound card result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionObjectReference {
    /// A battlefield permanent: the source, a declared target, or the object
    /// observed by the triggering event.
    Permanent(PermanentEffectReference),
    /// A card declared as the target of an earlier card zone change.
    CardTarget(TargetBinding),
    /// The singular card result of the current card-collection scope (the
    /// looked-at, revealed or chosen card).
    CardFocus,
    /// A declared permanent target that an earlier action of the program
    /// moved off the battlefield, read with its last known information
    /// (CR 608.2h): the numeric properties and the type line its departure
    /// recorded.
    DepartedTarget(TargetBinding),
    /// The spell a cast event's record binds (CR 601.2, 603.2): `it`, `its`
    /// or `that spell` in a cast trigger, read on the stack.
    EventSpell,
    /// The object a cost of this ability (or of the same spell face) paid
    /// with: `the sacrificed creature`, `the discarded card` (CR 601.2h,
    /// 602.2b). Its paying cost records it as last known information when
    /// paid (CR 608.2h); the producing cost is proven after card
    /// composition, a missing or ambiguous producer is a binding failure.
    CostPaid(CostPaidConditionObject),
}

/// The cost-paid object a condition names by the participle of its cost
/// verb. Each kind is the symbolic binding its unique producing cost
/// records (`contextual_binding.rs`, `bindObjectAs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostPaidConditionObject {
    /// The permanent a `sacrificePermanent` cost sacrificed, read as it last
    /// existed on the battlefield.
    SacrificedPermanent,
    /// The one card a discard cost (chosen or at random) discarded, read as
    /// the card it is.
    DiscardedCard,
}

impl CostPaidConditionObject {
    /// The stack value the producing cost records.
    pub(in crate::oracle::canonical) const fn decision_id(self) -> &'static str {
        match self {
            Self::SacrificedPermanent => "sacrificedPermanentObject",
            Self::DiscardedCard => "discardedCardObject",
        }
    }

    /// The object domain its recorded object is read in.
    pub(in crate::oracle::canonical) const fn domain(self) -> ObjectKind {
        match self {
            Self::SacrificedPermanent => ObjectKind::Permanent,
            Self::DiscardedCard => ObjectKind::Card,
        }
    }
}

impl ConditionObjectReference {
    /// The ability's own source.
    pub(in crate::oracle::canonical) const SOURCE: Self = Self::Permanent(
        PermanentEffectReference::Source(LocalObjectReference::SourceSelf),
    );

    /// The condition subject an antecedent outside the permanent scope is:
    /// a declared target or an action result keeps its permanent reference
    /// (the lowering reads its binding), the cast record its own subject.
    fn from_antecedent(antecedent: ConditionPronounAntecedent) -> Self {
        match antecedent {
            ConditionPronounAntecedent::Target(binding) => {
                Self::Permanent(PermanentEffectReference::Target(binding))
            }
            ConditionPronounAntecedent::ActionResult(result) => {
                Self::Permanent(PermanentEffectReference::ActionResult(result))
            }
            ConditionPronounAntecedent::EventObject(reference) => Self::Permanent(reference),
            ConditionPronounAntecedent::EventSpell => Self::EventSpell,
        }
    }
}

/// The Boolean program of a sentence conditional, an intervening `if` or a
/// static `as long as`: one program over the one atom grammar.
pub(in crate::oracle::canonical) type SentenceConditionNode = ConditionNode;

/// A conditional instruction over the flat action list of its sequence.
/// `then_first..then_end` are the actions of the `If` sentence. `else_end`
/// bounds the branch taken when the condition is false, on either side of
/// the `If` sentence:
///
/// - after it (`else_end > then_end`): `then_end..else_end` are the actions
///   of its `Otherwise` sentence;
/// - before it (`else_end < then_first`): `else_end..then_first` are the
///   actions of the sentence a replacing `If COND, ... instead` sentence
///   replaces (`replacing_conditional.rs`). The condition is checked where
///   that replaced sentence begins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SentenceConditionalNode {
    pub condition: SentenceConditionNode,
    pub then_first: usize,
    pub then_end: usize,
    pub else_end: Option<usize>,
}

impl SentenceConditionalNode {
    /// The first action of the replaced sentence of an `instead` form.
    pub(in crate::oracle::canonical) fn replaced_first(&self) -> Option<usize> {
        self.else_end.filter(|end| *end < self.then_first)
    }

    /// The end of the `Otherwise` sentence, when one follows.
    pub(in crate::oracle::canonical) fn otherwise_end(&self) -> Option<usize> {
        self.else_end.filter(|end| *end > self.then_end)
    }

    /// The first action the conditional instruction governs.
    pub(in crate::oracle::canonical) fn start(&self) -> usize {
        self.replaced_first().unwrap_or(self.then_first)
    }

    /// The end (exclusive) of the actions the conditional governs.
    pub(in crate::oracle::canonical) fn end(&self) -> usize {
        self.otherwise_end().unwrap_or(self.then_end)
    }

    pub(in crate::oracle::canonical) fn shifted(&self, offset: usize) -> Self {
        Self {
            condition: self.condition.clone(),
            then_first: self.then_first + offset,
            then_end: self.then_end + offset,
            else_end: self.else_end.map(|end| end + offset),
        }
    }
}

/// Antecedents proven by the enclosing sequence at the condition's position.
pub(in crate::oracle::canonical) struct SentenceConditionContext<'a> {
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    pub effects: &'a EffectRecognitionContext,
    /// The singular card declared by the latest card-target zone change.
    pub card_target: Option<TargetBinding>,
    /// Whether the latest object-introducing action is a card-collection
    /// action whose singular result `it` may denote (checked at lowering).
    pub card_focus: bool,
    /// Permanent targets that an earlier action of this sequence moved out of
    /// the battlefield. Testing them needs last-known information.
    pub departed_targets: &'a [TargetBinding],
}

/// What a condition parent supplies to the one atom reader.
#[derive(Clone, Copy)]
pub(in crate::oracle::canonical) struct ConditionContext<'a> {
    /// The allow-list of atom families of the parent's evaluation moment.
    pub scope: ConditionScope,
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    /// The antecedents of an effect-program position; absent outside a
    /// program, where `it` can only denote the source (`source_subject`).
    pub program: Option<&'a SentenceConditionContext<'a>>,
    /// True when `it` denotes the ability's own source (the subject of a
    /// static ability is its source).
    pub source_subject: bool,
    /// The optional payment whose omission `you don't` reads.
    pub payment: Option<ChoiceBinding>,
    /// How the ability's source is attached, which `enchanted PERMANENT` /
    /// `equipped creature` names (CR 301.5a, 303.4a).
    pub source_attachment: Option<AttachedObjectRole>,
    /// Outside a program, the object `it` denotes when it is not the source
    /// (the attached permanent a static ability's subject names).
    pub subject_object: Option<ConditionObjectReference>,
}

impl<'a> ConditionContext<'a> {
    pub(in crate::oracle::canonical) fn standalone(
        scope: ConditionScope,
        name_links: &'a [ResolvedNameLink],
        name_failure: Option<&'a NameResolutionFailure>,
        source_subject: bool,
    ) -> Self {
        Self {
            scope,
            name_links,
            name_failure,
            program: None,
            source_subject,
            payment: None,
            source_attachment: None,
            subject_object: None,
        }
    }

    pub(in crate::oracle::canonical) fn program(
        scope: ConditionScope,
        context: &'a SentenceConditionContext<'a>,
    ) -> Self {
        Self {
            scope,
            name_links: context.name_links,
            name_failure: context.name_failure,
            program: Some(context),
            source_subject: false,
            payment: None,
            source_attachment: context.effects.source_attachment,
            subject_object: None,
        }
    }

    fn atom_kind_state(&self) -> &'static str {
        match self.scope {
            ConditionScope::Static => "condition.static.atom-kind",
            ConditionScope::Program | ConditionScope::TriggerGuard => {
                "condition.sentence.atom-kind"
            }
            ConditionScope::Unless => "condition.unless.atom-kind",
            ConditionScope::Only(_) => "condition.envelope.atom-kind",
        }
    }
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn contraction_is(cursor: &OracleTokenCursor<'_>, offset: usize, base: &str, tail: &str) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        let raw = raw.replace('\u{2019}', "'").to_ascii_lowercase();
        raw == format!("{base}'{tail}")
    })
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a token"))?
        .to_string();
    cursor.consume(&raw)
}

/// `SUBJECT can't|cannot` where SUBJECT restates the performer of the
/// previous instruction: `you`, `they`, `that player`, `the player`. The
/// width of the subject; the program lowering binds that instruction.
fn impossible_outcome_subject_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let width = if word_is(cursor, 0, "you") || word_is(cursor, 0, "they") {
        1
    } else if (word_is(cursor, 0, "that") || word_is(cursor, 0, "the"))
        && word_is(cursor, 1, "player")
    {
        2
    } else {
        return None;
    };
    (contraction_is(cursor, width, "can", "t") || word_is(cursor, width, "cannot")).then_some(width)
}

/// `you do` / `you don't` closing a condition atom (before its comma or a
/// coordinated atom): whether the outcome is the performed one. Lexical
/// lookahead only; `you don't control ...` and `you do so` are not outcomes.
fn optional_outcome_atom(cursor: &OracleTokenCursor<'_>) -> Option<bool> {
    if !word_is(cursor, 0, "you") {
        return None;
    }
    let performed = if word_is(cursor, 1, "do") {
        true
    } else if contraction_is(cursor, 1, "don", "t") {
        false
    } else {
        return None;
    };
    // A bare `If you do,` after an unguarded `you may ...` sentence is the
    // outcome connector the effect-program parent reads with its producer
    // (`binding_environment.rs`); this atom claims only the forms that parent
    // does not: a disjunction (`If you do or if you control ...`) and an
    // outcome of an optional instruction a sentence conditional guards.
    match cursor.peek_ahead(2) {
        Some("or") => performed.then_some(true),
        Some(",") => previous_sentence_guards_an_option(cursor).then_some(performed),
        _ => None,
    }
}

/// Whether the sentence before the current one (the cursor stands after
/// its opening `if`) is a sentence conditional whose branch grants an
/// option: `If it shares a creature type with this creature, you may reveal
/// it.` Lexical lookback only; the program lowering proves the instruction.
fn previous_sentence_guards_an_option(cursor: &OracleTokenCursor<'_>) -> bool {
    let position = cursor.position();
    let Some(period) = position.checked_sub(2) else {
        return false;
    };
    let tokens = cursor.token_slice(0, period + 1);
    if tokens.get(period).is_none_or(|token| token.raw != ".") {
        return false;
    }
    let start = tokens[..period]
        .iter()
        .rposition(|token| token.raw == ".")
        .map_or(0, |index| index + 1);
    let sentence = &tokens[start..period];
    let is = |index: usize, expected: &str| {
        sentence
            .get(index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    };
    let Some(comma) = sentence.iter().position(|token| token.raw == ",") else {
        return false;
    };
    is(0, "if") && is(comma + 1, "you") && is(comma + 2, "may")
}

/// True at a sentence-initial `if` whose condition this child may read, or
/// at the head of a die-result row (`1-9 |`, CR 706.3).
pub(in crate::oracle::canonical) fn sentence_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "if") || super::die_result_row_starts(cursor)
}

/// Reads `if CONDITION,`. The comma is consumed here because the sequence
/// parent has already proved that this `if` opens its sentence.
pub(in crate::oracle::canonical) fn recognize_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionNode> {
    // A die-result row is the conditional instruction of its results.
    if super::die_result_row_starts(cursor) {
        return super::recognize_die_result_row(cursor);
    }
    cursor.enter("condition.sentence.if");
    cursor.consume("if")?;
    cursor.enter("condition.sentence.boolean-program");
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, context)
    })?;
    cursor.enter("condition.sentence.separator");
    cursor.consume(",")?;
    cursor.enter("condition.sentence.instead");
    // `If COND, instead BODY.` (CR 608.2c): only the program parent that
    // gives the conditional its replacement form reads the medial `instead`.
    if context.effects.replacing_conditionals
        && word_is(cursor, 0, "instead")
        && !word_is(cursor, 1, "of")
    {
        cursor.consume("instead")?;
        return Ok(condition);
    }
    cursor.require(
        !word_is(cursor, 0, "instead"),
        "a conditional sentence that does not replace an earlier instruction with `instead`",
    )?;
    Ok(condition)
}

/// Reads a postposed `if CONDITION` (`Draw a card if you control a
/// Wizard.`, #11 Branch): the same condition as the preposed form, without
/// its comma; the sequence parent owns the sentence end and the gated range.
pub(in crate::oracle::canonical) fn recognize_postposed_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionNode> {
    cursor.enter("condition.sentence.postposed-if");
    cursor.consume("if")?;
    cursor.enter("condition.sentence.boolean-program");
    recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, context)
    })
}

/// One atom at an effect-program position (every family).
pub(in crate::oracle::canonical) fn recognize_sentence_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    // A coordinated atom may restate the introducer (`If you do or if you
    // control another Dinosaur`); the first atom's `if` is the parent's.
    if word_is(cursor, 0, "if") {
        cursor.enter("condition.sentence.restated-if");
        cursor.consume("if")?;
    }
    recognize_condition_atom(
        cursor,
        &ConditionContext::program(ConditionScope::Program, context),
    )
}

/// The one atom grammar of the condition language. The atom is read subject
/// first; the parent's scope is checked on the typed result.
pub(in crate::oracle::canonical) fn recognize_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter(context.atom_kind_state());
    let atom = read_condition_atom(cursor, context)?;
    cursor.enter("condition.allow-list");
    cursor.require(context.scope.allows_atom(&atom), context.scope.refusal())?;
    Ok(atom)
}

fn read_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    // Compare: the excess damage an earlier damage action of the program
    // dealt (`If excess damage was dealt this way`, CR 120.4a).
    if super::excess_damage_condition_starts(cursor) {
        return super::recognize_excess_damage_condition_atom(cursor);
    }
    // Compare over the resolving ability's own record: `this is the third
    // time this ability has resolved this turn`, `this ability has been
    // activated four or more times this turn` (`ability_history.rs`).
    if super::ability_history_condition_starts(cursor) {
        return super::recognize_ability_history_condition(cursor);
    }
    // Compare over the latest random or contested outcome: `you win the
    // flip`, `it comes up heads`, `you roll a 6`, `you win` (a clash).
    if let Some(atom) = super::try_recognize_random_outcome_condition(cursor, context.scope)? {
        return Ok(atom);
    }
    if let Some(atom) = super::try_recognize_history_static_atom(cursor, context)? {
        return Ok(atom);
    }
    // ObjectControl: `you controlled it`, `you don't control that creature`.
    if let Some((width, negated)) = object_control_verb(cursor) {
        return recognize_object_control_atom(cursor, context, width, negated);
    }
    // ActionOutcome: `you can't`, `you don't`; `the player can't`, `they
    // can't` restate the performer of the previous instruction (CR 609.3).
    if let Some(subject_width) = impossible_outcome_subject_width(cursor) {
        cursor.enter("condition.action-outcome.previous-action");
        for _ in 0..subject_width {
            consume_current(cursor)?;
        }
        consume_current(cursor)?;
        return Ok(ConditionAtom::ActionOutcome(
            ActionOutcomeNode::PreviousActionImpossible,
        ));
    }
    // `you don't control QUERY` is the negated control proposition, the
    // absence count of the shared controlled-permanent query, not a denied
    // payment.
    if word_is(cursor, 0, "you")
        && contraction_is(cursor, 1, "don", "t")
        && word_is(cursor, 2, "control")
    {
        return recognize_controlled_quantity_atom(cursor);
    }
    // ActionOutcome of the program's previous optional instruction (`If you
    // do`, `If you don't`, CR 608.2c) when the parent bound no optional
    // payment: the program lowering resolves the instruction, and fails
    // explicitly when the previous instruction is not an optional one.
    if context.payment.is_none()
        && context.program.is_some()
        && let Some(performed) = optional_outcome_atom(cursor)
    {
        cursor.enter("condition.action-outcome.optional-instruction");
        cursor.consume("you")?;
        consume_current(cursor)?;
        return Ok(ConditionAtom::ActionOutcome(
            ActionOutcomeNode::OptionalInstruction { performed },
        ));
    }
    if word_is(cursor, 0, "you") && contraction_is(cursor, 1, "don", "t") {
        cursor.enter("condition.action-outcome.payment-omitted");
        let payment = cursor.require_value(
            context.payment,
            "an optional payment bound by the parent that `you don't` denies",
        )?;
        cursor.consume("you")?;
        consume_current(cursor)?;
        return Ok(ConditionAtom::ActionOutcome(
            ActionOutcomeNode::CostNotPaid { payment },
        ));
    }
    // A conjunct whose `you control` the preceding conjunct stated (`if you
    // control an artifact and an enchantment`).
    if super::elided_controlled_presence_starts(cursor) {
        return super::recognize_elided_controlled_presence_atom(cursor);
    }
    // Compare / PlayerDesignation: `you're the monarch`, `an opponent is
    // poisoned`, `you weren't the starting player`.
    if let Some(atom) = try_recognize_player_copula_atom(cursor)? {
        return Ok(atom);
    }
    // EventHistory read by the shared past life clause.
    if let Some(atom) = super::try_recognize_life_history_atom(cursor)? {
        return Ok(atom);
    }
    // Compare: two controlled-permanent counts of different players.
    if super::opponent_count_comparison_starts(cursor) {
        return super::recognize_opponent_count_comparison_atom(cursor);
    }
    // Compare: the controller's permanents.
    if word_is(cursor, 0, "you") && word_is(cursor, 1, "control") {
        return recognize_controlled_permanents_atom(cursor);
    }
    // Compare: one measure of two players (`an opponent has more cards in
    // hand than you`, `you have more life than each opponent`).
    if super::player_comparative_starts(cursor) {
        return super::recognize_player_comparative_atom(cursor);
    }
    // Compare: the state of the player the binding environment names.
    if super::condition_player_reference_starts(cursor)
        && (word_is(cursor, 2, "controls") || word_is(cursor, 2, "has"))
    {
        return recognize_bound_player_state_atom(cursor);
    }
    // Compare: one player's measure against every other player's (`a
    // player has more life than each other player`).
    if super::extremum_comparative_condition_starts(cursor) {
        return super::recognize_extremum_comparative_condition(cursor);
    }
    // Compare: a player population's state.
    if super::collective_opponents_condition_starts(cursor) {
        return super::recognize_collective_opponents_condition(cursor).map(player_state_atom);
    }
    if super::quantified_player_condition_starts(cursor) {
        return super::recognize_quantified_player_condition(cursor).map(player_state_atom);
    }
    // CastFact: `you cast it`, `you cast this spell from your hand`.
    if word_is(cursor, 0, "you")
        && word_is(cursor, 1, "cast")
        && cast_object_follows(cursor, context)
    {
        return recognize_controller_cast_source_atom(cursor, context);
    }
    // CastFact: `{R} was spent to cast this spell`, `at least three red mana
    // was spent to cast it`, `no mana was spent to cast that spell`.
    if mana_spent_atom_starts(cursor) {
        return recognize_mana_spent_atom(cursor, context);
    }
    // Compare: the objects an earlier action affected, restated in the
    // active voice by its performer (`you discard a creature card this way`).
    if super::active_action_result_condition_starts(cursor) {
        return super::recognize_active_action_result_condition_atom(cursor);
    }
    // EventHistory with a player actor: `an opponent searched their library
    // this turn`, `a player cast two or more spells last turn`.
    if super::actor_event_history_starts(cursor) {
        return super::recognize_actor_event_history_atom(cursor);
    }
    // Compare / PlayerDesignation with the controller as subject.
    if contraction_is(cursor, 0, "you", "ve")
        || (word_is(cursor, 0, "you")
            && (word_is(cursor, 1, "have") || contraction_is(cursor, 1, "haven", "t")))
    {
        return super::recognize_controller_history_atom(cursor);
    }
    // Compare: the objects an earlier action affected (`a land card was
    // milled this way`).
    if super::action_result_condition_starts(cursor) {
        return super::recognize_action_result_condition_atom(cursor);
    }
    // EventHistory with an object quantity: `a creature died this turn`, `no
    // spells were cast last turn`.
    if super::object_event_history_starts(cursor) {
        return super::recognize_object_event_history_atom(cursor);
    }
    // Compare: an object quantity where it is now (`no creatures are on the
    // battlefield`, `a Warrior card is in your graveyard`, `three or more
    // creatures are attacking`).
    if super::located_quantity_atom_starts(cursor) {
        return super::recognize_located_quantity_atom(cursor);
    }
    // ObjectSatisfies: `there are no KIND counters on OBJECT` (CR 122.1).
    if existential_counter_follows(cursor) {
        return recognize_existential_counter_atom(cursor, context)
            .map(ConditionAtom::ObjectSatisfies);
    }
    // Compare: `creatures you control have total power N or greater` (sum).
    if super::collection_total_atom_starts(cursor) {
        return super::recognize_collection_total_atom(cursor);
    }
    // Compare: `there are N or more MEASURE among COLLECTION` (distinct values).
    if super::distinct_measure_atom_starts(cursor) {
        return super::recognize_distinct_measure_atom(cursor);
    }
    // Compare: `there are N or more CARDS in your graveyard`.
    if word_is(cursor, 0, "there") || contraction_is(cursor, 0, "there", "s") {
        return super::recognize_graveyard_count_atom(cursor);
    }
    // Compare: `X is N or more`, `that creature's power is greater than ...`.
    if word_is(cursor, 0, "x") && word_is(cursor, 1, "is") {
        return recognize_numeric_comparison_atom(cursor);
    }
    if object_property_operand_starts(cursor) {
        return recognize_numeric_relation_atom(cursor);
    }
    // TurnOwnership of another player: `it's not their turn`.
    if player_turn_parts(cursor).is_some() {
        return recognize_player_turn_atom(cursor, context);
    }
    // TurnOwnership.
    if contraction_is(cursor, 0, "it", "s")
        && (word_is(cursor, 1, "your") || (word_is(cursor, 1, "not") && word_is(cursor, 2, "your")))
    {
        return recognize_controller_turn_proposition(cursor).map(ConditionAtom::TurnOwnership);
    }
    // ObjectSatisfies with the attached permanent as subject (`equipped
    // creature is a Human`, `enchanted creature is red`).
    if super::attached_object_reference_starts(cursor) {
        return recognize_attached_object_atom(cursor, context).map(ConditionAtom::ObjectSatisfies);
    }
    // ObjectSatisfies / CastFact with the source as subject.
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        context.name_links,
        context.name_failure,
    ) {
        return recognize_source_condition_atom(cursor, context);
    }
    // ObjectSatisfies with an object an action or a cost VERBed as subject
    // (`the sacrificed creature was legendary`, `the discarded card was
    // multicolored`, `the exiled card is a God card`).
    if participle_subject_starts(cursor) {
        cursor.enter("condition.sentence.object.subject");
        let object = recognize_participle_subject(cursor, context)?;
        let phrase = recognize_object_verb_phrase(cursor, false)?;
        return finish_object_verb_phrase(cursor, context, object, phrase);
    }
    // CastFact / CastChoice of the ability's own source named by `it`: `it
    // was kicked`, `it escaped` (CR 702.33d, 702.138c). Only a spell or the
    // permanent it became has such a record.
    if word_is(cursor, 0, "it") && source_cast_record_width(cursor, 1).is_some() {
        return recognize_pronoun_cast_record(cursor, context);
    }
    // ObjectSatisfies with a referenced object as subject.
    if contraction_is(cursor, 0, "it", "s")
        || word_is(cursor, 0, "it")
        || word_is(cursor, 0, "its")
        || word_is(cursor, 0, "that")
    {
        return recognize_referenced_object_atom(cursor, context);
    }
    // Compare: a numeric subject of the shared definition grammar (`your
    // devotion to white is less than five`, `your life total is 5 or less`).
    if let Some(atom) = try_numeric_subject_comparison(cursor)? {
        return Ok(atom);
    }
    Err(cursor.fail(
        "a supported condition: a player's state or history, a count comparison, an object's state, turn ownership, a cast fact of this spell, or the outcome of an earlier action",
    ))
}

/// `EXPRESSION is COMPARISON`, where EXPRESSION is any operand of the
/// shared numeric definition grammar. The subject is read transactionally:
/// only an expression followed by `is|was` commits this reader.
fn try_numeric_subject_comparison(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ConditionAtom>> {
    let mut trial = cursor.clone();
    let Ok(left) = recognize_variable_definition_expression(&mut trial) else {
        return Ok(None);
    };
    if !(word_is(&trial, 0, "is") || word_is(&trial, 0, "was")) {
        return Ok(None);
    }
    *cursor = trial;
    cursor.enter("condition.numeric-subject.copula");
    consume_current(cursor)?;
    let (operator, right, halved) = recognize_comparison_tail(cursor)?;
    // `L OP half R` is `2 * L OP R`: the comparison needs no rounding.
    let left = if halved {
        NumericExpression::multiply(NumericExpression::from(2), left)
    } else {
        left
    };
    Ok(Some(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(left),
        operator,
        right,
    })))
}

/// After the copula: `less than [or equal to] R`, `greater than [or equal
/// to] R`, `equal to R`, a threshold (`5 or less`, `at least 10`, `exactly
/// 20`) or a bare number. R is a number or an expression of the shared
/// definition grammar, optionally `half R` (returned as `true`).
fn recognize_comparison_tail(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(NumericComparisonOperator, ConditionNumber, bool)> {
    cursor.enter("condition.numeric-subject.operator");
    let relation = if word_is(cursor, 0, "less") && word_is(cursor, 1, "than") {
        Some(if word_is(cursor, 2, "or") {
            NumericComparisonOperator::LessThanOrEqual
        } else {
            NumericComparisonOperator::LessThan
        })
    } else if word_is(cursor, 0, "greater") && word_is(cursor, 1, "than") {
        Some(if word_is(cursor, 2, "or") {
            NumericComparisonOperator::GreaterThanOrEqual
        } else {
            NumericComparisonOperator::GreaterThan
        })
    } else if word_is(cursor, 0, "equal") && word_is(cursor, 1, "to") {
        Some(NumericComparisonOperator::Equal)
    } else {
        None
    };
    if let Some(operator) = relation {
        consume_current(cursor)?;
        consume_current(cursor)?;
        if matches!(
            operator,
            NumericComparisonOperator::LessThanOrEqual
                | NumericComparisonOperator::GreaterThanOrEqual
        ) {
            cursor.consume_phrase(&["or", "equal", "to"])?;
        }
        cursor.enter("condition.numeric-subject.right");
        let halved = word_is(cursor, 0, "half");
        if halved {
            cursor.consume("half")?;
        }
        let number = cursor
            .peek()
            .and_then(|raw| raw.parse::<i64>().ok().or_else(|| parse_number_word(raw)))
            .filter(|_| !word_is(cursor, 0, "a") && !word_is(cursor, 0, "an"));
        let right = match number {
            Some(value) if !halved => {
                consume_current(cursor)?;
                ConditionNumber::Integer(value)
            }
            _ => ConditionNumber::Expression(recognize_variable_definition_expression(cursor)?),
        };
        return Ok((operator, right, halved));
    }
    let bare = cursor
        .peek()
        .and_then(|raw| raw.parse::<i64>().ok().or_else(|| parse_number_word(raw)))
        .filter(|_| !word_is(cursor, 1, "or"));
    if let Some(value) = bare {
        consume_current(cursor)?;
        return Ok((
            NumericComparisonOperator::Equal,
            ConditionNumber::Integer(value),
            false,
        ));
    }
    // The quantifier `no` counts objects; a number compared with `0 or
    // less` is an ordinary upper bound (a power can be negative).
    cursor.require(
        !word_is(cursor, 0, "no"),
        "a numeric threshold rather than `no`",
    )?;
    let threshold = recognize_count_threshold(cursor, false)?;
    let (operator, value) = threshold_comparison(threshold);
    Ok((operator, ConditionNumber::Integer(value), false))
}

/// `you control a|an|another PERMANENT` (at least one) or `you control N or
/// more|fewer [other] PERMANENTS`: one comparison over the controller's
/// permanents, shared by every condition parent.
pub(in crate::oracle::canonical) fn recognize_controlled_permanents_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.controlled-permanents.kind");
    // `you control no QUERY` / `you control exactly N QUERY`: the absence or
    // exact count read by the shared threshold (CR 109.4 control).
    if word_is(cursor, 2, "no") || word_is(cursor, 2, "exactly") {
        return recognize_controlled_quantity_atom(cursor);
    }
    let presence = cursor.peek_ahead(2).is_some_and(|word| {
        word.eq_ignore_ascii_case("a")
            || word.eq_ignore_ascii_case("an")
            || word.eq_ignore_ascii_case("another")
    });
    let query = if presence {
        recognize_controlled_permanent_presence_query(cursor)?
    } else {
        recognize_controlled_permanent_count_query(cursor)?
    };
    Ok(query.into_atom())
}

/// `you control THRESHOLD [other] QUERY` with a quantifier the fixed count
/// query does not read (`no`, `exactly N`), or its negation `you don't
/// control a|another QUERY`: the controller's count of the shared permanent
/// query compared with the shared threshold.
fn recognize_controlled_quantity_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.controlled-permanents.quantity.subject");
    cursor.consume("you")?;
    let negated = contraction_is(cursor, 0, "don", "t");
    if negated {
        consume_current(cursor)?;
    }
    cursor.consume("control")?;
    cursor.enter("condition.controlled-permanents.quantity.threshold");
    // `you don't control a|an|another QUERY` (CR 109.4): no permanent of the
    // query; `another` is the query's source exclusion.
    let mut another = false;
    let threshold = if negated {
        if word_is(cursor, 0, "a") || word_is(cursor, 0, "an") {
            consume_current(cursor)?;
        } else if word_is(cursor, 0, "another") {
            consume_current(cursor)?;
            another = true;
        } else {
            // `you don't control two or more QUERY` negates a count
            // threshold (fewer than two), not the absence of the query.
            let counted = cursor.peek().is_some_and(|raw| {
                ["exactly", "at", "no"]
                    .iter()
                    .any(|word| raw.eq_ignore_ascii_case(word))
                    || raw.parse::<i64>().is_ok()
                    || parse_number_word(raw).is_some()
            });
            cursor.require(
                !counted,
                "an absence (`a`, `an`, `another`) after `you don't control`, not a negated count",
            )?;
        }
        CountThreshold {
            operator: ThresholdOperator::AtMost,
            value: 0,
        }
    } else {
        recognize_count_threshold(cursor, false)?
    };
    cursor.enter("condition.controlled-permanents.quantity.query");
    let mut query = super::recognize_query_excluding_source(cursor, recognize_object_query)?;
    cursor.require(
        !(another && query.exclude_source),
        "one source exclusion for one object query",
    )?;
    query.exclude_source |= another;
    cursor.require(
        query.controller.is_none(),
        "an unscoped permanent selector after `you control`",
    )?;
    let exclude_source = query.exclude_source;
    let controlled = ObjectQueryNode {
        controller: Some(ObjectControllerScope::AbilityController),
        ..query
    };
    cursor.require(
        controlled.denotes_battlefield_permanents(),
        "a selector that establishes the permanent domain",
    )?;
    Ok(ConditionAtom::Compare(ConditionComparison::measured(
        ConditionPlayerSubject::Controller,
        PlayerStateMeasure::ControlledPermanents {
            predicate: controlled.predicate,
            exclude_source,
        },
        threshold,
    )))
}

/// `that player controls|has ...`: a player-state proposition whose subject
/// is resolved by the one binding-environment resolver (`recognize_that_player`).
/// Only the player the triggering event records has a condition lowering; a
/// player declared by the program is refused instead of being re-read.
fn recognize_bound_player_state_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.bound-player.subject");
    let environment = installed_binding_environment(cursor);
    let subject = match super::recognize_condition_player_reference(cursor, &environment)? {
        BoundPlayerReference::EventPlayer => ConditionPlayerSubject::EventPlayer,
        BoundPlayerReference::DefendingPlayer => ConditionPlayerSubject::DefendingPlayer,
        _ => {
            cursor.enter("condition.bound-player.binding");
            return Err(cursor.fail(
                "the player the triggering event records; a declared player is not read by a condition",
            ));
        }
    };
    cursor.enter("condition.bound-player.verb");
    if word_is(cursor, 0, "has") {
        cursor.consume("has")?;
        let proposition = recognize_player_possession_proposition(cursor, subject)?;
        return Ok(player_state_atom(PlayerStateConditionNode {
            subject,
            proposition,
        }));
    }
    cursor.consume("controls")?;
    if word_is(cursor, 0, "more") || word_is(cursor, 0, "fewer") {
        // `that player controls more QUERY than you`: two counts of one
        // query, the bound player's and the controller's.
        cursor.enter("condition.bound-player.comparative");
        let more = word_is(cursor, 0, "more");
        consume_current(cursor)?;
        let query = recognize_object_query(cursor)?;
        cursor.require(
            query.controller.is_none(),
            "a permanent query without its own controller relation",
        )?;
        cursor.require(
            query.predicate.supports_domain(ObjectKind::Permanent),
            "a predicate valid for a battlefield permanent",
        )?;
        cursor.enter("condition.bound-player.comparative.than");
        cursor.consume("than")?;
        cursor.consume("you")?;
        let measure = || PlayerStateMeasure::ControlledPermanents {
            predicate: query.predicate.clone(),
            exclude_source: false,
        };
        return Ok(ConditionAtom::Compare(ConditionComparison {
            subject,
            left: ConditionNumber::Player(measure()),
            operator: if more {
                NumericComparisonOperator::GreaterThan
            } else {
                NumericComparisonOperator::LessThan
            },
            right: ConditionNumber::ControllerMeasure(measure()),
        }));
    }
    let proposition = recognize_controlled_permanents_proposition(cursor)?;
    Ok(player_state_atom(PlayerStateConditionNode {
        subject,
        proposition,
    }))
}

/// A player-state proposition is a comparison over a measure of its subject.
pub(in crate::oracle::canonical) fn player_state_atom(
    node: PlayerStateConditionNode,
) -> ConditionAtom {
    ConditionAtom::Compare(ConditionComparison::measured(
        node.subject,
        node.proposition.measure,
        node.proposition.threshold,
    ))
}

/// After `you cast`: a pronoun or a reference to the ability's own source
/// (a cast fact), rather than a counted spell query (an event history).
fn cast_object_follows(cursor: &OracleTokenCursor<'_>, context: &ConditionContext<'_>) -> bool {
    ["it", "him", "her", "them"]
        .iter()
        .any(|pronoun| word_is(cursor, 2, pronoun))
        || source_reference_starts_at(
            cursor.position() + 2,
            cursor.peek_ahead(2),
            context.name_links,
            context.name_failure,
        )
}

/// A genitive property operand starts here: `that NOUN's power` (the binding
/// environment's object) or `this NOUN's power` (the source).
fn object_property_operand_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    super::bound_object_property_starts(cursor)
        || (word_is(cursor, 0, "this")
            && cursor
                .token_slice(cursor.position() + 1, cursor.position() + 2)
                .first()
                .is_some_and(|token| token.token_type == "possessiveTypeWord")
            && (word_is(cursor, 2, "power") || word_is(cursor, 2, "toughness")))
}

/// `EXPRESSION is|was COMPARISON` with a genitive property subject (`that
/// creature's power is 2 or less`, `that spell's mana value was 3 or less`,
/// `this creature's power is greater than X`): the right side is the shared
/// comparison tail (a relation before an expression, a threshold or a
/// number), the same one every numeric subject reads.
fn recognize_numeric_relation_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.relation.left");
    let left = recognize_numeric_expression(cursor, None)?;
    cursor.enter("condition.sentence.relation.copula");
    cursor.require(
        word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
        "`is` or `was` after the compared property",
    )?;
    consume_current(cursor)?;
    cursor.enter("condition.sentence.relation.operator");
    // A strict relation or `equal to` before an operand of the shared
    // numeric expression grammar (`greater than each other creature's
    // power`); every other comparison is the shared tail.
    let strict = (if word_is(cursor, 0, "greater") && word_is(cursor, 1, "than") {
        Some(NumericComparisonOperator::GreaterThan)
    } else if word_is(cursor, 0, "less") && word_is(cursor, 1, "than") {
        Some(NumericComparisonOperator::LessThan)
    } else if word_is(cursor, 0, "equal") && word_is(cursor, 1, "to") {
        Some(NumericComparisonOperator::Equal)
    } else {
        None
    })
    .filter(|_| !word_is(cursor, 2, "or") && !word_is(cursor, 2, "half"));
    if let Some(operator) = strict {
        consume_current(cursor)?;
        consume_current(cursor)?;
        cursor.enter("condition.sentence.relation.right");
        let right = recognize_numeric_expression(cursor, None)?;
        return Ok(ConditionAtom::Compare(ConditionComparison {
            subject: ConditionPlayerSubject::Controller,
            left: ConditionNumber::Expression(left),
            operator,
            right: ConditionNumber::Expression(right),
        }));
    }
    let (operator, right, halved) = recognize_comparison_tail(cursor)?;
    // `L OP half R` is `2 * L OP R`: the comparison needs no rounding.
    let left = if halved {
        NumericExpression::multiply(NumericExpression::from(2), left)
    } else {
        left
    };
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(left),
        operator,
        right,
    }))
}

/// `EXPRESSION is THRESHOLD`: the left operand is read by the shared numeric
/// expression grammar and the right one by the shared count threshold.
fn recognize_numeric_comparison_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.numeric.left");
    let left = recognize_numeric_expression(cursor, None)?;
    cursor.enter("condition.sentence.numeric.copula");
    cursor.consume("is")?;
    cursor.enter("condition.sentence.numeric.threshold");
    // The quantifier `no` counts objects; a number compared with `0 or
    // less` is an ordinary upper bound (a power can be negative).
    cursor.require(
        !word_is(cursor, 0, "no"),
        "a numeric threshold rather than `no`",
    )?;
    let threshold = recognize_count_threshold(cursor, false)?;
    let (operator, value) = threshold_comparison(threshold);
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(left),
        operator,
        right: ConditionNumber::Integer(value),
    }))
}

/// `this spell was kicked`, `this spell was cast from ZONE`, and
/// `this creature is CHARACTERISTIC` over the ability's own source.
fn recognize_source_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.source.subject");
    let spell = recognize_source_reference(cursor, context.name_links, context.name_failure)?;
    cursor.enter("condition.source.predicate");
    if word_is(cursor, 0, "was") && word_is(cursor, 1, "kicked") {
        cursor.consume("was")?;
        cursor.consume("kicked")?;
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::Kicked,
        }));
    }
    // `this creature escaped`, `NAME didn't escape` (CR 702.138c).
    if let Some((width, negated)) = escape_record_width(cursor, 0) {
        for _ in 0..width {
            consume_current(cursor)?;
        }
        cursor.require(
            spell == LocalObjectReference::SourceSelf,
            "the ability's own source, whose cast record keeps whether it escaped",
        )?;
        return Ok(ConditionAtom::CastChoice(CastChoiceNode {
            choice: CastChoice::Escaped,
            negated,
        }));
    }
    if word_is(cursor, 0, "was") && word_is(cursor, 1, "cast") {
        cursor.consume("was")?;
        cursor.consume("cast")?;
        cursor.enter("condition.source.cast-origin");
        cursor.require(
            word_is(cursor, 0, "from"),
            "`was kicked`, or `was cast from` a zone (an unqualified cast record of a spell is not read)",
        )?;
        cursor.consume("from")?;
        let (zone, negated) = recognize_cast_zone(cursor)?;
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::Cast {
                zone: Some(zone),
                negated,
            },
        }));
    }
    // `Jor Kadeen's power is 4 or greater`: the source's name in the
    // genitive before a numeric property, compared like `its power`.
    if previous_token_is_genitive(cursor) && numeric_property_word_follows(cursor) {
        cursor.enter("condition.source.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.source.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        cursor.enter("condition.source.comparison");
        let predicate = recognize_numeric_property_comparison_tail(cursor, property)?;
        return finish_referenced_object(
            cursor,
            context,
            ConditionObjectReference::SOURCE,
            predicate,
        )
        .map(ConditionAtom::ObjectSatisfies);
    }
    let phrase = recognize_object_verb_phrase(cursor, false)?;
    finish_object_verb_phrase(cursor, context, ConditionObjectReference::SOURCE, phrase)
}

/// `escaped`, `didn't escape`, `did not escape` at `offset`: its width and
/// whether it is negated.
fn escape_record_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(usize, bool)> {
    if word_is(cursor, offset, "escaped") {
        return Some((1, false));
    }
    if contraction_is(cursor, offset, "didn", "t") && word_is(cursor, offset + 1, "escape") {
        return Some((2, true));
    }
    (word_is(cursor, offset, "did")
        && word_is(cursor, offset + 1, "not")
        && word_is(cursor, offset + 2, "escape"))
    .then_some((3, true))
}

/// The cast-record predicate of a pronoun at `offset`: `was kicked`,
/// `escaped`, `didn't escape`. Its width only.
fn source_cast_record_width(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<usize> {
    if word_is(cursor, offset, "was") && word_is(cursor, offset + 1, "kicked") {
        return Some(2);
    }
    escape_record_width(cursor, offset).map(|(width, _)| width)
}

/// `it was kicked`, `it escaped`: the cast record of the object `it`
/// names. The pronoun's proven antecedent is read as for every object
/// predicate. A record of kicker or escape belongs only to a spell or the
/// permanent it became (CR 702.33d, 702.138c), so a pronoun with no proven
/// antecedent at all names the ability's own source; several candidates
/// stay ambiguous and fail.
fn recognize_pronoun_cast_record(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.cast-record.subject");
    cursor.consume("it")?;
    let object = resolve_cast_record_pronoun(cursor, context)?;
    cursor.enter("condition.cast-record.predicate");
    if let Some((width, negated)) = escape_record_width(cursor, 0) {
        for _ in 0..width {
            consume_current(cursor)?;
        }
        cursor.require(
            object == ConditionObjectReference::SOURCE,
            "the ability's own source, whose cast record keeps whether it escaped",
        )?;
        return Ok(ConditionAtom::CastChoice(CastChoiceNode {
            choice: CastChoice::Escaped,
            negated,
        }));
    }
    // `was kicked`: the kicked status the object keeps (CR 702.33d), read
    // by the shared object verb phrase.
    let phrase = recognize_object_verb_phrase(cursor, false)?;
    finish_object_verb_phrase(cursor, context, object, phrase)
}

/// The object `it` names before a cast-record predicate: the proven
/// antecedent when there is one, else the ability's own source.
fn resolve_cast_record_pronoun(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    let Some(program) = context.program else {
        cursor.enter("condition.cast-record.antecedent");
        return Ok(context
            .subject_object
            .unwrap_or(ConditionObjectReference::SOURCE));
    };
    let mut trial = cursor.clone();
    match resolve_object_anaphor(&mut trial, context) {
        Ok(object) => {
            *cursor = trial;
            Ok(object)
        }
        Err(failure) => {
            let no_candidate = super::permanent_subject::proven_read_antecedent(program.effects)
                .is_none()
                && program.card_target.is_none()
                && !program.card_focus;
            if no_candidate {
                cursor.enter("condition.cast-record.source");
                Ok(ConditionObjectReference::SOURCE)
            } else {
                Err(failure)
            }
        }
    }
}

/// The token before the cursor is a genitive (`NAME's`, `creature's`).
fn previous_token_is_genitive(cursor: &OracleTokenCursor<'_>) -> bool {
    let position = cursor.position();
    position > 0
        && cursor
            .token_slice(position - 1, position)
            .first()
            .is_some_and(|token| {
                let raw = token.raw.replace('\u{2019}', "'").to_ascii_lowercase();
                raw.ends_with("'s") || token.token_type == "possessiveTypeWord"
            })
}

/// `power`, `toughness` or `mana value` at the cursor.
fn numeric_property_word_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    word_is(cursor, 0, "power")
        || word_is(cursor, 0, "toughness")
        || (word_is(cursor, 0, "mana") && word_is(cursor, 1, "value"))
}

/// `you cast it`, `you cast this spell from your hand`: the controller cast
/// the ability's own source. A pronoun must denote that source; a permanent
/// remembers only whether it was cast and whether from a hand.
fn recognize_controller_cast_source_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.cast.subject");
    cursor.consume("you")?;
    cursor.consume("cast")?;
    cursor.enter("condition.sentence.cast.object");
    let pronoun = ["it", "him", "her", "them"]
        .iter()
        .any(|pronoun| word_is(cursor, 0, pronoun));
    let spell = if pronoun {
        consume_current(cursor)?;
        let reference = resolve_object_anaphor(cursor, context)?;
        cursor.enter("condition.sentence.cast.source-pronoun");
        cursor.require(
            reference == ConditionObjectReference::SOURCE,
            "a pronoun that denotes the ability's own source",
        )?;
        LocalObjectReference::SourceSelf
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?
    };
    if word_is(cursor, 0, "during") {
        // `If you cast this spell during your main phase` (Addendum): a
        // timing the spell keeps from its cast. A permanent's pronoun is
        // not read here; only the spell resolving with the record does.
        cursor.enter("condition.sentence.cast.timing");
        cursor.require(!pronoun, "`this spell` for a cast timing")?;
        cursor.consume_phrase(&["during", "your", "main", "phase"])?;
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::CastDuringMainPhase,
        }));
    }
    if !word_is(cursor, 0, "from") {
        return Ok(ConditionAtom::CastFact(CastFactNode {
            spell: CastSpellReference::Source(spell),
            fact: CastFact::Cast {
                zone: None,
                negated: false,
            },
        }));
    }
    cursor.consume("from")?;
    let (zone, negated) = recognize_cast_zone(cursor)?;
    cursor.enter("condition.sentence.cast.permanent-zone");
    cursor.require(
        !pronoun || zone == SourceCastZone::Hand,
        "`from your hand`; a permanent does not remember another cast zone",
    )?;
    Ok(ConditionAtom::CastFact(CastFactNode {
        spell: CastSpellReference::Source(spell),
        fact: CastFact::Cast {
            zone: Some(zone),
            negated,
        },
    }))
}

/// Lexical lookahead: `AMOUNT was|were|wasn't spent to cast` before the end
/// of the clause, where AMOUNT is the subject of the cast fact.
fn mana_spent_atom_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    (1..10)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "," | "." | ";" | ":"))
        })
        .any(|offset| {
            word_is(cursor, offset, "spent")
                && (word_is(cursor, offset - 1, "was")
                    || word_is(cursor, offset - 1, "were")
                    || contraction_is(cursor, offset - 1, "wasn", "t"))
                && word_is(cursor, offset + 1, "to")
                && word_is(cursor, offset + 2, "cast")
        })
}

/// The color words of mana amounts, in `ManaSpentAmount::SYMBOLS` order.
const MANA_AMOUNT_COLORS: [&str; 6] = ["white", "blue", "black", "red", "green", "colorless"];

/// The mana symbol at the cursor without its braces (`{R}` is `R`).
fn mana_symbol_at(cursor: &OracleTokenCursor<'_>) -> Option<String> {
    cursor
        .peek()
        .filter(|raw| raw.starts_with('{') && raw.ends_with('}') && raw.len() > 2)
        .map(|raw| raw[1..raw.len() - 1].to_ascii_uppercase())
}

/// `{R}`, `{U}{U}`, `{R}{G}` (each symbol at least its count), `at least
/// three red mana`, `five or more mana`, `no mana`: the amount subject of a
/// mana-spent fact. The source of spent mana (`mana from a Treasure`) and
/// snow or hybrid symbols are not recorded and fail here.
fn recognize_mana_spent_amount(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ManaSpentAmount> {
    cursor.enter("condition.cast.mana-spent.amount");
    if mana_symbol_at(cursor).is_some() {
        let mut counts = [0_u8; 6];
        while let Some(symbol) = mana_symbol_at(cursor) {
            cursor.enter("condition.cast.mana-spent.symbol");
            let index = ManaSpentAmount::SYMBOLS
                .iter()
                .position(|known| *known == symbol)
                .ok_or_else(|| {
                    cursor.fail(
                        "a colored or colorless mana symbol (snow, hybrid and generic mana are not recorded by symbol)",
                    )
                })?;
            counts[index] = counts[index].saturating_add(1);
            consume_current(cursor)?;
        }
        return Ok(ManaSpentAmount::Symbols(counts));
    }
    cursor.require(
        !(word_is(cursor, 0, "mana") && word_is(cursor, 1, "from")),
        "an amount of spent mana (the source that produced spent mana is not recorded)",
    )?;
    if word_is(cursor, 0, "no") && word_is(cursor, 1, "mana") {
        cursor.consume_phrase(&["no", "mana"])?;
        return Ok(ManaSpentAmount::Total {
            operator: NumericComparisonOperator::Equal,
            value: 0,
        });
    }
    let threshold = recognize_count_threshold(cursor, false)?;
    cursor.enter("condition.cast.mana-spent.color");
    let color = cursor.peek().and_then(|raw| {
        MANA_AMOUNT_COLORS
            .iter()
            .position(|color| raw.eq_ignore_ascii_case(color))
    });
    if color.is_some() {
        consume_current(cursor)?;
    }
    cursor.enter("condition.cast.mana-spent.noun");
    cursor.consume("mana")?;
    match color {
        Some(index) => {
            cursor.require(
                threshold.operator == ThresholdOperator::AtLeast,
                "a lower bound on the mana of one color",
            )?;
            let mut counts = [0_u8; 6];
            counts[index] = u8::try_from(threshold.value)
                .map_err(|_| cursor.fail("a bounded amount of mana of one color"))?;
            Ok(ManaSpentAmount::Symbols(counts))
        }
        None => {
            let (operator, value) = threshold_comparison(threshold);
            Ok(ManaSpentAmount::Total { operator, value })
        }
    }
}

/// `this spell` (or the source's name), `that spell` (the spell a cast event
/// observed) or `it`, which names the observed spell after a cast event and
/// otherwise must denote the ability's own source.
fn recognize_cast_spell_reference(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<CastSpellReference> {
    cursor.enter("condition.cast.mana-spent.spell");
    let event_spell = context
        .program
        .is_some_and(|program| program.effects.frame.event_spell)
        || cursor
            .binding_environment()
            .is_some_and(|environment| environment.frame.event_spell);
    if word_is(cursor, 0, "that") && word_is(cursor, 1, "spell") {
        cursor.require(
            event_spell,
            "a cast event that records the spell `that spell` names",
        )?;
        cursor.consume_phrase(&["that", "spell"])?;
        return Ok(CastSpellReference::EventSpell);
    }
    if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        if event_spell {
            return Ok(CastSpellReference::EventSpell);
        }
        // Outside a program (`sacrifice it unless {W} was spent to cast
        // it`), the guarded action's own antecedent names the object.
        let environment_object = cursor
            .binding_environment()
            .and_then(|environment| environment.object.as_ref())
            .map(|antecedent| ConditionObjectReference::Permanent(antecedent.reference));
        let reference = match environment_object {
            Some(reference)
                if context.program.is_none()
                    && !context.source_subject
                    && context.subject_object.is_none() =>
            {
                reference
            }
            _ => resolve_object_anaphor(cursor, context)?,
        };
        cursor.enter("condition.cast.mana-spent.source-pronoun");
        cursor.require(
            reference == ConditionObjectReference::SOURCE,
            "a pronoun that denotes the ability's own source or the spell a cast event observed",
        )?;
        return Ok(CastSpellReference::Source(LocalObjectReference::SourceSelf));
    }
    recognize_source_reference(cursor, context.name_links, context.name_failure)
        .map(CastSpellReference::Source)
}

/// `AMOUNT was|wasn't spent to cast SPELL` (CR 601.2h): the cast record of
/// the ability's source or of the spell a cast event observed.
fn recognize_mana_spent_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    let amount = recognize_mana_spent_amount(cursor)?;
    cursor.enter("condition.cast.mana-spent.verb");
    let negated = contraction_is(cursor, 0, "wasn", "t");
    cursor.require(
        negated || word_is(cursor, 0, "was") || word_is(cursor, 0, "were"),
        "`was`, `were` or `wasn't` before `spent`",
    )?;
    consume_current(cursor)?;
    cursor.consume_phrase(&["spent", "to", "cast"])?;
    let spell = recognize_cast_spell_reference(cursor, context)?;
    Ok(ConditionAtom::CastFact(CastFactNode {
        spell,
        fact: CastFact::ManaSpent { amount, negated },
    }))
}

/// The origin of a cast: a zone noun phrase, or `anywhere other than` one.
fn recognize_cast_zone(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(SourceCastZone, bool)> {
    cursor.enter("condition.source.cast-zone");
    let negated = word_is(cursor, 0, "anywhere");
    if negated {
        cursor.consume("anywhere")?;
        cursor.enter("condition.source.cast-zone.complement");
        cursor.consume("other")?;
        cursor.consume("than")?;
    }
    let zone = if word_is(cursor, 0, "your") && word_is(cursor, 1, "hand") {
        cursor.consume("your")?;
        cursor.consume("hand")?;
        SourceCastZone::Hand
    } else if word_is(cursor, 0, "a") && word_is(cursor, 1, "graveyard") {
        cursor.consume("a")?;
        cursor.consume("graveyard")?;
        SourceCastZone::Graveyard
    } else if word_is(cursor, 0, "exile") {
        crate::oracle::canonical::automata::consume_lemma(
            cursor,
            crate::oracle::canonical::automata::VerbLemma::Exile,
            crate::oracle::canonical::automata::VerbAgreement::Base,
        )?;
        SourceCastZone::Exile
    } else {
        return Err(cursor.fail("`your hand`, `a graveyard` or `exile`"));
    };
    cursor.enter("condition.source.cast-zone.complement-zone");
    cursor.require(
        !negated || zone == SourceCastZone::Hand,
        "`anywhere other than your hand`; another complement has no cast record",
    )?;
    Ok((zone, negated))
}

/// `enchanted PERMANENT` / `equipped creature` followed by the shared object
/// state predicate: the permanent the source is attached to, which the
/// runtime binds (CR 301.5a, 303.4a). The role must match the source face.
fn recognize_attached_object_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.attached-object.subject");
    if contraction_is(cursor, 1, "creature", "s") {
        // `equipped creature's power is greater than EXPR`: a property of the
        // attached permanent compared with the shared numeric grammar.
        cursor.enter("condition.attached-object.possessive");
        let role = if word_is(cursor, 0, "equipped") {
            AttachedObjectRole::Equipped
        } else {
            AttachedObjectRole::Enchanted
        };
        cursor.require(
            context.source_attachment == Some(role),
            "the attachment relation of the source face",
        )?;
        consume_current(cursor)?;
        consume_current(cursor)?;
        let object = ConditionObjectReference::Permanent(PermanentEffectReference::Attached(role));
        cursor.enter("condition.attached-object.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.attached-object.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        let (operator, right, halved) = recognize_comparison_tail(cursor)?;
        cursor.require(!halved, "a whole operand of the property comparison")?;
        let right = match right {
            ConditionNumber::Integer(value) => NumericExpression::from(value),
            ConditionNumber::Expression(expression) => expression,
            _ => return Err(cursor.fail("a numeric operand of the property comparison")),
        };
        let predicate = ObjectPredicate::NumericComparison {
            left: property,
            operator,
            right: Box::new(right),
        };
        return finish_referenced_object(cursor, context, object, predicate);
    }
    let role = recognize_attached_object_reference(cursor, context.source_attachment)?;
    let object = ConditionObjectReference::Permanent(PermanentEffectReference::Attached(role));
    let predicate = recognize_object_state_predicate(cursor, false)?;
    finish_referenced_object(cursor, context, object, predicate)
}

/// `it's a CHARACTERISTIC`, `it isn't …`, `that NOUN is …`, `it has mana
/// value N or less`, `its PROPERTY is N or greater`, `it's tapped`.
fn recognize_referenced_object_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.sentence.object.subject");
    if word_is(cursor, 0, "its") {
        cursor.consume("its")?;
        let object = resolve_object_anaphor(cursor, context)?;
        cursor.enter("condition.sentence.object.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.sentence.object.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        cursor.enter("condition.sentence.object.comparison");
        let predicate = recognize_numeric_property_comparison_tail(cursor, property)?;
        return finish_referenced_object(cursor, context, object, predicate)
            .map(ConditionAtom::ObjectSatisfies);
    }
    if contraction_is(cursor, 0, "it", "s") {
        consume_current(cursor)?;
        let object = resolve_object_anaphor(cursor, context)?;
        let phrase = recognize_object_verb_phrase(cursor, true)?;
        return finish_object_verb_phrase(cursor, context, object, phrase);
    }
    let object = if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        resolve_object_anaphor(cursor, context)?
    } else if word_is(cursor, 0, "that") && word_is(cursor, 1, "card") {
        cursor.consume("that")?;
        cursor.consume("card")?;
        resolve_card_anaphor(cursor, context)?
    } else {
        // `that NOUN`: the one nominal-anaphor resolver of the binding
        // environment. A condition reads the object's last known
        // information, so the nominal may name a departed event object (`If
        // that creature was a Villain` after `another creature dies`); a
        // nominal without an antecedent in scope fails there, never guessed.
        cursor.enter("condition.sentence.object.nominal-anaphor");
        let program = cursor.require_value(
            context.program,
            "a nominal anaphor of an effect program's proven antecedent",
        )?;
        let environment = condition_binding_environment(cursor, program.effects);
        match recognize_condition_nominal_object(cursor, &environment)? {
            ConditionPronounAntecedentOrObject::Object(reference) => {
                ConditionObjectReference::Permanent(reference)
            }
            ConditionPronounAntecedentOrObject::Antecedent(antecedent) => {
                ConditionObjectReference::from_antecedent(antecedent)
            }
        }
    };
    let phrase = recognize_object_verb_phrase(cursor, false)?;
    finish_object_verb_phrase(cursor, context, object, phrase)
}

/// What the verb phrase after a referenced object states.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ObjectVerbPhrase {
    /// The shared object predicate (`is red`, `has flying`, `shares a
    /// creature type with this creature`, `entered this turn`).
    Satisfies(ObjectPredicate),
    /// `is [not] [still] on the battlefield`, `is in your graveyard`.
    Zone {
        zones: Vec<ConditionZone>,
        negated: bool,
    },
    /// `was cast`, `wasn't cast`, `was cast from your hand`: the cast record
    /// the ability's own source keeps.
    Cast {
        zone: Option<SourceCastZone>,
        negated: bool,
    },
}

/// The width of a copula (`is`, `was`, `isn't`, `wasn't`, `is not`) at the
/// cursor and whether it negates; `copula_read` after `it's` (an optional
/// `not` only).
fn object_copula_width(cursor: &OracleTokenCursor<'_>, copula_read: bool) -> Option<(usize, bool)> {
    let (width, negated) = if copula_read {
        (0, false)
    } else if word_is(cursor, 0, "is") || word_is(cursor, 0, "was") {
        (1, false)
    } else if contraction_is(cursor, 0, "isn", "t") || contraction_is(cursor, 0, "wasn", "t") {
        (1, true)
    } else {
        return None;
    };
    if !negated && word_is(cursor, width, "not") {
        return Some((width + 1, true));
    }
    Some((width, negated))
}

/// The zones of `[still] ZONE [or ZONE]...` at `offset`, with the phrase
/// width. `still` restates that the object has not left the zone; it adds no
/// condition (CR 400.7: a new object would not be this one).
fn object_zones_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(Vec<ConditionZone>, usize)> {
    let mut width = usize::from(word_is(cursor, offset, "still"));
    let (first, first_width) = ConditionZone::at(cursor, offset + width)?;
    width += first_width;
    let mut zones = vec![first];
    while word_is(cursor, offset + width, "or")
        && let Some((zone, zone_width)) = ConditionZone::at(cursor, offset + width + 1)
    {
        zones.push(zone);
        width += 1 + zone_width;
    }
    Some((zones, width))
}

/// The verb phrase after a referenced object. In order: a relative-clause
/// verb phrase of the shared object query (`shares a color with ...`,
/// `entered this turn`, `is attached to a creature`), the cast record of the
/// source (`was cast`), a zone (`is still on the battlefield`), and the
/// copula or possession predicates of `recognize_object_state_predicate`.
fn recognize_object_verb_phrase(
    cursor: &mut OracleTokenCursor<'_>,
    copula_read: bool,
) -> MachineResult<ObjectVerbPhrase> {
    cursor.enter("condition.sentence.object.verb");
    if !copula_read && super::relative_predicate_starts(cursor) {
        cursor.enter("condition.sentence.object.relative-predicate");
        return super::recognize_relative_predicate(cursor).map(ObjectVerbPhrase::Satisfies);
    }
    if !copula_read
        && (word_is(cursor, 0, "was") || contraction_is(cursor, 0, "wasn", "t"))
        && word_is(cursor, 1, "cast")
    {
        cursor.enter("condition.sentence.object.cast");
        let negated = !word_is(cursor, 0, "was");
        consume_current(cursor)?;
        cursor.consume("cast")?;
        if !word_is(cursor, 0, "from") {
            return Ok(ObjectVerbPhrase::Cast {
                zone: None,
                negated,
            });
        }
        cursor.consume("from")?;
        let (zone, complement) = recognize_cast_zone(cursor)?;
        cursor.require(!(negated && complement), "one negation of a cast origin")?;
        return Ok(ObjectVerbPhrase::Cast {
            zone: Some(zone),
            negated: negated || complement,
        });
    }
    if let Some((copula_width, negated)) = object_copula_width(cursor, copula_read)
        && let Some((zones, zones_width)) = object_zones_at(cursor, copula_width)
    {
        cursor.enter("condition.sentence.object.zone");
        for _ in 0..copula_width + zones_width {
            consume_current(cursor)?;
        }
        return Ok(ObjectVerbPhrase::Zone { zones, negated });
    }
    recognize_object_state_predicate(cursor, copula_read).map(ObjectVerbPhrase::Satisfies)
}

/// Lexical lookahead: a pronoun subject (`its`, `it's`, `it`) of a condition
/// over an object its parent bound itself.
pub(in crate::oracle::canonical) fn bound_pronoun_predicate_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "its") || word_is(cursor, 0, "it") || contraction_is(cursor, 0, "it", "s")
}

/// The predicate a pronoun's verb phrase states over an object the parent
/// bound itself (the target of the instruction the condition guards, read
/// when that instruction resolves): `its power is less than or equal to
/// EXPR`, `it's white`, `it has flying`, `it shares a color with ...`. The
/// verb phrase is the one of every referenced object; a zone or a cast
/// record is no characteristic of the target and fails, and so does a
/// predicate without a battlefield filter (it would match every object).
pub(in crate::oracle::canonical) fn recognize_bound_pronoun_predicate(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("condition.bound-object.subject");
    let predicate = if word_is(cursor, 0, "its") {
        cursor.consume("its")?;
        cursor.enter("condition.bound-object.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.bound-object.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        cursor.enter("condition.bound-object.comparison");
        let (operator, right, halved) = recognize_comparison_tail(cursor)?;
        cursor.require(!halved, "a whole operand of the property comparison")?;
        let right = match right {
            ConditionNumber::Integer(value) => NumericExpression::from(value),
            ConditionNumber::Expression(expression) => expression,
            _ => return Err(cursor.fail("a numeric operand of the property comparison")),
        };
        ObjectPredicate::NumericComparison {
            left: property,
            operator,
            right: Box::new(right),
        }
    } else {
        let copula_read = contraction_is(cursor, 0, "it", "s");
        cursor.require(
            copula_read || word_is(cursor, 0, "it"),
            "the bound-object anaphor `it`, `it's` or `its`",
        )?;
        consume_current(cursor)?;
        match recognize_object_verb_phrase(cursor, copula_read)? {
            ObjectVerbPhrase::Satisfies(predicate) => predicate,
            ObjectVerbPhrase::Zone { .. } | ObjectVerbPhrase::Cast { .. } => {
                cursor.enter("condition.bound-object.characteristic");
                return Err(cursor.fail(
                    "a characteristic of the bound target (its zone or cast record is not one)",
                ));
            }
        }
    };
    cursor.enter("condition.bound-object.filter");
    cursor.require(
        predicate.supports_domain(ObjectKind::Permanent)
            && !crate::oracle::canonical::effects::lower_permanent_object_predicate(&predicate)
                .is_null(),
        "a characteristic of the bound permanent that lowers to a nonempty filter",
    )?;
    Ok(predicate)
}

/// The atom a referenced object and its verb phrase state.
fn finish_object_verb_phrase(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
    object: ConditionObjectReference,
    phrase: ObjectVerbPhrase,
) -> MachineResult<ConditionAtom> {
    match phrase {
        ObjectVerbPhrase::Satisfies(predicate) => {
            finish_referenced_object(cursor, context, object, predicate)
                .map(ConditionAtom::ObjectSatisfies)
        }
        ObjectVerbPhrase::Zone { zones, negated } => {
            cursor.enter("condition.sentence.object.zone-of-function");
            // A static or triggered ability of a card functions from its
            // zone of function (CR 113.6): only its parent can declare one
            // outside the battlefield, which this condition does not.
            let continuous = matches!(
                context.scope,
                ConditionScope::Static | ConditionScope::TriggerGuard | ConditionScope::Unless
            );
            // The intervening `if` of a triggered ability states the card
            // zone it functions from (`if this card is in your graveyard`):
            // the triggered parent lifts it into the ability's zone of
            // function and keeps it as the condition rechecked on resolution.
            let declared_by_trigger = context.scope == ConditionScope::TriggerGuard
                && !negated
                && matches!(
                    zones.as_slice(),
                    [ConditionZone::Graveyard | ConditionZone::Exile | ConditionZone::Hand]
                );
            cursor.require(
                !(continuous
                    && object == ConditionObjectReference::SOURCE
                    && zones != [ConditionZone::Battlefield]
                    && !declared_by_trigger),
                "a zone of function the ability's parent declares for its source outside the battlefield (CR 113.6)",
            )?;
            cursor.enter("condition.sentence.object.zone-object");
            cursor.require(
                !matches!(
                    object,
                    ConditionObjectReference::DepartedTarget(_)
                        | ConditionObjectReference::CostPaid(_)
                ),
                "an object whose current zone the condition can read",
            )?;
            Ok(ConditionAtom::ObjectZone(ObjectZoneConditionNode {
                object,
                zones,
                negated,
            }))
        }
        ObjectVerbPhrase::Cast { zone, negated } => {
            cursor.enter("condition.sentence.object.cast-record");
            cursor.require(
                object == ConditionObjectReference::SOURCE,
                "the cast record of the ability's own source (another object's cast is not recorded)",
            )?;
            // A permanent remembers whether it was cast and whether from a
            // hand (CR 601.2a); any other origin is not kept for it.
            cursor.require(
                zone.is_none_or(|zone| zone == SourceCastZone::Hand),
                "`from your hand`; a permanent does not remember another cast zone",
            )?;
            Ok(ConditionAtom::CastFact(CastFactNode {
                spell: CastSpellReference::Source(LocalObjectReference::SourceSelf),
                fact: CastFact::Cast { zone, negated },
            }))
        }
    }
}

/// `you control|controlled|don't control|didn't control|did not control`
/// before a bound object reference (`it`, `that NOUN`): the verb's width
/// after `you` and whether it is negated. A counted query (`you control a
/// Forest`) is the controlled-permanent comparison's, not this atom's.
fn object_control_verb(cursor: &OracleTokenCursor<'_>) -> Option<(usize, bool)> {
    if !word_is(cursor, 0, "you") {
        return None;
    }
    let (width, negated) = if word_is(cursor, 1, "control") || word_is(cursor, 1, "controlled") {
        (1, false)
    } else if (contraction_is(cursor, 1, "don", "t") || contraction_is(cursor, 1, "didn", "t"))
        && word_is(cursor, 2, "control")
    {
        (2, true)
    } else if (word_is(cursor, 1, "do") || word_is(cursor, 1, "did"))
        && word_is(cursor, 2, "not")
        && word_is(cursor, 3, "control")
    {
        (3, true)
    } else {
        return None;
    };
    let mut object = cursor.clone();
    object.advance_to(cursor.position() + 1 + width).ok()?;
    super::bound_object_reference_starts(&object).then_some((width, negated))
}

/// `you controlled it`, `you didn't control that permanent` (CR 108.4): the
/// controller relation of one bound permanent, which the runtime reads as
/// last known information once the permanent left (CR 608.2h). The object
/// is resolved like every object condition subject; nothing is guessed.
fn recognize_object_control_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
    width: usize,
    negated: bool,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.object-control.subject");
    cursor.consume("you")?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.object-control.object");
    let object = if cursor.peek().is_some_and(super::is_source_anaphor) {
        consume_current(cursor)?;
        resolve_object_anaphor(cursor, context)?
    } else {
        let program = cursor.require_value(
            context.program,
            "a nominal anaphor of an effect program's proven antecedent",
        )?;
        let environment = condition_binding_environment(cursor, program.effects);
        ConditionObjectReference::Permanent(recognize_nominal_object_read(cursor, &environment)?)
    };
    cursor.enter("condition.object-control.permanent");
    cursor.require(
        matches!(object, ConditionObjectReference::Permanent(_)),
        "a permanent whose controller (or last known controller) the condition reads",
    )?;
    Ok(ConditionAtom::ObjectControl(ObjectControlConditionNode {
        object,
        negated,
    }))
}

/// A player predicate after a copula.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlayerCopulaPredicate {
    /// `the monarch` (CR 724.1).
    Monarch,
    /// `the starting player` (CR 103.1).
    StartingPlayer,
    /// `poisoned`: one or more poison counters (CR 122.1f, glossary).
    Poisoned,
}

fn player_copula_predicate_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(PlayerCopulaPredicate, usize)> {
    if word_is(cursor, offset, "the") && word_is(cursor, offset + 1, "monarch") {
        Some((PlayerCopulaPredicate::Monarch, 2))
    } else if word_is(cursor, offset, "the")
        && word_is(cursor, offset + 1, "starting")
        && word_is(cursor, offset + 2, "player")
    {
        Some((PlayerCopulaPredicate::StartingPlayer, 3))
    } else if word_is(cursor, offset, "poisoned") {
        Some((PlayerCopulaPredicate::Poisoned, 1))
    } else {
        None
    }
}

/// A player copula at `offset`: its width, whether it negates and whether
/// it is past (`is`, `are`, `was`, `were`, their contracted negations, and
/// a following `not`).
fn player_copula_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<(usize, bool, bool)> {
    let raw = cursor
        .peek_ahead(offset)?
        .replace('\u{2019}', "'")
        .to_ascii_lowercase();
    let (negated, past) = match raw.as_str() {
        "is" | "are" => (false, false),
        "was" | "were" => (false, true),
        "isn't" | "aren't" => (true, false),
        "wasn't" | "weren't" => (true, true),
        _ => return None,
    };
    if !negated && word_is(cursor, offset + 1, "not") {
        return Some((2, true, past));
    }
    Some((1, negated, past))
}

/// The subject and copula of a player predicate at the cursor.
struct PlayerCopulaParts {
    /// `None` for a bound player (`that player`, `defending player`), which
    /// the binding environment resolves.
    subject: Option<ConditionPlayerSubject>,
    subject_width: usize,
    copula_width: usize,
    negated: bool,
    past: bool,
}

impl PlayerCopulaParts {
    fn width(&self) -> usize {
        self.subject_width + self.copula_width
    }
}

fn player_copula_parts(cursor: &OracleTokenCursor<'_>) -> Option<PlayerCopulaParts> {
    // `you're [not]`: the subject and the copula in one token.
    if contraction_is(cursor, 0, "you", "re") {
        let negated = word_is(cursor, 1, "not");
        return Some(PlayerCopulaParts {
            subject: Some(ConditionPlayerSubject::Controller),
            subject_width: 1,
            copula_width: usize::from(negated),
            negated,
            past: false,
        });
    }
    let (subject, subject_width) = if word_is(cursor, 0, "you") {
        (Some(ConditionPlayerSubject::Controller), 1)
    } else if let Some((quantifier, population)) = super::quantified_player_head(cursor) {
        (
            Some(ConditionPlayerSubject::Quantified {
                quantifier,
                population,
            }),
            2,
        )
    } else if (word_is(cursor, 0, "that") || word_is(cursor, 0, "defending"))
        && word_is(cursor, 1, "player")
    {
        (None, 2)
    } else {
        return None;
    };
    let (copula_width, negated, past) = player_copula_at(cursor, subject_width)?;
    Some(PlayerCopulaParts {
        subject,
        subject_width,
        copula_width,
        negated,
        past,
    })
}

/// `you're the monarch`, `an opponent is the monarch`, `defending player is
/// poisoned`, `you weren't the starting player`: a player subject, a copula
/// and a player predicate. `None` (nothing consumed) when no player
/// predicate follows a player copula, so the other readers keep the clause.
fn try_recognize_player_copula_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ConditionAtom>> {
    let Some(parts) = player_copula_parts(cursor) else {
        return Ok(None);
    };
    let Some((predicate, predicate_width)) = player_copula_predicate_at(cursor, parts.width())
    else {
        return Ok(None);
    };
    cursor.enter("condition.player-copula.subject");
    let subject = match parts.subject {
        Some(subject) => {
            for _ in 0..parts.subject_width {
                consume_current(cursor)?;
            }
            subject
        }
        None => {
            let environment = installed_binding_environment(cursor);
            match super::recognize_condition_player_reference(cursor, &environment)? {
                BoundPlayerReference::EventPlayer => ConditionPlayerSubject::EventPlayer,
                BoundPlayerReference::DefendingPlayer => ConditionPlayerSubject::DefendingPlayer,
                _ => {
                    cursor.enter("condition.player-copula.binding");
                    return Err(cursor.fail(
                        "the player the triggering event records; a declared player is not read by a condition",
                    ));
                }
            }
        }
    };
    cursor.enter("condition.player-copula.copula");
    for _ in 0..parts.copula_width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.player-copula.tense");
    // A designation held now is not a record of a past one; the starting
    // player is a fact of the game in either tense.
    cursor.require(
        !parts.past || predicate == PlayerCopulaPredicate::StartingPlayer,
        "the present tense of a player's current state (a past state is not recorded)",
    )?;
    cursor.enter("condition.player-copula.predicate");
    for _ in 0..predicate_width {
        consume_current(cursor)?;
    }
    let designation = match predicate {
        PlayerCopulaPredicate::Poisoned => {
            let counter = crate::oracle::counter_name::CounterName::from_parts(["poison"])
                .ok_or_else(|| cursor.fail("the poison counter name"))?;
            let threshold = if parts.negated {
                CountThreshold {
                    operator: ThresholdOperator::AtMost,
                    value: 0,
                }
            } else {
                CountThreshold::at_least(1)
            };
            return Ok(Some(ConditionAtom::Compare(ConditionComparison::measured(
                subject,
                PlayerStateMeasure::PlayerCounters(counter),
                threshold,
            ))));
        }
        PlayerCopulaPredicate::Monarch => PlayerDesignation::Monarch,
        PlayerCopulaPredicate::StartingPlayer => PlayerDesignation::StartingPlayer,
    };
    cursor.enter("condition.player-copula.designation-subject");
    // The defending player of a static combat check is bound by its
    // measures only (`condition_reads_defending_player`); a designation of
    // that player has no such binding and is refused, never guessed.
    cursor.require(
        !matches!(
            subject,
            ConditionPlayerSubject::DefendingPlayer | ConditionPlayerSubject::OpponentsTogether
        ),
        "the controller, a quantified player or the triggering event's player as the designation holder",
    )?;
    Ok(Some(ConditionAtom::PlayerDesignation(
        PlayerDesignationNode {
            designation,
            held: !parts.negated,
            subject,
        },
    )))
}

/// `it's [not] POSSESSOR turn`, `it is [not] ...`, `it isn't ...` where
/// POSSESSOR is `their`, `that player's` or `an opponent's`: the width of
/// the subject and copula, whether it negates, and the possessor's width.
fn player_turn_parts(cursor: &OracleTokenCursor<'_>) -> Option<(usize, bool, usize)> {
    let (mut width, mut negated) = if contraction_is(cursor, 0, "it", "s") {
        (1, false)
    } else if word_is(cursor, 0, "it") && word_is(cursor, 1, "is") {
        (2, false)
    } else if word_is(cursor, 0, "it") && contraction_is(cursor, 1, "isn", "t") {
        (2, true)
    } else {
        return None;
    };
    if !negated && word_is(cursor, width, "not") {
        width += 1;
        negated = true;
    }
    let possessor = if word_is(cursor, width, "their") {
        1
    } else if (word_is(cursor, width, "that") && contraction_is(cursor, width + 1, "player", "s"))
        || (word_is(cursor, width, "an") && contraction_is(cursor, width + 1, "opponent", "s"))
    {
        2
    } else {
        return None;
    };
    word_is(cursor, width + possessor, "turn").then_some((width, negated, possessor))
}

/// `it's not their turn` (the player the triggering event records),
/// `it's an opponent's turn` (CR 102.1, 500.1): whose turn it is, for a
/// player other than the controller. `their` without a recorded event
/// player is a committed diagnostic.
fn recognize_player_turn_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.player-turn.subject");
    let (width, negated, possessor) = player_turn_parts(cursor)
        .ok_or_else(|| cursor.fail("`it's [not] their|that player's|an opponent's turn`"))?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.player-turn.possessor");
    let player = if word_is(cursor, 0, "an") {
        ConditionPlayerSubject::Quantified {
            quantifier: PlayerQuantifier::Some,
            population: PlayerPopulation::Opponents,
        }
    } else {
        let environment = installed_binding_environment(cursor);
        let event_player = context
            .program
            .is_some_and(|program| program.effects.frame.event_player)
            || environment.frame.event_player;
        cursor.require(
            event_player && environment.player.is_none() && environment.player_target.is_none(),
            "the one player the triggering event records for `their turn`",
        )?;
        ConditionPlayerSubject::EventPlayer
    };
    for _ in 0..possessor {
        consume_current(cursor)?;
    }
    cursor.enter("condition.player-turn.turn");
    cursor.consume("turn")?;
    Ok(ConditionAtom::PlayerTurn(PlayerTurnNode {
        player,
        is_turn: !negated,
    }))
}

/// The environment a condition's nominal anaphor reads: the condition
/// position's proven antecedent (its scope may be the one after the gated
/// action of a postposed condition), with the declared selector the
/// enclosing program published for that same antecedent when the condition
/// context does not carry it (`Target creature you control gains
/// indestructible until end of turn. Draw a card if that creature has a
/// +1/+1 counter on it.`). The selector belongs to the one declaration both
/// name; a different antecedent is never borrowed.
fn condition_binding_environment(
    cursor: &OracleTokenCursor<'_>,
    effects: &EffectRecognitionContext,
) -> BindingEnvironment {
    let mut environment = BindingEnvironment::for_permanent_context(effects);
    let installed = installed_binding_environment(cursor);
    // The program's published antecedents outside the permanent scope: the
    // one object an earlier action selected or moved, and the latest
    // declared target that need not be a permanent (a countered spell).
    inherit_installed_action_result(&mut environment, cursor);
    environment.declared_target = installed.declared_target.clone();
    if environment.object.is_none() {
        // The singular object the program parent installed for this action
        // (`Counter target spell. If that spell ...`) when the context
        // proves none of its own.
        environment.object = installed.object;
    } else if let (Some(object), Some(published)) = (environment.object.as_mut(), installed.object)
        && object.selector.is_none()
        && object.reference == published.reference
    {
        object.selector = published.selector;
    }
    environment
}

/// The width of a possession verb at the cursor (`has`, `had`, `have`,
/// `doesn't have`, `didn't have`, `does not have`, `did not have`) and
/// whether its auxiliary negates it.
fn possession_verb_width(cursor: &OracleTokenCursor<'_>) -> Option<(usize, bool)> {
    if word_is(cursor, 0, "has") || word_is(cursor, 0, "had") || word_is(cursor, 0, "have") {
        return Some((1, false));
    }
    let auxiliary = contraction_is(cursor, 0, "doesn", "t")
        || contraction_is(cursor, 0, "didn", "t")
        || contraction_is(cursor, 0, "don", "t");
    if auxiliary && word_is(cursor, 1, "have") {
        return Some((2, true));
    }
    let full = word_is(cursor, 0, "does") || word_is(cursor, 0, "did") || word_is(cursor, 0, "do");
    (full && word_is(cursor, 1, "not") && word_is(cursor, 2, "have")).then_some((3, true))
}

/// The verb phrase after a referenced object: a copula with a registered
/// status list or a shared object query, or a possession verb (negated by
/// its auxiliary) with what the object has. `copula_read` is true after
/// `it's`.
fn recognize_object_state_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    copula_read: bool,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("condition.sentence.object.verb");
    let mut negated = false;
    if !copula_read {
        if let Some((width, possession_negated)) = possession_verb_width(cursor) {
            for _ in 0..width {
                consume_current(cursor)?;
            }
            cursor.enter("condition.sentence.object.possession");
            let predicate = recognize_possessed_characteristic(cursor)?;
            return Ok(if possession_negated {
                ObjectPredicate::Not(Box::new(predicate))
            } else {
                predicate
            });
        }
        let verb = cursor
            .peek()
            .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase())
            .ok_or_else(|| cursor.fail("`is`, `was`, `isn't`, `wasn't`, `has` or `had`"))?;
        match verb.as_str() {
            "is" | "was" => consume_current(cursor)?,
            "isn't" | "wasn't" => {
                consume_current(cursor)?;
                negated = true;
            }
            _ => {
                return Err(cursor.fail(
                    "`is`, `was`, `isn't`, `wasn't`, `has`, `had`, `doesn't have` or `didn't have`",
                ));
            }
        }
    }
    if word_is(cursor, 0, "not") {
        cursor.consume("not")?;
        negated = !negated;
    }
    cursor.enter("condition.sentence.object.characteristic");
    let predicate = if let Some(statuses) = recognize_status_list(cursor)? {
        statuses
    } else {
        if word_is(cursor, 0, "a") || word_is(cursor, 0, "an") {
            consume_current(cursor)?;
        }
        let query = recognize_object_query(cursor)?;
        cursor.enter("condition.sentence.object.relation");
        cursor.require(
            query.controller.is_none(),
            "an object characteristic without a controller relation",
        )?;
        query.predicate
    };
    Ok(if negated {
        ObjectPredicate::Not(Box::new(predicate))
    } else {
        predicate
    })
}

/// What an object has after a possession verb: the predicate a `with`
/// qualifier of the shared object query states for the same object (`has a
/// +1/+1 counter on it` is `with a +1/+1 counter on it`). In order:
/// counters (CR 122.1), the chosen name (CR 201.2), a comparative property
/// (`greater power than this creature`), a keyword ability without or with
/// parameters (`flying`, `first strike`, `toxic`, `a cycling ability`), and
/// a numeric property comparison (`mana value 2 or less`).
fn recognize_possessed_characteristic(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    // `has a +1/+1 counter on it`, `had no counters on it`, `has three or
    // more quest counters on it`: the shared counter qualifier.
    if super::counter_qualifier_starts(cursor, 0) {
        return super::recognize_counter_qualifier(cursor);
    }
    // `has the chosen name`, `has that name` (CR 201.2, 608.2d): the shared
    // chosen-name relation `with the chosen name` reads, with the same proof
    // of an earlier `choose` of the program; outside a program it has no
    // producer.
    if word_is(cursor, 0, "the") && word_is(cursor, 1, "chosen") && word_is(cursor, 2, "name")
        || word_is(cursor, 0, "that") && word_is(cursor, 1, "name")
    {
        cursor.enter("condition.sentence.object.chosen-name");
        cursor.require(
            super::resolution_choice_in_scope(cursor),
            "a card name an earlier `choose` instruction of this program records",
        )?;
        let width = if word_is(cursor, 0, "the") { 3 } else { 2 };
        for _ in 0..width {
            consume_current(cursor)?;
        }
        return Ok(ObjectPredicate::Relation(ObjectRelation::ChosenName));
    }
    if comparative_operator_at(cursor, 0).is_some() {
        return recognize_possessed_comparative(cursor);
    }
    // `has flying`, `has first strike`: the shared parameterless keyword.
    if cursor.peek().is_some_and(keyword_possession_word) {
        cursor.enter("condition.sentence.object.keyword");
        return recognize_parameterless_keyword(cursor).map(ObjectPredicate::HasKeyword);
    }
    // `has toxic`, `has a kicker ability` (CR 702): a keyword ability with
    // parameters, the qualifier atom of `with toxic`.
    let article = usize::from(word_is(cursor, 0, "a") || word_is(cursor, 0, "an"));
    if let Some(keyword) = cursor
        .peek_ahead(article)
        .and_then(KeywordAbilityName::from_oracle_word)
    {
        cursor.enter("condition.sentence.object.keyword-ability");
        for _ in 0..=article {
            consume_current(cursor)?;
        }
        if article == 1 {
            cursor.consume("ability")?;
        }
        return Ok(ObjectPredicate::Qualifier(ObjectQualifier::KeywordAbility(
            keyword,
        )));
    }
    cursor.enter("condition.sentence.object.property");
    let property = recognize_selector_numeric_property(cursor)?;
    cursor.enter("condition.sentence.object.comparison");
    recognize_numeric_property_comparison_tail(cursor, property)
}

/// A word that begins a parameterless keyword (`flying`, `first strike`,
/// `double strike`); a numeric property (`mana value`, `power`,
/// `toughness`) never does.
fn keyword_possession_word(raw: &str) -> bool {
    let property = ["mana", "power", "toughness"]
        .iter()
        .any(|word| raw.eq_ignore_ascii_case(word));
    !property
        && (parameterless_keyword_from_oracle_word(raw).is_some()
            || raw.eq_ignore_ascii_case("first")
            || raw.eq_ignore_ascii_case("double"))
}

/// Width of `[equal or] greater|lesser` at `offset` and the operator it
/// states for the object against its anchor (CR 107.1).
fn comparative_operator_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(usize, NumericComparisonOperator)> {
    let inclusive = word_is(cursor, offset, "equal") && word_is(cursor, offset + 1, "or");
    let at = if inclusive { offset + 2 } else { offset };
    let raw = cursor.peek_ahead(at)?.to_ascii_lowercase();
    let operator = match (raw.as_str(), inclusive) {
        ("greater", false) => NumericComparisonOperator::GreaterThan,
        ("lesser", false) => NumericComparisonOperator::LessThan,
        ("greater", true) => NumericComparisonOperator::GreaterThanOrEqual,
        ("lesser", true) => NumericComparisonOperator::LessThanOrEqual,
        _ => return None,
    };
    Some((at + 1 - offset, operator))
}

/// `greater power than this creature`, `greater power or toughness than
/// Hulkling`, `lesser mana value than that creature` (CR 107.1): each
/// property of the object compared with the same property of the anchor,
/// a disjunction over `or`. The anchor is the ability's source or a bound
/// object the shared binding environment resolves; its property is the
/// shared bound-object read (live, else last known information).
fn recognize_possessed_comparative(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("condition.sentence.object.comparative");
    let (width, operator) = cursor.require_value(
        comparative_operator_at(cursor, 0),
        "`greater`, `lesser`, `equal or greater` or `equal or lesser`",
    )?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.sentence.object.comparative-property");
    let mut properties = vec![recognize_selector_numeric_property(cursor)?];
    while word_is(cursor, 0, "or") && !word_is(cursor, 1, "than") {
        cursor.consume("or")?;
        properties.push(recognize_selector_numeric_property(cursor)?);
    }
    cursor.enter("condition.sentence.object.comparative-anchor");
    cursor.consume("than")?;
    let anchor = if super::bound_object_reference_starts(cursor) {
        let environment = installed_binding_environment(cursor);
        super::recognize_bound_object_reference(cursor, &environment)?
    } else {
        let names = cursor.source_names().to_vec();
        PermanentEffectReference::Source(recognize_source_reference(cursor, &names, None)?)
    };
    let mut comparisons = Vec::with_capacity(properties.len());
    for property in properties {
        let right = super::bound_object_numeric_property(
            anchor,
            property,
            ObjectPredicate::ObjectKind(ObjectKind::Permanent),
        )
        .map_err(|expected| cursor.fail(expected))?;
        comparisons.push(ObjectPredicate::NumericComparison {
            left: property,
            operator,
            right: Box::new(right),
        });
    }
    Ok(if comparisons.len() == 1 {
        comparisons.remove(0)
    } else {
        ObjectPredicate::Union(comparisons)
    })
}

/// `tapped`, `attacking`, `monstrous`, `enchanted or equipped`: registered
/// statuses and designations of the object, one or a coordinated list with
/// one connector (their union or intersection). `None` when no registered
/// status starts here, so that the shared object query reads the phrase.
fn recognize_status_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectPredicate>> {
    let status_at = |cursor: &OracleTokenCursor<'_>, offset: usize| {
        cursor
            .peek_ahead(offset)
            .and_then(super::lookup_status_predicate)
    };
    let Some(first) = status_at(cursor, 0) else {
        return Ok(None);
    };
    // A status adjective that modifies a following noun (`a tapped
    // creature`) belongs to the object query.
    let ends_here = |cursor: &OracleTokenCursor<'_>, offset: usize| {
        cursor.peek_ahead(offset).is_none_or(|raw| {
            matches!(
                raw.to_ascii_lowercase().as_str(),
                "," | "." | "and" | "or" | ";" | ":"
            )
        })
    };
    if !ends_here(cursor, 1) {
        return Ok(None);
    }
    cursor.enter("condition.sentence.object.status");
    consume_current(cursor)?;
    let mut members = vec![first];
    let mut connector: Option<String> = None;
    while (word_is(cursor, 0, "or") || word_is(cursor, 0, "and"))
        && status_at(cursor, 1).is_some()
        && ends_here(cursor, 2)
    {
        let word = cursor
            .peek()
            .expect("the connector was checked")
            .to_ascii_lowercase();
        cursor.require(
            connector.as_deref().is_none_or(|previous| previous == word),
            "one connector in a status list",
        )?;
        consume_current(cursor)?;
        let member = status_at(cursor, 0).expect("the status was checked");
        consume_current(cursor)?;
        members.push(member);
        connector = Some(word);
    }
    Ok(Some(match (members.len(), connector.as_deref()) {
        (1, _) => members.pop().expect("one status was read"),
        (_, Some("and")) => ObjectPredicate::Intersection(members),
        _ => ObjectPredicate::Union(members),
    }))
}

/// The width of `there is|are` / `there's` at the cursor, if present.
fn existential_width(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    if contraction_is(cursor, 0, "there", "s") {
        Some(1)
    } else if word_is(cursor, 0, "there") && (word_is(cursor, 1, "is") || word_is(cursor, 1, "are"))
    {
        Some(2)
    } else {
        None
    }
}

/// Lexical lookahead: `there is|are QUANTITY [KIND] counter(s) on ...`, the
/// existential form of a counter count on one object. A zone count (`there
/// are N cards in your graveyard`) is not this form.
fn existential_counter_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let Some(start) = existential_width(cursor) else {
        return false;
    };
    for offset in start..start + 7 {
        let Some(raw) = cursor.peek_ahead(offset) else {
            return false;
        };
        if raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters") {
            return word_is(cursor, offset + 1, "on");
        }
        if matches!(raw, "," | "." | ";") || raw.eq_ignore_ascii_case("in") {
            return false;
        }
    }
    false
}

/// `there are no KIND counters on OBJECT`, `there is a KIND counter on it`,
/// `there are N or more counters on this permanent`: the number of counters
/// on one referenced object (CR 122.1), read with the shared threshold and
/// counter name, and tested with the shared object predicate. The object is
/// resolved like the subject of any object condition; it is never guessed.
fn recognize_existential_counter_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.existential-counter.existential");
    let width = existential_width(cursor).ok_or_else(|| cursor.fail("`there is|are`"))?;
    for _ in 0..width {
        consume_current(cursor)?;
    }
    cursor.enter("condition.existential-counter.quantity");
    let threshold = recognize_count_threshold(cursor, true)?;
    let (minimum, maximum) = match threshold.operator {
        ThresholdOperator::AtLeast => (threshold.value, None),
        ThresholdOperator::AtMost => (0, Some(threshold.value)),
        ThresholdOperator::Exactly => (threshold.value, Some(threshold.value)),
    };
    cursor.enter("condition.existential-counter.kind");
    let counter = if word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters") {
        None
    } else {
        Some(recognize_counter_name(cursor)?)
    };
    cursor.enter("condition.existential-counter.noun");
    cursor.require(
        word_is(cursor, 0, "counter") || word_is(cursor, 0, "counters"),
        "`counter` or `counters`",
    )?;
    consume_current(cursor)?;
    cursor.enter("condition.existential-counter.location");
    cursor.consume("on")?;
    cursor.enter("condition.existential-counter.object");
    let object = if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        resolve_object_anaphor(cursor, context)?
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        ConditionObjectReference::SOURCE
    };
    finish_referenced_object(
        cursor,
        context,
        object,
        ObjectPredicate::HasCounter(CounterPresence {
            counter,
            minimum,
            maximum,
        }),
    )
}

/// `it` denotes exactly one proven singular antecedent: a permanent subject,
/// the latest declared card target, or the current card-collection result.
/// Outside an effect program, `it` denotes the source when the parent's
/// subject is its source.
pub(in crate::oracle::canonical) fn resolve_object_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.antecedent");
    let Some(program) = context.program else {
        if let Some(subject) = context.subject_object {
            return Ok(subject);
        }
        cursor.require(
            context.source_subject,
            "`it` that denotes the ability's own source",
        )?;
        return Ok(ConditionObjectReference::SOURCE);
    };
    let mut candidates = Vec::new();
    if let Some(reference) = super::permanent_subject::proven_read_antecedent(program.effects) {
        candidates.push(ConditionObjectReference::Permanent(reference));
    }
    if let Some(binding) = program.card_target {
        candidates.push(ConditionObjectReference::CardTarget(binding));
    }
    if program.card_focus {
        candidates.push(ConditionObjectReference::CardFocus);
    }
    // Precedence typed by noun kind (CR 400.7): the card the latest card
    // action put onto the battlefield became the permanent that action
    // moved, so the singular card result and that permanent are one
    // antecedent, read as the permanent (`Return target creature card from
    // your graveyard to the battlefield. If it's an Angel, ...`). The scope
    // is the moved permanent only right after the move, whose card action is
    // then the latest one the card focus reads.
    if candidates.contains(&ConditionObjectReference::Permanent(
        PermanentEffectReference::Observed(TypedObservedEventObject::MovedOntoBattlefield),
    )) {
        candidates.retain(|candidate| *candidate != ConditionObjectReference::CardFocus);
    }
    match candidates.as_slice() {
        [reference] => Ok(*reference),
        // No permanent subject and no card result: the program's other
        // published antecedents, in the one binding environment (a declared
        // spell or `any target`, an action result, the cast record).
        [] => {
            let environment = condition_binding_environment(cursor, program.effects);
            resolve_condition_pronoun(&environment)
                .map(ConditionObjectReference::from_antecedent)
                .map_err(|expected| cursor.fail(expected))
        }
        _ => Err(cursor.fail("exactly one proven antecedent for the pronoun, not several")),
    }
}

fn resolve_card_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.card-antecedent");
    let program = cursor.require_value(
        context.program,
        "an effect program whose card antecedent `that card` names",
    )?;
    match (program.card_target, program.card_focus) {
        (Some(binding), false) => Ok(ConditionObjectReference::CardTarget(binding)),
        (None, true) => Ok(ConditionObjectReference::CardFocus),
        (None, false) => {
            let environment = condition_binding_environment(cursor, program.effects);
            resolve_condition_card(&environment)
                .map(ConditionObjectReference::from_antecedent)
                .map_err(|expected| cursor.fail(expected))
        }
        (Some(_), true) => {
            Err(cursor.fail("exactly one proven card antecedent for `that card`, not several"))
        }
    }
}

/// The registered nominal of one non-possessive token `offset` tokens
/// ahead (`creature`, `permanent`, `artifact`, `card`).
fn plain_nominal_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> Option<TargetNominal> {
    let token = cursor
        .token_slice(cursor.position() + offset, cursor.position() + offset + 1)
        .first()
        .filter(|token| token.token_type != "possessiveTypeWord")?;
    match lookup_object_selector_lexeme(&token.raw) {
        Some(CharacteristicLexeme::CardType(value)) => Some(TargetNominal::CardType(value)),
        Some(CharacteristicLexeme::ObjectDomain(value)) => Some(TargetNominal::Domain(value)),
        _ => None,
    }
}

/// Lexical lookahead for `the VERBed NOUN` as a condition subject: a
/// definite description of an object an action or a cost VERBed (`the
/// sacrificed creature`, `the discarded card`). Its genitive (`the
/// sacrificed creature's power`) is a numeric operand, not this subject.
fn participle_subject_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_is(cursor, 0, "the")
        && cursor
            .peek_ahead(1)
            .and_then(ActionResultVerb::from_word)
            .is_some()
        && plain_nominal_at(cursor, 2).is_some()
}

/// `the VERBed NOUN` (CR 608.2c, 608.2h): the one object an earlier action
/// of this program VERBed when its selection guarantees the nominal (`Exile
/// target enchantment. If the exiled card is a God card`), else the object
/// a cost of this ability VERBed (`Sacrifice another creature: ... If the
/// sacrificed creature was legendary`), whose producing cost the card
/// composition proves (`contextual_binding.rs`). A participle without a
/// producer of either kind is a committed diagnostic.
fn recognize_participle_subject(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.participle");
    cursor.consume("the")?;
    let verb = cursor
        .peek()
        .and_then(ActionResultVerb::from_word)
        .ok_or_else(|| cursor.fail("the past participle of an object-producing verb"))?;
    consume_current(cursor)?;
    cursor.enter("condition.sentence.object.participle-nominal");
    let nominal = plain_nominal_at(cursor, 0)
        .ok_or_else(|| cursor.fail("a registered card type or object domain nominal"))?;
    consume_current(cursor)?;
    cursor.enter("condition.sentence.object.participle-antecedent");
    if let Some(program) = context.program {
        let environment = condition_binding_environment(cursor, program.effects);
        if let Some(result) = environment.result_object.as_ref().filter(|result| {
            result.verb == Some(verb)
                && result.number == GrammaticalNumber::Singular
                && selector_names_nominals(&result.selector, &[nominal])
        }) {
            // The card a sacrifice made is another object (CR 400.7); the
            // permanent's last known information is not recorded for it.
            cursor.require(
                result.reference.permanent || verb != ActionResultVerb::Sacrificed,
                SACRIFICED_RESULT_CONDITION,
            )?;
            return Ok(ConditionObjectReference::Permanent(
                PermanentEffectReference::ActionResult(result.reference),
            ));
        }
    }
    let object = match verb {
        ActionResultVerb::Sacrificed => CostPaidConditionObject::SacrificedPermanent,
        ActionResultVerb::Discarded => CostPaidConditionObject::DiscardedCard,
        _ => {
            return Err(cursor.fail(
                "an earlier action of this program that VERBed exactly one object of this nominal, or a sacrifice or discard cost",
            ));
        }
    };
    // The nominal restates the cost's object: a sacrifice pays with a
    // permanent of the named kind, a discard with a card.
    cursor.require(
        match (object, nominal) {
            (CostPaidConditionObject::SacrificedPermanent, TargetNominal::CardType(card_type)) => {
                card_type.is_permanent_type()
            }
            (CostPaidConditionObject::SacrificedPermanent, TargetNominal::Domain(kind)) => {
                kind == ObjectKind::Permanent
            }
            (CostPaidConditionObject::DiscardedCard, TargetNominal::Domain(kind)) => {
                kind == ObjectKind::Card
            }
            (CostPaidConditionObject::DiscardedCard, TargetNominal::CardType(_)) => false,
        },
        "a nominal the paying cost's object has (a permanent kind for a sacrifice, `card` for a discard)",
    )?;
    Ok(ConditionObjectReference::CostPaid(object))
}

fn finish_referenced_object(
    cursor: &mut OracleTokenCursor<'_>,
    context: &ConditionContext<'_>,
    object: ConditionObjectReference,
    predicate: ObjectPredicate,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.sentence.object.last-known-information");
    let object = match (object, context.program) {
        (
            ConditionObjectReference::Permanent(PermanentEffectReference::Target(binding)),
            Some(program),
        ) if program.departed_targets.contains(&binding) => {
            // CR 608.2h: an object moved out of the battlefield by an earlier
            // instruction is tested with its last known information: the
            // numeric properties and the type line its departure recorded.
            // Any other characteristic is not captured and is refused
            // instead of being read on the new object.
            cursor.require(
                last_known_information_answers(&predicate),
                "a mana value, power, toughness, type-line, counter, tapped, combat or token characteristic of a moved permanent (its other last-known information is not captured)",
            )?;
            ConditionObjectReference::DepartedTarget(binding)
        }
        _ => object,
    };
    cursor.enter("condition.sentence.object.domain");
    let domain = match object {
        // The card an earlier action moved off the battlefield or out of a
        // hidden zone is read as that card (CR 400.7).
        ConditionObjectReference::Permanent(PermanentEffectReference::ActionResult(result))
            if !result.permanent =>
        {
            ObjectKind::Card
        }
        ConditionObjectReference::Permanent(_) | ConditionObjectReference::DepartedTarget(_) => {
            ObjectKind::Permanent
        }
        ConditionObjectReference::CardTarget(_) | ConditionObjectReference::CardFocus => {
            ObjectKind::Card
        }
        ConditionObjectReference::EventSpell => ObjectKind::Spell,
        ConditionObjectReference::CostPaid(object) => object.domain(),
    };
    cursor.require(
        predicate.supports_domain(domain),
        "a characteristic evaluable in the referenced object's domain",
    )?;
    Ok(ReferencedObjectConditionNode { object, predicate })
}

/// Whether a departed permanent's recorded last known information answers
/// the predicate: one numeric property comparison (the captured mana value,
/// power or toughness), or a predicate over what its departure record keeps:
/// the type line, the counters, the tapped and combat status and the token
/// status (CR 608.2h). Colors and other characteristics are not recorded.
fn last_known_information_answers(predicate: &ObjectPredicate) -> bool {
    fn recorded(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::CardType(_)
            | ObjectPredicate::Subtype(_)
            | ObjectPredicate::Supertype(_)
            | ObjectPredicate::HasCounter(_)
            | ObjectPredicate::IsToken
            | ObjectPredicate::ObjectKind(ObjectKind::Permanent)
            | ObjectPredicate::Status(
                ObjectStatus::Tapped
                | ObjectStatus::Untapped
                | ObjectStatus::Attacking
                | ObjectStatus::Blocking,
            ) => true,
            ObjectPredicate::Not(operand) => recorded(operand),
            ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
                !operands.is_empty() && operands.iter().all(recorded)
            }
            _ => false,
        }
    }
    matches!(predicate, ObjectPredicate::NumericComparison { .. }) || recorded(predicate)
}

pub(in crate::oracle::canonical) fn sentence_condition_depends_on_cast_choice(
    condition: &SentenceConditionNode,
) -> bool {
    condition_any(condition, &ConditionAtom::depends_on_cast_choice)
}

#[cfg(test)]
#[path = "sentence_condition_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "sentence_condition_object_atoms_tests.rs"]
mod object_atoms_tests;

#[cfg(test)]
#[path = "sentence_condition_possession_tests.rs"]
mod possession_tests;

//! One reduction amount; the subject is either this spell or a spell query.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

/// `the ORDINAL SPELL you cast [from ZONE] each turn | during each of your
/// turns`: one spell of the caster's turn history, matched by the shared
/// spell query (CR 601.2i). The ordinal counts the caster's earlier casts
/// of matching spells this turn; the cost is determined before this spell
/// becomes cast (CR 601.2f), so it is not one of them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OrdinalSpellSubjectNode {
    pub spells: SpellCastQueryNode,
    pub ordinal: i64,
    /// `during each of your turns`: only during the caster's own turns.
    pub own_turns_only: bool,
}

/// The spell query of a spell-population subject.
pub(in crate::oracle::canonical) fn reduction_subject_spells(
    subject: &CastingReductionSubject,
) -> Option<&SpellCastQueryNode> {
    match subject {
        CastingReductionSubject::CurrentSpell => None,
        CastingReductionSubject::SpellSet(query) => Some(query),
        CastingReductionSubject::OrdinalSpell(ordinal) => Some(&ordinal.spells),
        CastingReductionSubject::TargetingSpells { spells, .. } => Some(spells),
    }
}

/// `the ORDINAL SPELL you cast [from ZONE] (each turn | during each of your
/// turns)`; the caster must be the ability controller, whose cast history
/// the ordinal reads.
fn recognize_ordinal_spell_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OrdinalSpellSubjectNode> {
    cursor.enter("casting-reduction.ordinal.head");
    cursor.consume("the")?;
    let ordinal = recognize_ordinal_atom(cursor)?;
    cursor.enter("casting-reduction.ordinal.spells");
    let spells = recognize_spell_cast_collection_with_origin(cursor)?;
    cursor.enter("casting-reduction.ordinal.caster");
    cursor.require(
        spells.caster == Some(SpellCasterScope::AbilityController),
        "the ability controller as the caster whose turn history the ordinal counts",
    )?;
    cursor.enter("casting-reduction.ordinal.period");
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let own_turns_only = if word(cursor, 0, "during") {
        cursor.consume_phrase(&["during", "each", "of", "your", "turns"])?;
        true
    } else {
        cursor.consume_phrase(&["each", "turn"])?;
        false
    };
    Ok(OrdinalSpellSubjectNode {
        spells,
        ordinal,
        own_turns_only,
    })
}

pub(in crate::oracle::canonical) enum CastingReductionReadAttempt {
    NotClaimed,
    Parsed(CastingCostReductionNode),
    FailedCommitted(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_recognize_casting_cost_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> CastingReductionReadAttempt {
    let mut trial = cursor.clone();
    let mut owned = false;
    let result = read_reduction(&mut trial, &mut owned);
    match result {
        Ok(node) => {
            *cursor = trial;
            CastingReductionReadAttempt::Parsed(node)
        }
        Err(failure) if owned => CastingReductionReadAttempt::FailedCommitted(failure),
        Err(_) => CastingReductionReadAttempt::NotClaimed,
    }
}

fn read_reduction(
    cursor: &mut OracleTokenCursor<'_>,
    owned: &mut bool,
) -> MachineResult<CastingCostReductionNode> {
    cursor.enter("casting-reduction.subject");
    let (subject, quantified_each) = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("this"))
    {
        cursor.consume_phrase(&["this", "spell"])?;
        (CastingReductionSubject::CurrentSpell, true)
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
    {
        (
            CastingReductionSubject::OrdinalSpell(recognize_ordinal_spell_subject(cursor)?),
            true,
        )
    } else {
        let each = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"));
        if each {
            cursor.consume("each")?;
        }
        // `spells you cast from your graveyard`: the reduction records the
        // zone a spell is cast from (CR 601.2a, 601.2f).
        let query = recognize_spell_cast_collection_with_origin(cursor)?;
        if target_relative_follows(cursor) {
            // `Spells you cast that target this creature`: the targets are
            // chosen before the total cost (CR 601.2c, 601.2f).
            let target = recognize_target_relative(cursor, &[], None)?;
            (
                CastingReductionSubject::TargetingSpells {
                    spells: query,
                    target,
                },
                each,
            )
        } else {
            (CastingReductionSubject::SpellSet(query), each)
        }
    };
    cursor.enter("casting-reduction.cost-action");
    cursor.consume(if quantified_each { "costs" } else { "cost" })?;
    *owned = true; // The typed cost-adjustment subject/verb, never cursor distance.
    if let Some(query) = reduction_subject_spells(&subject) {
        cursor.enter("static.spell-cost-reduction.caster-scope");
        // `Spells cost {1} less` names every caster (CR 601.2f applies to
        // each spell); `your opponents cast` names the opponents.
        cursor.require(
            matches!(
                query.caster,
                None | Some(
                    SpellCasterScope::AbilityController
                        | SpellCasterScope::OpponentOfAbilityController
                )
            ),
            "spells cast by every player, the ability controller or its opponents",
        )?;
        cursor.require(
            crate::oracle::canonical::effects::domain_filter_is_representable(&query.predicate),
            "a spell filter preserving Boolean domain identities",
        )?;
    }
    cursor.enter("static.spell-cost-reduction.cost");
    let cost = recognize_mana_cost(cursor)?;
    // A different adjustment direction belongs to its own semantic consumer.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("more"))
    {
        *owned = false;
        return Err(cursor.fail("a reduction direction"));
    }
    cursor.enter("static.spell-cost-reduction.cost");
    let (fixed, colored_counts) = match ReductionDelta::of(&cost) {
        Some(ReductionDelta::Generic(fixed)) => (fixed, None),
        // `{X} less to cast, where X is EXPR` (CR 107.3): the postposed
        // definition is the whole reduction amount, read by the shared
        // numeric grammar; no other amount or condition follows it.
        Some(ReductionDelta::Variable) => {
            cursor.enter("casting-reduction.direction");
            cursor.consume_phrase(&["less", "to", "cast"])?;
            cursor.enter("casting-reduction.where-x");
            cursor.require(
                where_x_definition_starts(cursor),
                "`, where X is` defining the {X} reduction",
            )?;
            let amount = recognize_where_x_definition(cursor)?;
            return finish_reduction(cursor, subject, amount, None, None, None);
        }
        // `{W}{B} less`, `{2}{U} less`, `{G} less for each ...`: a delta
        // with colored symbols (CR 118.7b-c).
        None => {
            let (generic, counts) = cost
                .generic_and_whole_symbol_counts()
                .filter(|(_, counts)| counts.iter().any(|count| *count > 0))
                .ok_or_else(|| {
                    cursor.fail("a reduction of generic and whole colored mana symbols, or {X}")
                })?;
            (generic, Some(counts))
        }
    };
    cursor.enter("casting-reduction.direction");
    cursor.consume_phrase(&["less", "to", "cast"])?;
    let fixed_amount_only = !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("for"));
    // `for each QUERY` multiplies the whole delta, generic and colored.
    let multiplier = if !fixed_amount_only {
        let checkpoint = cursor.checkpoint();
        let count = match read_zone_query_count(cursor) {
            // A read zone the casting-cost reader cannot evaluate fails here;
            // the battlefield reading below never claims it instead.
            Ok(NumericExpression::CountQuery(query))
                if query.zones.contains(&CardZoneName::Library) =>
            {
                cursor.enter("casting-reduction.count-zone");
                return Err(cursor
                    .fail("a zone the casting-cost collection reader evaluates (not a library)"));
            }
            Ok(count) => count,
            Err(zone_failure) => {
                // `for each artifact you control` counts battlefield permanents
                // through the shared `for each` child; a card-zone query keeps
                // its own diagnostic when neither reading applies. The child
                // owns the amount only when it consumes the whole counted
                // operand: a battlefield prefix such as `creature` must not
                // drop a trailing `in your graveyard` scope.
                let zone_cursor = cursor.clone();
                cursor.rollback(checkpoint);
                match cursor.attempt(recognize_for_each_count) {
                    Ok(count) if reduction_amount_complete(cursor) => count,
                    _ => {
                        *cursor = zone_cursor;
                        return Err(zone_failure);
                    }
                }
            }
        };
        Some(count)
    } else {
        None
    };
    let amount = match &multiplier {
        Some(_) if fixed == 0 => NumericExpression::from(0_i64),
        Some(count) if fixed == 1 => count.clone(),
        Some(count) => {
            NumericExpression::multiply(NumericExpression::from(i64::from(fixed)), count.clone())
        }
        None => NumericExpression::from(i64::from(fixed)),
    };
    let colored = colored_counts.map(|counts| ColoredReductionNode {
        counts,
        times: multiplier
            .clone()
            .unwrap_or_else(|| NumericExpression::from(1_i64)),
        excess_reduces_generic: true,
    });
    let (condition, target_query) = match recognize_cost_adjustment_condition(cursor)? {
        None => (None, None),
        Some(condition) => {
            cursor.enter("casting-reduction.condition.subject");
            cursor.require(
                fixed_amount_only,
                "a fixed reduction amount before a condition",
            )?;
            match condition {
                CostAdjustmentCondition::Sentence(condition) => (Some(condition), None),
                CostAdjustmentCondition::Targets(query) => {
                    cursor.require(
                        matches!(subject, CastingReductionSubject::CurrentSpell)
                            && colored.is_none(),
                        "this spell as the subject of a generic target-conditioned reduction",
                    )?;
                    (None, Some(query))
                }
            }
        }
    };
    finish_reduction(cursor, subject, amount, condition, target_query, colored)
}

/// The condition gating a cost adjustment (CR 601.2f, 602.2b), read after
/// `less to cast|activate`. Every form is evaluated when the total cost is
/// determined, after targets are chosen (CR 601.2c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CostAdjustmentCondition {
    /// `if COND`, `as long as COND`, `during your turn`: one Boolean program
    /// of the shared condition language, read without sequence antecedents.
    Sentence(SentenceConditionNode),
    /// `if it targets a QUERY`: matched against the chosen targets.
    Targets(ObjectQueryNode),
}

/// Reads the optional condition of a cost adjustment. Nothing is consumed
/// when no condition introducer follows.
pub(in crate::oracle::canonical) fn recognize_cost_adjustment_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<CostAdjustmentCondition>> {
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    cursor.enter("cost-adjustment.condition.introducer");
    if word(cursor, 0, "during") && word(cursor, 1, "your") && word(cursor, 2, "turn") {
        // `during your turn` is the proposition `it's your turn` (CR 500.1).
        cursor.consume_phrase(&["during", "your", "turn"])?;
        return Ok(Some(CostAdjustmentCondition::Sentence(
            BooleanConditionNode::Atom(ConditionAtom::TurnOwnership(
                ControllerTurnPropositionNode {
                    is_controller_turn: true,
                },
            )),
        )));
    }
    if word(cursor, 0, "as") && word(cursor, 1, "long") && word(cursor, 2, "as") {
        cursor.consume_phrase(&["as", "long", "as"])?;
    } else if word(cursor, 0, "if") {
        cursor.consume("if")?;
        if word(cursor, 0, "it") && word(cursor, 1, "targets") {
            // The targets are chosen (CR 601.2c) before the total cost (601.2f).
            cursor.consume_phrase(&["it", "targets"])?;
            cursor.enter("casting-reduction.condition.target-query");
            let article = cursor
                .peek()
                .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                .ok_or_else(|| cursor.fail("a singular targeted object introduced by a/an"))?
                .to_string();
            cursor.consume(&article)?;
            let query = recognize_object_query(cursor)?;
            cursor.enter("casting-reduction.condition.target-query");
            cursor.require(
                query.controller.is_none()
                    && query.predicate.supports_domain(ObjectKind::Permanent)
                    && query.predicate.has_nominal_head(),
                "a controller-free permanent predicate for the targeted object",
            )?;
            return Ok(Some(CostAdjustmentCondition::Targets(query)));
        }
    } else {
        return Ok(None);
    }
    cursor.enter("casting-reduction.condition.boolean-program");
    // The cost is determined before any effect introduces an object, so the
    // shared atoms see no sequence antecedent.
    let effects = EffectRecognitionContext::default();
    let context = SentenceConditionContext {
        name_links: &[],
        name_failure: None,
        effects: &effects,
        card_target: None,
        card_focus: false,
        departed_targets: &[],
    };
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, &context)
    })?;
    Ok(Some(CostAdjustmentCondition::Sentence(condition)))
}

/// The reduction delta printed between `cost(s)` and `less`: one positive
/// generic symbol, or `{X}` whose value a postposed definition supplies.
/// Colored and mixed deltas are refused by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReductionDelta {
    Generic(i32),
    Variable,
}

impl ReductionDelta {
    pub(in crate::oracle::canonical) fn of(cost: &ManaCostNode) -> Option<Self> {
        match cost.generic_payment_expression()? {
            NumericExpression::Atom(NumericAtom::Integer(value)) if value > 0 => {
                i32::try_from(value).ok().map(Self::Generic)
            }
            NumericExpression::Atom(NumericAtom::VariableX) => Some(Self::Variable),
            _ => None,
        }
    }
}

fn finish_reduction(
    cursor: &mut OracleTokenCursor<'_>,
    subject: CastingReductionSubject,
    amount: NumericExpression,
    condition: Option<SentenceConditionNode>,
    target_query: Option<ObjectQueryNode>,
    mut colored: Option<ColoredReductionNode>,
) -> MachineResult<CastingCostReductionNode> {
    cursor.enter("casting-reduction.terminator");
    cursor.consume(".")?;
    if let Some(colored) = colored.as_mut() {
        recognize_colored_reduction_policy(cursor, colored)?;
    }
    // A trailing reminder restates either the amount's rule (devotion), the
    // colored-reduction rule, or a characteristic batch of the spell subject.
    if matches!(amount, NumericExpression::Devotion(_)) {
        recognize_optional_amount_reminder(cursor, &amount)?;
    } else if let Some(colored) = &colored {
        recognize_optional_colored_reduction_reminder(cursor, colored)?;
    } else if let Some(query) = reduction_subject_spells(&subject) {
        recognize_optional_batch_reminder_for(cursor, &query.predicate)?;
    }
    cursor.enter("casting-reduction.complete");
    cursor.finish()?;
    Ok(CastingCostReductionNode {
        subject,
        amount,
        condition,
        target_query,
        colored,
    })
}

const COLOR_WORDS: [&str; 5] = ["white", "blue", "black", "red", "green"];

/// `This effect reduces only the amount of colored|COLOR mana you pay.`
/// overrides CR 118.7b-c: an excess no longer reduces generic mana. A color
/// word must be the delta's only color.
fn recognize_colored_reduction_policy(
    cursor: &mut OracleTokenCursor<'_>,
    colored: &mut ColoredReductionNode,
) -> MachineResult<()> {
    let starts = ["this", "effect", "reduces", "only"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        });
    if !starts {
        return Ok(());
    }
    cursor.enter("casting-reduction.colored-policy");
    cursor.consume_phrase(&["this", "effect", "reduces", "only", "the", "amount", "of"])?;
    let word = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("colored or the delta's color"))?;
    let named = if word == "colored" {
        true
    } else {
        match COLOR_WORDS.iter().position(|color| *color == word) {
            Some(index) => colored
                .counts
                .iter()
                .enumerate()
                .all(|(other, count)| (other == index) == (*count > 0)),
            None => false,
        }
    };
    cursor.require(named, "colored or the delta's only color")?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.consume_phrase(&["mana", "you", "pay", "."])?;
    colored.excess_reduces_generic = false;
    Ok(())
}

/// The reminder of a colored reduction restates its rule: `(This can reduce
/// generic costs.)` and `(If it doesn't require {C}, it costs {1} less to
/// cast.)` the default CR 118.7b; `(For example, if you cast a SPELL with
/// mana cost COST, it costs COST to cast.)` the colored-only policy.
fn recognize_optional_colored_reduction_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    colored: &ColoredReductionNode,
) -> MachineResult<()> {
    if cursor.peek() != Some("(") {
        return Ok(());
    }
    cursor.enter("reminder.colored-reduction");
    cursor.consume("(")?;
    let word = |cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    if word(cursor, 0, "this") {
        cursor.require(
            colored.excess_reduces_generic,
            "the default colored reduction rule",
        )?;
        cursor.consume_phrase(&["this", "can", "reduce", "generic", "costs", "."])?;
    } else if word(cursor, 0, "if") {
        cursor.require(
            colored.excess_reduces_generic,
            "the default colored reduction rule",
        )?;
        cursor.consume_phrase(&["if", "it"])?;
        cursor.require(
            cursor
                .peek()
                .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case("doesn't")),
            "doesn't",
        )?;
        cursor.advance_to(cursor.position() + 1)?;
        cursor.consume("require")?;
        recognize_mana_cost(cursor)?;
        cursor.consume_phrase(&[",", "it", "costs"])?;
        recognize_mana_cost(cursor)?;
        cursor.consume_phrase(&["less", "to", "cast", "."])?;
    } else {
        cursor.require(
            !colored.excess_reduces_generic,
            "the colored-only reduction rule",
        )?;
        cursor.consume_phrase(&["for", "example", ",", "if", "you", "cast"])?;
        let article = cursor
            .peek()
            .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
            .ok_or_else(|| cursor.fail("an example spell introduced by a/an"))?
            .to_string();
        cursor.consume(&article)?;
        // The example spell is the noun phrase before `with mana cost`; the
        // object query must not read that printed cost as its own `with`
        // qualifier, so it is bounded to the noun phrase.
        let example_end = (0..)
            .map_while(|offset| cursor.peek_ahead(offset).map(|raw| (offset, raw)))
            .find(|(offset, raw)| {
                raw.eq_ignore_ascii_case("with")
                    && word(cursor, offset + 1, "mana")
                    && word(cursor, offset + 2, "cost")
            })
            .map(|(offset, _)| cursor.position() + offset)
            .ok_or_else(|| cursor.fail("an example spell followed by `with mana cost`"))?;
        cursor.bounded(example_end, |cursor| {
            recognize_object_query(cursor).map(|_| ())
        })?;
        cursor.consume_phrase(&["with", "mana", "cost"])?;
        recognize_mana_cost(cursor)?;
        cursor.consume_phrase(&[",", "it", "costs"])?;
        recognize_mana_cost(cursor)?;
        cursor.consume_phrase(&["to", "cast", "."])?;
    }
    cursor.enter("reminder.colored-reduction.close");
    cursor.consume(")")
}

/// The counted amount ends at the reduction terminator or its condition.
fn reduction_amount_complete(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw == "."
            || ["if", "as", "during"]
                .iter()
                .any(|word| raw.eq_ignore_ascii_case(word))
    })
}

/// `for each|every QUERY` over a card zone or the battlefield (`on the`).
fn read_zone_query_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericExpression> {
    cursor.consume("for")?;
    let quantifier = cursor
        .peek()
        .map(str::to_owned)
        .ok_or_else(|| cursor.fail("each or every before a counted query"))?;
    cursor.require(
        quantifier.eq_ignore_ascii_case("each") || quantifier.eq_ignore_ascii_case("every"),
        "each or every before a counted query",
    )?;
    cursor.consume(&quantifier)?;
    let query = recognize_scoped_object_query(cursor)?;
    Ok(NumericExpression::CountQuery(Box::new(query)))
}

pub(in crate::oracle::canonical) fn recognize_casting_cost_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastingCostReductionNode> {
    let mut owned = false;
    read_reduction(cursor, &mut owned)
}

/// Legacy name, now a typed subject adapter over the single reduction reader.
pub(in crate::oracle::canonical) fn recognize_permanent_scaled_casting_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CastingCostReductionNode> {
    let node = recognize_casting_cost_reduction(cursor)?;
    cursor.require(
        matches!(&node.subject, CastingReductionSubject::CurrentSpell),
        "the current spell as this compatibility reduction subject",
    )?;
    Ok(node)
}

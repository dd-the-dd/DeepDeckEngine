//! One reduction amount; the subject is either this spell or a spell query.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::*;

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
    } else {
        let each = cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"));
        if each {
            cursor.consume("each")?;
        }
        let query = recognize_spell_cast_collection(cursor)?;
        (CastingReductionSubject::SpellSet(query), each)
    };
    cursor.enter("casting-reduction.cost-action");
    cursor.consume(if quantified_each { "costs" } else { "cost" })?;
    *owned = true; // The typed cost-adjustment subject/verb, never cursor distance.
    if let CastingReductionSubject::SpellSet(query) = &subject {
        cursor.enter("static.spell-cost-reduction.caster-scope");
        cursor.require(
            query.caster == Some(SpellCasterScope::AbilityController),
            "spells cast by the ability controller",
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
    let fixed = cost
        .fixed_generic_amount()
        .filter(|n| *n > 0)
        .ok_or_else(|| cursor.fail("one positive generic mana symbol without X or colored mana"))?;
    cursor.enter("casting-reduction.direction");
    cursor.consume_phrase(&["less", "to", "cast"])?;
    let fixed_amount_only = !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("for"));
    let amount = if !fixed_amount_only {
        let checkpoint = cursor.checkpoint();
        let count = match read_zone_query_count(cursor) {
            Ok(count) => count,
            Err(zone_failure) => {
                // `for each artifact you control` counts battlefield permanents
                // through the shared `for each` child; a card-zone query keeps
                // its own diagnostic when neither reading applies.
                let zone_cursor = cursor.clone();
                cursor.rollback(checkpoint);
                match cursor.attempt(recognize_for_each_count) {
                    Ok(count) => count,
                    Err(_) => {
                        *cursor = zone_cursor;
                        return Err(zone_failure);
                    }
                }
            }
        };
        if fixed == 1 {
            count
        } else {
            NumericExpression::multiply(NumericExpression::from(i64::from(fixed)), count)
        }
    } else {
        NumericExpression::from(i64::from(fixed))
    };
    let mut target_query = None;
    let condition = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("if"))
    {
        cursor.enter("casting-reduction.condition.subject");
        cursor.require(
            matches!(subject, CastingReductionSubject::CurrentSpell),
            "this spell as the subject of a conditional reduction",
        )?;
        cursor.require(
            fixed_amount_only,
            "a fixed reduction amount before a condition",
        )?;
        cursor.consume("if")?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
            && cursor
                .peek_ahead(1)
                .is_some_and(|raw| raw.eq_ignore_ascii_case("targets"))
        {
            // The spell's targets are chosen (CR 601.2c) before its total cost (601.2f).
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
            target_query = Some(query);
            None
        } else {
            cursor.enter("casting-reduction.condition.boolean-program");
            Some(recognize_boolean_condition_list_before_separator(
                cursor,
                recognize_trigger_guard_atom,
            )?)
        }
    } else {
        None
    };
    cursor.enter("casting-reduction.terminator");
    cursor.consume(".")?;
    if let CastingReductionSubject::SpellSet(query) = &subject {
        recognize_optional_batch_reminder_for(cursor, &query.predicate)?;
    }
    cursor.enter("casting-reduction.complete");
    cursor.finish()?;
    Ok(CastingCostReductionNode {
        subject,
        amount,
        condition,
        target_query,
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

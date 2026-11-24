//! Qualifier atoms of the shared object query that are not one printed
//! characteristic read alone (`selector.rs` owns the noun phrase and calls
//! these readers from its postnominal slot).
//!
//! ```text
//! WithQualifier := "with" ( "different" PROPERTY-PLURAL           set constraint
//!                         | "total" PROPERTY N "or" "less"          set constraint
//!                         | "the" EXTREME PROPERTY "among" QUERY POPULATION
//!                         | MANA-SYMBOL "in" "its" "mana" "cost"
//!                         | ["a"] KEYWORD ["ability" | "abilities"] )
//! DamageHistory := "dealt" "damage" ["by" SOURCE] "this" "turn"
//! POPULATION    := "on" "the" "battlefield" | "they" "control"
//!                | "that" "player" "controls" | "you" "control"
//! ```
//!
//! A set constraint describes the chosen set, so it needs a plural head
//! (`cards with different names`); on a single object it fails. A
//! superlative always names its population: `the greatest power` without
//! `among` has no domain to rank in and fails.

use crate::oracle::canonical::automata::{
    DistinctProperty, Extreme, ExtremumPopulation, KeywordAbilityName, ManaCostSymbolClass,
    NumericAtom, NumericExpression, ObjectControllerScope, ObjectKind, ObjectQualifier,
    ObjectStatus, SelectorPredicate, recognize_query_numeric_expression,
    recognize_source_reference,
};
use crate::oracle::canonical::{MachineResult, OracleTokenCursor};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_card_type, lookup_object_domain,
};

use super::{ObjectQueryContext, recognize_object_noun_query, recognize_selector_numeric_property};

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The forms of this module after `with`, at offset 1. Pure lookahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WithQualifierForm {
    Distinct,
    Total,
    Extremum,
    ManaCostSymbol,
    KeywordAbility,
}

pub(super) fn with_qualifier_form(cursor: &OracleTokenCursor<'_>) -> Option<WithQualifierForm> {
    if !word_at(cursor, 0, "with") {
        return None;
    }
    if word_at(cursor, 1, "different") {
        return Some(WithQualifierForm::Distinct);
    }
    if word_at(cursor, 1, "total") {
        return Some(WithQualifierForm::Total);
    }
    if word_at(cursor, 1, "the") && cursor.peek_ahead(2).and_then(extreme_from_word).is_some() {
        return Some(WithQualifierForm::Extremum);
    }
    let symbol = cursor
        .token_slice(cursor.position() + 1, cursor.position() + 2)
        .first()
        .is_some_and(|token| mana_symbol_class(&token.raw).is_some());
    if symbol && word_at(cursor, 2, "in") {
        return Some(WithQualifierForm::ManaCostSymbol);
    }
    let keyword_offset = if word_at(cursor, 1, "a") || word_at(cursor, 1, "an") {
        2
    } else {
        1
    };
    if cursor
        .peek_ahead(keyword_offset)
        .and_then(KeywordAbilityName::from_oracle_word)
        .is_some()
    {
        return Some(WithQualifierForm::KeywordAbility);
    }
    None
}

/// A plural noun naming permanents (`planeswalkers`, `permanents`), which
/// may continue a superlative's population after `and`/`or`.
fn population_noun_starts(raw: &str) -> bool {
    lookup_card_type(raw, AcceptedForms::SingularOrPlural).is_some()
        || lookup_object_domain(raw, AcceptedForms::SingularOrPlural).is_some()
}

fn extreme_from_word(raw: &str) -> Option<Extreme> {
    match raw.to_ascii_lowercase().as_str() {
        "greatest" | "highest" => Some(Extreme::Greatest),
        "least" | "lowest" => Some(Extreme::Least),
        _ => None,
    }
}

fn mana_symbol_class(raw: &str) -> Option<ManaCostSymbolClass> {
    raw.eq_ignore_ascii_case("{x}")
        .then_some(ManaCostSymbolClass::VariableX)
}

/// Reads one qualifier of this module, `with` included. `plural_head` is
/// whether the qualified noun is plural: a set constraint needs one.
pub(super) fn recognize_with_qualifier(
    cursor: &mut OracleTokenCursor<'_>,
    form: WithQualifierForm,
    plural_head: bool,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.with-qualifier.relation");
    cursor.consume("with")?;
    let qualifier = match form {
        WithQualifierForm::Distinct | WithQualifierForm::Total => {
            // `with different mana values and total mana value 6 or less`:
            // set constraints coordinated by `and` constrain one selection.
            let mut constraints = vec![SelectorPredicate::Qualifier(recognize_set_constraint(
                cursor,
                form,
                plural_head,
            )?)];
            while word_at(cursor, 0, "and")
                && (word_at(cursor, 1, "different") || word_at(cursor, 1, "total"))
            {
                cursor.enter("selector.set-constraint.conjunction");
                cursor.consume("and")?;
                let next = if word_at(cursor, 0, "different") {
                    WithQualifierForm::Distinct
                } else {
                    WithQualifierForm::Total
                };
                constraints.push(SelectorPredicate::Qualifier(recognize_set_constraint(
                    cursor,
                    next,
                    plural_head,
                )?));
            }
            return Ok(match constraints.len() {
                1 => constraints.pop().expect("one set constraint"),
                _ => SelectorPredicate::Intersection(constraints),
            });
        }
        WithQualifierForm::Extremum => recognize_extremum(cursor)?,
        WithQualifierForm::ManaCostSymbol => {
            cursor.enter("selector.mana-cost-symbol.symbol");
            let raw = cursor
                .peek()
                .ok_or_else(|| cursor.fail("a mana symbol"))?
                .to_string();
            let class = mana_symbol_class(&raw)
                .ok_or_else(|| cursor.fail("a supported mana symbol class (`{X}`)"))?;
            cursor.advance_to(cursor.position() + 1)?;
            cursor.enter("selector.mana-cost-symbol.location");
            cursor.consume_phrase(&["in", "its", "mana", "cost"])?;
            ObjectQualifier::ManaCostSymbol(class)
        }
        WithQualifierForm::KeywordAbility => {
            cursor.enter("selector.keyword-ability.determiner");
            let article = word_at(cursor, 0, "a") || word_at(cursor, 0, "an");
            if article {
                let raw = cursor.peek().expect("checked article").to_string();
                cursor.consume(&raw)?;
            }
            cursor.enter("selector.keyword-ability.keyword");
            let raw = cursor
                .peek()
                .ok_or_else(|| cursor.fail("a keyword ability"))?
                .to_string();
            let keyword = KeywordAbilityName::from_oracle_word(&raw)
                .ok_or_else(|| cursor.fail("a keyword ability"))?;
            cursor.advance_to(cursor.position() + 1)?;
            // `a cycling ability` / `cycling abilities`: the noun agrees with
            // the article; a bare keyword names the ability itself.
            cursor.enter("selector.keyword-ability.noun");
            if article {
                cursor.consume("ability")?;
            } else if word_at(cursor, 0, "abilities") {
                cursor.consume("abilities")?;
            }
            ObjectQualifier::KeywordAbility(keyword)
        }
    };
    Ok(SelectorPredicate::Qualifier(qualifier))
}

/// One set constraint after `with` (or after its coordinating `and`).
fn recognize_set_constraint(
    cursor: &mut OracleTokenCursor<'_>,
    form: WithQualifierForm,
    plural_head: bool,
) -> MachineResult<ObjectQualifier> {
    Ok(match form {
        WithQualifierForm::Distinct => {
            cursor.enter("selector.set-constraint.distinct.head");
            cursor.require(
                plural_head,
                "a plural object head for a constraint over the chosen set",
            )?;
            cursor.consume("different")?;
            cursor.enter("selector.set-constraint.distinct.property");
            ObjectQualifier::DistinctValues(recognize_distinct_property(cursor)?)
        }
        WithQualifierForm::Total => {
            cursor.enter("selector.set-constraint.total.head");
            cursor.require(
                plural_head,
                "a plural object head for a constraint over the chosen set",
            )?;
            cursor.consume("total")?;
            cursor.enter("selector.set-constraint.total.property");
            let property = recognize_selector_numeric_property(cursor)?;
            // `total power and toughness` sums two properties of one object:
            // another grammar, not a set aggregate.
            cursor.require(
                !word_at(cursor, 0, "and"),
                "one property summed over the chosen set",
            )?;
            cursor.enter("selector.set-constraint.total.maximum");
            let maximum = recognize_query_numeric_expression(cursor)?;
            cursor.require(
                matches!(
                    maximum,
                    NumericExpression::Atom(NumericAtom::Integer(_) | NumericAtom::VariableX)
                ),
                "a literal or declared-X total",
            )?;
            cursor.enter("selector.set-constraint.total.direction");
            cursor.consume("or")?;
            cursor.require(
                word_at(cursor, 0, "less") || word_at(cursor, 0, "fewer"),
                "`or less` after the total",
            )?;
            let direction = cursor.peek().expect("checked direction").to_string();
            cursor.consume(&direction)?;
            ObjectQualifier::TotalAtMost {
                property,
                maximum: Box::new(maximum),
            }
        }
        _ => return Err(cursor.fail("a set constraint (`different`, `total`)")),
    })
}

fn recognize_distinct_property(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DistinctProperty> {
    let property = if word_at(cursor, 0, "names") {
        cursor.consume("names")?;
        DistinctProperty::Name
    } else if word_at(cursor, 0, "powers") {
        cursor.consume("powers")?;
        DistinctProperty::Power
    } else if word_at(cursor, 0, "mana") && word_at(cursor, 1, "values") {
        cursor.consume_phrase(&["mana", "values"])?;
        DistinctProperty::ManaValue
    } else {
        return Err(cursor.fail("`names`, `powers` or `mana values`"));
    };
    Ok(property)
}

/// `the greatest|least PROPERTY among QUERY POPULATION` (after `with`).
fn recognize_extremum(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<ObjectQualifier> {
    cursor.enter("selector.superlative.extreme");
    cursor.consume("the")?;
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("`greatest` or `least`"))?
        .to_string();
    let extreme = extreme_from_word(&raw).ok_or_else(|| cursor.fail("`greatest` or `least`"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("selector.superlative.property");
    let property = recognize_selector_numeric_property(cursor)?;
    // A superlative ranks within a stated population; without `among` it
    // has no domain.
    cursor.enter("selector.superlative.domain");
    cursor.consume("among")?;
    let mut branches = Vec::new();
    let controller = loop {
        cursor.enter("selector.superlative.population");
        let query = recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)?
            .into_object_query(cursor)?;
        cursor.require(
            query.predicate.supports_domain(ObjectKind::Permanent)
                && query.predicate.has_nominal_head(),
            "a population of battlefield permanents named by a noun",
        )?;
        branches.push(query.predicate);
        // `creatures and planeswalkers they control`: a population names
        // the union of its coordinated nouns.
        if query.controller.is_none()
            && (word_at(cursor, 0, "and") || word_at(cursor, 0, "or"))
            && cursor.peek_ahead(1).is_some_and(population_noun_starts)
        {
            let connector = cursor.peek().expect("checked connector").to_string();
            cursor.consume(&connector)?;
            continue;
        }
        break query.controller;
    };
    cursor.enter("selector.superlative.population-relation");
    let population = match controller {
        Some(ObjectControllerScope::AbilityController) => ExtremumPopulation::AbilityController,
        // `that player controls`: the player choosing among their own
        // permanents is the candidate's controller.
        Some(ObjectControllerScope::EventPlayer) => ExtremumPopulation::CandidateController,
        Some(_) => {
            return Err(cursor.fail(
                "a superlative population on the battlefield or of the candidate's controller",
            ));
        }
        None if word_at(cursor, 0, "they") && word_at(cursor, 1, "control") => {
            cursor.consume_phrase(&["they", "control"])?;
            ExtremumPopulation::CandidateController
        }
        None if word_at(cursor, 0, "on")
            && word_at(cursor, 1, "the")
            && word_at(cursor, 2, "battlefield") =>
        {
            cursor.consume_phrase(&["on", "the", "battlefield"])?;
            ExtremumPopulation::Battlefield
        }
        None => {
            return Err(cursor.fail(
                "`on the battlefield`, `they control` or `you control` naming the population",
            ));
        }
    };
    let among = match branches.len() {
        1 => branches.pop().expect("one population branch"),
        _ => SelectorPredicate::Union(branches),
    };
    Ok(ObjectQualifier::Extremum {
        property,
        extreme,
        among: Box::new(among),
        population,
    })
}

/// `dealt damage [by SOURCE] this turn` after an object noun. Pure
/// lookahead: the participle is claimed only when the whole phrase reads.
pub(super) fn damage_history_participle_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if !(word_at(cursor, 0, "dealt") && word_at(cursor, 1, "damage")) {
        return false;
    }
    let mut trial = cursor.clone();
    recognize_damage_history_participle(&mut trial).is_ok()
}

/// `dealt damage this turn` (CR 120.1: the permanent was dealt damage this
/// turn) or `dealt damage by SOURCE this turn`, where SOURCE is the ability
/// source (`this creature`, its name).
pub(super) fn recognize_damage_history_participle(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SelectorPredicate> {
    cursor.enter("selector.damage-history.participle");
    cursor.consume_phrase(&["dealt", "damage"])?;
    let predicate = if word_at(cursor, 0, "by") {
        cursor.enter("selector.damage-history.source");
        cursor.consume("by")?;
        let names = cursor.source_names().to_vec();
        recognize_source_reference(cursor, &names, None)?;
        SelectorPredicate::Qualifier(ObjectQualifier::DamagedBySourceThisTurn)
    } else {
        SelectorPredicate::Status(ObjectStatus::WasDealtDamageThisTurn)
    };
    cursor.enter("selector.damage-history.window");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(predicate)
}

//! The object noun-phrase category: one recognizer for an object argument.
//!
//! ```text
//! NounPhrase := Determiner ObjectQuery
//! Determiner := "target" | N ["other"] "target" | "up to" N ["other"] "target"
//!             | "another" "target"                     (declared, CR 601.2c)
//!             | "a" | "an" | "another" | N | "up to" N
//!             | "any number" ["of"]                    (chosen on resolution)
//!             | "each" | "all"                         (universal)
//!             | (none: a bare plural)                  (universal)
//! ```
//!
//! The noun phrase returns its determiner, whether it is targeted, its source
//! exclusion, its query (characteristic predicate plus every player relation
//! the relation verbs state) and the CR 109.2a domain it is in. A parent
//! (a verb frame, a count, a zone collection) only type-checks this result:
//! it never re-reads a quantity, an `other`, a relation or a domain itself.
//! The quantity is the shared numeric expression, so a count read here is the
//! same typed amount every other numeric slot reads.
use super::super::{MachineResult, OracleTokenCursor, parse_number_word};
use super::{
    ExpressedTargetCardinality, NumericExpression, ObjectKind, ObjectQueryContext,
    RecognizedObjectQuery, recognize_expressed_target_cardinality, recognize_numeric_expression,
    recognize_object_noun_query, recognize_object_set_query, recognize_optional_source_exclusion,
};

/// How many objects a resolution-time choice selects (CR 608.2c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ChosenObjectQuantity {
    Exactly(NumericExpression),
    UpTo(NumericExpression),
    /// `any number [of]`: zero or more, unbounded.
    AnyNumber,
}

/// The determiner of an object noun phrase. Target cardinality, a
/// resolution choice and universal membership are distinct contracts; the
/// determiner states which one, and the parent decides which it admits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectDeterminer {
    /// `target`, `two target`, `up to X target`, `another target`, `two
    /// other target`: declared on casting or activation (CR 601.2c).
    Target {
        cardinality: ExpressedTargetCardinality,
        exclude_source: bool,
    },
    /// `a`, `an`, `another`, `N`, `up to N`, `any number`: chosen while the
    /// effect resolves.
    Chosen {
        quantity: ChosenObjectQuantity,
        /// `another`: other than the ability source.
        another: bool,
    },
    /// `each`, `all`: every member, no choice.
    Universal { each: bool },
}

impl ObjectDeterminer {
    pub(in crate::oracle::canonical) fn is_targeted(&self) -> bool {
        matches!(self, Self::Target { .. })
    }

    fn excludes_source(&self) -> bool {
        match self {
            Self::Target { exclude_source, .. } => *exclude_source,
            Self::Chosen { another, .. } => *another,
            Self::Universal { .. } => false,
        }
    }
}

/// The domains a parent's slot can hold, in the order the noun phrase tries
/// them, and the facts its query receives. The recognizer infers the domain;
/// the parent requires the one it admits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectNounPhraseExpectation<'a> {
    pub domains: &'a [ObjectKind],
    pub context: ObjectQueryContext,
}

impl<'a> ObjectNounPhraseExpectation<'a> {
    pub(in crate::oracle::canonical) const fn in_domains(domains: &'a [ObjectKind]) -> Self {
        Self {
            domains,
            context: ObjectQueryContext::ORDINARY,
        }
    }
}

/// One recognized object noun phrase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectNounPhrase {
    /// `None` for a bare plural, which is universal like `each`/`all`.
    pub determiner: Option<ObjectDeterminer>,
    /// `other`/`another`: the ability source is not a member.
    pub exclude_source: bool,
    pub query: RecognizedObjectQuery,
    /// The first expected domain the phrase is in (CR 109.2a), or `None`.
    pub domain: Option<ObjectKind>,
}

impl ObjectNounPhrase {
    pub(in crate::oracle::canonical) fn is_targeted(&self) -> bool {
        self.determiner
            .as_ref()
            .is_some_and(ObjectDeterminer::is_targeted)
    }

    /// Every member, with no choice: a bare plural, `each` or `all`.
    pub(in crate::oracle::canonical) fn is_universal(&self) -> bool {
        matches!(
            self.determiner,
            None | Some(ObjectDeterminer::Universal { .. })
        )
    }
}

/// Whether a numeric count (`two`, `X`, `that many`, `half X`) begins at the
/// cursor. Dispatch hint only; the shared numeric grammar reads the amount.
fn numeric_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let Some(token) = cursor.peek_token() else {
        return false;
    };
    let raw = token.raw.as_str();
    if raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an") {
        return false;
    }
    token.token_type == "quantity"
        || raw.eq_ignore_ascii_case("x")
        || parse_number_word(raw).is_some()
        || raw.eq_ignore_ascii_case("half")
        || raw.eq_ignore_ascii_case("twice")
        || (raw.eq_ignore_ascii_case("that")
            && cursor.peek_ahead(1).is_some_and(|next| {
                next.eq_ignore_ascii_case("many") || next.eq_ignore_ascii_case("much")
            }))
}

/// `N [other] target` / `up to N [other] target` at the cursor: the target
/// cardinality, its optional `other`, and the `target` head. Transactional:
/// the cursor moves only when the whole target determiner is present.
fn try_recognize_target_determiner(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectDeterminer>> {
    let mut trial = cursor.clone();
    let cardinality = if trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
    {
        ExpressedTargetCardinality::EXACTLY_ONE
    } else {
        match recognize_expressed_target_cardinality(&mut trial) {
            Ok(cardinality) => cardinality,
            Err(_) => return Ok(None),
        }
    };
    let exclude_source = recognize_optional_source_exclusion(&mut trial)?;
    if !trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
    {
        return Ok(None);
    }
    trial.consume("target")?;
    *cursor = trial;
    Ok(Some(ObjectDeterminer::Target {
        cardinality,
        exclude_source,
    }))
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// Reads the determiner of an object noun phrase, or `None` when the phrase
/// begins with its noun (a bare plural). Transactional: an absent determiner
/// consumes nothing, so a parent never fails inside a numeric grammar for a
/// phrase that has no count.
pub(in crate::oracle::canonical) fn recognize_object_determiner(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectDeterminer>> {
    cursor.enter("noun-phrase.determiner");
    if word_at(cursor, 0, "each") || word_at(cursor, 0, "all") {
        let each = word_at(cursor, 0, "each");
        let raw = cursor
            .peek()
            .expect("checked universal determiner")
            .to_string();
        cursor.consume(&raw)?;
        return Ok(Some(ObjectDeterminer::Universal { each }));
    }
    if word_at(cursor, 0, "another") {
        cursor.consume("another")?;
        if word_at(cursor, 0, "target") {
            cursor.consume("target")?;
            return Ok(Some(ObjectDeterminer::Target {
                cardinality: ExpressedTargetCardinality::EXACTLY_ONE,
                exclude_source: true,
            }));
        }
        return Ok(Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::Exactly(NumericExpression::from(1)),
            another: true,
        }));
    }
    if word_at(cursor, 0, "any") && word_at(cursor, 1, "number") {
        cursor.consume_phrase(&["any", "number"])?;
        return Ok(Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::AnyNumber,
            another: false,
        }));
    }
    if word_at(cursor, 0, "target") || word_at(cursor, 0, "up") || numeric_count_starts(cursor) {
        if let Some(target) = try_recognize_target_determiner(cursor)? {
            return Ok(Some(target));
        }
    }
    if word_at(cursor, 0, "a") || word_at(cursor, 0, "an") {
        let article = cursor.peek().expect("checked article").to_string();
        cursor.consume(&article)?;
        return Ok(Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::Exactly(NumericExpression::from(1)),
            another: false,
        }));
    }
    if word_at(cursor, 0, "up") && word_at(cursor, 1, "to") {
        cursor.consume_phrase(&["up", "to"])?;
        cursor.enter("noun-phrase.determiner.up-to");
        return Ok(Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::UpTo(recognize_numeric_expression(cursor, None)?),
            another: false,
        }));
    }
    if numeric_count_starts(cursor) {
        cursor.enter("noun-phrase.determiner.count");
        return Ok(Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::Exactly(recognize_numeric_expression(cursor, None)?),
            another: false,
        }));
    }
    Ok(None)
}

/// The domain of a recognized phrase among the parent's expected domains:
/// the shared CR 109.2a authority first; then a nominal head the permanent
/// domain admits (`tokens`, `commanders`) related by control names
/// permanents (CR 108.4, 110.2).
pub(in crate::oracle::canonical) fn infer_noun_phrase_domain(
    query: &RecognizedObjectQuery,
    expected: &[ObjectKind],
) -> Option<ObjectKind> {
    query.predicate().inferred_domain(expected).or_else(|| {
        (expected.contains(&ObjectKind::Permanent)
            && query.relations().has_controller()
            && query.predicate().supports_domain(ObjectKind::Permanent)
            && query.predicate().has_nominal_head())
        .then_some(ObjectKind::Permanent)
    })
}

/// Recognizes one object noun phrase: determiner, source exclusion, the
/// shared object query, and the inferred domain. A universal phrase reads a
/// coordinated set (`all artifacts and creatures`); a targeted or chosen
/// phrase reads one query with its own coordination.
pub(in crate::oracle::canonical) fn recognize_object_noun_phrase(
    cursor: &mut OracleTokenCursor<'_>,
    expected: ObjectNounPhraseExpectation<'_>,
) -> MachineResult<ObjectNounPhrase> {
    let determiner = recognize_object_determiner(cursor)?;
    let (query, exclude_source) = match &determiner {
        None | Some(ObjectDeterminer::Universal { .. }) => {
            cursor.enter("noun-phrase.collection");
            let set = recognize_object_set_query(cursor)?;
            (set.query, set.exclude_source)
        }
        Some(determiner) => {
            if matches!(
                determiner,
                ObjectDeterminer::Chosen {
                    quantity: ChosenObjectQuantity::AnyNumber,
                    ..
                }
            ) && cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("of"))
            {
                cursor.consume("of")?;
            }
            cursor.enter("noun-phrase.source-exclusion");
            let other = recognize_optional_source_exclusion(cursor)?;
            cursor.require(
                !(other && determiner.excludes_source()),
                "one source exclusion for one noun phrase",
            )?;
            cursor.enter("noun-phrase.query");
            let query = recognize_object_noun_query(cursor, expected.context)?;
            (query, other || determiner.excludes_source())
        }
    };
    let domain = infer_noun_phrase_domain(&query, expected.domains);
    Ok(ObjectNounPhrase {
        determiner,
        exclude_source,
        query,
        domain,
    })
}

#[cfg(test)]
#[path = "object_argument_tests.rs"]
mod tests;

//! Shared `for each COUNTABLE` numeric child (CR 107.3 style multiplier).
//!
//! The child owns only the counted operand. The parent that owns the action
//! (an effect sequence, a mana production or a continuous modifier) decides
//! which quantity the count multiplies, so no action-specific recognizer is
//! needed for a new combination.
//!
//! ```text
//! ForEach  := "for" "each" Countable
//! Countable := ["other"] permanent collection      (countPermanents)
//!            | counter-name "counter(s)" "on this" NOUN   (countCounters)
//!            | "color" "among" ["other"] collection (countDistinctColors)
//!            | collection "in" zone-holders zone        (countCards)
//!            | collection "on the battlefield"          (countPermanents, any controller)
//! ```
use super::super::{MachineResult, OracleTokenCursor};
use super::*;

/// True when the cursor is at the `for each` multiplier introducer.
pub(in crate::oracle::canonical) fn for_each_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("for"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
}

fn counter_operand_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .token_slice(cursor.position(), cursor.position() + 1)
        .first()
        .is_some_and(|token| {
            matches!(
                token.token_type.as_str(),
                "counterNamePart" | "counterValue"
            )
        })
}

fn predicate_needs_declaration(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::NumericComparison { right, .. } => {
            right.requires_target_binding() || right.uses_x()
        }
        ObjectPredicate::Not(child) => predicate_needs_declaration(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            children.iter().any(predicate_needs_declaration)
        }
        _ => false,
    }
}

/// Reads one `for each COUNTABLE` multiplier and returns the count as a typed
/// numeric expression. Unsupported countables fail with a precise state.
pub(in crate::oracle::canonical) fn recognize_for_each_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.quantifier");
    cursor.consume_phrase(&["for", "each"])?;
    recognize_counted_collection(cursor, "color")
}

/// True when the cursor is at the `the number of` quantity introducer.
pub(in crate::oracle::canonical) fn number_of_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let total = usize::from(
        cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("total")),
    );
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && ["number", "of"].iter().enumerate().all(|(i, word)| {
            cursor
                .peek_ahead(1 + total + i)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads `the number of COUNTABLE`. The countable is the same typed child as
/// the `for each` multiplier, so both introducers share one count semantics.
pub(in crate::oracle::canonical) fn recognize_number_of_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.number-of.quantifier");
    cursor.consume("the")?;
    // `the total number of COUNTABLE` (CR 107.1): the same count; `total`
    // only stresses that every member of the collection is counted.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("total"))
    {
        cursor.consume("total")?;
    }
    cursor.consume_phrase(&["number", "of"])?;
    recognize_counted_collection(cursor, "colors")
}

/// Lexical lookahead: `MEASURE among ...` where MEASURE is one of the
/// distinct-value measures the counted-collection child reads (`card types`,
/// `colors`, `basic land types`).
pub(in crate::oracle::canonical) fn distinct_measure_among_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    let types = |offset: usize| word(offset, "type") || word(offset, "types");
    (word(0, "card") && types(1) && word(2, "among"))
        || ((word(0, "colors") || word(0, "color")) && word(1, "among"))
        || (word(0, "basic") && word(1, "land") && types(2) && word(3, "among"))
        || distinct_measure_at(cursor).is_some()
}

/// Reads `MEASURE among COLLECTION` as the distinct-value aggregate the
/// shared counting child produces (`there are four or more card types among
/// cards in your graveyard`). Only the distinct measures are accepted: a
/// plain object count has its own existential grammar.
pub(in crate::oracle::canonical) fn recognize_distinct_measure_among(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.distinct-measure.start");
    cursor.require(
        distinct_measure_among_starts(cursor),
        "`card types`, `colors` or `basic land types` followed by `among`",
    )?;
    let noun = cursor.peek().unwrap_or_default().to_ascii_lowercase();
    let color_noun = if noun == "color" { "color" } else { "colors" };
    let count = recognize_counted_collection(cursor, color_noun)?;
    cursor.enter("numeric.distinct-measure.kind");
    cursor.require(
        match &count {
            NumericExpression::CountDistinctCardTypes { .. } => true,
            NumericExpression::PermanentCount(node) => {
                node.measure != PermanentCountMeasure::Objects
            }
            NumericExpression::Quantity(quantity) => matches!(
                quantity.as_ref(),
                NumericQuantity::Aggregate(ObjectAggregateNode {
                    function: AggregateFunction::Distinct,
                    ..
                })
            ),
            _ => false,
        },
        "a distinct-value measure among a collection",
    )?;
    Ok(count)
}

/// The countable operand shared by every counting introducer. `color_noun`
/// is the introducer's grammatical number of the distinct-colors noun.
fn recognize_counted_collection(
    cursor: &mut OracleTokenCursor<'_>,
    color_noun: &str,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.countable");
    // `colors of mana spent to cast this spell` (Converge): a fact of the
    // spell's own payment, not a collection.
    if mana_spent_colors_starts(cursor, color_noun) {
        return recognize_mana_spent_colors(cursor, color_noun);
    }
    // `storage counter removed this way`, `cards exiled this way` before any
    // earlier action: the number this activation's cost chose (CR 601.2h).
    if cost_chosen_amount_reference_starts(cursor) {
        return recognize_cost_chosen_amount_reference(cursor);
    }
    // `{E} paid this way`: the amount an earlier payment paid.
    if super::paid_resource_result_starts(cursor) {
        return super::recognize_paid_resource_result(cursor);
    }
    // `cards revealed this way`: the objects an earlier action affected.
    if !distinct_measure_among_starts(cursor) && action_result_reference_starts(cursor) {
        return recognize_action_result_count(cursor);
    }
    if counter_operand_starts(cursor) {
        return recognize_source_counter_operand(cursor);
    }
    // `counter(s) on Kyler`: counters of every kind on the holder.
    if any_counter_operand_starts(cursor) {
        return recognize_counter_quantity(cursor, None);
    }
    // `time(s) you've cast your commander from the command zone this game`.
    if commander_cast_count_starts(cursor) {
        return recognize_commander_cast_count(cursor);
    }
    // `1 life your opponents have lost this turn`: one per point of life.
    if life_point_history_starts(cursor) {
        return recognize_life_point_history(cursor);
    }
    // `1 life you lost` in a life-loss trigger: one per point of the
    // triggering event's amount.
    if event_life_point_starts(cursor) {
        return recognize_event_life_point(cursor);
    }
    // `different mana value among CARDS`, `black mana symbols in the mana
    // costs of CARDS`: aggregates over a card collection.
    if distinct_card_value_starts(cursor) {
        return recognize_distinct_card_values(cursor);
    }
    if mana_symbols_among_cards_starts(cursor) {
        return recognize_mana_symbols_among_cards(cursor);
    }
    if opponent_count_starts(cursor) {
        return recognize_opponent_count(cursor);
    }
    // `opponents who control more lands than you`: the members whose
    // measure compares with the controller's (a dynamic threshold).
    if comparative_player_count_starts(cursor) {
        return recognize_comparative_player_count(cursor);
    }
    // `opponent who lost life this turn`, `opponents who control an
    // artifact`, `player with three or more poison counters`: the members of
    // a restricted player population (`player_set.rs`).
    if player_population_count_starts(cursor) {
        return recognize_player_population_count(cursor);
    }
    if history_count_collection_starts(cursor) {
        return recognize_history_count_collection(cursor);
    }
    // `of its colors`, `of that spell's colors` (`for each of ...`): the
    // colors of one object the binding environment names (CR 105.2).
    if object_colors_starts(cursor) {
        return recognize_object_colors(cursor);
    }
    // `colors that spell is`: the colors of the spell a cast event records.
    if spell_colors_clause_starts(cursor, color_noun) {
        return recognize_spell_colors_clause(cursor, color_noun);
    }
    // `kind(s) of counter(s) on|among HOLDER`, `creature types among SET`,
    // `different powers among SET`: the distinct values of one property
    // over any object set (`numeric.rs`, `ObjectAggregateNode`).
    if let Some((width, property)) = distinct_measure_at(cursor) {
        return recognize_distinct_measure_aggregate(cursor, width, property);
    }
    let colors = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(color_noun))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("among"));
    if colors {
        cursor.enter("numeric.for-each.colors");
        cursor.consume_phrase(&[color_noun, "among"])?;
        return recognize_distinct_among(cursor, AggregateProperty::Colors);
    }
    // `basic land type(s) among`: the distinct basic land types measure.
    let basic_land_types = ["basic", "land"].iter().enumerate().all(|(i, word)| {
        cursor
            .peek_ahead(i)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    }) && cursor
        .peek_ahead(2)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("type") || raw.eq_ignore_ascii_case("types"))
        && cursor
            .peek_ahead(3)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("among"));
    if basic_land_types {
        cursor.enter("numeric.for-each.basic-land-types");
        cursor.require(!colors, "one counting measure, not colors and land types")?;
        let noun = cursor.peek_ahead(2).unwrap_or_default().to_string();
        cursor.consume_phrase(&["basic", "land", noun.as_str(), "among"])?;
    }
    // `card type(s) among COLLECTION` (CR 205.2a): the distinct card types
    // measure over a graveyard collection of the shared scoped query.
    let card_types = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("card"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("type") || raw.eq_ignore_ascii_case("types")
        })
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("among"));
    if card_types {
        cursor.enter("numeric.for-each.card-types");
        cursor.require(!colors && !basic_land_types, "one counting measure")?;
        let noun = cursor.peek_ahead(1).unwrap_or_default().to_string();
        cursor.consume_phrase(&["card", noun.as_str(), "among"])?;
        // A graveyard collection keeps the established graveyard count;
        // any other set is the shared distinct aggregate.
        let mut trial = cursor.clone();
        if let Ok(count) = recognize_distinct_card_type_collection(&mut trial) {
            *cursor = trial;
            return Ok(count);
        }
        return recognize_distinct_among(cursor, AggregateProperty::CardTypes);
    }
    let distinct_names = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("differently"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("named"));
    if distinct_names {
        cursor.enter("numeric.for-each.distinct-names");
        cursor.require(!colors, "one counting measure, not colors and names")?;
        cursor.consume_phrase(&["differently", "named"])?;
    }
    if !colors && !distinct_names && !basic_land_types && counted_zone_relation_follows(cursor) {
        // CR 400.1: a collection with an explicit `in ZONE` relation counts
        // cards in that zone. The shared scoped query owns the predicate and
        // the zone holders; this child only names the count.
        cursor.enter("numeric.for-each.zone-collection");
        let query = recognize_scoped_object_query(cursor)?;
        cursor.require(
            query.domain == ObjectKind::Card,
            "a card collection in a graveyard, hand or exile zone",
        )?;
        return Ok(NumericExpression::CountQuery(Box::new(query)));
    }
    cursor.enter("numeric.for-each.collection");
    let query = match recognize_counted_object_collection(cursor)? {
        CountedObjectCollection::Present(query) => query,
        // `for each creature that died this turn`: a count of the turn's
        // departures through the shared scoped query (CR 700.4).
        CountedObjectCollection::Departed(departed) => {
            cursor.enter("numeric.for-each.departures");
            cursor.require(
                !colors && !distinct_names && !basic_land_types,
                "an object count of the departures of this turn",
            )?;
            return Ok(NumericExpression::CountQuery(Box::new(departed)));
        }
    };
    let exclude_source = query.exclude_source;
    // CR 110.1: `on the battlefield` names every permanent there, whoever
    // controls it. The explicit zone anchors a nominal head such as `Elves`.
    let explicit_battlefield = query.controller.is_none()
        && ["on", "the", "battlefield"]
            .iter()
            .enumerate()
            .all(|(i, word)| {
                cursor
                    .peek_ahead(i)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
            });
    if explicit_battlefield {
        cursor.enter("numeric.for-each.battlefield");
        cursor.consume_phrase(&["on", "the", "battlefield"])?;
    }
    cursor.require(
        query.denotes_battlefield_permanents()
            || (explicit_battlefield
                && query.predicate.supports_domain(ObjectKind::Permanent)
                && query.predicate.has_nominal_head()),
        "a counted collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !predicate_needs_declaration(&query.predicate),
        "a counted collection predicate independent of targets and declared variables",
    )?;
    cursor.enter("numeric.for-each.controller");
    let measure = if colors {
        // The engine evaluates distinct colors for one named player only.
        cursor.require(
            query.controller == Some(ObjectControllerScope::AbilityController),
            "a color-counted collection controlled by you",
        )?;
        PermanentCountMeasure::DistinctColors
    } else if distinct_names {
        // The engine evaluates distinct names for one named player only.
        cursor.require(
            query.controller == Some(ObjectControllerScope::AbilityController),
            "a name-counted collection controlled by you",
        )?;
        PermanentCountMeasure::DistinctNames
    } else if basic_land_types {
        // The measure is meaningful over lands; which player's lands the
        // engine can count is the lowering's decision.
        cursor.require(
            query.predicate.guarantees_card_type(ObjectCardType::Land),
            "basic land types counted among lands",
        )?;
        PermanentCountMeasure::DistinctBasicLandTypes
    } else {
        // Any control relation of the shared query is a count; the lowering
        // names the engine aggregate for it or fails explicitly.
        PermanentCountMeasure::Objects
    };
    Ok(NumericExpression::PermanentCount(Box::new(
        PermanentCountNode {
            predicate: query.predicate,
            controller: query.controller,
            exclude_source,
            measure,
        },
    )))
}

/// `colors of mana spent to cast` after a counting introducer. Lexical
/// lookahead only.
fn mana_spent_colors_starts(cursor: &OracleTokenCursor<'_>, color_noun: &str) -> bool {
    [color_noun, "of", "mana", "spent", "to", "cast"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// `the number of colors of mana spent to cast this spell` / `for each color
/// of mana spent to cast this spell` (CR 601.2h, 105.1): the distinct colors
/// among the mana paid for the spell's own cast. The spell is the ability's
/// own source (`this spell`, `it` naming the entering source, its name); the
/// card-context validation keeps the amount where that payment is recorded
/// (the spell's resolution, or the replacement applied as the cast permanent
/// enters). The spell a cast event observed has no colors record and fails.
fn recognize_mana_spent_colors(
    cursor: &mut OracleTokenCursor<'_>,
    color_noun: &str,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.mana-spent-colors.measure");
    cursor.consume_phrase(&[color_noun, "of", "mana", "spent", "to", "cast"])?;
    let spell = recognize_cast_record_spell(cursor)?;
    cursor.enter("numeric.mana-spent-colors.spell");
    cursor.require(
        spell == CastSpellReference::Source(LocalObjectReference::SourceSelf),
        "the ability's own source, whose cast payment records its colors",
    )?;
    Ok(NumericExpression::ColorsOfManaSpentToCastSource)
}

/// The collection of a card-types measure: cards of the shared scoped query
/// in the controller's graveyard or in all graveyards (the engine reads
/// those two holder populations). Any other zone or holder is refused.
fn recognize_distinct_card_type_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.card-types.collection");
    let query = recognize_scoped_object_query(cursor)?;
    cursor.require(
        query.domain == ObjectKind::Card && query.zones == [CardZoneName::Graveyard],
        "a card collection in graveyards for the card-types measure",
    )?;
    cursor.enter("numeric.for-each.card-types.holders");
    let zone = match query.holders {
        Some(ZoneHolderQueryNode::Player(LocalPlayerReference::Controller)) => {
            GraveyardScope::Controller
        }
        Some(ZoneHolderQueryNode::Players(PlayerSetReferenceNode::EachPlayer)) => {
            GraveyardScope::AllPlayers
        }
        _ => return Err(cursor.fail("`your graveyard` or `all graveyards`")),
    };
    Ok(NumericExpression::CountDistinctCardTypes {
        zone,
        predicate: query.objects.objects.predicate,
    })
}

/// The recorded event a counted history collection reads, from the verb of
/// its relative clause: `spells you've cast`, `cards you've drawn|discarded|
/// cycled`, `permanents you've sacrificed`, `creatures that died`.
fn history_count_event(lemma: VerbLemma) -> Option<(HistoryEvent, ObjectKind)> {
    Some(match lemma {
        VerbLemma::Cast => (HistoryEvent::SpellCast, ObjectKind::Spell),
        VerbLemma::Draw => (HistoryEvent::CardDrawn, ObjectKind::Card),
        VerbLemma::Discard => (HistoryEvent::CardDiscarded, ObjectKind::Card),
        VerbLemma::Cycle => (HistoryEvent::CardCycled, ObjectKind::Card),
        VerbLemma::Sacrifice => (HistoryEvent::PermanentSacrificed, ObjectKind::Permanent),
        // CR 111.1: a created token is a permanent the creation records.
        VerbLemma::Create => (HistoryEvent::TokenCreated, ObjectKind::Permanent),
        _ => return None,
    })
}

const HISTORY_COUNT_LEMMAS: &[VerbLemma] = &[
    VerbLemma::Cast,
    VerbLemma::Draw,
    VerbLemma::Discard,
    VerbLemma::Cycle,
    VerbLemma::Sacrifice,
    VerbLemma::Create,
];

/// The lemmas whose simple past (`tokens you created this turn`, `cards you
/// drew this turn`) states the same recorded action as the perfect. `cast`
/// is not among them: `spells you cast` is also the present relation.
fn simple_past_history_lemma(lemma: VerbLemma) -> bool {
    lemma != VerbLemma::Cast
}

/// The relative clause of a counted history collection at `offset`: its
/// width before the verb, the actor, and the verb's lemma.
fn history_relative_at(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> Option<(usize, HistoryPlayer, VerbLemma)> {
    let word = |at: usize, expected: &str| {
        cursor
            .peek_ahead(at)
            .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
    };
    let (width, player, form) = if word(offset, "you've") {
        (1, HistoryPlayer::Controller, VerbForm::PastParticiple)
    } else if word(offset, "you") && word(offset + 1, "have") {
        (2, HistoryPlayer::Controller, VerbForm::PastParticiple)
    } else if word(offset, "your") && word(offset + 1, "opponents") && word(offset + 2, "have") {
        // `cards your opponents have drawn this turn`: every opponent's
        // recorded actions (CR 102.3).
        (3, HistoryPlayer::Opponent, VerbForm::PastParticiple)
    } else if word(offset, "you") {
        (1, HistoryPlayer::Controller, VerbForm::Past)
    } else {
        return None;
    };
    let raw = cursor.peek_ahead(offset + width)?;
    let (entry, forms) = lookup_verb(raw, HISTORY_COUNT_LEMMAS)?;
    // The simple past claims only a this-turn history (`cards you drew this
    // turn`); `cards you discarded this way` is an action's result.
    if form == VerbForm::Past
        && !(simple_past_history_lemma(entry.lemma)
            && word(offset + width + 1, "this")
            && word(offset + width + 2, "turn"))
    {
        return None;
    }
    // `NOUN that died this turn` is a departure of the shared object query
    // (`relative_clause.rs`), counted through the scoped query; this reader
    // owns only recorded actions of the controller or its opponents.
    forms
        .contains(&form)
        .then_some((width, player, entry.lemma))
}

/// Offset of the relative clause after the counted noun phrase.
fn history_relative_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    (1..7)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| history_relative_at(cursor, *offset).is_some())
}

/// Lexical lookahead for `NOUN you've VERBed this turn`.
pub(in crate::oracle::canonical) fn history_count_collection_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    history_relative_offset(cursor).is_some()
}

/// `spells you've cast this turn`, `cards you've drawn this turn`: the number of recorded events of one
/// history pattern (CR 601.2i, 121.1, 701.9, 700.4) — the same pattern the
/// condition language's EventHistory atom compares, here counted. The noun is
/// the shared object query; its domain must be the event's object domain.
pub(in crate::oracle::canonical) fn recognize_history_count_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.history-count.noun");
    let offset = history_relative_offset(cursor)
        .ok_or_else(|| cursor.fail("a counted noun followed by a history relative clause"))?;
    let (width, player, lemma) =
        history_relative_at(cursor, offset).expect("the relative clause was checked");
    let (event, domain) = history_count_event(lemma).expect("a history count lemma");
    // `other spells you've cast this turn`: the cast record of the ability's
    // own source is not counted (CR 601.2i records every cast).
    let exclude_source = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("other"));
    if exclude_source {
        cursor.enter("numeric.history-count.other");
        cursor.require(
            event == HistoryEvent::SpellCast,
            "`other` before a cast history, whose record names the source",
        )?;
        cursor.consume("other")?;
    }
    let end = cursor.position() + offset - usize::from(exclude_source);
    let predicate = if domain == ObjectKind::Spell {
        let query = cursor.bounded(end, |cursor| {
            recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)?
                .into_spell_cast_query(cursor)
        })?;
        cursor.require(
            query.caster.is_none(),
            "one caster relation, supplied by the relative clause",
        )?;
        query.predicate
    } else {
        let query = cursor.bounded(end, recognize_object_query)?;
        cursor.require(
            query.controller.is_none(),
            "one player relation, supplied by the relative clause",
        )?;
        query.predicate
    };
    cursor.require(
        predicate.has_domain(domain) || predicate.supports_domain(domain),
        "a counted noun in the recorded event's object domain",
    )?;
    // A creation records tokens (CR 111.1): the `token` noun restates the
    // event and the rest is the created token's type line.
    let predicate = if event == HistoryEvent::TokenCreated {
        cursor.enter("numeric.history-count.created-token");
        let rest = cursor.require_value(
            created_token_filter(predicate),
            "a token noun counted by its type-line characteristics",
        )?;
        rest.unwrap_or(ObjectPredicate::ObjectKind(domain))
    } else {
        predicate
    };
    cursor.enter("numeric.history-count.relative");
    for _ in 0..=width {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    }
    cursor.enter("numeric.history-count.scope");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(NumericExpression::EventHistoryCount(Box::new(
        EventHistoryCountNode {
            pattern: EventHistoryPattern {
                event,
                player,
                object: (predicate != ObjectPredicate::ObjectKind(domain)).then_some(predicate),
                measure: HistoryMeasure::Occurrences,
                qualifiers: HistoryQualifiers {
                    exclude_source,
                    ..HistoryQualifiers::default()
                },
            },
            scope: HistoryScope::ThisTurn,
            aggregate: HistoryAggregate::Total,
        },
    )))
}

/// The created-token filter of `[TYPE] tokens you created`: the `token`
/// atom removed, the rest read on the recorded type line. `None` without a
/// `token` noun or with a characteristic a creation record does not keep.
fn created_token_filter(predicate: ObjectPredicate) -> Option<Option<ObjectPredicate>> {
    fn type_line_only(predicate: &ObjectPredicate) -> bool {
        match predicate {
            ObjectPredicate::CardType(_)
            | ObjectPredicate::Subtype(_)
            | ObjectPredicate::Supertype(_) => true,
            ObjectPredicate::Not(operand) => type_line_only(operand),
            ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => {
                !operands.is_empty() && operands.iter().all(type_line_only)
            }
            _ => false,
        }
    }
    let rest = match predicate {
        ObjectPredicate::IsToken => None,
        ObjectPredicate::Intersection(operands) if operands.contains(&ObjectPredicate::IsToken) => {
            let mut rest = operands
                .into_iter()
                .filter(|operand| *operand != ObjectPredicate::IsToken)
                .collect::<Vec<_>>();
            Some(match rest.len() {
                1 => rest.remove(0),
                _ => ObjectPredicate::Intersection(rest),
            })
        }
        _ => return None,
    };
    match rest {
        Some(rest) if !type_line_only(&rest) => None,
        rest => Some(rest),
    }
}

/// `opponent(s) you attacked ...` / `opponents you have`: the counted
/// players are opponents.
fn opponent_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("opponent") || raw.eq_ignore_ascii_case("opponents")
    }) && cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
}

/// `opponent you attacked [with a creature | with one or more creatures]
/// this combat` (CR 702.121a): the opponents the controller attacked.
fn recognize_opponent_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.opponent");
    let noun = cursor
        .peek()
        .expect("the opponent noun was checked")
        .to_string();
    cursor.consume(&noun)?;
    cursor.enter("numeric.for-each.opponent.relation");
    // `opponents you have` (CR 102.3): every opponent of the controller.
    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("have"))
    {
        cursor.consume_phrase(&["you", "have"])?;
        return Ok(NumericExpression::OpponentCount(OpponentFact::Any));
    }
    cursor.consume_phrase(&["you", "attacked"])?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("with"))
    {
        cursor.enter("numeric.for-each.opponent.attackers");
        cursor.consume("with")?;
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("one"))
        {
            cursor.consume_phrase(&["one", "or", "more", "creatures"])?;
        } else {
            cursor.consume_phrase(&["a", "creature"])?;
        }
    }
    cursor.enter("numeric.for-each.opponent.period");
    cursor.consume("this")?;
    // CR 506.2: one combat, or every combat of the turn.
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("turn"))
    {
        cursor.consume("turn")?;
        return Ok(NumericExpression::OpponentCount(
            OpponentFact::AttackedByControllerThisTurn,
        ));
    }
    cursor.consume("combat")?;
    Ok(NumericExpression::OpponentCount(
        OpponentFact::AttackedByControllerThisCombat,
    ))
}

/// The population a player noun names after a counting introducer, and
/// whether the noun is plural (`opponents who control`).
fn counted_player_noun(raw: &str) -> Option<(PlayerSetReferenceNode, bool)> {
    Some(match raw.to_ascii_lowercase().as_str() {
        "opponent" => (PlayerSetReferenceNode::OpponentsOfAbilityController, false),
        "opponents" => (PlayerSetReferenceNode::OpponentsOfAbilityController, true),
        "player" => (PlayerSetReferenceNode::EachPlayer, false),
        "players" => (PlayerSetReferenceNode::EachPlayer, true),
        _ => return None,
    })
}

/// Lexical lookahead: a player noun followed by a relative restriction
/// (`who`, `whose`, `with`). A bare `opponent` count is not this reader's.
fn player_population_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    if cursor.peek().and_then(counted_player_noun).is_none() {
        return false;
    }
    let mut lookahead = cursor.clone();
    lookahead.advance_to(lookahead.position() + 1).is_ok()
        && player_relative_clause_starts(&lookahead)
        && !(lookahead
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("other")))
}

/// `PLAYER-NOUN RESTRICTION` counted (CR 102.1-102.3): the members of the
/// population that satisfy the shared relative predicate. The restriction
/// must state a predicate: a population exclusion alone names no
/// independent fact a count reads here.
fn recognize_player_population_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.player-count.population");
    let raw = cursor
        .peek()
        .expect("the player noun was checked")
        .to_string();
    let (players, plural) = counted_player_noun(&raw).expect("the player noun was checked");
    cursor.consume(&raw)?;
    cursor.enter("numeric.player-count.restriction");
    let query = recognize_player_population_restriction_in_number(
        cursor,
        players,
        false,
        &PlayerRelativeContext::default(),
        plural,
    )?;
    cursor.require(
        query.predicate.is_some() && query.except.is_none(),
        "a relative predicate of each counted player",
    )?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::Players(query),
    )))
}

/// Lexical lookahead: `PLAYER-NOUN who VERB more|fewer|less ...`.
fn comparative_player_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().and_then(counted_player_noun).is_some()
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("who"))
        && comparative_proposition_starts(cursor, 2)
}

/// `opponent(s)|player(s) who control(s) more|fewer QUERY than you`, `who
/// has|have more|less life than you` (CR 102.1-102.3): the members of the
/// population whose measure compares with the ability controller's. The
/// verb agrees with the noun; the compared player is `you`.
fn recognize_comparative_player_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.player-count.comparative.population");
    let raw = cursor
        .peek()
        .expect("the player noun was checked")
        .to_string();
    let (players, plural) = counted_player_noun(&raw).expect("the player noun was checked");
    cursor.consume(&raw)?;
    cursor.consume("who")?;
    cursor.enter("numeric.player-count.comparative.agreement");
    let verb = cursor.peek().unwrap_or_default().to_ascii_lowercase();
    cursor.require(
        if plural {
            matches!(verb.as_str(), "control" | "have")
        } else {
            matches!(verb.as_str(), "controls" | "has")
        },
        "a relative verb that agrees with the counted player noun",
    )?;
    let proposition = recognize_comparative_proposition(cursor)?;
    cursor.enter("numeric.player-count.comparative.other");
    cursor.require(
        proposition.other == ComparedPlayers::Controller,
        "`than you`: each member compared with the ability's controller",
    )?;
    Ok(NumericExpression::Quantity(Box::new(
        NumericQuantity::PlayersComparedWithController {
            players,
            measure: proposition.measure,
            operator: proposition.operator,
        },
    )))
}

/// Lexical lookahead: the counted collection is followed by `in` before the
/// end of its noun phrase. The scoped query proves the actual zone grammar;
/// a collection without that relation keeps the battlefield reading.
fn counted_zone_relation_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    let mut trial = cursor.clone();
    recognize_object_collection(&mut trial)
        .is_ok_and(|query| query.controller.is_none() && !query.exclude_source)
        && trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("in"))
}

/// `for each other X`: whether the count excludes the ability source. Only
/// a count whose `other` antecedent is the source lowers to `excludeSource`.
/// It visits the whole expression, so a count nested in arithmetic (`X is
/// twice the number of other Elves`) is seen too.
pub(in crate::oracle::canonical) fn count_excludes_source(count: &NumericExpression) -> bool {
    match count {
        NumericExpression::PermanentCount(node) => node.exclude_source,
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            count_excludes_source(left) || count_excludes_source(right)
        }
        NumericExpression::Quantity(quantity) => quantity.excludes_source(),
        NumericExpression::Atom(_)
        | NumericExpression::ObjectProperty { .. }
        | NumericExpression::PowerOf(_)
        | NumericExpression::ManaValueOf(_)
        | NumericExpression::QualifiedTargetProperty { .. }
        | NumericExpression::CountQuery(_)
        | NumericExpression::CountCounters { .. }
        | NumericExpression::CountersOnRecipient { .. }
        | NumericExpression::SourceCastX
        | NumericExpression::ColorsOfManaSpentToCastSource
        | NumericExpression::EventValue(_)
        | NumericExpression::CountDistinctCardTypes { .. }
        | NumericExpression::OpponentCount(_)
        | NumericExpression::EventHistoryCount(_)
        | NumericExpression::Devotion(_)
        | NumericExpression::ActionResultQuantity { .. }
        | NumericExpression::PlayerFact { .. }
        | NumericExpression::SacrificedCostObjectProperty(_)
        | NumericExpression::BoundObjectProperty { .. }
        | NumericExpression::ActionResultCount { .. }
        | NumericExpression::CardsInZone { .. }
        | NumericExpression::GreatestPropertyAmong { .. } => false,
    }
}

/// Whether an action's affected object is the ability source itself, the
/// only antecedent `other` in a count may name here (`it gets +1/+0 for
/// each other attacking Goblin`). For a collection, a target or an attached
/// object, `other` means other than each affected object (`Nephilim you
/// control get +1/+1 for each other Nephilim`), which needs the affected
/// object's binding; such a composition is refused instead of lowered to a
/// source exclusion.
pub(in crate::oracle::canonical) fn operand_is_source(operand: &PermanentOperandNode) -> bool {
    matches!(
        operand,
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf)
            | PermanentOperandNode::BoundReference(PermanentEffectReference::Source(
                LocalObjectReference::SourceSelf
            ))
    )
}

/// Lexical lookahead for `the greatest|highest|least|lowest ...`.
pub(in crate::oracle::canonical) fn greatest_property_among_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor.peek_ahead(1).and_then(extremum_function).is_some()
}

/// The aggregate function a superlative names.
fn extremum_function(raw: &str) -> Option<AggregateFunction> {
    match raw.to_ascii_lowercase().as_str() {
        "greatest" | "highest" => Some(AggregateFunction::Greatest),
        "least" | "lowest" => Some(AggregateFunction::Least),
        _ => None,
    }
}

/// `the greatest|highest|least|lowest PROPERTY among OBJECTS` (CR 107.1):
/// the extremum of one numeric property over any object set of the shared
/// aggregate reader. The greatest value among the ability controller's
/// permanents keeps its established evaluator (`GreatestPropertyAmong`);
/// every other set is the shared aggregate. A set that names an earlier
/// action's result is read only where the parent reads program results.
pub(in crate::oracle::canonical) fn recognize_greatest_property_among(
    cursor: &mut OracleTokenCursor<'_>,
    allow_action_results: bool,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.greatest-among.quantifier");
    cursor.consume("the")?;
    let superlative = cursor.peek().unwrap_or_default().to_string();
    let function = cursor.require_value(
        extremum_function(&superlative),
        "`greatest`, `highest`, `least` or `lowest`",
    )?;
    cursor.consume(&superlative)?;
    // `the greatest number of PERMANENTS that have a creature type in
    // common`: the members sharing the most common creature type.
    if function == AggregateFunction::Greatest
        && word_at(cursor, 0, "number")
        && word_at(cursor, 1, "of")
    {
        return recognize_shared_value_count(cursor);
    }
    cursor.enter("numeric.greatest-among.property");
    // The same numeric properties a selector compares (CR 208, 202.3).
    let property = recognize_selector_numeric_property(cursor)?;
    cursor.enter("numeric.greatest-among.among");
    cursor.consume("among")?;
    cursor.enter("numeric.greatest-among.collection");
    let objects = recognize_aggregate_objects(cursor, allow_action_results)?;
    if function == AggregateFunction::Greatest
        && let AggregateObjects::Permanents(query) = &objects
        && query.controller == Some(ObjectControllerScope::AbilityController)
        && !query.exclude_source
    {
        return Ok(NumericExpression::GreatestPropertyAmong {
            property,
            predicate: query.predicate.clone(),
            aggregate: PropertyAggregate::Greatest,
        });
    }
    aggregate_expression(
        cursor,
        function,
        AggregateProperty::from_selector(property),
        objects,
    )
}

/// The shared value a `greatest number of` count ranks its members by.
const SHARED_CREATURE_TYPE: [&str; 7] = ["that", "have", "a", "creature", "type", "in", "common"];

/// `number of SET that have a creature type in common` after `the
/// greatest` (CR 205.3m): the number of members of the set that have the
/// creature type most of them have (a changeling has every one).
fn recognize_shared_value_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.most-common.quantifier");
    cursor.consume_phrase(&["number", "of"])?;
    cursor.enter("numeric.most-common.collection");
    let end = (1..12)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            SHARED_CREATURE_TYPE
                .iter()
                .enumerate()
                .all(|(index, word)| word_at(cursor, offset + index, word))
        })
        .map(|offset| cursor.position() + offset);
    let end = cursor.require_value(
        end,
        "`that have a creature type in common` after the counted set",
    )?;
    let objects = cursor.bounded(end, recognize_aggregate_operand)?;
    cursor.enter("numeric.most-common.shared-value");
    cursor.consume_phrase(&SHARED_CREATURE_TYPE)?;
    aggregate_expression(
        cursor,
        AggregateFunction::MostCommon,
        AggregateProperty::CreatureTypes,
        objects,
    )
}

/// One aggregate node as a numeric expression; a non-numeric property is
/// readable only by the distinct count.
fn aggregate_expression(
    cursor: &mut OracleTokenCursor<'_>,
    function: AggregateFunction,
    property: AggregateProperty,
    objects: AggregateObjects,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.aggregate.function");
    let node = ObjectAggregateNode {
        function,
        property,
        objects,
        require_members: false,
    };
    cursor.require_value(
        node.into_expression(),
        "a numeric property for a greatest, least or total aggregate",
    )
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case(expected))
}

/// The set an aggregate reads (`among SET`, `of SET`):
///
/// ```text
/// Objects := ActionResult | Operand ("and" Operand)*
/// Operand := CARDS "in" HOLDERS ZONE          (shared scoped query)
///          | ["other"] PERMANENTS [RELATION]  (shared object collection)
///          | PERMANENTS "on the battlefield"
/// ActionResult := OBJECTS VERBed "this way" | "the" VERBed OBJECTS
/// ```
///
/// A coordination of operands is their union (`creatures you control and
/// creature cards in your graveyard`). A program result is read only where
/// the parent admits it and only after an earlier action (CR 608.2c).
pub(in crate::oracle::canonical) fn recognize_aggregate_objects(
    cursor: &mut OracleTokenCursor<'_>,
    allow_action_results: bool,
) -> MachineResult<AggregateObjects> {
    cursor.enter("numeric.aggregate.objects");
    if action_result_reference_starts(cursor) || participle_collection_starts(cursor) {
        return recognize_aggregate_action_result(cursor, allow_action_results);
    }
    let mut parts = vec![recognize_aggregate_operand(cursor)?];
    while word_at(cursor, 0, "and") {
        let mut trial = cursor.clone();
        trial.consume("and")?;
        let Ok(part) = recognize_aggregate_operand(&mut trial) else {
            break;
        };
        *cursor = trial;
        parts.push(part);
    }
    Ok(match parts.len() {
        1 => parts.pop().expect("one aggregated operand"),
        _ => AggregateObjects::Union(parts),
    })
}

/// One operand of an aggregated set: the cards of one zone, else a
/// battlefield collection (any player relation, `other`, `on the
/// battlefield`). A predicate that needs a declaration is refused.
fn recognize_aggregate_operand(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AggregateObjects> {
    cursor.enter("numeric.aggregate.operand");
    let mut trial = cursor.clone();
    if let Ok(scoped) = recognize_scoped_object_query(&mut trial)
        && scoped.departures.is_none()
        && scoped.domain != ObjectKind::Spell
    {
        *cursor = trial;
        if scoped.domain == ObjectKind::Card {
            cursor.require(
                !scoped.requires_target_binding() && !scoped.uses_x(),
                "an aggregated card collection independent of targets and declared variables",
            )?;
            return Ok(AggregateObjects::Cards(Box::new(scoped)));
        }
        // `creatures on the battlefield`: every permanent there.
        let query = scoped.objects.objects;
        cursor.require(
            !predicate_needs_declaration(&query.predicate),
            "an aggregated collection predicate independent of targets and declared variables",
        )?;
        return Ok(AggregateObjects::Permanents(query));
    }
    let query = recognize_object_collection(cursor)?;
    cursor.enter("numeric.aggregate.permanents");
    // CR 110.1: `on the battlefield` names every permanent there.
    let explicit_battlefield = query.controller.is_none()
        && word_at(cursor, 0, "on")
        && word_at(cursor, 1, "the")
        && word_at(cursor, 2, "battlefield");
    if explicit_battlefield {
        cursor.consume_phrase(&["on", "the", "battlefield"])?;
    }
    cursor.require(
        query.denotes_battlefield_permanents()
            || (explicit_battlefield
                && query.predicate.supports_domain(ObjectKind::Permanent)
                && query.predicate.has_nominal_head()),
        "an aggregated collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !predicate_needs_declaration(&query.predicate),
        "an aggregated collection predicate independent of targets and declared variables",
    )?;
    refuse_unread_relative_tail(cursor)?;
    Ok(AggregateObjects::Permanents(query))
}

/// `OBJECTS VERBed this way` / `the VERBed OBJECTS` as an aggregated set:
/// the objects the nearest earlier action of the verb affected (CR 608.2c),
/// bound by the program lowering.
fn recognize_aggregate_action_result(
    cursor: &mut OracleTokenCursor<'_>,
    allow_action_results: bool,
) -> MachineResult<AggregateObjects> {
    cursor.enter("numeric.aggregate.action-result");
    cursor.require(
        allow_action_results,
        "an aggregate over an earlier action's result where the parent reads program results",
    )?;
    let reference = if action_result_reference_starts(cursor) {
        recognize_action_result_reference(cursor)?
    } else {
        recognize_participle_collection(cursor)?
    };
    cursor.enter("numeric.aggregate.reader");
    let reader = cursor
        .binding_environment()
        .and_then(|environment| environment.effect_index)
        .filter(|index| *index > 0);
    let reader = cursor.require_value(
        reader,
        "an earlier action of this program whose affected objects the aggregate reads",
    )?;
    Ok(AggregateObjects::ActionResult { reader, reference })
}

/// `MEASURE among SET` read after its measure: the distinct values of one
/// property over the set. The colors among the ability controller's
/// permanents keep their established count (`countDistinctColors`).
fn recognize_distinct_among(
    cursor: &mut OracleTokenCursor<'_>,
    property: AggregateProperty,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.distinct.collection");
    let objects = recognize_aggregate_objects(cursor, true)?;
    if property == AggregateProperty::Colors
        && let AggregateObjects::Permanents(query) = &objects
        && query.controller == Some(ObjectControllerScope::AbilityController)
    {
        return Ok(NumericExpression::PermanentCount(Box::new(
            PermanentCountNode {
                predicate: query.predicate.clone(),
                controller: query.controller,
                exclude_source: query.exclude_source,
                measure: PermanentCountMeasure::DistinctColors,
            },
        )));
    }
    aggregate_expression(cursor, AggregateFunction::Distinct, property, objects)
}

/// The width of a distinct-value measure before its relation word (`among`,
/// `on`) and the property it counts: `creature type(s)`, `different
/// power(s)`, `[different] kind(s) of counter(s)` (`on` only for counters).
fn distinct_measure_at(cursor: &OracleTokenCursor<'_>) -> Option<(usize, AggregateProperty)> {
    let types = |offset: usize| word_at(cursor, offset, "type") || word_at(cursor, offset, "types");
    let different = usize::from(word_at(cursor, 0, "different"));
    let (width, property) = if different == 0 && word_at(cursor, 0, "creature") && types(1) {
        (2, AggregateProperty::CreatureTypes)
    } else if different == 1 && (word_at(cursor, 1, "power") || word_at(cursor, 1, "powers")) {
        (2, AggregateProperty::Power)
    } else if (word_at(cursor, different, "kind") || word_at(cursor, different, "kinds"))
        && word_at(cursor, different + 1, "of")
        && (word_at(cursor, different + 2, "counter") || word_at(cursor, different + 2, "counters"))
    {
        (different + 3, AggregateProperty::CounterKinds)
    } else {
        return None;
    };
    let relation = word_at(cursor, width, "among")
        || (property == AggregateProperty::CounterKinds && word_at(cursor, width, "on"));
    relation.then_some((width, property))
}

/// `MEASURE among SET` / `kind(s) of counter(s) on HOLDER` (CR 205.3m,
/// 208.1, 122.1): the number of different values of the measured property.
fn recognize_distinct_measure_aggregate(
    cursor: &mut OracleTokenCursor<'_>,
    width: usize,
    property: AggregateProperty,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.distinct-measure");
    for _ in 0..width {
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
    }
    if word_at(cursor, 0, "among") {
        cursor.consume("among")?;
        return recognize_distinct_among(cursor, property);
    }
    cursor.enter("numeric.for-each.distinct-measure.holder");
    cursor.consume("on")?;
    let objects = recognize_aggregate_holder(cursor)?;
    aggregate_expression(cursor, AggregateFunction::Distinct, property, objects)
}

/// The holder of `kind(s) of counter(s) on HOLDER`: the ability's source
/// (`this creature`, its name), a pronoun the binding environment resolves,
/// or a battlefield set. A target cannot be declared inside a count.
fn recognize_aggregate_holder(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AggregateObjects> {
    let names = cursor.source_names().to_vec();
    if word_at(cursor, 0, "this")
        || (!names.is_empty() && source_reference_starts_at(cursor.position(), None, &names, None))
    {
        cursor.enter("numeric.aggregate.holder.source");
        let source = recognize_source_reference(cursor, &names, None)?;
        return Ok(AggregateObjects::Object(PermanentEffectReference::Source(
            source,
        )));
    }
    if ["it", "him", "her"]
        .iter()
        .any(|pronoun| word_at(cursor, 0, pronoun))
    {
        cursor.enter("numeric.aggregate.holder.pronoun");
        let environment = installed_binding_environment(cursor);
        let reference = resolve_possessive_antecedent(&environment)
            .map_err(|expected| cursor.fail(expected))?;
        let pronoun = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&pronoun)?;
        return Ok(AggregateObjects::Object(reference));
    }
    cursor.enter("numeric.aggregate.holder.collection");
    cursor.require(
        !word_at(cursor, 0, "target"),
        "a holder that needs no target declaration inside a count",
    )?;
    recognize_aggregate_objects(cursor, true)
}

/// Lexical lookahead: `of its colors` / `of that NOUN's colors`.
fn object_colors_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "of")
        && ((word_at(cursor, 1, "its") && word_at(cursor, 2, "colors"))
            || (word_at(cursor, 1, "that")
                && cursor
                    .token_slice(cursor.position() + 2, cursor.position() + 3)
                    .first()
                    .is_some_and(|token| token.token_type == "possessiveTypeWord")
                && word_at(cursor, 3, "colors")))
}

/// `of its colors` / `of that spell's colors` after `for each` (CR 105.2):
/// the colors of one object. `its` names the binding environment's
/// possessive antecedent; `that spell's` the spell a cast event records.
/// Any other nominal has no antecedent this reader resolves and fails.
fn recognize_object_colors(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.for-each.object-colors");
    cursor.consume("of")?;
    let environment = installed_binding_environment(cursor);
    let objects = if word_at(cursor, 0, "its") {
        cursor.enter("numeric.for-each.object-colors.possessive");
        let reference = resolve_possessive_antecedent(&environment)
            .map_err(|expected| cursor.fail(expected))?;
        cursor.consume("its")?;
        AggregateObjects::Object(reference)
    } else {
        cursor.enter("numeric.for-each.object-colors.genitive");
        cursor.consume("that")?;
        cursor.require(
            word_at(cursor, 0, "spell's") && environment.frame.event_spell,
            "`that spell's` after a cast event that records the spell",
        )?;
        let raw = cursor.peek().unwrap_or_default().to_owned();
        cursor.consume(&raw)?;
        AggregateObjects::EventSpell
    };
    cursor.enter("numeric.for-each.object-colors.noun");
    cursor.consume("colors")?;
    aggregate_expression(
        cursor,
        AggregateFunction::Distinct,
        AggregateProperty::Colors,
        objects,
    )
}

/// Lexical lookahead: `colors that spell is`.
fn spell_colors_clause_starts(cursor: &OracleTokenCursor<'_>, color_noun: &str) -> bool {
    word_at(cursor, 0, color_noun)
        && word_at(cursor, 1, "that")
        && word_at(cursor, 2, "spell")
        && word_at(cursor, 3, "is")
}

/// `the number of colors that spell is` (CR 105.2): the colors of the spell
/// a cast event records.
fn recognize_spell_colors_clause(
    cursor: &mut OracleTokenCursor<'_>,
    color_noun: &str,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.number-of.spell-colors");
    cursor.consume_phrase(&[color_noun, "that", "spell", "is"])?;
    cursor.require(
        installed_binding_environment(cursor).frame.event_spell,
        "a cast event that records the spell `that spell` names",
    )?;
    aggregate_expression(
        cursor,
        AggregateFunction::Distinct,
        AggregateProperty::Colors,
        AggregateObjects::EventSpell,
    )
}

/// Lexical lookahead for `the total power|toughness|mana value of`.
pub(in crate::oracle::canonical) fn property_total_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "the")
        && word(1, "total")
        && ((word(2, "power") || word(2, "toughness")) && word(3, "of")
            || word(2, "mana") && word(3, "value") && word(4, "of"))
}

/// `the VERBed NOUNS` restating the objects an earlier action VERBed (`the
/// exiled cards`, `the destroyed permanents`). Lexical lookahead only.
fn participle_collection_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor
            .peek_ahead(1)
            .and_then(ActionResultVerb::from_word)
            .is_some()
}

/// `the VERBed NOUNS`: the same restated result as `NOUNS VERBed this way`
/// (CR 608.2c). The noun is one plural nominal token of the shared object
/// collection; it names no controller.
fn recognize_participle_collection(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActionResultReferenceNode> {
    cursor.enter("numeric.property-total.participle");
    cursor.consume("the")?;
    let verb = cursor
        .peek()
        .and_then(ActionResultVerb::from_word)
        .ok_or_else(|| cursor.fail("the past participle of an object-producing verb"))?;
    let raw = cursor.peek().unwrap_or_default().to_owned();
    cursor.consume(&raw)?;
    cursor.enter("numeric.property-total.participle-noun");
    let end = cursor.position() + 1;
    let query = cursor.bounded(end, recognize_object_collection)?;
    cursor.require(
        !query.exclude_source && query.controller.is_none() && query.predicate.has_nominal_head(),
        "a nominal object phrase without a controller relation",
    )?;
    Ok(ActionResultReferenceNode {
        predicate: query.predicate,
        verb,
    })
}

/// `the total PROPERTY of COLLECTION` (CR 107.1): one numeric
/// characteristic summed over a collection, zero when it is empty.
///
/// ```text
/// Total := "the" "total" Property "of" Collection
/// Collection := OBJECTS VERBed "this" "way" | "the" VERBed OBJECTS
///             | battlefield collection controlled by you
/// ```
///
/// A restated action result is read only where the parent admits program
/// action results (`allow_action_results`); its producer is bound by the
/// program lowering and read as last known information once the objects
/// left the battlefield (CR 608.2h). A battlefield collection is the sum
/// sibling of `the greatest PROPERTY among`.
pub(in crate::oracle::canonical) fn recognize_property_total(
    cursor: &mut OracleTokenCursor<'_>,
    allow_action_results: bool,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.property-total.quantifier");
    cursor.consume_phrase(&["the", "total"])?;
    cursor.enter("numeric.property-total.property");
    let property = recognize_selector_numeric_property(cursor)?;
    cursor.consume("of")?;
    cursor.enter("numeric.property-total.collection");
    let restated = action_result_reference_starts(cursor);
    if restated || participle_collection_starts(cursor) {
        cursor.enter("numeric.property-total.action-result");
        cursor.require(
            allow_action_results,
            "a total over an earlier action's result where the parent reads program results",
        )?;
        let reference = if restated {
            recognize_action_result_reference(cursor)?
        } else {
            recognize_participle_collection(cursor)?
        };
        cursor.enter("numeric.property-total.reader");
        let reader = cursor
            .binding_environment()
            .and_then(|environment| environment.effect_index)
            .filter(|index| *index > 0);
        let reader = cursor.require_value(
            reader,
            "an earlier action of this program whose affected objects the total reads",
        )?;
        return Ok(NumericExpression::Quantity(Box::new(
            NumericQuantity::PropertyTotal(PropertyTotalNode {
                property,
                collection: PropertyTotalCollection::ActionResult { reader, reference },
            }),
        )));
    }
    // Any other set is the shared aggregate; the ability controller's
    // permanents keep their established total (`sumPowers`).
    let objects = recognize_aggregate_objects(cursor, false)?;
    cursor.enter("numeric.property-total.battlefield");
    if let AggregateObjects::Permanents(query) = &objects
        && query.controller == Some(ObjectControllerScope::AbilityController)
        && !query.exclude_source
    {
        return Ok(NumericExpression::GreatestPropertyAmong {
            property,
            predicate: query.predicate.clone(),
            aggregate: PropertyAggregate::Sum,
        });
    }
    aggregate_expression(
        cursor,
        AggregateFunction::Total,
        AggregateProperty::from_selector(property),
        objects,
    )
}

/// Multiplies an action's literal base quantity by a shared count. A base of
/// one is the count itself; any other positive literal is an explicit product.
/// A non-literal base (a variable, an expression) has no proved reading of
/// `N for each`, so it is refused instead of reinterpreted.
pub(in crate::oracle::canonical) fn scale_by_for_each_count(
    base: &NumericExpression,
    count: NumericExpression,
) -> Option<NumericExpression> {
    match base {
        NumericExpression::Atom(NumericAtom::Integer(1)) => Some(count),
        NumericExpression::Atom(NumericAtom::Integer(amount)) if *amount > 1 => Some(
            NumericExpression::multiply(NumericExpression::from(*amount), count),
        ),
        _ => None,
    }
}

/// Applies a trailing `for each` count to the quantity of the action it
/// follows. Only actions whose quantity is the shared numeric expression
/// accept it; every other action is refused so no text is dropped.
pub(in crate::oracle::canonical) fn apply_for_each_multiplier(
    effect: &mut LocalEffectNode,
    count: NumericExpression,
) -> bool {
    // `gets +1/+1 for each ...`: each component of the signed delta is
    // scaled by the same count; a zero component stays zero (CR 613.4c).
    let affected = match effect {
        LocalEffectNode::ModifyStats(node) => Some(&node.targets),
        // One count scales one placement; listed or chosen kinds would keep
        // their own unscaled counts, so they are refused.
        LocalEffectNode::PutCounters(node) if !node.kinds.is_single() => return false,
        LocalEffectNode::PutCounters(node) => Some(&node.permanent),
        _ => None,
    };
    if count_excludes_source(&count) && affected.is_some_and(|operand| !operand_is_source(operand))
    {
        return false;
    }
    if let LocalEffectNode::ModifyStats(node) = effect {
        let scale = |component: &NumericExpression| match component {
            NumericExpression::Atom(NumericAtom::Integer(0)) => Some(component.clone()),
            NumericExpression::Atom(NumericAtom::Integer(amount)) if *amount < 0 => {
                scale_by_for_each_count(&NumericExpression::from(-*amount), count.clone())
                    .map(|scaled| NumericExpression::multiply(NumericExpression::from(-1), scaled))
            }
            _ => scale_by_for_each_count(component, count.clone()),
        };
        let (Some(power), Some(toughness)) =
            (scale(&node.delta.power), scale(&node.delta.toughness))
        else {
            return false;
        };
        node.delta.power = power;
        node.delta.toughness = toughness;
        return true;
    }
    // `create a Treasure token for each ...`: a catalog creation with a
    // literal count reads its count as the shared creation body of the same
    // predefined identity (CR 111.10), whose quantity is an expression.
    if let LocalEffectNode::CreateNamedTokens(node) = &*effect {
        let Some(scaled) = scaled_catalog_creation(node, count) else {
            return false;
        };
        *effect = LocalEffectNode::CreateExplicitTokens(scaled);
        return true;
    }
    let quantity = match effect {
        // `investigate for each ...`: that many separate investigations.
        LocalEffectNode::Investigate(node) => &mut node.times,
        LocalEffectNode::DrawCards(node) => &mut node.count,
        LocalEffectNode::GainLife(node) => &mut node.amount,
        LocalEffectNode::LoseLife(node) => &mut node.amount,
        LocalEffectNode::PutCounters(node) => &mut node.count,
        LocalEffectNode::DealDamage(node) => &mut node.amount,
        LocalEffectNode::CreateExplicitTokens(node) => &mut node.body.quantity,
        LocalEffectNode::PlayerAction(node) => match &mut node.action {
            SingularPlayerAction::Draw(quantity) | SingularPlayerAction::Mill(quantity) => quantity,
            SingularPlayerAction::Counters(node) => &mut node.count,
            SingularPlayerAction::Life(life) => &mut life.amount,
            SingularPlayerAction::CreateTokens(body) => &mut body.quantity,
            SingularPlayerAction::Scry(count) => count,
            SingularPlayerAction::Discard(_)
            | SingularPlayerAction::Sacrifice(_)
            | SingularPlayerAction::Venture(_)
            | SingularPlayerAction::Proliferate(_)
            | SingularPlayerAction::Hand(_)
            | SingularPlayerAction::ExtraTurn
            | SingularPlayerAction::Designation(_)
            | SingularPlayerAction::Skip(_)
            | SingularPlayerAction::Prohibition(_)
            | SingularPlayerAction::Status(_)
            | SingularPlayerAction::SetLifeTotal(_)
            | SingularPlayerAction::Pay(_)
            | SingularPlayerAction::ReferencedCards { .. } => return false,
        },
        _ => return false,
    };
    match scale_by_for_each_count(quantity, count) {
        Some(scaled) => {
            *quantity = scaled;
            true
        }
        None => false,
    }
}

/// The catalog creation `create N IDENTITY token(s)` scaled by a `for each`
/// count: the same predefined identity, tapped state and creating player,
/// with the scaled quantity. A choice among identities (`a Food or a
/// Treasure token`) or a non-literal count is not one scalable quantity.
fn scaled_catalog_creation(
    node: &CreateNamedTokensEffectNode,
    count: NumericExpression,
) -> Option<ExplicitTokenCreationNode> {
    let NamedTokenCreationPattern::Single(spec) = &node.creation else {
        return None;
    };
    let NumericAtom::Integer(base) = spec.quantity else {
        return None;
    };
    let quantity = scale_by_for_each_count(&NumericExpression::from(base), count)?;
    Some(ExplicitTokenCreationNode {
        player: node.player,
        body: TokenCreationBodyNode {
            quantity,
            spec: TokenSpecNode::Predefined(spec.token_name.clone()),
            keywords: Vec::new(),
            abilities: Vec::new(),
            tapped: spec.tapped,
            attacking: false,
            attack_defender: None,
            name: None,
            legendary: false,
            attached_to: None,
        },
        additional: Vec::new(),
    })
}

/// Offset of `with the EXTREMUM` after `you control the NOUN`, before the
/// end of the clause.
fn superlative_holder_offset(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    if !(word_at(cursor, 0, "you") && word_at(cursor, 1, "control") && word_at(cursor, 2, "the")) {
        return None;
    }
    (4..12)
        .take_while(|offset| {
            cursor
                .peek_ahead(offset - 1)
                .is_some_and(|raw| !matches!(raw, "." | "," | ";" | ":"))
        })
        .find(|offset| {
            word_at(cursor, *offset, "with")
                && word_at(cursor, offset + 1, "the")
                && cursor
                    .peek_ahead(offset + 2)
                    .and_then(extremum_function)
                    .is_some()
        })
}

/// Lexical lookahead for `you control the NOUN with the greatest|least ...`.
pub(in crate::oracle::canonical) fn superlative_holder_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    superlative_holder_offset(cursor).is_some()
}

/// `you control the NOUN with the greatest|least PROPERTY [among SET] or
/// tied for the greatest|least PROPERTY` (CR 107.1): one of the controller's
/// NOUNs holds the extremum of the population (every NOUN on the
/// battlefield, or SET). It is the comparison of two shared aggregates:
/// the extremum among the controller's NOUNs, undefined when there is none,
/// reaches the extremum of the population. The strict form (`the creature
/// with the greatest power` without the tie) is refused: a strict
/// comparison with another population's empty extremum has no value.
pub(in crate::oracle::canonical) fn recognize_superlative_holder_atom(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ConditionAtom> {
    cursor.enter("condition.superlative-holder.subject");
    let offset = cursor.require_value(
        superlative_holder_offset(cursor),
        "`you control the NOUN with the greatest|least PROPERTY`",
    )?;
    let with = cursor.position() + offset;
    cursor.consume_phrase(&["you", "control", "the"])?;
    cursor.enter("condition.superlative-holder.noun");
    let noun = cursor.bounded(with, recognize_object_collection)?;
    cursor.require(
        noun.controller.is_none()
            && !noun.exclude_source
            && noun.predicate.has_nominal_head()
            && noun.predicate.supports_domain(ObjectKind::Permanent)
            && !predicate_needs_declaration(&noun.predicate),
        "a permanent noun without a relation of its own, ranked by the superlative",
    )?;
    cursor.enter("condition.superlative-holder.extremum");
    cursor.consume_phrase(&["with", "the"])?;
    let raw = cursor.peek().unwrap_or_default().to_owned();
    let function = cursor.require_value(
        extremum_function(&raw),
        "`greatest`, `highest`, `least` or `lowest`",
    )?;
    cursor.consume(&raw)?;
    let property = recognize_selector_numeric_property(cursor)?;
    cursor.enter("condition.superlative-holder.population");
    let population = if word_at(cursor, 0, "among") {
        cursor.consume("among")?;
        match recognize_aggregate_operand(cursor)? {
            AggregateObjects::Permanents(query) => query,
            _ => return Err(cursor.fail("a population of battlefield permanents")),
        }
    } else {
        ObjectQueryNode {
            predicate: noun.predicate.clone(),
            controller: None,
            exclude_source: false,
        }
    };
    cursor.enter("condition.superlative-holder.tie");
    cursor.require(
        word_at(cursor, 0, "or") && word_at(cursor, 1, "tied"),
        "`or tied for the ...`: a holder that may share the extremum",
    )?;
    cursor.consume_phrase(&["or", "tied", "for", "the"])?;
    let restated = cursor.peek().unwrap_or_default().to_owned();
    cursor.require(
        extremum_function(&restated) == Some(function),
        "the same extremum restated after `tied for the`",
    )?;
    cursor.consume(&restated)?;
    let restated_property = recognize_selector_numeric_property(cursor)?;
    cursor.require(
        restated_property == property,
        "the same property restated after `tied for the`",
    )?;
    let property = AggregateProperty::from_selector(property);
    let held = ObjectAggregateNode {
        function,
        property,
        objects: AggregateObjects::Permanents(ObjectQueryNode {
            predicate: noun.predicate,
            controller: Some(ObjectControllerScope::AbilityController),
            exclude_source: false,
        }),
        require_members: true,
    };
    let ranked = ObjectAggregateNode {
        function,
        property,
        objects: AggregateObjects::Permanents(population),
        require_members: true,
    };
    let (Some(held), Some(ranked)) = (held.into_expression(), ranked.into_expression()) else {
        return Err(cursor.fail("a numeric property ranked by the superlative"));
    };
    Ok(ConditionAtom::Compare(ConditionComparison {
        subject: ConditionPlayerSubject::Controller,
        left: ConditionNumber::Expression(held),
        operator: match function {
            AggregateFunction::Least => NumericComparisonOperator::LessThanOrEqual,
            _ => NumericComparisonOperator::GreaterThanOrEqual,
        },
        right: ConditionNumber::Expression(ranked),
    }))
}

#[cfg(test)]
#[path = "player_population_count_tests.rs"]
mod player_population_count_tests;

#[cfg(test)]
#[path = "history_count_relation_tests.rs"]
mod history_count_relation_tests;

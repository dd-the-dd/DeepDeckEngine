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
    ["the", "number", "of"].iter().enumerate().all(|(i, word)| {
        cursor
            .peek_ahead(i)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    })
}

/// Reads `the number of COUNTABLE`. The countable is the same typed child as
/// the `for each` multiplier, so both introducers share one count semantics.
pub(in crate::oracle::canonical) fn recognize_number_of_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.number-of.quantifier");
    cursor.consume_phrase(&["the", "number", "of"])?;
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
    let colors = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(color_noun))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("among"));
    if colors {
        cursor.enter("numeric.for-each.colors");
        cursor.consume_phrase(&[color_noun, "among"])?;
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
        return recognize_distinct_card_type_collection(cursor);
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

/// Lexical lookahead for `the greatest power|toughness among ...`.
pub(in crate::oracle::canonical) fn greatest_property_among_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.eq_ignore_ascii_case("greatest") || raw.eq_ignore_ascii_case("highest")
        })
}

/// `the greatest power|toughness among COLLECTION`: a maximum aggregate over
/// the same battlefield collection the counting introducers read. The engine
/// evaluates it for the ability controller's permanents only, so any other
/// controller relation is refused explicitly.
pub(in crate::oracle::canonical) fn recognize_greatest_property_among(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<NumericExpression> {
    cursor.enter("numeric.greatest-among.quantifier");
    cursor.consume("the")?;
    let superlative = cursor.peek().unwrap_or_default().to_string();
    cursor.require(
        superlative.eq_ignore_ascii_case("greatest") || superlative.eq_ignore_ascii_case("highest"),
        "`greatest` or `highest`",
    )?;
    cursor.consume(&superlative)?;
    cursor.enter("numeric.greatest-among.property");
    // The same numeric properties a selector compares (CR 208, 202.3).
    let property = recognize_selector_numeric_property(cursor)?;
    cursor.enter("numeric.greatest-among.among");
    cursor.consume("among")?;
    cursor.enter("numeric.greatest-among.collection");
    let query = recognize_object_collection(cursor)?;
    cursor.require(
        !query.exclude_source,
        "an aggregated collection without a source exclusion",
    )?;
    cursor.require(
        query.denotes_battlefield_permanents(),
        "an aggregated collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !predicate_needs_declaration(&query.predicate),
        "an aggregated collection predicate independent of targets and declared variables",
    )?;
    cursor.enter("numeric.greatest-among.controller");
    cursor.require(
        query.controller == Some(ObjectControllerScope::AbilityController),
        "an aggregated collection controlled by you",
    )?;
    Ok(NumericExpression::GreatestPropertyAmong {
        property,
        predicate: query.predicate,
        aggregate: PropertyAggregate::Greatest,
    })
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
    let query = recognize_object_collection(cursor)?;
    cursor.enter("numeric.property-total.battlefield");
    cursor.require(
        !query.exclude_source,
        "a totaled collection without a source exclusion",
    )?;
    cursor.require(
        query.denotes_battlefield_permanents(),
        "a totaled collection anchored in the battlefield-permanent domain",
    )?;
    cursor.require(
        !predicate_needs_declaration(&query.predicate),
        "a totaled collection predicate independent of targets and declared variables",
    )?;
    cursor.enter("numeric.property-total.controller");
    cursor.require(
        query.controller == Some(ObjectControllerScope::AbilityController),
        "a totaled collection controlled by you",
    )?;
    Ok(NumericExpression::GreatestPropertyAmong {
        property,
        predicate: query.predicate,
        aggregate: PropertyAggregate::Sum,
    })
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
    let quantity = match effect {
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

#[cfg(test)]
#[path = "player_population_count_tests.rs"]
mod player_population_count_tests;

#[cfg(test)]
#[path = "history_count_relation_tests.rs"]
mod history_count_relation_tests;

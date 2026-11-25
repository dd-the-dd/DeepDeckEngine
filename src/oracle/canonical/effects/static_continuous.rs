//! Static continuous-effect frame (CR 611.3, 604.1):
//!
//! ```text
//! Static    := [ability word "—"] [("as long as" Condition | TurnCondition) ","]
//!              (Subjects Predicates Sentence* | "you control" AttachedObject
//!               | "all creatures able to block" Subject "do so")
//!              [keyword reminder]
//! Subjects  := Subject (("," | "and" | ", and") Subject)*
//! Subject   := source reference | "equipped creature" | "enchanted" PERMANENT
//!            | ATTACHED NOUN "'s" (possessed predicate) | player subject
//!            | ["each" | "all"] ["other"] permanent collection
//! Predicates:= Predicate (("and" | "," ["and"]) Predicate)* [Gate] "."
//! Sentence  := ("It" | "They" | "It's" | "Its" | "Otherwise," PRONOUN)
//!              Predicates           -- a continuation of the same subject
//! Gate      := "as long as" Condition | "if" Condition | "unless" Condition
//!            | TurnCondition
//! TurnCondition := "during your turn" | "during turns other than yours"
//! Predicate := ("get" | "gets") signed P/T delta ["instead"]
//!            | ("have" | "has") keyword list
//!            | ("is" | "are") (type change | COLOR list | "all colors" | "colorless"
//!                            | characteristic complement)
//!              -- `a 3/3 black Ooze creature. It's still a land.`, `1/1
//!              -- creatures that are still lands`, `a creature with base
//!              -- power and toughness 5/5 in addition to its other types`:
//!              -- the shared characteristic-change complement (CR 205.1b)
//!            | ("isn't" | "aren't") "a" CARD-TYPE | shared combat clause
//!            | ("enter" | "enters") ("tapped" | "with" N "additional" COUNTER "counter(s) on them")
//!            | ("doesn't" | "don't") "untap during" OWNER "untap step"
//!            | ("loses" | "lose") "all abilities"
//!            | "can attack as though" PRONOUN "didn't have defender"
//!            | "can be blocked as though" PRONOUN "didn't have" LANDWALK
//!            | ("assigns" | "assign") "combat damage equal to" PRONOUN
//!              "toughness rather than" PRONOUN "power"
//!            | "can't have counters put on" PRONOUN
//!            | POSSESSIVE "activated abilities can't be activated"
//!              ["unless they're mana abilities"]
//!            | "all creatures able to block" PRONOUN "do so"
//! ```
//!
//! A player subject (`you`, `players`, `each opponent`, an attached
//! permanent's controller) takes the shared player predicates
//! (`static_player_predicate`): keywords, prohibitions, revealed hands, hand
//! size, additional land plays and skipped steps.
//!
//! A turn condition (CR 611.3a) is a continuously evaluated condition on the
//! turn's active player; it gates the whole ability like `as long as`.
//! Combat restrictions and requirements reuse the shared combat-clause child
//! (CR 508.1c-d, 509.1b-c) for any subject.
//!
//! Predicates are joined by `and`, or by a serial comma before a predicate
//! verb. The copula's number must agree with the subject (`each land is`,
//! `lands are`); the shared type-change child then reads the same number.
//! Coordinated subjects keep their own qualifiers and share the predicate
//! list: each predicate lowers once per subject.
//!
//! `as long as` (CR 611.3a) gates the whole static ability with one shared
//! Boolean condition that the engine re-evaluates continuously; it lowers to
//! the static rule's `condition`, never to a per-predicate approximation.
//! When continuation sentences follow, each sentence gates its own
//! modifiers: `Otherwise` applies while the previous condition is false,
//! and `instead` suspends the replaced modifier while its condition holds.
//!
//! Every slot is filled by an existing shared child (source reference, object
//! collection, signed P/T delta, keyword list). Each predicate lowers to one
//! battlefield-active modifier the engine already executes. Adding a subject
//! form or a predicate verb extends this frame once for every combination.
use super::super::*;
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_landwalk_word, lookup_permanent_card_type,
    type_line_has_permanent_card_type,
};
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticContinuousParseAttempt {
    Parsed(CanonicalRuleDraft),
    /// The frame committed (a complete subject followed by a continuous
    /// predicate verb) and then failed. Callers keep trying later readers and
    /// surface this failure only when no reader claims the ability.
    Rejected(MachineFailure),
    NotClaimed,
}

/// Late adapter: specific static families keep priority, and any shape this
/// frame cannot fully consume is left to the following native readers so
/// their diagnostics are preserved. Nothing is accepted partially.
pub(in crate::oracle::canonical) fn try_parse_static_continuous_effect(
    input: &AbilityInput<'_>,
) -> StaticContinuousParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticContinuousParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-continuous-effect", input.tokens);
    let mut committed = false;
    let mut zone_of_function = None;
    match read_static_continuous_effect(&mut cursor, input, &mut committed, &mut zone_of_function) {
        Ok((modifiers, condition)) => {
            let trace = cursor
                .transitions()
                .iter()
                .map(|transition| transition.state)
                .collect::<Vec<_>>()
                .join(" -> ");
            let mut rule = json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_static_zone(zone_of_function),
                "modifiers": modifiers,
            });
            if let Some(condition) = &condition {
                rule["condition"] = lower_condition(condition);
            }
            // CR 702.103b: an attachment subject read on a bestow face waits
            // for card composition to prove that face's bestow keyword.
            if attachment_subject_source(input.face_type_line, false, true)
                == Some(AttachmentSubjectSource::Bestow)
                && value_mentions_attached_permanent(&rule["modifiers"])
            {
                rule[BESTOW_SUBJECT_DIRECTIVE] = json!("bestow");
            }
            StaticContinuousParseAttempt::Parsed(draft(
                rule,
                &[
                    "Read the affected objects with the shared source/attachment/collection subject",
                    "Read each continuous predicate with the shared P/T delta or keyword-list child",
                    "Lower each predicate to one battlefield-active modifier on the same objects",
                    &trace,
                ],
            ))
        }
        Err(failure) if committed => StaticContinuousParseAttempt::Rejected(failure),
        Err(_) => StaticContinuousParseAttempt::NotClaimed,
    }
}

/// The leading gates of a static ability (CR 611.3a, 113.6): any number of
/// `during …` turn conditions and `as long as …` conditions, each closed by
/// its comma, joined by `and`. `as long as SOURCE is in your graveyard`
/// declares the source's zone of function instead of a condition the engine
/// evaluates on the battlefield.
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct LeadingStaticCondition {
    pub condition: Option<StaticConditionNode>,
    /// The static zone of function (`graveyard`) the condition declares.
    pub zone_of_function: Option<&'static str>,
}

/// `as long as this card is in your graveyard [and …]` (CR 113.6, 604.3): the
/// width of the source reference and its zone phrase at the cursor, when the
/// atom closes the clause or is followed by `and`.
fn source_graveyard_declaration_width(
    cursor: &OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> Option<usize> {
    if !source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        input.name_links,
        input.name_resolution_failure,
    ) {
        return None;
    }
    let mut trial = cursor.clone();
    recognize_source_reference(&mut trial, input.name_links, input.name_resolution_failure).ok()?;
    let reference_width = trial.position() - cursor.position();
    let zone_phrase = ["is", "in", "your", "graveyard"];
    let zone_matches = zone_phrase
        .iter()
        .enumerate()
        .all(|(offset, word)| word_in(trial.peek_ahead(offset), &[*word]));
    let closes = trial
        .peek_ahead(zone_phrase.len())
        .is_some_and(|raw| raw == "," || raw.eq_ignore_ascii_case("and"));
    (zone_matches && closes).then_some(reference_width + zone_phrase.len())
}

/// Reads every leading gate of a static ability with its comma. A failure
/// after `as long as` or `during` is committed: the frame owns the gate.
pub(in crate::oracle::canonical) fn read_leading_static_conditions(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    committed: &mut bool,
) -> MachineResult<LeadingStaticCondition> {
    let mut conditions = Vec::new();
    let mut zone_of_function = None;
    loop {
        if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("during"))
        {
            // `during` introduces a turn condition of the whole static ability.
            *committed = true;
        }
        if let Some(turn_condition) = recognize_optional_turn_condition(cursor)? {
            conditions.push(turn_condition);
        } else if as_long_as_starts(cursor) {
            cursor.enter("static.continuous.leading-condition");
            cursor.consume_phrase(&["as", "long", "as"])?;
            *committed = true;
            if let Some(width) = source_graveyard_declaration_width(cursor, input) {
                // CR 113.6: the parent declares the source's zone of
                // function; the engine reads the static ability only while
                // the card is in that zone.
                cursor.enter("static.continuous.leading-condition.zone-of-function");
                cursor.require(
                    zone_of_function.is_none(),
                    "one zone of function for the static ability",
                )?;
                cursor.advance_to(cursor.position() + width)?;
                zone_of_function = Some("graveyard");
                if word_in(cursor.peek(), &["and"]) {
                    cursor.consume("and")?;
                } else {
                    cursor.enter("static.continuous.leading-condition.separator");
                    cursor.consume(",")?;
                    continue;
                }
            }
            // `it` before the subject is not yet bound to the source.
            conditions.push(recognize_static_condition_for(
                cursor,
                condition_context(input, false),
                condition_subject(input, false),
            )?);
        } else {
            break;
        }
        cursor.enter("static.continuous.leading-condition.separator");
        cursor.consume(",")?;
    }
    let condition = match conditions.len() {
        0 => None,
        1 => conditions.pop(),
        _ => Some(BooleanConditionNode::And(conditions)),
    };
    Ok(LeadingStaticCondition {
        condition,
        zone_of_function,
    })
}

/// The `activeWhile` of a static rule functioning from `zone` (CR 113.6).
pub(in crate::oracle::canonical) fn active_while_static_zone(zone: Option<&str>) -> Value {
    match zone {
        None => active_while_battlefield(),
        Some(zone) => json!({
            "kind": "inZone",
            "object": self_ref(),
            "zone": { "kind": zone },
        }),
    }
}

fn read_static_continuous_effect(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    committed: &mut bool,
    zone_of_function: &mut Option<&'static str>,
) -> MachineResult<(Vec<Value>, Option<StaticConditionNode>)> {
    recognize_optional_ability_word_envelope(cursor)?;
    let leading = read_leading_static_conditions(cursor, input, committed)?;
    *zone_of_function = leading.zone_of_function;
    let condition = leading.condition;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("control"))
    {
        let modifier = read_player_control(cursor, input, committed)?;
        cursor.require(
            condition.is_none(),
            "an unconditional control-changing static ability",
        )?;
        return Ok((vec![modifier], None));
    }
    if word_in(cursor.peek(), &["untap"]) {
        // `Untap OBJECTS during each other player's untap step.` (CR 502.3)
        let modifier = read_other_player_untap_sentence(cursor, input, committed)?;
        return Ok((vec![modifier], condition));
    }
    if block_requirement_sentence_starts(cursor) {
        // `All creatures able to block SUBJECT do so.` (CR 509.1c): the
        // subject is read after the requirement's blockers.
        let modifier = read_block_requirement_sentence(cursor, input, committed)?;
        return Ok((vec![modifier], condition));
    }
    if legend_rule_waiver_starts(cursor) {
        // `The "legend rule" doesn't apply [to OBJECTS].` (CR 704.5j).
        let modifier = read_legend_rule_waiver(cursor, committed)?;
        cursor.enter("static.continuous.legend-rule.terminator");
        cursor.consume(".")?;
        cursor.enter("static.continuous.complete");
        cursor.finish()?;
        return Ok((vec![modifier], condition));
    }
    if combat_participant_limit_starts(cursor) {
        // `No more than N creatures can attack|block each combat.`
        let modifier = read_combat_participant_limit(cursor, committed)?;
        cursor.enter("static.continuous.combat-limit.terminator");
        cursor.consume(".")?;
        cursor.enter("static.continuous.complete");
        cursor.finish()?;
        return Ok((vec![modifier], condition));
    }
    if imperative_player_predicate_starts(cursor) {
        // `Play with the top card of your library revealed.` / `Skip your
        // draw step.`: an imperative rule of the ability's controller, the
        // implied subject `you` (CR 604.1).
        let reading = StaticPlayerSubjectReading {
            subject: StaticPlayerSubjectNode::You,
            number: PlayerSubjectNumber::Base,
            possessive: false,
        };
        return read_player_predicates(cursor, input, reading, condition, committed);
    }
    let subjects = read_static_subject_list(cursor, input, condition.as_ref())?;
    if let [subject] = subjects.as_slice() {
        return read_subject_predicates(cursor, input, subject, condition, committed);
    }
    // Coordinated subjects share one predicate list and form one continuous
    // effect over the union of their populations (CR 604.1, 611.3): an object
    // in two subjects is affected once. Subjects of one selector shape merge
    // into one population whose filter is the disjunction of the branches;
    // other subjects lower separately only when their populations are
    // provably disjoint.
    cursor.enter("static.continuous.subject.union");
    let subjects = merge_coordinated_object_subjects(subjects);
    if let [subject] = subjects.as_slice() {
        return read_subject_predicates(cursor, input, subject, condition, committed);
    }
    cursor.require(
        coordinated_subjects_disjoint(&subjects),
        "coordinated subjects forming one population or provably disjoint populations",
    )?;
    let start = cursor.clone();
    let mut end: Option<OracleTokenCursor<'_>> = None;
    let mut modifiers = Vec::new();
    let mut shared_condition = None;
    for subject in &subjects {
        let mut trial = start.clone();
        let (subject_modifiers, subject_condition) =
            read_subject_predicates(&mut trial, input, subject, condition.clone(), committed)?;
        match end.as_ref().map(OracleTokenCursor::position) {
            None => {
                shared_condition = subject_condition;
                end = Some(trial);
            }
            Some(first_end) => {
                trial.enter("static.continuous.subject.coordination");
                trial.require(
                    first_end == trial.position() && shared_condition == subject_condition,
                    "coordinated subjects sharing one predicate list and one condition",
                )?;
            }
        }
        modifiers.extend(subject_modifiers);
    }
    *cursor = end.expect("a coordinated subject list has at least one subject");
    Ok((modifiers, shared_condition))
}

/// Merges the coordinated `permanents` subjects that differ only by their
/// filter (`Goblins you control and Elementals you control`) into one
/// collection whose filter is the disjunction of the branch filters. Every
/// other relation (controller, ownership, source exclusion) must be equal, so
/// each branch keeps its own local qualifiers inside its filter.
fn merge_coordinated_object_subjects(subjects: Vec<StaticSubjectItem>) -> Vec<StaticSubjectItem> {
    let mergeable = |subject: &StaticObjectSubject| {
        subject.objects["kind"] == "permanents"
            && !subject.objects["where"]["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("chosen"))
    };
    let relations = |subject: &StaticObjectSubject| {
        let mut relations = subject.objects.clone();
        if let Some(map) = relations.as_object_mut() {
            map.remove("where");
        }
        relations
    };
    let mut merged: Vec<StaticSubjectItem> = Vec::new();
    for subject in subjects {
        let StaticSubjectItem::Objects(object) = &subject else {
            merged.push(subject);
            continue;
        };
        let into = merged.iter_mut().find_map(|item| match item {
            StaticSubjectItem::Objects(existing)
                if mergeable(existing)
                    && mergeable(object)
                    && relations(existing) == relations(object) =>
            {
                Some(existing)
            }
            _ => None,
        });
        let Some(existing) = into else {
            merged.push(subject);
            continue;
        };
        let branch = |objects: &Value| objects.get("where").cloned();
        match (branch(&existing.objects), branch(&object.objects)) {
            // A branch without a filter is the whole collection already.
            (None, _) => {}
            (Some(_), None) => {
                if let Some(map) = existing.objects.as_object_mut() {
                    map.remove("where");
                }
            }
            (Some(left), Some(right)) => {
                // `or` is associative: an earlier union extends in place.
                let mut operands = if left["kind"] == "or" {
                    left["operands"].as_array().cloned().unwrap_or_default()
                } else {
                    vec![left]
                };
                if !operands.contains(&right) {
                    operands.push(right);
                }
                existing.objects["where"] = if operands.len() == 1 {
                    operands.remove(0)
                } else {
                    json!({ "kind": "or", "operands": operands })
                };
            }
        }
        if existing.predicate != object.predicate {
            // The union guarantees no single branch characteristic.
            existing.predicate = None;
        }
    }
    merged
}

/// Coordinated subjects that lower separately must not share a member, or a
/// shared member would receive the same continuous effect twice. Players
/// never overlap objects; a source subject is disjoint from a collection that
/// excludes the source; a collection you control is disjoint from one your
/// opponents control (CR 108.4, each permanent has one controller).
fn coordinated_subjects_disjoint(subjects: &[StaticSubjectItem]) -> bool {
    let players = subjects
        .iter()
        .filter(|subject| matches!(subject, StaticSubjectItem::Player(_)))
        .count();
    if players > 1 {
        return false;
    }
    let objects = subjects
        .iter()
        .filter_map(|subject| match subject {
            StaticSubjectItem::Objects(object) => Some(&object.objects),
            StaticSubjectItem::Player(_) => None,
        })
        .collect::<Vec<_>>();
    let you = controller();
    let opponents = json!({ "kind": "opponentsOf", "player": controller() });
    let disjoint = |left: &Value, right: &Value| {
        let source_and_others = |source: &Value, others: &Value| {
            *source == self_ref()
                && others["kind"] == "permanents"
                && others["excludeSource"].as_bool() == Some(true)
        };
        let split_control = |a: &Value, b: &Value| {
            a["kind"] == "permanents"
                && b["kind"] == "permanents"
                && a["controller"] == you
                && b["controller"] == opponents
        };
        source_and_others(left, right)
            || source_and_others(right, left)
            || split_control(left, right)
            || split_control(right, left)
    };
    objects.iter().enumerate().all(|(index, left)| {
        objects[index + 1..]
            .iter()
            .all(|right| disjoint(left, right))
    })
}

/// One coordinated subject of a static ability (CR 604.1): an object
/// subject of the shared object grammar or a player population.
#[derive(Clone, Debug)]
enum StaticSubjectItem {
    Objects(StaticObjectSubject),
    Player(StaticPlayerSubjectReading),
}

#[derive(Clone, Debug)]
struct StaticObjectSubject {
    objects: Value,
    predicate: Option<ObjectPredicate>,
    number: TypeChangeNumber,
    /// `enchanted creature's`: a possessed noun of the object starts the
    /// predicate (`activated abilities can't be activated`).
    possessive: bool,
    /// `it's`: the subject pronoun carried the copula of the first predicate.
    contracted_copula: bool,
}

fn read_subject_predicates(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    subject: &StaticSubjectItem,
    condition: Option<StaticConditionNode>,
    committed: &mut bool,
) -> MachineResult<(Vec<Value>, Option<StaticConditionNode>)> {
    match subject {
        StaticSubjectItem::Objects(subject) => {
            read_object_predicates(cursor, input, subject, condition, committed)
        }
        StaticSubjectItem::Player(reading) => {
            read_player_predicates(cursor, input, *reading, condition, committed)
        }
    }
}

/// `SUBJECT ("," | "and" | ", and") SUBJECT …` before the shared predicate
/// list. A coordinator followed by a predicate head belongs to the predicate
/// list, never to the subject.
fn read_static_subject_list(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    condition: Option<&StaticConditionNode>,
) -> MachineResult<Vec<StaticSubjectItem>> {
    // `As long as this creature is equipped, it ...`: the condition's object.
    // `it's` contracts that pronoun with the copula of the first predicate
    // (`As long as NAME has a slumber counter on it, it's a land`).
    let contracted_copula = condition.is_some() && word_in(cursor.peek(), &["it's"]);
    let reference = if contracted_copula {
        cursor.enter("static.condition-antecedent.contracted-pronoun");
        let reference = condition
            .and_then(condition_object_antecedent)
            .filter(|reference| {
                matches!(
                    reference,
                    PermanentEffectReference::Source(_) | PermanentEffectReference::Attached(_)
                )
            });
        let reference = cursor.require_value(
            reference,
            "a leading condition about the source or its attached permanent for `it's`",
        )?;
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
        Some(reference)
    } else {
        recognize_condition_pronoun_subject(cursor, condition)?
    };
    if let Some(reference) = reference {
        let objects = cursor.require_value(
            bound_permanent_reference_value(reference, &[]),
            "a static subject the condition's object lowers to",
        )?;
        return Ok(vec![StaticSubjectItem::Objects(StaticObjectSubject {
            objects,
            predicate: None,
            number: TypeChangeNumber::Singular,
            possessive: false,
            contracted_copula,
        })]);
    }
    let mut items = Vec::new();
    loop {
        match read_optional_player_subject(cursor)? {
            Some(reading) => items.push(StaticSubjectItem::Player(reading)),
            None => items.push(StaticSubjectItem::Objects(read_static_object_subject(
                cursor, input,
            )?)),
        }
        let width = match cursor.peek() {
            Some(",") if word_in(cursor.peek_ahead(1), &["and"]) => 2,
            Some(",") => 1,
            Some(raw) if raw.eq_ignore_ascii_case("and") => 1,
            _ => break,
        };
        if !subject_starts_at(cursor, width) {
            break;
        }
        cursor.enter("static.continuous.subject.connector");
        for _ in 0..width {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
        }
    }
    if items.len() > 1 {
        for item in &mut items {
            match item {
                StaticSubjectItem::Objects(subject) => {
                    cursor.enter("static.continuous.subject.coordination");
                    cursor.require(
                        !subject.possessive,
                        "a coordinated subject without a possessive",
                    )?;
                    // A coordinated subject is plural (`you and this creature have`).
                    subject.number = TypeChangeNumber::Plural;
                }
                StaticSubjectItem::Player(reading) => {
                    cursor.require(
                        !reading.possessive,
                        "a coordinated subject without a possessive",
                    )?;
                    reading.number = PlayerSubjectNumber::Base;
                }
            }
        }
    }
    Ok(items)
}

/// A subject (not a predicate) at `offset`: a later coordinated subject.
fn subject_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let mut ahead = cursor.clone();
    ahead.advance_to(ahead.position() + offset).is_ok()
        && ahead.peek().is_some()
        && static_predicate_head(&ahead).is_none()
        && !static_player_predicate_starts(&ahead)
        && !trailing_condition_starts(&ahead)
        && !word_in(ahead.peek(), &["its", "their", "also"])
}

/// A player subject followed by a player predicate (or by a coordinated
/// subject). `you control`, `you may` and other player verbs stay with
/// their own readers: the frame does not claim them.
fn read_optional_player_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<StaticPlayerSubjectReading>> {
    let claimed = if let Some((_, width)) = static_player_possessive_width(cursor) {
        let mut ahead = cursor.clone();
        ahead.advance_to(ahead.position() + width).is_ok()
    } else if let Some((_, width)) = static_player_subject_width(cursor) {
        let mut ahead = cursor.clone();
        ahead.advance_to(ahead.position() + width).is_ok()
            && (static_player_predicate_starts(&ahead)
                || ahead.peek() == Some(",")
                || word_in(ahead.peek(), &["and"]))
    } else {
        false
    };
    if !claimed {
        return Ok(None);
    }
    recognize_static_player_subject(cursor).map(Some)
}

/// The predicate list of a player subject (CR 604.1): each predicate is the
/// shared player-predicate child, lowered on the same population. `and`
/// followed by another player subject starts an independent clause with its
/// own population (`you can't lose the game and your opponents can't …`).
fn read_player_predicates(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    reading: StaticPlayerSubjectReading,
    mut condition: Option<StaticConditionNode>,
    committed: &mut bool,
) -> MachineResult<(Vec<Value>, Option<StaticConditionNode>)> {
    let mut reading = reading;
    let mut players = lower_static_player_subject(cursor, reading.subject, input.face_type_line)?;
    let mut modifiers = Vec::new();
    let mut keywords = false;
    loop {
        cursor.enter("static.continuous.player-predicate");
        if !reading.possessive {
            cursor.require(
                static_player_predicate_starts(cursor),
                "a player predicate supported by the static frame",
            )?;
        }
        *committed = true;
        let predicate = recognize_static_player_predicate(cursor, reading)?;
        keywords |= matches!(predicate, StaticPlayerPredicateNode::HasKeywords(_));
        modifiers.extend(lower_static_player_predicate(cursor, &players, &predicate)?);
        cursor.enter("static.continuous.player-connector");
        if cursor.peek() == Some(",") && {
            let mut ahead = cursor.clone();
            ahead.advance_to(ahead.position() + 1).is_ok()
                && (static_player_predicate_starts(&ahead)
                    || (word_in(ahead.peek(), &["and"]) && {
                        let mut after = ahead.clone();
                        after.advance_to(after.position() + 1).is_ok()
                            && static_player_predicate_starts(&after)
                    }))
        } {
            cursor.consume(",")?;
            if word_in(cursor.peek(), &["and"]) {
                cursor.consume("and")?;
            }
            continue;
        }
        if word_in(cursor.peek(), &["and"]) {
            cursor.consume("and")?;
            if !static_player_predicate_starts(cursor) {
                // An independent clause with its own player subject.
                cursor.enter("static.continuous.player-clause");
                let subject = read_optional_player_subject(cursor)?;
                let next = cursor
                    .require_value(subject, "a player predicate or a player clause after `and`")?;
                reading = next;
                players =
                    lower_static_player_subject(cursor, reading.subject, input.face_type_line)?;
            }
            continue;
        }
        if trailing_condition_starts(cursor) {
            cursor.require(condition.is_none(), "one condition for the static ability")?;
            cursor.enter("static.continuous.trailing-condition");
            condition = Some(read_trailing_condition(cursor, input, false, false)?);
        }
        cursor.enter("static.continuous.player-terminator");
        cursor.consume(".")?;
        break;
    }
    if (keywords || condition.is_some()) && cursor.peek() == Some("(") {
        // CR 207.2: the reminder of a granted player keyword or of the
        // condition's term.
        read_closing_reminder(cursor)?;
    }
    cursor.enter("static.continuous.complete");
    cursor.finish()?;
    Ok((modifiers, condition))
}

/// One trailing, balanced parenthetical that closes the ability (CR 207.2).
/// A parenthetical followed by more text is not a reminder.
fn read_closing_reminder(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("static.continuous.keyword-reminder");
    cursor.consume("(")?;
    while cursor.peek().is_some_and(|raw| raw != ")" && raw != "(") {
        cursor.advance_to(cursor.position() + 1)?;
    }
    cursor.consume(")")?;
    cursor.enter("static.continuous.keyword-reminder.end");
    cursor.finish()
}

/// How a sentence after the first relates to the earlier sentences of the
/// same subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ContinuationKind {
    /// The first sentence, or `It has flying.`: independent predicates.
    Plain,
    /// `It gets +3/+1 instead as long as C`: replaces the earlier predicate
    /// of the same kind while C holds.
    Instead,
    /// `Otherwise, PREDICATES`: applies while the previous sentence's
    /// condition does not hold.
    Otherwise,
}

/// One sentence of a static ability's subject and the modifiers it read.
struct StaticSentence {
    first_modifier: usize,
    condition: Option<StaticConditionNode>,
    kind: ContinuationKind,
}

/// The predicate list of an object subject, its continuation sentences and
/// its closing reminder.
fn read_object_predicates(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    subject: &StaticObjectSubject,
    condition: Option<StaticConditionNode>,
    committed: &mut bool,
) -> MachineResult<(Vec<Value>, Option<StaticConditionNode>)> {
    let objects = &subject.objects;
    let subject_predicate = &subject.predicate;
    let subject_number = subject.number;
    let source_subject = *objects == self_ref();
    let attached_subject = objects["kind"] == "attachedPermanent";
    let mut modifiers = Vec::new();
    let mut granted = Vec::new();
    let mut changes_types = false;
    let mut goaded = false;
    // `This effect doesn't remove this Aura|Auras.` after a protection grant.
    let mut spared_auras: Option<&'static str> = None;
    let mut sentences = vec![StaticSentence {
        first_modifier: 0,
        condition,
        kind: ContinuationKind::Plain,
    }];
    // `Enchanted creature's activated abilities …`: the possessed noun is
    // read without a pronoun; `It's a Dragon …`: the copula is contracted.
    let mut possessed_start = subject.possessive;
    let mut contracted_copula = subject.contracted_copula;
    loop {
        cursor.enter("static.continuous.predicate");
        let verb = cursor
            .peek()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| cursor.fail("a continuous predicate"))?;
        let head = if possessed_start {
            possessed_start = false;
            Some(StaticPredicateHead::PossessedAbilities)
        } else if contracted_copula {
            contracted_copula = false;
            Some(StaticPredicateHead::ContractedCopula(subject_number))
        } else {
            static_predicate_head(cursor)
        };
        match head {
            Some(StaticPredicateHead::Combat) => {
                // The shared combat clause (CR 508.1c-d, 509.1b-c) lowers on
                // the same objects as every other predicate of this subject.
                let mut clauses = Vec::new();
                recognize_combat_clause(
                    cursor,
                    CombatClauseContext::Static,
                    committed,
                    &mut clauses,
                    &mut None,
                )?;
                cursor.enter("static.continuous.combat-clause.lowering");
                for clause in &clauses {
                    modifiers.push(lower_static_combat_clause(cursor, objects, clause)?);
                }
            }
            Some(StaticPredicateHead::ModifyStats) => {
                cursor.consume(&verb)?;
                *committed = true;
                if !cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "counterValue")
                {
                    let (power, toughness) =
                        read_where_x_power_toughness_delta(cursor, source_subject)?;
                    modifiers.push(json!({
                        "kind": "modifyPowerToughness",
                        "objects": objects.clone(),
                        "power": power,
                        "toughness": toughness,
                    }));
                } else {
                    let delta = recognize_signed_power_toughness_delta(cursor)?;
                    // The count may name the source (`for each counter on NAME`).
                    let names = cursor.replace_source_names(Some(input.name_links.into()));
                    let scale = read_optional_for_each_count(cursor, source_subject);
                    cursor.replace_source_names(names);
                    let scale = scale?;
                    let scaled = |amount: i64| match &scale {
                        None => integer(amount),
                        Some(_) if amount == 0 => integer(0),
                        Some(count) if amount == 1 => count.clone(),
                        Some(count) => json!({
                            "kind": "multiply", "left": integer(amount), "right": count.clone(),
                        }),
                    };
                    modifiers.push(json!({
                        "kind": "modifyPowerToughness",
                        "objects": objects.clone(),
                        "power": scaled(delta.power),
                        "toughness": scaled(delta.toughness),
                    }));
                }
                if word_in(cursor.peek(), &["instead"]) {
                    // `It gets +3/+1 instead …` replaces the earlier delta.
                    cursor.enter("static.continuous.instead");
                    cursor.require(
                        sentences.len() > 1,
                        "`instead` in a sentence after the predicate it replaces",
                    )?;
                    cursor.consume("instead")?;
                    if let Some(sentence) = sentences.last_mut() {
                        sentence.kind = ContinuationKind::Instead;
                    }
                }
            }
            Some(StaticPredicateHead::Grant) if word_in(cursor.peek_ahead(1), &["base"]) => {
                // `have base power and toughness P/T` (CR 613.4b, layer 7b).
                cursor.consume(&verb)?;
                *committed = true;
                cursor.enter("static.continuous.base-power-toughness");
                cursor.consume_phrase(&["base", "power", "and", "toughness"])?;
                let statistics = recognize_numeric_power_toughness(cursor)?.literal();
                let statistics =
                    cursor.require_value(statistics, "a literal base power and toughness")?;
                modifiers.push(json!({
                    "kind": "setBasePowerToughness",
                    "objects": objects.clone(),
                    "power": integer(statistics.power),
                    "toughness": integer(statistics.toughness),
                }));
            }
            Some(StaticPredicateHead::Grant) => {
                cursor.consume(&verb)?;
                *committed = true;
                cursor.enter("static.continuous.keywords");
                for keyword in
                    recognize_granted_list_before_connector(cursor, GRANTED_OBJECT_TYPE_LINE)?
                        .keywords
                {
                    granted.push(keyword.clone());
                    modifiers.push(match keyword {
                        KeywordAtomNode::Quoted(ability) => {
                            lower_static_quoted_ability(cursor, objects, ability)?
                        }
                        KeywordAtomNode::Parameterless(keyword) => json!({
                            "kind": "grantKeyword",
                            "objects": objects.clone(),
                            "keyword": keyword.canonical_name(),
                        }),
                        KeywordAtomNode::Ward { cost } => json!({
                            "kind": "grantWard",
                            "objects": objects.clone(),
                            "cost": lower_resource_payment(
                                &cost,
                                ResourcePaymentPayer::WardTargetingPlayer,
                            ),
                        }),
                        KeywordAtomNode::Protection { qualities } => json!({
                            "kind": "grantProtection",
                            "objects": objects.clone(),
                            "from": lower_protection_qualities(&qualities),
                        }),
                        KeywordAtomNode::ManaCost(_) => {
                            return Err(cursor.fail(
                                "a granted keyword the engine reads from a static grant (a mana-cost keyword is read from the card itself)",
                            ));
                        }
                    });
                }
            }
            Some(
                head @ (StaticPredicateHead::Copula(_) | StaticPredicateHead::ContractedCopula(_)),
            ) => {
                let number = match head {
                    StaticPredicateHead::Copula(number)
                    | StaticPredicateHead::ContractedCopula(number) => number,
                    _ => unreachable!("matched copula heads"),
                };
                cursor.require(
                    number == subject_number,
                    "a copula agreeing in number with the static subject",
                )?;
                if matches!(head, StaticPredicateHead::Copula(_)) {
                    cursor.consume(&verb)?;
                }
                if word_in(cursor.peek(), &["goaded"]) {
                    // CR 701.15: the attached permanent is goaded by the
                    // source's controller for as long as it is attached.
                    cursor.enter("static.continuous.goaded");
                    *committed = true;
                    cursor.require(
                        *objects
                            == json!({ "kind": "attachedPermanent", "attachment": self_ref() }),
                        "a goaded permanent the source is attached to",
                    )?;
                    cursor.consume("goaded")?;
                    modifiers
                        .push(json!({ "kind": "goadAttachedPermanent", "attachment": self_ref() }));
                    goaded = true;
                } else if all_creature_types_follow(cursor) {
                    // CR 205.3m: every creature type, in the creature
                    // subtype class (layer 4); the objects keep their card
                    // types and other subtypes.
                    cursor.enter("static.continuous.all-creature-types");
                    *committed = true;
                    let quantifier = cursor.peek().unwrap_or_default().to_string();
                    cursor.consume(&quantifier)?;
                    cursor.consume("creature")?;
                    let noun = cursor.peek().unwrap_or_default().to_string();
                    cursor.consume(&noun)?;
                    modifiers.push(json!({
                        "kind": "modifySubtypeSet",
                        "objects": objects.clone(),
                        "subtypeClass": "creature",
                        "mode": "all",
                    }));
                    changes_types = true;
                } else if word_in(cursor.peek(), &["the"])
                    && word_in(cursor.peek_ahead(1), &["chosen"])
                {
                    modifiers.push(read_chosen_type_addition(
                        cursor, objects, number, committed,
                    )?);
                    changes_types = true;
                } else if let Some(modifier) =
                    read_optional_color_predicate(cursor, objects, committed)?
                {
                    modifiers.push(modifier);
                } else if let Some(complement) = read_characteristic_copula(cursor, number) {
                    // A complement stating more than types (power and
                    // toughness, colors, granted abilities, a retained type)
                    // is the shared characteristic-change complement.
                    *committed = true;
                    let lands = subject_predicate.as_ref().is_some_and(|predicate| {
                        predicate.guarantees_card_type(ObjectCardType::Land)
                    });
                    cursor.enter("static.continuous.characteristic-copula.lowering");
                    modifiers.extend(
                        lower_continuous_characteristic_complement(objects, lands, &complement)
                            .map_err(|expected| cursor.fail(expected))?,
                    );
                    for ability in complement.characteristics.abilities {
                        modifiers.push(lower_static_quoted_ability(cursor, objects, ability)?);
                    }
                    changes_types = true;
                } else {
                    cursor.enter("static.continuous.type-change");
                    cursor.require(
                        type_change_characteristics_start(cursor, number),
                        "a type-change characteristic or a color after the copula",
                    )?;
                    *committed = true;
                    let change = recognize_type_change_characteristics(cursor, number)?;
                    // A replacing land-subtype change needs a land-only subject.
                    let lands = subject_predicate.as_ref().is_some_and(|predicate| {
                        predicate.guarantees_card_type(ObjectCardType::Land)
                    });
                    cursor.enter("static.continuous.type-change.lowering");
                    modifiers.extend(
                        lower_continuous_type_change(objects, lands, &change)
                            .map_err(|expected| cursor.fail(expected))?,
                    );
                    changes_types = true;
                }
            }
            Some(StaticPredicateHead::NegatedCopula(number)) => {
                cursor.require(
                    number == subject_number,
                    "a negated copula agreeing in number with the static subject",
                )?;
                cursor.consume(&verb)?;
                *committed = true;
                modifiers.push(read_card_type_removal(cursor, objects)?);
            }
            Some(StaticPredicateHead::NegatedDo) => {
                cursor.consume(&verb)?;
                *committed = true;
                read_untap_step_exclusion(cursor)?;
                modifiers.push(json!({ "kind": "doesNotUntap", "objects": objects.clone() }));
            }
            Some(StaticPredicateHead::Entry) => {
                cursor.consume(&verb)?;
                *committed = true;
                modifiers.push(read_collection_entry_modification(cursor, objects)?);
            }
            Some(StaticPredicateHead::AbilityLoss) => {
                cursor.consume(&verb)?;
                *committed = true;
                cursor.enter("static.continuous.ability-loss");
                if word_in(cursor.peek(), &["all"]) {
                    cursor.consume_phrase(&["all", "abilities"])?;
                    modifiers
                        .push(json!({ "kind": "loseAllAbilities", "objects": objects.clone() }));
                } else {
                    // `lose(s) KEYWORDS` (CR 613.1f).
                    modifiers.push(read_keyword_loss(cursor, objects)?);
                }
            }
            Some(StaticPredicateHead::KeywordProhibition) => {
                // `can't have or gain KEYWORDS`: the objects don't have them
                // whatever grants them (CR 613.1f).
                *committed = true;
                cursor.enter("static.continuous.keyword-prohibition");
                read_prohibition_word(cursor)?;
                cursor.consume_phrase(&["have", "or", "gain"])?;
                modifiers.push(read_keyword_loss(cursor, objects)?);
            }
            Some(StaticPredicateHead::Ability)
                if word_in(cursor.peek_ahead(1), &["be"])
                    && word_in(cursor.peek_ahead(2), &["blocked"])
                    && word_in(cursor.peek_ahead(3), &["as"]) =>
            {
                // `can be blocked as though they didn't have swampwalk`
                // (CR 702.14): the objects keep the keyword; only its
                // blocking restriction is ignored.
                cursor.consume("can")?;
                *committed = true;
                cursor.enter("static.continuous.evasion-exception");
                cursor.consume_phrase(&["be", "blocked", "as", "though"])?;
                read_subject_pronoun(cursor, subject_number)?;
                read_negated_have(cursor)?;
                cursor.enter("static.continuous.evasion-exception.keyword");
                let landwalk = cursor.require_value(
                    cursor.peek().and_then(lookup_landwalk_word),
                    "a landwalk keyword whose evasion is ignored (CR 702.14)",
                )?;
                let raw = cursor.peek().unwrap_or_default().to_string();
                cursor.consume(&raw)?;
                modifiers.push(json!({
                    "kind": "ignoreBlockEvasion",
                    "objects": objects.clone(),
                    "keyword": landwalk.keyword,
                }));
            }
            Some(StaticPredicateHead::Ability) => {
                cursor.consume("can")?;
                *committed = true;
                cursor.enter("static.continuous.defender-exception");
                cursor.consume_phrase(&["attack", "as", "though"])?;
                read_subject_pronoun(cursor, subject_number)?;
                read_negated_have(cursor)?;
                cursor.consume("defender")?;
                modifiers.push(json!({
                    "kind": "allowAttackWithDefender", "objects": objects.clone(),
                }));
            }
            Some(StaticPredicateHead::DamageAssignment) => {
                cursor.consume(&verb)?;
                *committed = true;
                cursor.enter("static.continuous.combat-damage-assignment");
                cursor.consume_phrase(&["combat", "damage", "equal", "to"])?;
                read_possessive(cursor, subject_number)?;
                cursor.consume_phrase(&["toughness", "rather", "than"])?;
                read_possessive(cursor, subject_number)?;
                cursor.consume("power")?;
                modifiers.push(json!({
                    "kind": "assignCombatDamageUsingToughness",
                    "objects": objects.clone(),
                    "onlyIfToughnessGreater": false,
                }));
            }
            Some(StaticPredicateHead::CounterProhibition) => {
                // CR 122.1: `can't have [KIND] counters put on it|them`.
                *committed = true;
                cursor.enter("static.continuous.counter-prohibition");
                read_prohibition_word(cursor)?;
                cursor.consume("have")?;
                let counter = if word_in(cursor.peek(), &["counters"]) {
                    None
                } else {
                    cursor.enter("static.continuous.counter-prohibition.kind");
                    Some(recognize_counter_name(cursor)?)
                };
                cursor.consume_phrase(&["counters", "put", "on"])?;
                read_object_pronoun(cursor, subject_number)?;
                let mut modifier = json!({ "kind": "preventCounters", "objects": objects.clone() });
                if let Some(counter) = counter {
                    modifier["counter"] = json!(counter.as_str());
                }
                modifiers.push(modifier);
            }
            Some(StaticPredicateHead::PassiveProhibition) => {
                *committed = true;
                modifiers.push(read_passive_prohibition(cursor, objects)?);
            }
            Some(StaticPredicateHead::PossessiveAbilities) => {
                cursor.enter("static.continuous.activation-prohibition");
                read_possessive(cursor, subject_number)?;
                *committed = true;
                modifiers.push(read_activation_prohibition(cursor, objects)?);
            }
            Some(StaticPredicateHead::PossessedAbilities) => {
                cursor.enter("static.continuous.activation-prohibition");
                *committed = true;
                modifiers.push(read_activation_prohibition(cursor, objects)?);
            }
            Some(StaticPredicateHead::BlockRequirement) => {
                *committed = true;
                cursor.enter("static.continuous.block-requirement");
                read_all_able_blockers(cursor)?;
                read_object_pronoun(cursor, subject_number)?;
                cursor.consume_phrase(&["do", "so"])?;
                modifiers.push(block_requirement_modifier(objects));
            }
            None => {
                return Err(cursor.fail(
                    "a continuous predicate: `get(s)`, `have`/`has`, `is`/`are`, `isn't`, a combat clause, `doesn't untap`, `enter(s)`, `loses all abilities`, `can attack as though`, `assigns combat damage`, `can't have counters`, an activation prohibition or a block requirement",
                ));
            }
        }
        cursor.enter("static.continuous.connector");
        // A quoted ability that ends with its own period closes the sentence;
        // its reminder or a continuation sentence may follow.
        if quoted_sentence_just_closed(cursor)
            && (cursor.peek().is_none() || cursor.peek() == Some("("))
        {
            break;
        }
        if quoted_sentence_just_closed(cursor) {
            match read_continuation_start(cursor, &mut sentences, &modifiers, subject_number)? {
                Some(contracted) => {
                    contracted_copula = contracted;
                    continue;
                }
                None => break,
            }
        }
        if cursor.peek() == Some(",") && serial_predicate_follows(cursor) {
            // A serial comma before the next predicate verb (`is legendary,
            // gets +1/+1, and has flying`); `and` may follow the comma.
            cursor.consume(",")?;
            if cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            {
                cursor.consume("and")?;
            }
            continue;
        }
        match cursor.peek() {
            Some(raw) if raw.eq_ignore_ascii_case("and") => cursor.consume("and")?,
            Some(_) if trailing_condition_starts(cursor) => {
                // One trailing gate of the sentence (CR 611.3a): `as long as
                // C`, `if C`, `unless C` (not-C) or a turn condition (`during
                // your turn`), read by the shared condition readers.
                let sentence = sentences.last_mut().expect("one sentence at least");
                cursor.require(
                    sentence.condition.is_none(),
                    "one condition for the static ability",
                )?;
                cursor.enter("static.continuous.trailing-condition");
                sentence.condition = Some(read_trailing_condition(
                    cursor,
                    input,
                    source_subject,
                    attached_subject,
                )?);
                cursor.enter("static.continuous.trailing-condition.terminator");
                cursor.consume(".")?;
                match read_continuation_start(cursor, &mut sentences, &modifiers, subject_number)? {
                    Some(contracted) => contracted_copula = contracted,
                    None => break,
                }
            }
            Some(".") => {
                cursor.consume(".")?;
                if protection_aura_exception_starts(cursor) {
                    cursor.require(spared_auras.is_none(), "one Aura exception")?;
                    spared_auras = Some(read_protection_aura_exception(cursor)?);
                }
                match read_continuation_start(cursor, &mut sentences, &modifiers, subject_number)? {
                    Some(contracted) => contracted_copula = contracted,
                    None => break,
                }
            }
            _ => return Err(cursor.fail("`and` or the end of the static ability")),
        }
    }
    if let Some(spared) = spared_auras {
        // CR 702.16c/d, 303.4d: the protection still applies to every
        // other quality and event; only the state-based removal of the
        // spared Auras is waived.
        cursor.enter("static.continuous.aura-exception.protection");
        let mut protections = 0;
        for modifier in &mut modifiers {
            if modifier["kind"] == "grantProtection" {
                modifier["sparesAuras"] = json!(spared);
                protections += 1;
            }
        }
        cursor.require(
            protections > 0 && attached_subject,
            "a protection granted to the enchanted permanent before the Aura exception",
        )?;
    }
    // A gated ability may close with the reminder of its condition's term
    // (`(Each {U} in the mana costs of permanents you control counts toward
    // your devotion to blue.)`, CR 700.5).
    let conditioned = sentences
        .iter()
        .any(|sentence| sentence.condition.is_some());
    if (!granted.is_empty() || changes_types || goaded || conditioned) && cursor.peek() == Some("(")
    {
        // CR 207.2: reminder text of a granted keyword or of a type change
        // (`They're still affected by summoning sickness.`) has no rules meaning.
        // Only one trailing, balanced parenthetical that closes the ability is
        // admitted. Schema validation is deferred: the shared reminder
        // schemas do not yet cover the official collection/attachment
        // wordings (136 abilities).
        read_closing_reminder(cursor)?;
    }
    cursor.enter("static.continuous.complete");
    cursor.finish()?;
    resolve_sentence_conditions(cursor, modifiers, sentences)
}

/// After a sentence's period: `Otherwise, PREDICATES`, or a pronoun of the
/// same subject followed by a predicate (`It has flying.`, `Its activated
/// abilities …`, `It's a Dragon …`). Returns whether the copula was
/// contracted into the pronoun, or `None` when no continuation follows.
fn read_continuation_start(
    cursor: &mut OracleTokenCursor<'_>,
    sentences: &mut Vec<StaticSentence>,
    modifiers: &[Value],
    number: TypeChangeNumber,
) -> MachineResult<Option<bool>> {
    let first_modifier = modifiers.len();
    if word_in(cursor.peek(), &["otherwise"]) && cursor.peek_ahead(1) == Some(",") {
        cursor.enter("static.continuous.otherwise");
        cursor.consume("otherwise")?;
        cursor.consume(",")?;
        sentences.push(StaticSentence {
            first_modifier,
            condition: None,
            kind: ContinuationKind::Otherwise,
        });
        let contracted = read_continuation_pronoun(cursor, number)?;
        return Ok(Some(contracted));
    }
    let (pronoun, possessive, contraction) = match number {
        TypeChangeNumber::Singular => ("it", "its", "it's"),
        TypeChangeNumber::Plural => ("they", "their", "they're"),
    };
    let mut ahead = cursor.clone();
    let continues = if word_in(ahead.peek(), &[pronoun]) {
        ahead.advance_to(ahead.position() + 1).is_ok() && static_predicate_head(&ahead).is_some()
    } else if word_in(ahead.peek(), &[contraction]) {
        true
    } else {
        word_in(ahead.peek(), &[possessive])
            && static_predicate_head(&ahead) == Some(StaticPredicateHead::PossessiveAbilities)
    };
    if !continues {
        return Ok(None);
    }
    cursor.enter("static.continuous.continuation");
    sentences.push(StaticSentence {
        first_modifier,
        condition: None,
        kind: ContinuationKind::Plain,
    });
    read_continuation_pronoun(cursor, number).map(Some)
}

/// The subject pronoun of a continuation (`it`, `they`, `it's`); a
/// possessive pronoun stays for the predicate it introduces.
fn read_continuation_pronoun(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> MachineResult<bool> {
    let (pronoun, possessive, contraction) = match number {
        TypeChangeNumber::Singular => ("it", "its", "it's"),
        TypeChangeNumber::Plural => ("they", "their", "they're"),
    };
    if word_in(cursor.peek(), &[possessive]) {
        return Ok(false);
    }
    if word_in(cursor.peek(), &[contraction]) {
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
        return Ok(true);
    }
    cursor.require(
        word_in(cursor.peek(), &[pronoun]),
        "the subject pronoun of a continuation sentence",
    )?;
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)?;
    Ok(false)
}

/// Modifier kinds whose own `condition` the engine evaluates.
fn modifier_takes_own_condition(modifier: &Value) -> bool {
    matches!(
        modifier["kind"].as_str(),
        Some(
            "modifyPowerToughness"
                | "grantKeyword"
                | "setBasePowerToughness"
                | "setPower"
                | "doesNotUntap"
                | "loseAllAbilities"
                | "loseKeywords"
                | "preventCounters"
                | "cantBeTargeted"
                | "cantBeSacrificed"
                | "cantBecomeUntapped"
                | "allowAttackWithDefender"
                | "removeCardTypeWhile"
                | "setCardTypes"
                | "modifySubtypeSet"
        )
    )
}

/// The gate of each sentence (CR 611.3a). One sentence keeps the historical
/// rule-level condition. Several sentences gate their own modifiers: an
/// `Otherwise` sentence applies while the previous condition is false, and
/// an `instead` sentence suspends the earlier modifiers it replaces while its
/// condition holds. A card-type removal always carries its own condition.
fn resolve_sentence_conditions(
    cursor: &OracleTokenCursor<'_>,
    mut modifiers: Vec<Value>,
    sentences: Vec<StaticSentence>,
) -> MachineResult<(Vec<Value>, Option<StaticConditionNode>)> {
    if sentences.len() == 1 {
        let sentence = sentences.into_iter().next().expect("one sentence");
        // The engine reads a card-type removal's own condition (CR 205.1a);
        // it is the same continuously evaluated gate as the rule's.
        for modifier in &mut modifiers {
            if modifier["kind"] == "removeCardTypeWhile" {
                let condition = cursor.require_value(
                    sentence.condition.as_ref(),
                    "a condition while the card type is removed (`as long as …, it isn't a TYPE`)",
                )?;
                modifier["condition"] = lower_condition(condition);
            }
        }
        return Ok((modifiers, sentence.condition));
    }
    let mut gates: Vec<Option<StaticConditionNode>> = vec![None; modifiers.len()];
    let and = |left: Option<StaticConditionNode>, right: StaticConditionNode| match left {
        None => right,
        Some(left) => BooleanConditionNode::And(vec![left, right]),
    };
    for index in 0..sentences.len() {
        let sentence = &sentences[index];
        let end = sentences
            .get(index + 1)
            .map_or(modifiers.len(), |next| next.first_modifier);
        let gate = match sentence.kind {
            ContinuationKind::Plain => sentence.condition.clone(),
            ContinuationKind::Instead => {
                let condition = cursor.require_value(
                    sentence.condition.clone(),
                    "a condition for the `instead` predicate",
                )?;
                // Suspend each earlier modifier of the same kind on the same
                // objects while the replacing predicate applies.
                for replacing in sentence.first_modifier..end {
                    for earlier in 0..sentence.first_modifier {
                        if modifiers[earlier]["kind"] == modifiers[replacing]["kind"]
                            && modifiers[earlier]["objects"] == modifiers[replacing]["objects"]
                        {
                            gates[earlier] = Some(and(
                                gates[earlier].take(),
                                BooleanConditionNode::Not(Box::new(condition.clone())),
                            ));
                        }
                    }
                }
                Some(condition)
            }
            ContinuationKind::Otherwise => {
                cursor.require(
                    sentence.condition.is_none(),
                    "an `Otherwise` sentence without its own condition",
                )?;
                let previous = cursor.require_value(
                    index
                        .checked_sub(1)
                        .and_then(|previous| sentences[previous].condition.clone()),
                    "a conditional sentence before `Otherwise`",
                )?;
                Some(BooleanConditionNode::Not(Box::new(previous)))
            }
        };
        if let Some(gate) = gate {
            for slot in &mut gates[sentence.first_modifier..end] {
                *slot = Some(and(slot.take(), gate.clone()));
            }
        }
    }
    cursor.require(
        modifiers
            .iter()
            .zip(&gates)
            .all(|(modifier, gate)| gate.is_none() || modifier_takes_own_condition(modifier)),
        "conditional sentences whose modifiers the engine gates individually",
    )?;
    for (modifier, gate) in modifiers.iter_mut().zip(gates) {
        if let Some(gate) = gate {
            modifier["condition"] = lower_condition(&gate);
        }
    }
    cursor.require(
        modifiers.iter().all(|modifier| {
            modifier["kind"] != "removeCardTypeWhile" || modifier.get("condition").is_some()
        }),
        "a condition while the card type is removed (`as long as …, it isn't a TYPE`)",
    )?;
    Ok((modifiers, None))
}

/// `is the chosen type in addition to its other types` (CR 205.1b, 205.3m):
/// the source adds the creature type stored as it entered (the entry
/// choice's `chosenCreatureType` binding, validated with the card). Only the
/// source itself has an engine primitive for the stored type.
fn read_chosen_type_addition(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
    number: TypeChangeNumber,
    committed: &mut bool,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.chosen-type");
    cursor.consume_phrase(&["the", "chosen"])?;
    *committed = true;
    if word_in(cursor.peek(), &["creature"]) {
        cursor.consume("creature")?;
    }
    cursor.consume("type")?;
    cursor.enter("static.continuous.chosen-type.retention");
    cursor.consume_phrase(&["in", "addition", "to"])?;
    read_possessive(cursor, number)?;
    cursor.consume("other")?;
    if word_in(cursor.peek(), &["creature"]) {
        cursor.consume("creature")?;
    }
    cursor.consume("types")?;
    cursor.enter("static.continuous.chosen-type.object");
    cursor.require(
        *objects == self_ref(),
        "the source itself taking the chosen creature type",
    )?;
    Ok(json!({
        "kind": "addChosenSubtype",
        "object": self_ref(),
        "choice": chosen_creature_type(),
    }))
}

/// `every creature type` / `all creature types` after a copula (CR 205.3m).
fn all_creature_types_follow(cursor: &OracleTokenCursor<'_>) -> bool {
    let every = word_in(cursor.peek(), &["every"]) && word_in(cursor.peek_ahead(2), &["type"]);
    let all = word_in(cursor.peek(), &["all"]) && word_in(cursor.peek_ahead(2), &["types"]);
    (every || all) && word_in(cursor.peek_ahead(1), &["creature"])
}

/// `isn't a TYPE` (CR 205.1a): a source that is not that card type while
/// its condition holds. Only the source's own type can be removed.
fn read_card_type_removal(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.type-removal");
    cursor.require(
        *objects == self_ref(),
        "a card-type removal on the source itself",
    )?;
    cursor.require(word_in(cursor.peek(), &["a", "an"]), "`a` or `an`")?;
    let article = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&article)?;
    let noun = cursor.peek().unwrap_or_default().to_string();
    let card_type = cursor.require_value(
        lookup_permanent_card_type(&noun, AcceptedForms::SingularOnly),
        "a permanent card type",
    )?;
    cursor.consume(&noun)?;
    Ok(json!({
        "kind": "removeCardTypeWhile",
        "object": self_ref(),
        "cardType": card_type.ir_name(),
    }))
}

/// The characteristic-change complement of a copula when it states more
/// than the type nouns the type-change child reads: power and toughness,
/// colors, supertypes, granted abilities or a retained type. `None` leaves
/// the copula to the type-change child and its own diagnostics.
fn read_characteristic_copula(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> Option<BecomeTypesNode<()>> {
    let mut trial = cursor.clone();
    let complement = recognize_characteristic_copula_complement(&mut trial, number).ok()?;
    let states_more_than_types = complement.base_power_toughness().is_some()
        || complement.colors.is_some()
        || !complement.supertypes.is_empty()
        || !complement.characteristics.keywords.is_empty()
        || !complement.characteristics.abilities.is_empty()
        || matches!(complement.retention, TypeChangeRetention::StillA(_));
    if !states_more_than_types {
        return None;
    }
    *cursor = trial;
    Some(complement)
}

/// `KEYWORD ("," | "and" | ", and") KEYWORD …` lost by the subject's objects:
/// the shared keyword list, restricted to parameterless keywords.
fn read_keyword_loss(cursor: &mut OracleTokenCursor<'_>, objects: &Value) -> MachineResult<Value> {
    let mut keywords = Vec::new();
    for keyword in
        recognize_granted_list_before_connector(cursor, GRANTED_OBJECT_TYPE_LINE)?.keywords
    {
        match keyword {
            KeywordAtomNode::Parameterless(keyword) => keywords.push(keyword.canonical_name()),
            _ => {
                return Err(cursor.fail("a parameterless keyword the objects lose"));
            }
        }
    }
    Ok(json!({ "kind": "loseKeywords", "objects": objects.clone(), "keywords": keywords }))
}

/// `COLOR (("," | "and") COLOR)*`, `all colors` or `colorless` after the
/// copula (CR 105.3): a color-setting effect on the subject in layer 5.
fn read_optional_color_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
    committed: &mut bool,
) -> MachineResult<Option<Value>> {
    if word_in(cursor.peek(), &["all"]) && word_in(cursor.peek_ahead(1), &["colors"]) {
        cursor.enter("static.continuous.color");
        *committed = true;
        cursor.consume_phrase(&["all", "colors"])?;
        return Ok(Some(
            json!({ "kind": "setAllColors", "objects": objects.clone() }),
        ));
    }
    if word_in(cursor.peek(), &["colorless"])
        && !cursor.peek_ahead(1).is_some_and(|raw| {
            lookup_permanent_card_type(raw, AcceptedForms::SingularOrPlural).is_some()
        })
    {
        cursor.enter("static.continuous.color");
        *committed = true;
        cursor.consume("colorless")?;
        return Ok(Some(
            json!({ "kind": "setColor", "objects": objects.clone(), "colors": [] }),
        ));
    }
    let color_at = |cursor: &OracleTokenCursor<'_>, offset: usize| {
        cursor
            .peek_ahead(offset)
            .and_then(crate::oracle::characteristic_vocabulary::lookup_card_color)
    };
    let Some(first) = color_at(&*cursor, 0) else {
        return Ok(None);
    };
    // `is black` sets a color; `is a black Zombie` is a type change.
    let mut width = 1;
    let mut colors = vec![first];
    loop {
        let connector = cursor.peek_ahead(width);
        let next = if connector == Some(",") && word_in(cursor.peek_ahead(width + 1), &["and"]) {
            2
        } else if connector == Some(",") || word_in(connector, &["and"]) {
            1
        } else {
            break;
        };
        match color_at(&*cursor, width + next) {
            Some(color) => {
                colors.push(color);
                width += next + 1;
            }
            None => break,
        }
    }
    let ends_predicate = {
        let mut ahead = cursor.clone();
        ahead.advance_to(ahead.position() + width).is_ok()
            && (ahead.peek().is_none_or(|raw| raw == "." || raw == ",")
                || word_in(ahead.peek(), &["and"])
                || trailing_condition_starts(&ahead))
    };
    if !ends_predicate {
        return Ok(None);
    }
    cursor.enter("static.continuous.color");
    *committed = true;
    cursor.advance_to(cursor.position() + width)?;
    Ok(Some(json!({
        "kind": "setColor",
        "objects": objects.clone(),
        "colors": colors.iter().map(|color| color.ir_symbol()).collect::<Vec<_>>(),
    })))
}

fn protection_aura_exception_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_in(cursor.peek(), &["this"])
        && word_in(cursor.peek_ahead(1), &["effect"])
        && word_in(cursor.peek_ahead(2), &["doesn't"])
        && word_in(cursor.peek_ahead(3), &["remove"])
}

/// `This effect doesn't remove this Aura.` / `... doesn't remove Auras.`:
/// the protection granted by this ability doesn't make this Aura (`source`)
/// or any Aura (`all`) fall off the protected permanent (CR 303.4d).
fn read_protection_aura_exception(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<&'static str> {
    cursor.enter("static.continuous.aura-exception");
    cursor.consume_phrase(&["this", "effect"])?;
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)?;
    cursor.consume("remove")?;
    let spared = if word_in(cursor.peek(), &["this"]) {
        cursor.consume_phrase(&["this", "aura"])?;
        "source"
    } else {
        cursor.consume("auras")?;
        "all"
    };
    cursor.enter("static.continuous.aura-exception.terminator");
    cursor.consume(".")?;
    Ok(spared)
}

/// `can't be the target(s) of …`, `can't be sacrificed`, `can't become
/// untapped`.
fn passive_prohibition_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    targeting_prohibition_starts(cursor)
        || (word_in(cursor.peek(), &["can't", "cannot"])
            && ((word_in(cursor.peek_ahead(1), &["be"])
                && word_in(cursor.peek_ahead(2), &["sacrificed"]))
                || (word_in(cursor.peek_ahead(1), &["become"])
                    && word_in(cursor.peek_ahead(2), &["untapped"]))))
}

/// One passive prohibition of the subject's objects, each its own
/// battlefield modifier:
///
/// - `can't be the target(s) of SOURCES` (CR 115.4): `cantBeTargeted`, one
///   branch per kind of targeting spell or ability with the source's
///   characteristics and controller (the shared targeting-source reader);
/// - `can't be sacrificed` (CR 701.21a): `cantBeSacrificed`;
/// - `can't become untapped` (CR 502.3, 701.26b): `cantBecomeUntapped`, for
///   the untap step and every untap effect.
fn read_passive_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.passive-prohibition");
    if targeting_prohibition_starts(cursor) {
        let node = recognize_targeting_prohibition(cursor)?;
        cursor.enter("static.continuous.targeting-prohibition.lowering");
        let mut sources = Vec::new();
        for branch in &node.branches {
            let mut source = json!({
                "stackObject": match branch.kind {
                    TargetingSourceKind::Spell => "spell",
                    TargetingSourceKind::Ability => "ability",
                },
            });
            if let Some(predicate) = &branch.source {
                let filter = lower_card_object_predicate(predicate);
                cursor.require(
                    !filter.is_null(),
                    "source characteristics the shared card filter represents",
                )?;
                source["where"] = filter;
            }
            if let Some(scope) = branch.controller {
                source["controller"] = match scope {
                    ObjectControllerScope::AbilityController => controller(),
                    ObjectControllerScope::OpponentOfAbilityController => {
                        json!({ "kind": "opponentsOf", "player": controller() })
                    }
                    ObjectControllerScope::NotAbilityController
                    | ObjectControllerScope::EventPlayer
                    | ObjectControllerScope::Player(_) => {
                        return Err(cursor.fail(
                            "a targeting controller relative to the ability controller (`you control`, `your opponents control`)",
                        ));
                    }
                };
            }
            sources.push(source);
        }
        return Ok(json!({
            "kind": "cantBeTargeted",
            "objects": objects.clone(),
            "sources": sources,
        }));
    }
    read_prohibition_word(cursor)?;
    if word_in(cursor.peek(), &["be"]) {
        cursor.consume_phrase(&["be", "sacrificed"])?;
        return Ok(json!({ "kind": "cantBeSacrificed", "objects": objects.clone() }));
    }
    cursor.consume_phrase(&["become", "untapped"])?;
    Ok(json!({ "kind": "cantBecomeUntapped", "objects": objects.clone() }))
}

fn read_prohibition_word(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.require(
        word_in(cursor.peek(), &["can't", "cannot"]),
        "`can't` or `cannot`",
    )?;
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)
}

/// `it` / `them`, the object pronoun of the subject.
fn read_object_pronoun(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> MachineResult<()> {
    cursor.consume(match number {
        TypeChangeNumber::Singular => "it",
        TypeChangeNumber::Plural => "them",
    })
}

/// `activated abilities can't be activated [unless they're mana abilities]`
/// (CR 602.5): the activated abilities of the subject's objects.
fn read_activation_prohibition(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
) -> MachineResult<Value> {
    cursor.consume_phrase(&["activated", "abilities"])?;
    read_prohibition_word(cursor)?;
    cursor.consume_phrase(&["be", "activated"])?;
    let mut abilities = json!({ "kind": "activatedAbilities", "sources": objects.clone() });
    if word_in(cursor.peek(), &["unless"]) {
        cursor.enter("static.continuous.activation-prohibition.exception");
        cursor.consume("unless")?;
        cursor.require(
            word_in(cursor.peek(), &["they're"]),
            "`they're mana abilities`",
        )?;
        let raw = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&raw)?;
        cursor.consume_phrase(&["mana", "abilities"])?;
        abilities["where"] = json!({ "kind": "not", "operand": { "kind": "isManaAbility" } });
    }
    Ok(json!({ "kind": "prohibitActivation", "abilities": abilities }))
}

/// `Untap OBJECTS during each other player's untap step.` (CR 502.3): the
/// objects also untap during the untap step of every other player. The
/// objects are the shared static subject (the source or a collection).
fn read_other_player_untap_sentence(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    committed: &mut bool,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.other-player-untap");
    cursor.consume("untap")?;
    let subject = read_static_object_subject(cursor, input)?;
    cursor.require(!subject.possessive, "the untapped objects")?;
    cursor.enter("static.continuous.other-player-untap.step");
    cursor.consume_phrase(&["during", "each", "other"])?;
    *committed = true;
    cursor.require(
        word_in(cursor.peek(), &["player's"]),
        "`player's untap step`",
    )?;
    let raw = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&raw)?;
    cursor.consume_phrase(&["untap", "step", "."])?;
    cursor.enter("static.continuous.complete");
    cursor.finish()?;
    Ok(if subject.objects == self_ref() {
        json!({ "kind": "untapDuringOtherPlayersUntap", "object": self_ref() })
    } else {
        json!({ "kind": "untapDuringOtherPlayersUntap", "objects": subject.objects })
    })
}

/// `all creatures able to block`: every creature that could block the
/// subject must (CR 509.1c). A qualified blocker set has no primitive.
fn read_all_able_blockers(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.consume_phrase(&["all", "creatures", "able", "to", "block"])
}

fn block_requirement_modifier(objects: &Value) -> Value {
    json!({
        "kind": "grantKeyword",
        "objects": objects.clone(),
        "keyword": "allMustBlockIfAble",
    })
}

fn block_requirement_sentence_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_in(cursor.peek(), &["all"])
        && word_in(cursor.peek_ahead(1), &["creatures"])
        && word_in(cursor.peek_ahead(2), &["able"])
}

/// `All creatures able to block SUBJECT do so.`: the requirement's subject
/// is the shared static subject, in object position.
fn read_block_requirement_sentence(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    committed: &mut bool,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.block-requirement");
    read_all_able_blockers(cursor)?;
    *committed = true;
    let subject = read_static_object_subject(cursor, input)?;
    cursor.require(
        subject.number == TypeChangeNumber::Singular && !subject.possessive,
        "one attacker that every able creature must block",
    )?;
    cursor.enter("static.continuous.block-requirement.do-so");
    cursor.consume_phrase(&["do", "so", "."])?;
    cursor.enter("static.continuous.complete");
    cursor.finish()?;
    Ok(block_requirement_modifier(&subject.objects))
}

/// `Play with ...` / `Skip your ...` opening the ability: an imperative
/// player predicate whose subject is the controller.
fn imperative_player_predicate_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    (word_in(cursor.peek(), &["play"]) && word_in(cursor.peek_ahead(1), &["with"]))
        || (word_in(cursor.peek(), &["skip"]) && word_in(cursor.peek_ahead(1), &["your"]))
}

/// `the "legend rule" doesn't apply [to OBJECTS]` (CR 704.5j): the shared
/// waiver reader; the state-based action ignores the permanents of its
/// collection, or every permanent without one.
fn read_legend_rule_waiver(
    cursor: &mut OracleTokenCursor<'_>,
    committed: &mut bool,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.legend-rule");
    let waiver = recognize_legend_rule_waiver(cursor)?;
    *committed = true;
    lower_legend_rule_waiver(cursor, &waiver)
}

/// The `legendRuleDoesNotApply` modifier of a waiver, static or created.
pub(in crate::oracle::canonical) fn lower_legend_rule_waiver(
    cursor: &OracleTokenCursor<'_>,
    waiver: &LegendRuleWaiverNode,
) -> MachineResult<Value> {
    let objects = match &waiver.objects {
        Some(query) => lower_static_permanent_query(cursor, query)?,
        None => json!({ "kind": "permanents" }),
    };
    Ok(json!({ "kind": "legendRuleDoesNotApply", "objects": objects }))
}

fn combat_participant_limit_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_in(cursor.peek(), &["no"])
        && word_in(cursor.peek_ahead(1), &["more"])
        && word_in(cursor.peek_ahead(2), &["than"])
}

/// `No more than N creature(s) can attack|block each combat` (CR 508.1c,
/// 509.1c): a restriction on the whole declaration, counted over every
/// declared attacker or blocker.
fn read_combat_participant_limit(
    cursor: &mut OracleTokenCursor<'_>,
    committed: &mut bool,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.combat-limit");
    cursor.consume_phrase(&["no", "more", "than"])?;
    let maximum = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => value,
        _ => return Err(cursor.fail("a positive literal number of creatures")),
    };
    cursor.enter("static.continuous.combat-limit.creatures");
    cursor.consume(if maximum == 1 {
        "creature"
    } else {
        "creatures"
    })?;
    cursor.consume("can")?;
    *committed = true;
    cursor.enter("static.continuous.combat-limit.role");
    let role = if word_in(cursor.peek(), &["attack"]) {
        "attack"
    } else {
        cursor.require(word_in(cursor.peek(), &["block"]), "`attack` or `block`")?;
        "block"
    };
    cursor.consume(role)?;
    cursor.enter("static.continuous.combat-limit.period");
    cursor.consume_phrase(&["each", "combat"])?;
    Ok(json!({
        "kind": "combatParticipantLimit",
        "role": role,
        "maximum": integer(maximum),
    }))
}

/// The head of one continuous predicate (S3): a lexicon lemma in a present
/// form, the copula, a modal or negated auxiliary, or the shared combat
/// clause. The predicate reader matches on it instead of a verb word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticPredicateHead {
    /// The shared combat clause (CR 508.1c-d, 509.1b-c).
    Combat,
    /// `get(s)` a signed P/T delta.
    ModifyStats,
    /// `have`/`has` a keyword list.
    Grant,
    /// `is`/`are` a type change, in the copula's number.
    Copula(TypeChangeNumber),
    /// `doesn't`/`don't` untap.
    NegatedDo,
    /// `enter(s)` with an entry modification.
    Entry,
    /// `lose(s)` all abilities.
    AbilityLoss,
    /// `can` attack as though it didn't have defender.
    Ability,
    /// `assign(s)` combat damage.
    DamageAssignment,
    /// `it's` / `they're` (CR 205.1): the copula contracted into the
    /// continuation pronoun, in that pronoun's number.
    ContractedCopula(TypeChangeNumber),
    /// `isn't` / `aren't` a card type.
    NegatedCopula(TypeChangeNumber),
    /// `can't have [KIND] counters put on it|them` (CR 122.1).
    CounterProhibition,
    /// `can't be the target of …`, `can't be sacrificed`, `can't become
    /// untapped`: a passive prohibition of what may happen to the objects.
    PassiveProhibition,
    /// `its|their activated abilities can't be activated` (CR 602.5).
    PossessiveAbilities,
    /// The same prohibition after a possessive subject (`enchanted
    /// creature's activated abilities …`).
    PossessedAbilities,
    /// `all creatures able to block it|them do so` (CR 509.1c).
    BlockRequirement,
    /// `can't have or gain KEYWORDS` (CR 613.1f).
    KeywordProhibition,
}

impl StaticPredicateHead {
    /// Heads that continue a predicate list after a serial comma.
    fn continues_serial_list(self) -> bool {
        matches!(
            self,
            Self::Combat
                | Self::ModifyStats
                | Self::Grant
                | Self::Copula(_)
                | Self::CounterProhibition
                | Self::PassiveProhibition
                | Self::PossessiveAbilities
                | Self::BlockRequirement
                | Self::KeywordProhibition
        )
    }
}

/// The predicate head at the cursor, read from the verb lexicon.
fn static_predicate_head(cursor: &OracleTokenCursor<'_>) -> Option<StaticPredicateHead> {
    use crate::oracle::canonical::automata::{
        Auxiliary, VerbAgreement, VerbForm, VerbLemma, copula_agreement, lookup_auxiliary,
        lookup_effect_verb,
    };
    if word_in(cursor.peek(), &["can't", "cannot"])
        && word_in(cursor.peek_ahead(1), &["have"])
        && (word_in(cursor.peek_ahead(2), &["counters"])
            || word_in(cursor.peek_ahead(3), &["counters"]))
    {
        return Some(StaticPredicateHead::CounterProhibition);
    }
    if passive_prohibition_starts(cursor) {
        return Some(StaticPredicateHead::PassiveProhibition);
    }
    if word_in(cursor.peek(), &["can't", "cannot"])
        && word_in(cursor.peek_ahead(1), &["have"])
        && word_in(cursor.peek_ahead(2), &["or"])
        && word_in(cursor.peek_ahead(3), &["gain"])
    {
        return Some(StaticPredicateHead::KeywordProhibition);
    }
    if word_in(cursor.peek(), &["its", "their"])
        && word_in(cursor.peek_ahead(1), &["activated"])
        && word_in(cursor.peek_ahead(2), &["abilities"])
    {
        return Some(StaticPredicateHead::PossessiveAbilities);
    }
    if word_in(cursor.peek(), &["all"])
        && word_in(cursor.peek_ahead(1), &["creatures"])
        && word_in(cursor.peek_ahead(2), &["able"])
    {
        return Some(StaticPredicateHead::BlockRequirement);
    }
    if word_in(cursor.peek(), &["isn't"]) {
        return Some(StaticPredicateHead::NegatedCopula(
            TypeChangeNumber::Singular,
        ));
    }
    if word_in(cursor.peek(), &["aren't"]) {
        return Some(StaticPredicateHead::NegatedCopula(TypeChangeNumber::Plural));
    }
    if combat_clause_starts(cursor) {
        return Some(StaticPredicateHead::Combat);
    }
    let raw = cursor.peek()?;
    if let Some(agreement) = copula_agreement(raw) {
        return Some(StaticPredicateHead::Copula(match agreement {
            VerbAgreement::ThirdPersonSingular => TypeChangeNumber::Singular,
            VerbAgreement::Base => TypeChangeNumber::Plural,
        }));
    }
    if let Some((auxiliary, _)) = lookup_auxiliary(raw) {
        return match auxiliary {
            Auxiliary::DoNot => Some(StaticPredicateHead::NegatedDo),
            Auxiliary::Can => Some(StaticPredicateHead::Ability),
            Auxiliary::May | Auxiliary::Cannot | Auxiliary::Would | Auxiliary::Must => None,
        };
    }
    let (entry, forms) = lookup_effect_verb(raw)?;
    if !forms.contains(&VerbForm::Base) && !forms.contains(&VerbForm::ThirdPersonSingular) {
        return None;
    }
    match entry.lemma {
        VerbLemma::Get => Some(StaticPredicateHead::ModifyStats),
        VerbLemma::Have => Some(StaticPredicateHead::Grant),
        VerbLemma::Enter => Some(StaticPredicateHead::Entry),
        VerbLemma::Lose => Some(StaticPredicateHead::AbilityLoss),
        VerbLemma::Assign => Some(StaticPredicateHead::DamageAssignment),
        _ => None,
    }
}

/// `you control ATTACHED-PERMANENT.` (CR 613.1b): the control-changing
/// effect of an Aura over the permanent it enchants. The object is read with
/// the shared static subject; the engine evaluates only the attached object.
fn read_player_control(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    committed: &mut bool,
) -> MachineResult<Value> {
    cursor.enter("static.control.subject");
    cursor.consume_phrase(&["you", "control"])?;
    *committed = true;
    cursor.enter("static.control.object");
    let (objects, _, _) = read_static_subject(cursor, input)?;
    cursor.require(
        objects == json!({ "kind": "attachedPermanent", "attachment": self_ref() }),
        "the permanent this Aura enchants (the engine changes control of no other static object)",
    )?;
    cursor.enter("static.control.terminator");
    cursor.consume(".")?;
    cursor.enter("static.control.complete");
    cursor.finish()?;
    Ok(json!({ "kind": "controlAttachedPermanent", "attachment": self_ref() }))
}

/// `enter tapped` / `enter with an additional COUNTER counter on them [for
/// each …]` (CR 614.1c-d): a replacement applied to every permanent of a
/// battlefield collection as it enters, generated by this static ability.
/// The source's own entry belongs to the entry-replacement frame.
fn read_collection_entry_modification(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.entry-modification");
    cursor.require(
        objects["kind"] == "permanents",
        "a permanent collection entering the battlefield (the source's own entry is a replacement frame)",
    )?;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
    {
        cursor.consume("tapped")?;
        return Ok(json!({ "kind": "permanentsEnterTapped", "objects": objects.clone() }));
    }
    cursor.enter("static.continuous.entry-counters");
    cursor.consume("with")?;
    let atom = recognize_numeric_atom(cursor)?;
    cursor.require(
        !matches!(atom, NumericAtom::Integer(value) if value <= 0),
        "a positive counter quantity",
    )?;
    let singular = matches!(atom, NumericAtom::Integer(1));
    cursor.consume("additional")?;
    let counter = recognize_counter_name(cursor)?;
    cursor.consume(if singular { "counter" } else { "counters" })?;
    cursor.consume("on")?;
    let pronoun = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("them") || raw.eq_ignore_ascii_case("it"))
        .map(str::to_string);
    let pronoun = cursor.require_value(pronoun, "`them` or `it` for each entering permanent")?;
    cursor.consume(&pronoun)?;
    let base = NumericExpression::Atom(atom);
    let count = if for_each_count_starts(cursor) {
        let counted = recognize_for_each_count(cursor)?;
        cursor.require(
            !count_excludes_source(&counted),
            "an `other` count whose antecedent is the source, not each entering permanent",
        )?;
        cursor.require_value(
            scale_by_for_each_count(&base, counted),
            "a literal counter quantity multiplied by `for each`",
        )?
    } else {
        base
    };
    Ok(json!({
        "kind": "addEnteringCounters",
        "objects": objects.clone(),
        "counter": counter.as_str(),
        "count": cursor.require_value(
            lower_numeric_expression(&count),
            "a counter quantity the entry modification can evaluate",
        )?,
    }))
}

/// `during your turn` / `during turns other than yours` (CR 611.3a): whether
/// the ability's controller is the active player, re-evaluated continuously.
fn recognize_optional_turn_condition(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<StaticConditionNode>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("during"))
    {
        return Ok(None);
    }
    cursor.enter("static.continuous.turn-condition");
    let not_your_turn = BooleanConditionNode::Atom(ConditionAtom::TurnOwnership(
        ControllerTurnPropositionNode {
            is_controller_turn: false,
        },
    ));
    if cursor
        .peek_ahead(1)
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        cursor.consume_phrase(&["during", "your", "turn"])?;
        return Ok(Some(BooleanConditionNode::Not(Box::new(not_your_turn))));
    }
    cursor.consume_phrase(&["during", "turns", "other", "than", "yours"])?;
    Ok(Some(not_your_turn))
}

fn word_in(raw: Option<&str>, words: &[&str]) -> bool {
    raw.is_some_and(|raw| {
        let raw = raw.replace('\u{2019}', "'");
        words.iter().any(|word| raw.eq_ignore_ascii_case(word))
    })
}

/// `untap during OWNER untap step`: a permanent untaps only during its
/// controller's untap step (CR 502.3), which every printed owner denotes.
fn read_untap_step_exclusion(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.enter("static.continuous.untap-exclusion");
    cursor.consume_phrase(&["untap", "during"])?;
    cursor.enter("static.continuous.untap-exclusion.step-owner");
    if word_in(cursor.peek(), &["your"]) {
        cursor.consume("your")?;
    } else if word_in(cursor.peek(), &["its", "their"]) {
        let pronoun = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&pronoun)?;
        cursor.require(
            word_in(cursor.peek(), &["controller's", "controllers'"]),
            "the possessive `controller's` after the pronoun",
        )?;
        cursor.advance_to(cursor.position() + 1)?;
    } else {
        return Err(cursor.fail("`your`, `its controller's` or `their controllers'` untap step"));
    }
    cursor.enter("static.continuous.untap-exclusion.step");
    crate::oracle::canonical::automata::consume_lemma(
        cursor,
        crate::oracle::canonical::automata::VerbLemma::Untap,
        crate::oracle::canonical::automata::VerbAgreement::Base,
    )?;
    cursor.require(
        word_in(cursor.peek(), &["step", "steps"]),
        "`step` after the untap step owner",
    )?;
    cursor.advance_to(cursor.position() + 1)
}

fn read_subject_pronoun(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> MachineResult<()> {
    let pronoun = match number {
        TypeChangeNumber::Singular => "it",
        TypeChangeNumber::Plural => "they",
    };
    cursor.consume(pronoun)
}

fn read_possessive(
    cursor: &mut OracleTokenCursor<'_>,
    number: TypeChangeNumber,
) -> MachineResult<()> {
    let possessive = match number {
        TypeChangeNumber::Singular => "its",
        TypeChangeNumber::Plural => "their",
    };
    cursor.consume(possessive)
}

fn read_negated_have(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    cursor.require(
        word_in(cursor.peek(), &["didn't"]),
        "the counterfactual `didn't have`",
    )?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.consume("have")
}

/// A predicate that continues a serial list (`get`, `have`, the copula or
/// the shared combat clause) at `offset`.
fn predicate_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let mut ahead = cursor.clone();
    ahead.advance_to(ahead.position() + offset).is_ok()
        && static_predicate_head(&ahead).is_some_and(StaticPredicateHead::continues_serial_list)
}

/// `, VERB` or `, and VERB` continues the predicate list of the same subject.
fn serial_predicate_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    predicate_starts_at(cursor, 1)
        || (cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && predicate_starts_at(cursor, 2))
}

/// A trailing gate of the static ability: `as long as`, `unless`, `if` (not
/// a requirement's `if able`) or a `during` turn condition.
fn trailing_condition_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    as_long_as_starts(cursor)
        || word_in(cursor.peek(), &["unless", "during"])
        || (word_in(cursor.peek(), &["if"]) && !word_in(cursor.peek_ahead(1), &["able"]))
}

fn read_trailing_condition(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    source_subject: bool,
    attached_subject: bool,
) -> MachineResult<StaticConditionNode> {
    if let Some(turn_condition) = recognize_optional_turn_condition(cursor)? {
        return Ok(turn_condition);
    }
    let context = condition_context(input, source_subject);
    let subject = condition_subject(input, attached_subject);
    if as_long_as_starts(cursor) {
        cursor.consume_phrase(&["as", "long", "as"])?;
        return recognize_static_condition_for(cursor, context, subject);
    }
    if word_in(cursor.peek(), &["if"]) {
        cursor.consume("if")?;
        return recognize_static_condition_for(cursor, context, subject);
    }
    cursor.consume("unless")?;
    recognize_static_condition_for(cursor, context, subject)
        .map(|condition| BooleanConditionNode::Not(Box::new(condition)))
}

/// The source's attachment (from its face) and whether the ability's
/// subject is the attached permanent, for the condition's references.
fn condition_subject(input: &AbilityInput<'_>, attached_subject: bool) -> StaticConditionSubject {
    StaticConditionSubject {
        source_attachment: source_attachment_role(input.face_type_line),
        attached_subject,
    }
}

fn as_long_as_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    ["as", "long", "as"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// The failure inside the static frame, for the frame claim of an otherwise
/// unclaimed ability (CR 604.1): once a predicate was reached, or once the
/// shared subject reader consumed at least one subject token. A subject that
/// no reader starts is reported by the frame claim itself.
pub(in crate::oracle::canonical) fn static_continuous_frame_failure(
    input: &AbilityInput<'_>,
) -> Option<MachineFailure> {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return None;
    }
    let mut cursor = OracleTokenCursor::new("static-continuous-effect", input.tokens);
    let mut committed = false;
    let mut zone_of_function = None;
    let failure =
        read_static_continuous_effect(&mut cursor, input, &mut committed, &mut zone_of_function)
            .err()?;
    let reached_predicate = cursor
        .transitions()
        .iter()
        .any(|transition| transition.state == "static.continuous.predicate");
    let subject_start = cursor
        .transitions()
        .iter()
        .find(|transition| transition.state == "static.continuous.subject")
        .map(|transition| transition.token_index);
    let read_into_subject = subject_start.is_some_and(|start| failure.token_index > start);
    (committed || reached_predicate || read_into_subject).then_some(failure)
}

fn condition_context<'a>(
    input: &'a AbilityInput<'a>,
    source_subject: bool,
) -> StaticConditionContext<'a> {
    StaticConditionContext {
        source_subject,
        name_links: input.name_links,
        name_failure: input.name_resolution_failure,
    }
}

/// A complete rule granted by a static ability (CR 113.1, 604.1, 613.1f):
/// the rule's own kind selects the layer-6 grant the engine reads. Inside the
/// rule, `this creature` is the object that has it (the quote was compiled
/// as an ability of the granted object), so a granted trigger fires from the
/// grantee and a granted static ability applies with the grantee as source.
/// A parameterized keyword (`annihilator 1`, `toxic 2`) arrives as its
/// compiled keyword rule. Any other rule kind fails explicitly.
fn lower_static_quoted_ability(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
    ability: QuotedAbilityNode,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.quoted-ability");
    let kind = match ability.rule["kind"].as_str() {
        Some("activatedAbility" | "manaAbility") => "grantActivatedAbility",
        Some("triggeredAbility") => "grantTriggeredAbility",
        Some("staticAbility") => "grantStaticAbility",
        Some("keywordAbility" | "keywordAbilityGroup") => "grantKeywordAbility",
        _ => {
            return Err(cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::GrantedNonActivatedAbility,
                "a granted activated, mana, triggered, static or keyword rule",
            ));
        }
    };
    Ok(json!({
        "kind": kind,
        "objects": objects.clone(),
        "ability": ability.rule,
    }))
}

/// `+X/+Y, where X is EXPR`: the shared numeric stat pair and the shared
/// postposed definition (CR 107.3). A static ability's value is continuously
/// recomputed (CR 611.3a), so the definition replaces X in each component
/// instead of being bound once; both components lower through the shared
/// numeric projection, and a component it cannot represent fails.
fn read_where_x_power_toughness_delta(
    cursor: &mut OracleTokenCursor<'_>,
    source_subject: bool,
) -> MachineResult<(Value, Value)> {
    cursor.enter("static.continuous.numeric-delta");
    let delta = recognize_signed_numeric_power_toughness_delta(cursor)?;
    cursor.enter("static.continuous.where-x");
    cursor.require(
        (delta.power.uses_x() || delta.toughness.uses_x()) && where_x_definition_starts(cursor),
        "a postposed `where X is` definition of the X in the stat delta",
    )?;
    let definition = recognize_where_x_definition(cursor)?;
    // The same `other` antecedent rule as a `for each` count.
    cursor.enter("static.continuous.where-x.other-antecedent");
    cursor.require(
        source_subject || !count_excludes_source(&definition),
        "an `other` count whose antecedent is the source, not each modified object",
    )?;
    cursor.enter("static.continuous.where-x.lowering");
    let mut lowered = [&delta.power, &delta.toughness]
        .into_iter()
        .map(|component| {
            component
                .substitute_x(&definition)
                .as_ref()
                .and_then(lower_numeric_expression)
        });
    let (Some(Some(power)), Some(Some(toughness))) = (lowered.next(), lowered.next()) else {
        return Err(cursor.fail("a definition of X the shared numeric projection represents"));
    };
    Ok((power, toughness))
}

/// `for each COUNTABLE`: the delta is multiplied by the shared count child
/// (the same one every action quantity uses). The count lowers through the
/// shared numeric projection, so the modifier IR is the common count IR.
fn read_optional_for_each_count(
    cursor: &mut OracleTokenCursor<'_>,
    source_subject: bool,
) -> MachineResult<Option<Value>> {
    if !for_each_count_starts(cursor) {
        return Ok(None);
    }
    cursor.enter("static.continuous.for-each");
    // `This creature gets +2/+2 for each Aura attached to it`: a pronoun in
    // the count names the modified source, the clause's subject.
    let subject = source_subject.then_some(PermanentEffectReference::Source(
        LocalObjectReference::SourceSelf,
    ));
    let count = with_clause_subject(cursor, subject, recognize_for_each_count)?;
    // `other` names the source only when the source is the modified object;
    // otherwise it means other than each modified object.
    cursor.enter("static.continuous.for-each.other-antecedent");
    cursor.require(
        source_subject || !count_excludes_source(&count),
        "an `other` count whose antecedent is the source, not each modified object",
    )?;
    lower_numeric_expression(&count)
        .map(Some)
        .ok_or_else(|| cursor.fail("a counted collection lowered to a runtime count"))
}

/// Returns the lowered subject, its predicate for a queried collection, and
/// the grammatical number a copula must agree with.
pub(in crate::oracle::canonical) fn read_static_subject(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> MachineResult<(Value, Option<ObjectPredicate>, TypeChangeNumber)> {
    let subject = read_static_subject_reading(cursor, input, false)?;
    // Only this frame's rule carries the bestow composition directive.
    cursor.require(
        !(subject.objects["kind"] == "attachedPermanent"
            && attachment_subject_source(input.face_type_line, false, true)
                == Some(AttachmentSubjectSource::Bestow)),
        "an attachment subject on an Aura or Equipment source",
    )?;
    Ok((subject.objects, subject.predicate, subject.number))
}

/// The shared object subject of the static frame, admitting a possessive
/// attachment subject (`enchanted creature's`).
fn read_static_object_subject(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
) -> MachineResult<StaticObjectSubject> {
    read_static_subject_reading(cursor, input, true)
}

fn read_static_subject_reading(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    allow_possessive: bool,
) -> MachineResult<StaticObjectSubject> {
    cursor.enter("static.continuous.subject");
    let attachment = cursor.peek().map(str::to_ascii_lowercase);
    let raw_noun = cursor
        .peek_ahead(1)
        .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase());
    // `enchanted creature's activated abilities`: the possessed noun follows.
    let possessive = allow_possessive
        && matches!(attachment.as_deref(), Some("equipped" | "enchanted"))
        && raw_noun.as_deref().is_some_and(|noun| noun.ends_with("'s"))
        && !word_in(cursor.peek_ahead(2), &["controller"]);
    let attached_noun = raw_noun.map(|noun| {
        if possessive {
            noun.trim_end_matches("'s").to_string()
        } else {
            noun
        }
    });
    // An Aura may enchant any permanent kind (CR 303.4); Equipment only
    // creatures (CR 301.5).
    let enchanted_permanent = attachment.as_deref() == Some("enchanted")
        && attached_noun.as_deref().is_some_and(|noun| {
            noun == "permanent"
                || lookup_permanent_card_type(noun, AcceptedForms::SingularOnly).is_some()
        });
    // `enchanted Wall` / `enchanted Mountain`: a capitalized subtype noun
    // names the enchanted permanent and proves its subtype (CR 205.3).
    let enchanted_subtype = (attachment.as_deref() == Some("enchanted") && !possessive)
        .then(|| cursor.peek_ahead(1))
        .flatten()
        .and_then(attached_subtype_noun);
    if (matches!(attachment.as_deref(), Some("equipped" | "enchanted"))
        && attached_noun.as_deref() == Some("creature"))
        || enchanted_permanent
        || enchanted_subtype.is_some()
    {
        // The attachment subject is meaningful only on the matching
        // attachment subtype (CR 301.5, 303.4), or on a bestow creature
        // whose Aura form enchants a creature (CR 702.103b): card
        // composition proves that face's bestow keyword.
        cursor.enter("static.continuous.subject.attachment");
        cursor.require(
            attachment_subject_source(
                input.face_type_line,
                attachment.as_deref() == Some("equipped"),
                attached_noun.as_deref() == Some("creature"),
            )
            .is_some(),
            "an attachment subject on the matching Equipment or Aura source, or on a bestow creature",
        )?;
        if let Some(subtype) = enchanted_subtype {
            let word = attachment.expect("checked attachment word");
            cursor.consume(&word)?;
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
            return Ok(StaticObjectSubject {
                objects: json!({ "kind": "attachedPermanent", "attachment": self_ref() }),
                predicate: Some(ObjectPredicate::Subtype(subtype)),
                number: TypeChangeNumber::Singular,
                possessive: false,
                contracted_copula: false,
            });
        }
        let word = attachment.expect("checked attachment word");
        cursor.consume(&word)?;
        let noun = attached_noun.expect("checked attached noun");
        if possessive {
            let raw = cursor.peek().unwrap_or_default().to_string();
            cursor.consume(&raw)?;
        } else {
            cursor.consume(&noun)?;
        }
        // The attached noun is the subject's type (`enchanted land`); it is
        // the proof a land-only change such as `is a Swamp` requires.
        let noun_type = lookup_permanent_card_type(&noun, AcceptedForms::SingularOnly)
            .map(ObjectPredicate::CardType);
        return Ok(StaticObjectSubject {
            objects: json!({ "kind": "attachedPermanent", "attachment": self_ref() }),
            predicate: noun_type,
            number: TypeChangeNumber::Singular,
            possessive,
            contracted_copula: false,
        });
    }
    // A sentence-initial word the name resolver could not link (`Nonblack
    // creatures`, `Face-down creatures you control`) is a source reference
    // only when no object collection reads it: a linked name or `this NOUN`
    // stays a source reference.
    let linked_source =
        source_reference_starts_at(cursor.position(), cursor.peek(), input.name_links, None);
    if !linked_source
        && source_reference_starts_at(
            cursor.position(),
            cursor.peek(),
            input.name_links,
            input.name_resolution_failure,
        )
    {
        let mut trial = cursor.clone();
        if let Ok(subject) = read_static_collection_subject(&mut trial) {
            *cursor = trial;
            return Ok(subject);
        }
    }
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        input.name_links,
        input.name_resolution_failure,
    ) {
        cursor.enter("static.continuous.subject.source");
        recognize_source_reference(cursor, input.name_links, input.name_resolution_failure)?;
        return Ok(StaticObjectSubject {
            objects: self_ref(),
            predicate: None,
            number: TypeChangeNumber::Singular,
            possessive: false,
            contracted_copula: false,
        });
    }
    read_static_collection_subject(cursor)
}

/// `[each | all] COLLECTION`: the shared object collection as the subject.
fn read_static_collection_subject(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticObjectSubject> {
    cursor.enter("static.continuous.subject.collection");
    // `each` distributes over a singular noun; `all` and a bare collection
    // are plural. Both denote universal membership (no choice).
    let number = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("each") => {
            cursor.consume("each")?;
            TypeChangeNumber::Singular
        }
        Some("all") => {
            cursor.consume("all")?;
            TypeChangeNumber::Plural
        }
        _ => TypeChangeNumber::Plural,
    };
    let query = recognize_object_collection(cursor)?;
    let objects = lower_static_permanent_query(cursor, &query)?;
    Ok(StaticObjectSubject {
        objects,
        predicate: Some(query.predicate),
        number,
        possessive: false,
        contracted_copula: false,
    })
}

/// A battlefield-permanent collection as a static object selector: its
/// predicate, ownership, controller relation and source exclusion. Shared by
/// static abilities and the rules a resolving effect creates (CR 611.2a).
pub(in crate::oracle::canonical) fn lower_static_permanent_query(
    cursor: &OracleTokenCursor<'_>,
    query: &ObjectQueryNode,
) -> MachineResult<Value> {
    cursor.require(
        query.denotes_battlefield_permanents(),
        "a collection anchored in the battlefield-permanent domain",
    )?;
    let mut objects = cursor.require_value(
        lower_permanent_collection(&query.predicate),
        "an ownership relation a permanent collection can carry",
    )?;
    if let Some(scope) = &query.controller {
        objects["controller"] = match scope {
            ObjectControllerScope::AbilityController => controller(),
            ObjectControllerScope::OpponentOfAbilityController => {
                json!({ "kind": "opponentsOf", "player": controller() })
            }
            ObjectControllerScope::NotAbilityController
            | ObjectControllerScope::EventPlayer
            | ObjectControllerScope::Player(_) => {
                return Err(
                    cursor.fail("a static controller scope representable as a player relation")
                );
            }
        };
    }
    if query.exclude_source {
        objects["excludeSource"] = json!(true);
    }
    Ok(objects)
}

/// The permanent that makes an attachment subject meaningful (CR 301.5,
/// 303.4, 702.103b).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttachmentSubjectSource {
    /// An Equipment or an Aura face: the subject is its attached object.
    AttachmentSubtype,
    /// An enchantment creature without the Aura subtype: only its bestow
    /// Aura form enchants a creature; composition proves the keyword.
    Bestow,
}

fn type_line_word(type_line: &str, expected: &str) -> bool {
    type_line
        .split(|ch: char| !ch.is_alphanumeric())
        .any(|word| word == expected)
}

/// Which source admits `equipped|enchanted NOUN` on this face, if any.
pub(in crate::oracle::canonical) fn attachment_subject_source(
    face_type_line: &str,
    equipped: bool,
    creature_noun: bool,
) -> Option<AttachmentSubjectSource> {
    if type_line_word(face_type_line, if equipped { "Equipment" } else { "Aura" }) {
        return Some(AttachmentSubjectSource::AttachmentSubtype);
    }
    (!equipped
        && creature_noun
        && type_line_word(face_type_line, "Enchantment")
        && type_line_word(face_type_line, "Creature"))
    .then_some(AttachmentSubjectSource::Bestow)
}

/// A capitalized creature or noncreature subtype noun after `enchanted`.
fn attached_subtype_noun(raw: &str) -> Option<String> {
    if !raw.chars().next().is_some_and(|ch| ch.is_ascii_uppercase()) {
        return None;
    }
    crate::oracle::characteristic_vocabulary::lookup_creature_subtype(
        raw,
        AcceptedForms::SingularOnly,
    )
    .map(|subtype| subtype.ir_name().to_string())
    .or_else(|| {
        crate::oracle::characteristic_vocabulary::lookup_noncreature_subtype(raw)
            .filter(|name| {
                crate::oracle::characteristic_vocabulary::subtype_denotes_permanent(name)
            })
            .map(str::to_string)
    })
}

fn value_mentions_attached_permanent(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            map.get("kind").and_then(Value::as_str) == Some("attachedPermanent")
                || map.values().any(value_mentions_attached_permanent)
        }
        Value::Array(items) => items.iter().any(value_mentions_attached_permanent),
        _ => false,
    }
}

/// The composition marker of a static rule read on a bestow face: the
/// rule's attachment subject is the bestowed Aura's (CR 702.103b).
pub(in crate::oracle::canonical) const BESTOW_SUBJECT_DIRECTIVE: &str = "attachmentSubjectRequires";

/// Card composition (CR 702.103b): a static rule whose `enchanted creature`
/// subject was read on an enchantment creature face is meaningful only when
/// the same face has bestow. The directive is removed once proven; without
/// the keyword the ability becomes unsupported with an explicit diagnostic.
pub(crate) fn validate_bestow_attachment_subjects(
    abilities: &mut [crate::oracle::model::ParsedOracleAbility],
) {
    let bestow_faces = abilities
        .iter()
        .filter(|ability| {
            ability.rule.as_ref().is_some_and(|rule| {
                rule["kind"] == "keywordAbility" && rule["ability"]["kind"] == "bestow"
            })
        })
        .map(|ability| ability.source.face_id.clone())
        .collect::<Vec<_>>();
    for ability in abilities.iter_mut() {
        let Some(rule) = ability.rule.as_mut() else {
            continue;
        };
        if rule.get(BESTOW_SUBJECT_DIRECTIVE).is_none() {
            continue;
        }
        if bestow_faces.contains(&ability.source.face_id) {
            if let Some(map) = rule.as_object_mut() {
                map.remove(BESTOW_SUBJECT_DIRECTIVE);
            }
            continue;
        }
        let length = ability.source.text.len();
        ability.status = "unsupported".to_string();
        ability.rule = None;
        ability
            .diagnostics
            .push(crate::oracle::model::ParserDiagnostic {
                code: "oracle_card_composition_failure".to_string(),
                message: "An `enchanted creature` subject on a non-Aura enchantment creature requires bestow on the same face (CR 702.103b).".to_string(),
                severity: "unsupported".to_string(),
                automaton: Some("card-bestow-subject-composition".to_string()),
                state: Some("composition.bestow-subject".to_string()),
                token_index: None,
                span_start: Some(0),
                span_end: Some(length),
                expected: Some("a bestow keyword on the same face".to_string()),
                found: None,
                trace: Vec::new(),
                owner: None,
            });
    }
}

#[cfg(test)]
#[path = "static_continuous_condition_tests.rs"]
mod condition_tests;

#[cfg(test)]
#[path = "static_continuous_predicate_tests.rs"]
mod predicate_tests;

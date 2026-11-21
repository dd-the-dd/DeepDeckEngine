//! Static continuous-effect frame (CR 611.3, 604.1):
//!
//! ```text
//! Static    := [ability word "—"] [("as long as" Condition | TurnCondition) ","]
//!              (Subject Predicate ("and" Predicate)* | "you control" AttachedObject)
//!              ["as long as" Condition] "." [keyword reminder]
//! TurnCondition := "during your turn" | "during turns other than yours"
//! Subject   := source reference | "equipped creature" | "enchanted" PERMANENT
//!            | ["each" | "all"] ["other"] permanent collection
//! Predicate := ("get" | "gets") signed P/T delta | ("have" | "has") keyword list
//!            | ("is" | "are") type change | shared combat clause
//!            | ("enter" | "enters") ("tapped" | "with" N "additional" COUNTER "counter(s) on them")
//!            | ("doesn't" | "don't") "untap during" OWNER "untap step"
//!            | ("loses" | "lose") "all abilities"
//!            | "can attack as though" PRONOUN "didn't have defender"
//!            | ("assigns" | "assign") "combat damage equal to" PRONOUN
//!              "toughness rather than" PRONOUN "power"
//! ```
//!
//! A turn condition (CR 611.3a) is a continuously evaluated condition on the
//! turn's active player; it gates the whole ability like `as long as`.
//! Combat restrictions and requirements reuse the shared combat-clause child
//! (CR 508.1c-d, 509.1b-c) for any subject.
//!
//! Predicates are joined by `and`, or by a serial comma before a predicate
//! verb. The copula's number must agree with the subject (`each land is`,
//! `lands are`); the shared type-change child then reads the same number.
//!
//! `as long as` (CR 611.3a) gates the whole static ability with one shared
//! Boolean condition that the engine re-evaluates continuously; it lowers to
//! the static rule's `condition`, never to a per-predicate approximation.
//!
//! Every slot is filled by an existing shared child (source reference, object
//! collection, signed P/T delta, keyword list). Each predicate lowers to one
//! battlefield-active modifier the engine already executes. Adding a subject
//! form or a predicate verb extends this frame once for every combination.
use super::super::*;
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, lookup_permanent_card_type, type_line_has_permanent_card_type,
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
    match read_static_continuous_effect(&mut cursor, input, &mut committed) {
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
                "activeWhile": active_while_battlefield(),
                "modifiers": modifiers,
            });
            if let Some(condition) = &condition {
                rule["condition"] = lower_static_condition(condition);
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

fn read_static_continuous_effect(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    committed: &mut bool,
) -> MachineResult<(Vec<Value>, Option<StaticConditionNode>)> {
    recognize_optional_ability_word_envelope(cursor)?;
    let mut condition = None;
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("during"))
    {
        // `during` introduces a turn condition of the whole static ability.
        *committed = true;
    }
    if let Some(turn_condition) = recognize_optional_turn_condition(cursor)? {
        condition = Some(turn_condition);
        cursor.enter("static.continuous.leading-condition.separator");
        cursor.consume(",")?;
    } else if as_long_as_starts(cursor) {
        cursor.enter("static.continuous.leading-condition");
        cursor.consume_phrase(&["as", "long", "as"])?;
        *committed = true;
        // `it` before the subject is not yet bound to the source.
        condition = Some(recognize_static_condition(
            cursor,
            condition_context(input, false),
        )?);
        cursor.enter("static.continuous.leading-condition.separator");
        cursor.consume(",")?;
    }
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
    let (objects, subject_predicate, subject_number) = read_static_subject(cursor, input)?;
    let source_subject = objects == self_ref();
    let mut modifiers = Vec::new();
    let mut granted = Vec::new();
    let mut changes_types = false;
    loop {
        cursor.enter("static.continuous.predicate");
        let verb = cursor
            .peek()
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| cursor.fail("a continuous predicate"))?;
        match verb.as_str() {
            _ if combat_clause_starts(cursor) => {
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
                    modifiers.push(lower_static_combat_clause(cursor, &objects, clause)?);
                }
            }
            "get" | "gets" => {
                cursor.consume(&verb)?;
                *committed = true;
                if !cursor
                    .peek_token()
                    .is_some_and(|token| token.token_type == "counterValue")
                {
                    let (power, toughness) = read_where_x_power_toughness_delta(cursor)?;
                    modifiers.push(json!({
                        "kind": "modifyPowerToughness",
                        "objects": objects.clone(),
                        "power": power,
                        "toughness": toughness,
                    }));
                } else {
                    let delta = recognize_signed_power_toughness_delta(cursor)?;
                    let scale = read_optional_for_each_count(cursor)?;
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
            }
            "have" | "has" => {
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
                            lower_static_quoted_ability(cursor, &objects, ability)?
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
            "is" | "are" => {
                let number = if verb == "is" {
                    TypeChangeNumber::Singular
                } else {
                    TypeChangeNumber::Plural
                };
                cursor.require(
                    number == subject_number,
                    "a copula agreeing in number with the static subject",
                )?;
                cursor.consume(&verb)?;
                cursor.enter("static.continuous.type-change");
                cursor.require(
                    type_change_characteristics_start(cursor, number),
                    "a type-change characteristic after the copula",
                )?;
                *committed = true;
                let change = recognize_type_change_characteristics(cursor, number)?;
                // A replacing land-subtype change needs a land-only subject.
                let lands = subject_predicate
                    .as_ref()
                    .is_some_and(|predicate| predicate.guarantees_card_type(ObjectCardType::Land));
                cursor.enter("static.continuous.type-change.lowering");
                modifiers.extend(
                    lower_continuous_type_change(&objects, lands, &change)
                        .map_err(|expected| cursor.fail(expected))?,
                );
                changes_types = true;
            }
            "doesn't" | "doesn\u{2019}t" | "don't" | "don\u{2019}t" => {
                cursor.consume(&verb)?;
                *committed = true;
                read_untap_step_exclusion(cursor)?;
                modifiers.push(json!({ "kind": "doesNotUntap", "objects": objects.clone() }));
            }
            "enter" | "enters" => {
                cursor.consume(&verb)?;
                *committed = true;
                modifiers.push(read_collection_entry_modification(cursor, &objects)?);
            }
            "loses" | "lose" => {
                cursor.consume(&verb)?;
                *committed = true;
                cursor.enter("static.continuous.ability-loss");
                cursor.consume_phrase(&["all", "abilities"])?;
                modifiers.push(json!({ "kind": "loseAllAbilities", "objects": objects.clone() }));
            }
            "can" => {
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
            "assigns" | "assign" => {
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
            _ => {
                return Err(cursor.fail(
                    "a continuous predicate: `get(s)`, `have`/`has`, `is`/`are`, a combat clause, `doesn't untap`, `enter(s)`, `loses all abilities`, `can attack as though`, or `assigns combat damage`",
                ));
            }
        }
        cursor.enter("static.continuous.connector");
        // A quoted ability that ends with its own period closes the sentence.
        if cursor.peek().is_none() && quoted_sentence_just_closed(cursor) {
            break;
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
            Some(_) if as_long_as_starts(cursor) => {
                cursor.require(
                    condition.is_none(),
                    "one `as long as` condition for the static ability",
                )?;
                cursor.enter("static.continuous.trailing-condition");
                cursor.consume_phrase(&["as", "long", "as"])?;
                condition = Some(recognize_static_condition(
                    cursor,
                    condition_context(input, source_subject),
                )?);
                cursor.enter("static.continuous.trailing-condition.terminator");
                cursor.consume(".")?;
                break;
            }
            Some(".") => {
                cursor.consume(".")?;
                break;
            }
            _ => return Err(cursor.fail("`and` or the end of the static ability")),
        }
    }
    if (!granted.is_empty() || changes_types) && cursor.peek() == Some("(") {
        // CR 207.2: reminder text of a granted keyword or of a type change
        // (`They're still affected by summoning sickness.`) has no rules meaning.
        // Only one trailing, balanced parenthetical is admitted. Schema
        // validation is deferred: the shared reminder schemas do not yet
        // cover the official collection/attachment wordings (136 abilities).
        cursor.enter("static.continuous.keyword-reminder");
        cursor.consume("(")?;
        while cursor.peek().is_some_and(|raw| raw != ")" && raw != "(") {
            cursor.advance_to(cursor.position() + 1)?;
        }
        cursor.consume(")")?;
    }
    cursor.enter("static.continuous.complete");
    cursor.finish()?;
    Ok((modifiers, condition))
}

const PREDICATE_VERBS: [&str; 6] = ["get", "gets", "have", "has", "is", "are"];

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
    let not_your_turn =
        BooleanConditionNode::Atom(StaticConditionAtom::Local(LocalConditionNode::NotYourTurn));
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
    cursor.consume("untap")?;
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

fn predicate_verb_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        PREDICATE_VERBS
            .iter()
            .any(|verb| raw.eq_ignore_ascii_case(verb))
    })
}

/// A predicate verb, or the start of the shared combat clause, at `offset`.
fn predicate_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    predicate_verb_at(cursor, offset) || {
        let mut ahead = cursor.clone();
        ahead.advance_to(ahead.position() + offset).is_ok() && combat_clause_starts(&ahead)
    }
}

/// `, VERB` or `, and VERB` continues the predicate list of the same subject.
fn serial_predicate_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    predicate_starts_at(cursor, 1)
        || (cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && predicate_starts_at(cursor, 2))
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
    let failure = read_static_continuous_effect(&mut cursor, input, &mut committed).err()?;
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

/// A granted activated or mana ability is read by the engine from the static
/// grant (CR 113.1, 604.1). Granted triggered and static rules are not
/// observed through static grants by the engine's event publishers.
fn lower_static_quoted_ability(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &Value,
    ability: QuotedAbilityNode,
) -> MachineResult<Value> {
    cursor.enter("static.continuous.quoted-ability");
    cursor.require(
        matches!(
            ability.rule["kind"].as_str(),
            Some("activatedAbility" | "manaAbility")
        ),
        "a granted activated or mana ability (the engine observes no static grant of a triggered or static ability)",
    )?;
    Ok(json!({
        "kind": "grantActivatedAbility",
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
) -> MachineResult<(Value, Value)> {
    cursor.enter("static.continuous.numeric-delta");
    let delta = recognize_signed_numeric_power_toughness_delta(cursor)?;
    cursor.enter("static.continuous.where-x");
    cursor.require(
        (delta.power.uses_x() || delta.toughness.uses_x()) && where_x_definition_starts(cursor),
        "a postposed `where X is` definition of the X in the stat delta",
    )?;
    let definition = recognize_where_x_definition(cursor)?;
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
) -> MachineResult<Option<Value>> {
    if !for_each_count_starts(cursor) {
        return Ok(None);
    }
    cursor.enter("static.continuous.for-each");
    let count = recognize_for_each_count(cursor)?;
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
    cursor.enter("static.continuous.subject");
    let attachment = cursor.peek().map(str::to_ascii_lowercase);
    let attached_noun = cursor.peek_ahead(1).map(str::to_ascii_lowercase);
    // An Aura may enchant any permanent kind (CR 303.4); Equipment only
    // creatures (CR 301.5).
    let enchanted_permanent = attachment.as_deref() == Some("enchanted")
        && attached_noun.as_deref().is_some_and(|noun| {
            noun == "permanent"
                || lookup_permanent_card_type(noun, AcceptedForms::SingularOnly).is_some()
        });
    if (matches!(attachment.as_deref(), Some("equipped" | "enchanted"))
        && attached_noun.as_deref() == Some("creature"))
        || enchanted_permanent
    {
        // The attachment subject is meaningful only on the matching
        // attachment subtype (CR 301.5, 303.4).
        let required = if attachment.as_deref() == Some("equipped") {
            "Equipment"
        } else {
            "Aura"
        };
        cursor.enter("static.continuous.subject.attachment");
        cursor.require(
            input
                .face_type_line
                .split(|ch: char| !ch.is_alphanumeric())
                .any(|word| word == required),
            "an attachment subject on the matching Equipment or Aura source",
        )?;
        let word = attachment.expect("checked attachment word");
        cursor.consume(&word)?;
        let noun = attached_noun.expect("checked attached noun");
        cursor.consume(&noun)?;
        // The attached noun is the subject's type (`enchanted land`); it is
        // the proof a land-only change such as `is a Swamp` requires.
        let noun_type = lookup_permanent_card_type(&noun, AcceptedForms::SingularOnly)
            .map(ObjectPredicate::CardType);
        return Ok((
            json!({ "kind": "attachedPermanent", "attachment": self_ref() }),
            noun_type,
            TypeChangeNumber::Singular,
        ));
    }
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        input.name_links,
        input.name_resolution_failure,
    ) {
        cursor.enter("static.continuous.subject.source");
        recognize_source_reference(cursor, input.name_links, input.name_resolution_failure)?;
        return Ok((self_ref(), None, TypeChangeNumber::Singular));
    }
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
    let exclude_source = recognize_optional_source_exclusion(cursor)?;
    let query = recognize_object_collection(cursor)?;
    cursor.require(
        query.predicate.has_domain(ObjectKind::Permanent)
            && query.predicate.supports_domain(ObjectKind::Permanent),
        "a collection anchored in the battlefield-permanent domain",
    )?;
    let mut objects = json!({
        "kind": "permanents",
        "where": lower_permanent_object_predicate(&query.predicate),
    });
    if let Some(scope) = query.controller {
        objects["controller"] = match scope {
            ObjectControllerScope::AbilityController => controller(),
            ObjectControllerScope::OpponentOfAbilityController => {
                json!({ "kind": "opponentsOf", "player": controller() })
            }
            ObjectControllerScope::NotAbilityController => {
                return Err(
                    cursor.fail("a static controller scope representable as a player relation")
                );
            }
        };
    }
    if exclude_source {
        objects["excludeSource"] = json!(true);
    }
    Ok((objects, Some(query.predicate), number))
}

#[cfg(test)]
#[path = "static_continuous_condition_tests.rs"]
mod condition_tests;

#[cfg(test)]
#[path = "static_continuous_predicate_tests.rs"]
mod predicate_tests;

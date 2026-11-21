//! Static continuous-effect frame (CR 611.3, 604.1):
//!
//! ```text
//! Static    := [ability word "—"] ["as long as" Condition ","]
//!              Subject Predicate ("and" Predicate)*
//!              ["as long as" Condition] "." [keyword reminder]
//! Subject   := source reference | "equipped creature" | "enchanted" PERMANENT
//!            | ["each" | "all"] ["other"] permanent collection
//! Predicate := ("get" | "gets") signed P/T delta | ("have" | "has") keyword list
//!            | ("is" | "are") type change
//! ```
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
    if as_long_as_starts(cursor) {
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
    let (objects, subject_predicate, subject_number) = read_subject(cursor, input)?;
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
            "get" | "gets" => {
                cursor.consume(&verb)?;
                *committed = true;
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
            _ => {
                return Err(
                    cursor.fail("`get(s)`, `have`/`has` or `is`/`are` continuous predicate")
                );
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

fn predicate_verb_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        PREDICATE_VERBS
            .iter()
            .any(|verb| raw.eq_ignore_ascii_case(verb))
    })
}

/// `, VERB` or `, and VERB` continues the predicate list of the same subject.
fn serial_predicate_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    predicate_verb_at(cursor, 1)
        || (cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
            && predicate_verb_at(cursor, 2))
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
fn read_subject(
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

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    EffectSequenceNode, ObjectControllerScope, ObjectKind, ObjectQueryNode, recognize_object_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AbilityWordPrefixNode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentEnteredEnvelopeNode {
    pub label: Option<AbilityWordPrefixNode>,
    pub object: ObjectQueryNode,
    pub exclude_source: bool,
    pub optional: bool,
    pub body_start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PermanentEnteredEffectSequenceNode {
    pub envelope: PermanentEnteredEnvelopeNode,
    pub effects: EffectSequenceNode,
}

fn recognize_optional_ability_word(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<AbilityWordPrefixNode>> {
    if !cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "abilityWord")
    {
        return Ok(None);
    }
    cursor.enter("trigger.permanent-entered.label");
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "abilityWord")
    {
        cursor.consume_type("abilityWord")?;
    }
    cursor.enter("trigger.permanent-entered.label-separator");
    let separator = cursor.consume_type("separator")?;
    cursor.require(
        separator.raw == "\u{2014}",
        "an em dash after the ability word",
    )?;
    Ok(Some(AbilityWordPrefixNode))
}

/// Reads only the reusable trigger envelope. The body is deliberately left to
/// `EffectSequence`, allowing dispatch to distinguish an unowned first action
/// from a malformed action that a native child already started consuming.
pub(in crate::oracle::canonical) fn recognize_permanent_entered_envelope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PermanentEnteredEnvelopeNode> {
    let label = recognize_optional_ability_word(cursor)?;
    cursor.enter("trigger.permanent-entered.trigger");
    let trigger = cursor.consume_type("triggerWord")?;
    cursor.require(
        trigger.raw.eq_ignore_ascii_case("when") || trigger.raw.eq_ignore_ascii_case("whenever"),
        "a `when` or `whenever` trigger",
    )?;

    cursor.enter("trigger.permanent-entered.quantifier");
    let exclude_source = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("another"));
    if exclude_source {
        cursor.consume("another")?;
    } else {
        let article = cursor
            .peek()
            .ok_or_else(|| cursor.fail("`a` or `an` before the permanent selector"))?
            .to_string();
        cursor.require(
            article.eq_ignore_ascii_case("a") || article.eq_ignore_ascii_case("an"),
            "`a` or `an` before the permanent selector",
        )?;
        cursor.consume(&article)?;
    }

    cursor.enter("trigger.permanent-entered.object");
    let object = recognize_object_query(cursor)?;
    cursor.require(
        object.predicate.supports_domain(ObjectKind::Permanent)
            && object.predicate.has_nominal_head(),
        "an object query with a noun head valid in the battlefield-permanent domain",
    )?;
    cursor.require(
        object.controller != Some(ObjectControllerScope::NotAbilityController),
        "a positive or omitted controller relation",
    )?;

    cursor.enter("trigger.permanent-entered.event");
    let event = cursor.consume_type("eventWord")?;
    cursor.require(
        event.raw.eq_ignore_ascii_case("enters"),
        "the `enters` event",
    )?;
    cursor.enter("trigger.permanent-entered.separator");
    cursor.consume(",")?;

    cursor.enter("trigger.permanent-entered.optionality");
    let optional = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor
            .peek_ahead(1)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("may"));
    if optional {
        cursor.consume("you")?;
        cursor.consume("may")?;
    }

    Ok(PermanentEnteredEnvelopeNode {
        label,
        object,
        exclude_source,
        optional,
        body_start: cursor.position(),
    })
}

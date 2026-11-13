use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_counter_prohibition_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let self_spell = tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("this"))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("spell"));
    let controlled_spell_set = tokens.iter().any(|token| {
        token.raw.eq_ignore_ascii_case("spell") || token.raw.eq_ignore_ascii_case("spells")
    }) && tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("you") && window[1].raw.eq_ignore_ascii_case("control")
    });
    (self_spell || controlled_spell_set)
        && tokens.windows(3).any(|window| {
            (window[0].raw.eq_ignore_ascii_case("can't")
                || window[0].raw.eq_ignore_ascii_case("cannot"))
                && window[1].raw.eq_ignore_ascii_case("be")
                && window[2].raw.eq_ignore_ascii_case("countered")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_counter_prohibition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("counter-prohibition", input.tokens);
    let prohibition = recognize_counter_prohibition(&mut cursor)?;
    let (active_while, modifier) = match prohibition.subject {
        CounterProhibitionSubjectNode::SourceSelf(LocalObjectReference::SourceSelf) => (
            json!({
                "kind": "inZone",
                "object": self_ref(),
                "zone": { "kind": "stack" },
            }),
            json!({
                "kind": "cantBeCountered",
                "object": self_ref(),
            }),
        ),
        CounterProhibitionSubjectNode::ObjectSet(subject) => {
            cursor.enter("counter-prohibition.subject.validation");
            cursor.require(
                subject.controller == Some(ObjectControllerScope::AbilityController),
                "a spell set controlled by the ability controller",
            )?;
            cursor.require(
                subject.predicate.supports_domain(ObjectKind::Spell)
                    && subject.predicate.has_domain(ObjectKind::Spell),
                "a selector in the spell object domain",
            )?;
            let mut objects = json!({
                "kind": "spells",
                "controller": controller(),
            });
            if let Some(filter) = lower_spell_object_predicate(&subject.predicate) {
                objects["where"] = filter;
            }
            (
                active_while_battlefield(),
                json!({
                    "kind": "cantBeCountered",
                    "objects": objects,
                }),
            )
        }
    };
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while,
            "modifiers": [modifier],
        }),
        &[
            "Recognize a source spell or controlled spell set from the shared token stream",
            "Recognize the passive counter prohibition",
            "Preserve the existing atomic cantBeCountered modifier",
            &trace,
        ],
    ))
}

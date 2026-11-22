use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum TriggerMultiplicityParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_trigger_multiplicity(
    input: &AbilityInput<'_>,
) -> TriggerMultiplicityParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line)
        || !["if", "a", "triggered", "ability"]
            .iter()
            .enumerate()
            .all(|(i, word)| {
                input
                    .tokens
                    .get(i)
                    .is_some_and(|token| token.raw.eq_ignore_ascii_case(word))
            })
    {
        return TriggerMultiplicityParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("trigger-multiplicity", input.tokens);
    let result = (|| {
        let node = recognize_trigger_multiplicity(&mut cursor)?;
        cursor.enter("trigger-multiplicity.source-domain");
        let query = &node.sources.objects;
        cursor.require(
            query.predicate.supports_domain(ObjectKind::Permanent)
                && domain_filter_is_representable(&query.predicate),
            "a representable permanent-source predicate",
        )?;
        cursor.require(
            !query.predicate.contains_numeric_comparison(),
            "a source characteristic predicate without an unbound numeric comparison",
        )?;
        let mut sources =
            json!({"kind":"permanents","where":lower_permanent_object_predicate(&query.predicate)});
        if let Some(scope) = query.controller {
            sources["controller"] = lower_object_controller_scope(scope);
        }
        if let Some(scope) = node.sources.ownership {
            cursor.require(
                !matches!(
                    scope,
                    ObjectOwnershipScope::OpponentOfAbilityController
                        | ObjectOwnershipScope::EventPlayer
                ),
                "ownership relative to the ability controller, not an unsupported owner player-set",
            )?;
            sources["ownership"] = json!(match scope {
                ObjectOwnershipScope::AbilityController => "owned",
                ObjectOwnershipScope::NotAbilityController => "notOwned",
                ObjectOwnershipScope::OpponentOfAbilityController
                | ObjectOwnershipScope::EventPlayer => unreachable!("validated owner relation"),
            });
        }
        Ok(draft(
            json!({"kind":"staticAbility","source":self_ref(),"activeWhile":active_while_battlefield(),
            "modifiers":[{"kind":"multiplyTriggeredAbility","sources":sources,"additionalTriggers":node.additional}]}),
            &[
                "Read a trigger-occurrence modification envelope",
                "Compose the source query with typed controller and owner relations",
                "Reuse the generic trigger multiplicity modifier",
            ],
        ))
    })();
    match result {
        Ok(rule) => TriggerMultiplicityParseAttempt::Parsed(rule),
        Err(error) => TriggerMultiplicityParseAttempt::Failed(error),
    }
}

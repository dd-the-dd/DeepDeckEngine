use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticCollectionKeywordParseAttempt {
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
    NotClaimed,
}

/// A parse-once late static adapter. More specific families retain priority;
/// temporal, conditional, and multi-sentence suffixes are never approximated
/// as an unconditional permanent grant.
pub(in crate::oracle::canonical) fn try_parse_promoted_static_collection_keyword_grant(
    input: &AbilityInput<'_>,
) -> StaticCollectionKeywordParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticCollectionKeywordParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-collection-keyword", input.tokens);
    let grant = match try_recognize_static_collection_keyword_grant(&mut cursor) {
        StaticCollectionKeywordRecognition::Parsed(grant) => grant,
        StaticCollectionKeywordRecognition::NotClaimed
        | StaticCollectionKeywordRecognition::UnsupportedEnvelope(_) => {
            return StaticCollectionKeywordParseAttempt::NotClaimed;
        }
        StaticCollectionKeywordRecognition::Failed(failure) => {
            return StaticCollectionKeywordParseAttempt::Failed(failure);
        }
    };
    let Some(mut objects) = lower_permanent_collection(&grant.objects.predicate) else {
        return StaticCollectionKeywordParseAttempt::Failed(
            cursor.fail("an ownership relation a permanent collection can carry"),
        );
    };
    if let Some(scope) = grant.objects.controller {
        cursor.enter("static.collection-keyword.lower.controller");
        objects["controller"] = match scope {
            ObjectControllerScope::AbilityController => controller(),
            ObjectControllerScope::OpponentOfAbilityController => {
                json!({ "kind": "opponentsOf", "player": controller() })
            }
            ObjectControllerScope::NotAbilityController
            | ObjectControllerScope::EventPlayer
            | ObjectControllerScope::Player(_) => {
                return StaticCollectionKeywordParseAttempt::Failed(cursor.fail(
                    "a static controller scope representable without changing player relations",
                ));
            }
        };
    }
    if grant.exclude_source {
        objects["excludeSource"] = json!(true);
    }
    if grant
        .keywords
        .iter()
        .any(|keyword| matches!(keyword, KeywordAtomNode::ManaCost(_)))
    {
        return StaticCollectionKeywordParseAttempt::Failed(cursor.fail(
            "a granted keyword the engine reads from a static grant (a mana-cost keyword is read from the card itself)",
        ));
    }
    let modifiers = grant
        .keywords
        .into_iter()
        .map(|keyword| match keyword {
            KeywordAtomNode::Parameterless(keyword) => json!({
                "kind":"grantKeyword","objects":objects.clone(),"keyword":keyword.canonical_name(),
            }),
            KeywordAtomNode::Ward { cost } => json!({
                "kind":"grantWard","objects":objects.clone(),
                "cost":lower_resource_payment(&cost, ResourcePaymentPayer::WardTargetingPlayer),
            }),
            KeywordAtomNode::Protection { qualities } => json!({
                "kind":"grantProtection","objects":objects.clone(),
                "from":lower_protection_qualities(&qualities),
            }),
            KeywordAtomNode::Quoted(_) => {
                unreachable!("this parent never enables quoted keyword atoms")
            }
            KeywordAtomNode::ManaCost(_) => {
                unreachable!("mana-cost keyword grants are rejected before lowering")
            }
        })
        .collect::<Vec<_>>();
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    StaticCollectionKeywordParseAttempt::Parsed(draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": modifiers,
        }),
        &[
            "Recognize one non-targeting permanent collection through the shared ObjectQuery grammar",
            "Delegate keyword atoms, reminders and list connectors to the shared keyword child",
            "Keep object alternatives separate from independent granted keywords",
            "Lower each keyword to one battlefield-active grantKeyword modifier",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "static_collection_keyword_tests.rs"]
mod tests;

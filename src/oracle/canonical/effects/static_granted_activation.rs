use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticGrantedActivationParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_static_granted_activation(
    input: &AbilityInput<'_>,
) -> StaticGrantedActivationParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticGrantedActivationParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-granted-activation", input.tokens);
    let grant = match try_recognize_static_collection_activated_grant(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        StaticCollectionActivatedGrantAttempt::NotClaimed => {
            return StaticGrantedActivationParseAttempt::NotClaimed;
        }
        StaticCollectionActivatedGrantAttempt::Failed(failure) => {
            return StaticGrantedActivationParseAttempt::Failed(failure);
        }
        StaticCollectionActivatedGrantAttempt::Parsed(grant) => grant,
    };
    let mut objects = json!({"kind":"permanents","where":lower_permanent_object_predicate(&grant.collection.objects.predicate)});
    if let Some(scope) = grant.collection.objects.controller {
        objects["controller"] = lower_object_controller_scope(scope);
    }
    if grant.collection.exclude_source {
        objects["excludeSource"] = json!(true);
    }
    let ability = json!({"kind":"manaAbility","source":self_ref(),
        "costs":grant.ability.costs.costs.iter().flat_map(lower_local_activation_costs).collect::<Vec<_>>(),
        "effects":[lower_add_mana_effect(&grant.ability.effect)]});
    StaticGrantedActivationParseAttempt::Parsed(draft(
        json!({
            "kind":"staticAbility","source":self_ref(),"activeWhile":active_while_battlefield(),
            "modifiers":[{"kind":"grantActivatedAbility","objects":objects,"ability":ability}]
        }),
        &[
            "Read the permanent collection through the shared object-query automaton",
            "Read the bounded quoted activation from the original token stream",
            "Nest the ordinary mana ability in the generic activated-ability grant",
        ],
    ))
}

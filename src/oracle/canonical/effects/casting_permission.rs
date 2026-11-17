//! Lower the typed future permission to the existing generic static modifier.
use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

pub(in crate::oracle::canonical) enum CastingPermissionProgramAttempt {
    NotClaimed,
    Failed(MachineFailure),
    Parsed(CanonicalRuleDraft),
}
pub(in crate::oracle::canonical) fn try_parse_casting_permission(
    input: &AbilityInput<'_>,
) -> CastingPermissionProgramAttempt {
    // Spell instructions containing `you may cast` belong to resolution parents.
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return CastingPermissionProgramAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-casting-permission", input.tokens);
    let node = match try_recognize_casting_permission(&mut cursor) {
        CastingPermissionRecognition::Parsed(node) => node,
        CastingPermissionRecognition::NotClaimed
        | CastingPermissionRecognition::RejectedEnvelope(_) => {
            return CastingPermissionProgramAttempt::NotClaimed;
        }
        CastingPermissionRecognition::FailedCommitted(f) => {
            return CastingPermissionProgramAttempt::Failed(f);
        }
    };
    match lower_casting_permission(&mut cursor, node) {
        Ok(rule) => CastingPermissionProgramAttempt::Parsed(rule),
        Err(f) => CastingPermissionProgramAttempt::Failed(f),
    }
}
fn lower_casting_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingPermissionNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("permission.cast.zone-capability");
    cursor.require(
        matches!(
            node.zone,
            CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
        ),
        "a runtime-supported casting zone: hand, graveyard or exile",
    )?;
    cursor.enter("permission.cast.waived-base-zone-capability");
    cursor.require(
        node.base_cost != CastingBaseCostPolicyNode::WithoutManaCost
            || node.zone == CardZoneName::Hand,
        "waived base cost from hand; free nonhand casting is not yet proved by the public pipeline",
    )?;
    cursor.enter("permission.cast.spell-query-scope");
    cursor.require(
        node.spells.caster.is_none(),
        "the permission actor without a second caster relation",
    )?;
    cursor.enter("permission.cast.predicate-capability");
    cursor.require(
        domain_filter_is_representable(&node.spells.predicate),
        "a spell predicate preserving Boolean domain identities",
    )?;
    cursor.require(
        !object_predicate_uses_x(&node.spells.predicate),
        "a bound static predicate without a declaration-local variable",
    )?;
    let players = match node.actor {
        CastingPermissionActorNode::Controller => controller(),
        CastingPermissionActorNode::Set(PlayerSetReferenceNode::EachPlayer) => {
            json!({"kind":"eachPlayer"})
        }
        CastingPermissionActorNode::Set(PlayerSetReferenceNode::OpponentsOfAbilityController) => {
            json!({"kind":"opponentsOf","player":controller()})
        }
    };
    Ok(draft(
        json!({
            "kind":"staticAbility", "source":self_ref(), "activeWhile":active_while_battlefield(),
            "modifiers":[{
                "kind":"castingPermission", "players":players, "sourceZone":node.zone.ir_name(),
                "where":lower_spell_object_predicate(&node.spells.predicate).unwrap_or(Value::Null),
                "withoutPayingManaCost":node.base_cost == CastingBaseCostPolicyNode::WithoutManaCost,
                "asThoughFlash":false
            }]
        }),
        &[
            "Read the actor, shared spell query, source zone and alternative base-cost policy independently",
            "Preserve normal casting timing; additional costs use the ordinary casting-cost pipeline",
            "A static future permission does not execute optional effects at resolution",
        ],
    ))
}

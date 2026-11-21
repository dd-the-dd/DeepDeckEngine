//! Lower the typed future permission to the existing generic static modifiers.
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
    let lowered = match node {
        PlayerPermissionNode::Act(node) => lower_casting_permission(&mut cursor, node),
        PlayerPermissionNode::LookAtLibraryTopAnyTime { actor } => {
            lower_look_permission(&mut cursor, actor)
        }
    };
    match lowered {
        Ok(rule) => CastingPermissionProgramAttempt::Parsed(rule),
        Err(f) => CastingPermissionProgramAttempt::Failed(f),
    }
}

fn static_permission(modifiers: Vec<Value>, operations: &[&str]) -> CanonicalRuleDraft {
    draft(
        json!({
            "kind":"staticAbility", "source":self_ref(), "activeWhile":active_while_battlefield(),
            "modifiers":modifiers
        }),
        operations,
    )
}

/// The runtime binds these modifiers to the source's controller only.
fn require_controller_actor(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<()> {
    cursor.enter("permission.actor-capability");
    cursor.require(
        actor == CastingPermissionActorNode::Controller,
        "the permanent's controller as the holder of this permission",
    )
}

fn lower_look_permission(
    cursor: &mut OracleTokenCursor<'_>,
    actor: CastingPermissionActorNode,
) -> MachineResult<CanonicalRuleDraft> {
    require_controller_actor(cursor, actor)?;
    Ok(static_permission(
        vec![json!({"kind":"lookAtTopLibrary", "player":controller()})],
        &[
            "Read the actor, the shared library-top operand and the any-time allowance",
            "Grant private knowledge of the top card without moving it",
        ],
    ))
}

fn require_spell_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    spells: &SpellCastQueryNode,
) -> MachineResult<Value> {
    cursor.enter("permission.cast.predicate-capability");
    cursor.require(
        domain_filter_is_representable(&spells.predicate),
        "a spell predicate preserving Boolean domain identities",
    )?;
    cursor.require(
        !object_predicate_uses_x(&spells.predicate),
        "a bound static predicate without a declaration-local variable",
    )?;
    cursor.require(
        !object_predicate_uses_chosen_type(&spells.predicate),
        "a static predicate without a resolution-time chosen type",
    )?;
    Ok(lower_spell_object_predicate(&spells.predicate).unwrap_or(Value::Null))
}

fn object_predicate_uses_chosen_type(predicate: &ObjectPredicate) -> bool {
    match predicate {
        ObjectPredicate::ChosenCreatureType => true,
        ObjectPredicate::Not(child) => object_predicate_uses_chosen_type(child),
        ObjectPredicate::Intersection(children) | ObjectPredicate::Union(children) => {
            children.iter().any(object_predicate_uses_chosen_type)
        }
        _ => false,
    }
}

fn lower_casting_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingPermissionNode,
) -> MachineResult<CanonicalRuleDraft> {
    match node.origin {
        PermissionOriginNode::LibraryTop => lower_library_top_permission(cursor, node),
        PermissionOriginNode::UsualZone
            if node.base_cost == CastingBaseCostPolicyNode::Printed && node.as_though_flash =>
        {
            lower_flash_timing_permission(cursor, node)
        }
        PermissionOriginNode::UsualZone => lower_zone_permission(cursor, node, CardZoneName::Hand),
        PermissionOriginNode::Zone(zone) => lower_zone_permission(cursor, node, zone),
    }
}

/// Spells cast from their usual zone keep every other casting rule; only the
/// timing restriction is lifted (CR 702.8a), optionally for the first
/// matching spell the controller casts each turn.
fn lower_flash_timing_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingPermissionNode,
) -> MachineResult<CanonicalRuleDraft> {
    require_controller_actor(cursor, node.actor)?;
    let mut modifiers = Vec::new();
    for action in &node.actions {
        cursor.enter("permission.flash.action-capability");
        let PermittedActionNode::Cast { spells, scope } = action else {
            return Err(cursor.fail("a cast action for a flash timing permission"));
        };
        cursor.require(
            spells.caster.is_none(),
            "the permission actor without a second caster relation",
        )?;
        let filter = require_spell_predicate(cursor, spells)?;
        modifiers.push(json!({
            "kind":"grantFlashCasting", "player":controller(), "where":filter,
            "firstEachTurn":*scope == PermittedSpellScopeNode::FirstEachTurn
        }));
    }
    Ok(static_permission(
        modifiers,
        &[
            "Read the actor, the shared spell query and its optional spell-history ordinal",
            "Lift only the casting-time restriction; costs and zones are unchanged",
        ],
    ))
}

/// Only the top card is available; lands are played and other cards cast
/// (CR 305.1, 601.2a). A cast permission never admits a land card.
fn lower_library_top_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingPermissionNode,
) -> MachineResult<CanonicalRuleDraft> {
    require_controller_actor(cursor, node.actor)?;
    cursor.enter("permission.library-top.policy-capability");
    cursor.require(
        node.base_cost == CastingBaseCostPolicyNode::Printed && !node.as_though_flash,
        "printed costs and normal timing from the top of a library",
    )?;
    let mut filters = Vec::new();
    for action in &node.actions {
        match action {
            PermittedActionNode::Cast { spells, scope } => {
                cursor.enter("permission.library-top.cast-scope");
                cursor.require(
                    *scope == PermittedSpellScopeNode::Each && spells.caster.is_none(),
                    "every matching spell, without a spell-history ordinal",
                )?;
                let spell = require_spell_predicate(cursor, spells)?;
                let nonland = not(card_type(ObjectCardType::Land.ir_name()));
                filters.push(if spell.is_null() {
                    nonland
                } else {
                    and(vec![nonland, spell])
                });
            }
            PermittedActionNode::Play { cards } => {
                cursor.enter("permission.library-top.play-predicate");
                cursor.require(
                    domain_filter_is_representable(cards),
                    "a card predicate preserving Boolean domain identities",
                )?;
                filters.push(lower_card_object_predicate(cards));
            }
        }
    }
    let filter = if filters.len() == 1 {
        filters.pop().expect("one library-top filter")
    } else {
        or(filters)
    };
    Ok(static_permission(
        vec![json!({"kind":"playCardsFromTopLibrary", "player":controller(), "where":filter})],
        &[
            "Read the actor, each permitted play/cast action and the library-top origin",
            "Compose one card filter: played cards as read, cast cards restricted to nonlands",
        ],
    ))
}

fn lower_zone_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: CastingPermissionNode,
    zone: CardZoneName,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("permission.cast.zone-capability");
    cursor.require(
        matches!(
            zone,
            CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
        ),
        "a runtime-supported casting zone: hand, graveyard or exile",
    )?;
    cursor.enter("permission.cast.waived-base-zone-capability");
    cursor.require(
        node.base_cost != CastingBaseCostPolicyNode::WithoutManaCost || zone == CardZoneName::Hand,
        "waived base cost from hand; free nonhand casting is not yet proved by the public pipeline",
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
    let mut modifiers = Vec::new();
    for action in &node.actions {
        match action {
            PermittedActionNode::Cast { spells, scope } => {
                cursor.enter("permission.cast.spell-query-scope");
                cursor.require(
                    spells.caster.is_none() && *scope == PermittedSpellScopeNode::Each,
                    "the permission actor without a second caster relation",
                )?;
                let filter = require_spell_predicate(cursor, spells)?;
                modifiers.push(json!({
                    "kind":"castingPermission", "players":players, "sourceZone":zone.ir_name(),
                    "where":filter,
                    "withoutPayingManaCost":node.base_cost == CastingBaseCostPolicyNode::WithoutManaCost,
                    "asThoughFlash":node.as_though_flash
                }));
            }
            PermittedActionNode::Play { cards } => {
                // Playing a land from a graveyard (CR 305.1) is a land play;
                // other played cards from a graveyard have no represented route.
                cursor.enter("permission.play.zone-capability");
                cursor.require(
                    zone == CardZoneName::Graveyard
                        && *cards == ObjectPredicate::CardType(ObjectCardType::Land)
                        && node.base_cost == CastingBaseCostPolicyNode::Printed
                        && !node.as_though_flash,
                    "land plays from a graveyard with printed timing",
                )?;
                require_controller_actor(cursor, node.actor)?;
                modifiers.push(json!({"kind":"playLandsFromGraveyard", "player":controller()}));
            }
        }
    }
    Ok(static_permission(
        modifiers,
        &[
            "Read the actor, each permitted action, the shared query, source zone and base-cost policy independently",
            "Preserve normal casting timing; additional costs use the ordinary casting-cost pipeline",
            "A static future permission does not execute optional effects at resolution",
        ],
    ))
}

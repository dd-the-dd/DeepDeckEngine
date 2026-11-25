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
    let mut cursor = OracleTokenCursor::new("static-casting-permission", input.tokens);
    // Spell instructions containing `you may cast` belong to resolution
    // parents. A card's own casting permission is a static ability of any
    // card (CR 113.6), so it is the one kind read on a spell face.
    let kind = static_permission_kind(&cursor, input.name_links, input.name_resolution_failure);
    if !type_line_has_permanent_card_type(input.face_type_line)
        && kind != Some(StaticPermissionKind::SelfCast)
    {
        return CastingPermissionProgramAttempt::NotClaimed;
    }
    let node = match try_recognize_static_player_permission(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        CastingPermissionRecognition::Parsed(node) => node,
        CastingPermissionRecognition::NotClaimed
        | CastingPermissionRecognition::RejectedEnvelope(_) => {
            return CastingPermissionProgramAttempt::NotClaimed;
        }
        CastingPermissionRecognition::FailedCommitted(f) => {
            return CastingPermissionProgramAttempt::Failed(f);
        }
    };
    match lower_player_permission(&mut cursor, node) {
        Ok(rule) => CastingPermissionProgramAttempt::Parsed(rule),
        Err(f) => CastingPermissionProgramAttempt::Failed(f),
    }
}

fn lower_player_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: PlayerPermissionNode,
) -> MachineResult<CanonicalRuleDraft> {
    match node {
        PlayerPermissionNode::Act(node) => lower_casting_permission(cursor, node),
        PlayerPermissionNode::LookAtLibraryTopAnyTime { actor } => {
            lower_look_permission(cursor, actor)
        }
        PlayerPermissionNode::CastSelf(node) => lower_self_cast_permission(cursor, &node),
        PlayerPermissionNode::ExertAsAttacks { reflexive } => {
            lower_exert_permission(cursor, reflexive.as_ref())
        }
        PlayerPermissionNode::AssignCombatDamageAsThoughUnblocked => Ok(static_permission(
            vec![json!({"kind":"assignCombatDamageAsThoughUnblocked", "object":self_ref()})],
            &[
                "Read the source operand and the damage-assignment manner",
                "Offer the source's controller an unblocked assignment when it is blocked",
            ],
        )),
        PlayerPermissionNode::MayChooseNotToUntap => Ok(static_permission(
            vec![json!({"kind":"optionalUntap", "object":self_ref()})],
            &[
                "Read the source operand and the untap step of its controller",
                "Offer the controller the choice to leave the source tapped",
            ],
        )),
        PlayerPermissionNode::AlternativeManaCost { cost, spells } => {
            lower_alternative_mana_cost_permission(cursor, &cost, spells)
        }
        PlayerPermissionNode::Conjoined(nodes) => lower_conjoined_permissions(cursor, nodes),
    }
}

/// Allowances granted by one static ability: each lowers alone, and the
/// ability carries every modifier. Only unconditional battlefield statics
/// combine (a part with its own zone of function or condition is refused).
fn lower_conjoined_permissions(
    cursor: &mut OracleTokenCursor<'_>,
    nodes: Vec<PlayerPermissionNode>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut modifiers = Vec::new();
    let mut operations = Vec::new();
    for node in nodes {
        let draft = lower_player_permission(cursor, node)?;
        cursor.enter("permission.conjunction-capability");
        cursor.require(
            draft.rule["kind"] == "staticAbility"
                && draft.rule["activeWhile"] == active_while_battlefield()
                && draft.rule.get("condition").is_none(),
            "allowances that are unconditional statics of the permanent",
        )?;
        modifiers.extend(
            draft.rule["modifiers"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
        );
        operations.extend(draft.operations);
    }
    let operations = operations.iter().map(String::as_str).collect::<Vec<_>>();
    Ok(static_permission(modifiers, &operations))
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
        ObjectPredicate::ChosenCreatureType | ObjectPredicate::ChosenColor => true,
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
    // A turn window is the zone permission's usage limit; the other routes
    // have no window field.
    cursor.enter("permission.window-capability");
    cursor.require(
        node.window == PermissionTurnWindowNode::Always
            || matches!(node.origin, PermissionOriginNode::Zone(_)),
        "a turn window over a permission to cast from a graveyard or exile",
    )?;
    match node.origin {
        PermissionOriginNode::LibraryTop => lower_library_top_permission(cursor, node),
        // The controller's flash grant keeps its spell-history ordinal; a
        // player set (`Any player may cast ...`) holds the same timing
        // permission through the shared hand-zone permission.
        PermissionOriginNode::UsualZone
            if node.base_cost == CastingBaseCostPolicyNode::Printed
                && node.as_though_flash
                && node.actor == CastingPermissionActorNode::Controller =>
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
            spells.caster.is_none() && *scope != PermittedSpellScopeNode::One,
            "the permission actor without a second caster relation, over each matching spell",
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
                    spells.caster.is_none()
                        && match scope {
                            PermittedSpellScopeNode::Each => true,
                            // One spell per use: the once-per-turn window
                            // counts the uses (CR 611.3a).
                            PermittedSpellScopeNode::One => {
                                node.window == PermissionTurnWindowNode::OnceDuringYourTurns
                            }
                            PermittedSpellScopeNode::FirstEachTurn => false,
                        },
                    "the permission actor without a second caster relation",
                )?;
                let filter = require_spell_predicate(cursor, spells)?;
                let mut modifier = json!({
                    "kind":"castingPermission", "players":players, "sourceZone":zone.ir_name(),
                    "where":filter,
                    "withoutPayingManaCost":node.base_cost == CastingBaseCostPolicyNode::WithoutManaCost,
                    "asThoughFlash":node.as_though_flash
                });
                if node.window != PermissionTurnWindowNode::Always {
                    modifier["duringControllerTurnOnly"] = json!(true);
                }
                if node.window == PermissionTurnWindowNode::OnceDuringYourTurns {
                    modifier["onceEachTurn"] = json!(true);
                }
                modifiers.push(modifier);
            }
            PermittedActionNode::Play { cards } => {
                // Playing a land from a graveyard (CR 305.1) is a land play;
                // other played cards from a graveyard have no represented route.
                cursor.enter("permission.play.zone-capability");
                cursor.require(
                    zone == CardZoneName::Graveyard
                        && *cards == ObjectPredicate::CardType(ObjectCardType::Land)
                        && node.base_cost == CastingBaseCostPolicyNode::Printed
                        && !node.as_though_flash
                        && node.window == PermissionTurnWindowNode::Always,
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

/// The card's own casting permission (CR 601.3): a static ability that
/// functions from the zone it names (CR 113.6), or from the hand for the
/// timing manner alone (CR 702.8a). Its condition is the shared static
/// condition, checked where the permission functions.
fn lower_self_cast_permission(
    cursor: &mut OracleTokenCursor<'_>,
    node: &SelfCastPermissionNode,
) -> MachineResult<CanonicalRuleDraft> {
    let condition = node
        .condition
        .as_ref()
        .map(|condition| {
            lower_sentence_condition(
                cursor,
                condition,
                &ConditionObjectBindings {
                    permanent_targets: &[],
                    card_targets: &[],
                    card_focus: None,
                    previous_action_result: None,
                },
            )
        })
        .transpose()?;
    let (zone, modifier) = match node.origin {
        Some(zone) => {
            cursor.enter("permission.self-cast.manner-capability");
            cursor.require(
                !node.as_though_flash && node.timing_cost.is_none(),
                "printed timing for a card cast from its own permission zone",
            )?;
            (
                zone.ir_name(),
                json!({
                    "kind":"castingPermission", "players":controller(),
                    "sourceZone":zone.ir_name(), "where":Value::Null, "object":self_ref(),
                    "withoutPayingManaCost":false, "asThoughFlash":false
                }),
            )
        }
        None => {
            let mut modifier = json!({"kind":"castAsThoughFlash", "object":self_ref()});
            if let Some(cost) = &node.timing_cost {
                // CR 601.2f: an increase paid only when the permission is
                // what lets the spell be cast at that time.
                modifier["additionalManaCost"] = json!(cost.as_oracle_cost());
            }
            (CardZoneName::Hand.ir_name(), modifier)
        }
    };
    let mut rule = json!({
        "kind":"staticAbility", "source":self_ref(),
        "activeWhile":{"kind":"inZone", "object":self_ref(), "zone":{"kind":zone}},
        "modifiers":[modifier],
    });
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    Ok(draft(
        rule,
        &[
            "Read the source operand, its origin zone, timing manner and condition independently",
            "Function from that zone; costs and other casting rules are unchanged",
        ],
    ))
}

/// CR 701.43, 508.1g, 603.12: the attack-declaration option, and the
/// reflexive ability it creates when the creature is exerted. The reflexive
/// program is the shared effect program lowered as a triggered body.
fn lower_exert_permission(
    cursor: &mut OracleTokenCursor<'_>,
    reflexive: Option<&EffectProgramNode>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut modifier = json!({"kind":"exertAsAttacks", "object":self_ref()});
    if let Some(program) = reflexive {
        let lowered = lower_effect_program(cursor, program)?;
        cursor.enter("permission.exert.reflexive-lowering");
        cursor.require(
            !lowered.effects.is_empty(),
            "at least one executable reflexive action",
        )?;
        let mut ability = json!({
            "kind":"triggeredAbility", "source":self_ref(),
            "event":{"kind":"reflexiveTriggerCreated", "object":self_ref()},
            "effects":lowered.effects,
        });
        if !lowered.decisions.is_empty() {
            ability["declaration"] =
                json!({"kind":"castingDeclaration", "decisions":lowered.decisions});
        }
        modifier["reflexive"] = ability;
    }
    Ok(static_permission(
        vec![modifier],
        &[
            "Read the source operand and the attack-declaration timing",
            "Compose the optional reflexive program with the shared effect program",
        ],
    ))
}

/// CR 118.9: an alternative cost for each matching spell its controller
/// casts from hand. The base cost is waived and the alternative mana is the
/// cost the static permission names.
fn lower_alternative_mana_cost_permission(
    cursor: &mut OracleTokenCursor<'_>,
    cost: &ManaCostNode,
    spells: ObjectPredicate,
) -> MachineResult<CanonicalRuleDraft> {
    let filter = require_spell_predicate(
        cursor,
        &SpellCastQueryNode {
            predicate: spells,
            caster: None,
        },
    )?;
    Ok(static_permission(
        vec![json!({
            "kind":"castingPermission", "players":controller(), "sourceZone":"hand",
            "where":filter, "withoutPayingManaCost":true, "asThoughFlash":false,
            "alternativeManaCost":cost.as_oracle_cost(),
        })],
        &[
            "Read the alternative mana cost and the shared spell query of the controller's casts",
            "Waive the printed mana cost and pay the named mana instead",
        ],
    ))
}

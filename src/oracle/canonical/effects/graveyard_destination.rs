use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_entry_zone_batch_family(
    input: &AbilityInput<'_>,
) -> bool {
    input.tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord" && token.raw.eq_ignore_ascii_case("when")
    }) && input
        .tokens
        .iter()
        .any(|token| token.token_type == "eventWord" && token.raw.eq_ignore_ascii_case("enters"))
        && input.tokens.windows(3).any(|window| {
            window[0].token_type == "actionWord"
                && is_verb_form(&window[0].raw, VerbLemma::Exile, VerbForm::Base)
                && window[1].token_type == "quantifierWord"
                && window[1].raw.eq_ignore_ascii_case("all")
                && window[2].token_type == "zoneWord"
                && window[2].raw.eq_ignore_ascii_case("graveyards")
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_entry_zone_batch(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("entry-zone-batch", input.tokens);
    let _node = recognize_source_entry_exile_all_graveyards(&mut cursor)?;
    cursor.enter("entry-zone-batch.generic-movement-unavailable");
    Err(cursor.fail("a proved generic zone collection and moveCards implementation"))
}

pub(in crate::oracle::canonical) fn claims_promoted_graveyard_destination_replacement_family(
    input: &AbilityInput<'_>,
) -> bool {
    if !input.tokens.first().is_some_and(|token| {
        token.token_type == "logicWord" && token.raw.eq_ignore_ascii_case("if")
    }) {
        return false;
    }
    // The historical card/token envelope keeps its claim.
    let historical = input
        .tokens
        .iter()
        .any(|token| token.token_type == "replacementModalWord")
        && input.tokens.iter().any(|token| {
            token.token_type == "zoneWord" && token.raw.eq_ignore_ascii_case("graveyard")
        })
        && input.tokens.iter().any(|token| {
            token.token_type == "logicWord" && token.raw.eq_ignore_ascii_case("instead")
        });
    if historical {
        let mut cursor = OracleTokenCursor::new("graveyard-destination-replacement", input.tokens);
        if recognize_graveyard_destination_replacement(&mut cursor).is_ok() {
            return true;
        }
    }
    // The shared departure frame claims the ability once its subject,
    // `would` and departure verb are read (a committed failure or a match).
    let mut cursor = OracleTokenCursor::new("departure-replacement", input.tokens);
    !matches!(
        try_recognize_departure_replacement(
            &mut cursor,
            DepartureFrame::Ability,
            input.name_links,
            input.name_resolution_failure,
        ),
        ChildOutcome::NoMatch(_)
    )
}

pub(in crate::oracle::canonical) fn parse_promoted_graveyard_destination_replacement(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("graveyard-destination-replacement", input.tokens);
    let historical = match recognize_graveyard_destination_replacement(&mut cursor) {
        Ok(node) => return Ok(lower_historical_graveyard_destination(&cursor, node)),
        Err(failure) => failure,
    };
    let mut cursor = OracleTokenCursor::new("departure-replacement", input.tokens);
    match try_recognize_departure_replacement(
        &mut cursor,
        DepartureFrame::Ability,
        input.name_links,
        input.name_resolution_failure,
    ) {
        ChildOutcome::Matched(node) => {
            lower_static_departure_replacement(&mut cursor, input, &node)
        }
        ChildOutcome::Committed(failure) => Err(failure),
        ChildOutcome::NoMatch(_) => Err(historical),
    }
}

fn lower_historical_graveyard_destination(
    cursor: &OracleTokenCursor<'_>,
    node: GraveyardDestinationReplacementNode,
) -> CanonicalRuleDraft {
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let where_filter = match node.object_kind {
        GraveyardObjectKindNode::Card => not(json!({ "kind": "isToken" })),
        GraveyardObjectKindNode::Token => json!({ "kind": "isToken" }),
        GraveyardObjectKindNode::CardOrToken => Value::Null,
    };
    let graveyard_owners = lower_graveyard_owners(node.graveyard_owners);
    let from = match node.from {
        ZoneOriginNode::AnyZone => json!({ "kind": "anyZone" }),
    };
    let to = match node.to {
        ZoneDestinationNode::Exile => json!({ "kind": "exile" }),
    };

    draft(
        json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "replaceGraveyardDestination",
                "graveyardOwners": graveyard_owners,
                "where": where_filter,
                "from": from,
                "to": to,
            }],
        }),
        &[
            "Recognize card/token applicability independently",
            "Resolve the graveyard owner scope",
            "Resolve origin and replacement destination as typed zones",
            "Install one graveyard-destination replacement modifier",
            &trace,
        ],
    )
}

fn lower_graveyard_owners(scope: GraveyardOwnerScopeNode) -> Value {
    match scope {
        GraveyardOwnerScopeNode::EachPlayer => json!({ "kind": "eachPlayer" }),
        GraveyardOwnerScopeNode::Controller => controller(),
        GraveyardOwnerScopeNode::Opponents => json!({
            "kind": "opponentsOf",
            "player": controller(),
        }),
    }
}

/// `If SUBJECT would die|be put into a graveyard, MOVE instead.` as a
/// battlefield-active static replacement (CR 614.1a), with its reflexive
/// `When you do` ability (CR 603.12).
fn lower_static_departure_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    node: &DepartureReplacementNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("lower.departure-replacement.static");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent source face for a battlefield-active replacement",
    )?;
    cursor.require(
        node.lifetime.is_none(),
        "a static replacement without a created lifetime",
    )?;
    let mut modifier = lower_departure_replacement_modifier(cursor, node)?;
    if let Some(reflexive) = &node.reflexive {
        cursor.enter("lower.departure-replacement.reflexive");
        let lowered = lower_effect_program(cursor, reflexive)?;
        cursor.require(
            !lowered.effects.is_empty(),
            "a reflexive ability with at least one instruction",
        )?;
        let mut ability = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
            "effects": lowered.effects,
        });
        if !lowered.decisions.is_empty() {
            ability["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": lowered.decisions,
            });
        }
        modifier["onReplaced"] = ability;
    }
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
            "activeWhile": active_while_battlefield(),
            "modifiers": [modifier],
        }),
        &[
            "Read the departing object with the shared object query and its relations",
            "Read the departure event (dying or a move to a graveyard) and its origin",
            "Read the replacing move of the event's object",
            "Install one graveyard-destination replacement modifier",
            &trace,
        ],
    ))
}

/// `If SUBJECT would die this turn, MOVE instead.` read as one sentence of
/// a resolving effect: an effect-created replacement through this turn
/// (CR 611.2a, 614.1a). `dealt damage this way` restricts it to the objects
/// the resolving spell dealt damage to (CR 608.2c).
pub(in crate::oracle::canonical) fn lower_created_departure_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    node: &DepartureReplacementNode,
) -> MachineResult<Value> {
    cursor.enter("lower.departure-replacement.created");
    cursor.require(
        node.lifetime == Some(CreatedRuleLifetime::ThisTurn),
        "a created replacement lasting this turn (`this turn` or `Until end of turn,`)",
    )?;
    cursor.require(
        node.reflexive.is_none(),
        "a created replacement without a reflexive ability",
    )?;
    let DepartureSubjectNode::Objects(objects) = &node.subject else {
        return Err(
            cursor.fail("an object query naming the objects the created replacement follows")
        );
    };
    let mut effect = json!({
        "kind": "installGraveyardDestinationReplacement",
        "replacement": lower_departure_replacement_modifier(cursor, node)?,
        "duration": { "kind": "untilEndOfCurrentTurn" },
    });
    if objects.damaged_this_way {
        effect["objects"] = json!({ "kind": "damagedThisResolution" });
    }
    Ok(effect)
}

/// The `replaceGraveyardDestination` modifier shape shared by the static
/// and the effect-created replacement.
fn lower_departure_replacement_modifier(
    cursor: &mut OracleTokenCursor<'_>,
    node: &DepartureReplacementNode,
) -> MachineResult<Value> {
    cursor.enter("lower.departure-replacement.event");
    let (graveyard_owners, origin) = match node.event {
        DepartureEventNode::Dies => (
            GraveyardOwnerScopeNode::EachPlayer,
            DepartureOriginNode::Battlefield,
        ),
        DepartureEventNode::PutIntoGraveyard { graveyard, origin } => (graveyard, origin),
    };
    let mut modifier = json!({
        "kind": "replaceGraveyardDestination",
        "graveyardOwners": lower_graveyard_owners(graveyard_owners),
        "to": lower_replacement_destination(node.destination),
    });
    let from_battlefield = match &node.subject {
        DepartureSubjectNode::Source => {
            modifier["object"] = self_ref();
            // A permanent's own move from an unstated zone leaves the
            // battlefield; `from anywhere` functions in every zone.
            origin != DepartureOriginNode::Anywhere
        }
        DepartureSubjectNode::Objects(objects) => {
            let from_battlefield = match (objects.domain, origin) {
                (ObjectKind::Permanent, DepartureOriginNode::Anywhere) => {
                    return Err(cursor.fail(
                        "a card noun for an object that can move to a graveyard from anywhere",
                    ));
                }
                (ObjectKind::Permanent, _) | (_, DepartureOriginNode::Battlefield) => true,
                _ => false,
            };
            lower_departure_objects(cursor, objects, from_battlefield, &mut modifier)?;
            from_battlefield
        }
    };
    modifier["from"] = if from_battlefield {
        json!({ "kind": "battlefield" })
    } else {
        json!({ "kind": "anyZone" })
    };
    Ok(modifier)
}

fn lower_replacement_destination(destination: ReplacementDestinationNode) -> Value {
    match destination {
        ReplacementDestinationNode::Exile => json!({ "kind": "exile" }),
        ReplacementDestinationNode::OwnerHand => json!({ "kind": "ownerHand" }),
        ReplacementDestinationNode::OwnerLibraryTop => {
            json!({ "kind": "ownerLibrary", "position": "top" })
        }
        ReplacementDestinationNode::OwnerLibraryBottom => {
            json!({ "kind": "ownerLibrary", "position": "bottom" })
        }
        ReplacementDestinationNode::OwnerLibraryShuffle { revealed } => {
            json!({ "kind": "ownerLibrary", "position": "shuffle", "revealed": revealed })
        }
    }
}

/// The departing objects' filter and player relations. Permanents are read
/// with their effective characteristics while still on the battlefield;
/// cards by their printed ones.
fn lower_departure_objects(
    cursor: &mut OracleTokenCursor<'_>,
    objects: &DepartureObjectsNode,
    from_battlefield: bool,
    modifier: &mut Value,
) -> MachineResult<()> {
    cursor.enter("lower.departure-replacement.objects");
    let (predicate, damaged_by_source) = split_damaged_by_source(&objects.predicate);
    if from_battlefield {
        let filter = lower_permanent_object_predicate(&predicate);
        cursor.require(
            !filter.is_null(),
            "a permanent predicate the shared object filter lowers",
        )?;
        modifier["permanentWhere"] = filter;
    } else {
        let filter = lower_card_object_predicate(&predicate);
        cursor.require(
            !filter.is_null(),
            "a card predicate the shared object filter lowers",
        )?;
        modifier["where"] = filter;
    }
    if damaged_by_source {
        // `dealt damage by this creature this turn` (CR 120.1).
        cursor.require(
            from_battlefield,
            "a permanent dealt damage by the source this turn",
        )?;
        modifier["damagedBySourceThisTurn"] = json!(true);
    }
    cursor.enter("lower.departure-replacement.controller");
    if let Some(scope) = objects.controller {
        cursor.require(
            from_battlefield,
            "a controller relation of a permanent that leaves the battlefield",
        )?;
        cursor.require(
            matches!(
                scope,
                ObjectControllerScope::AbilityController
                    | ObjectControllerScope::OpponentOfAbilityController
            ),
            "`you control` or `an opponent controls` relative to the replacement's controller",
        )?;
        modifier["controller"] = lower_object_controller_scope(scope);
    }
    cursor.enter("lower.departure-replacement.owner");
    if let Some(ownership) = objects.ownership {
        modifier["owner"] = match ownership {
            ObjectOwnershipScope::AbilityController => controller(),
            ObjectOwnershipScope::OpponentOfAbilityController => json!({
                "kind": "opponentsOf",
                "player": controller(),
            }),
            ObjectOwnershipScope::NotAbilityController | ObjectOwnershipScope::EventPlayer => {
                return Err(cursor.fail(
                    "`you own` or `an opponent owns` relative to the replacement's controller",
                ));
            }
        };
    }
    if objects.exclude_source {
        modifier["excludeSource"] = json!(true);
    }
    Ok(())
}

/// `dealt damage by SOURCE this turn` read as a conjunct of the query: the
/// replacement checks it against the turn's damage records instead of the
/// object filter.
fn split_damaged_by_source(predicate: &ObjectPredicate) -> (ObjectPredicate, bool) {
    let damaged = |child: &ObjectPredicate| {
        matches!(
            child,
            ObjectPredicate::Qualifier(ObjectQualifier::DamagedBySourceThisTurn)
        )
    };
    match predicate {
        ObjectPredicate::Intersection(children) if children.iter().any(damaged) => {
            let mut rest = children
                .iter()
                .filter(|child| !damaged(child))
                .cloned()
                .collect::<Vec<_>>();
            let rest = match rest.len() {
                0 => ObjectPredicate::ObjectKind(ObjectKind::Permanent),
                1 => rest.remove(0),
                _ => ObjectPredicate::Intersection(rest),
            };
            (rest, true)
        }
        other if damaged(other) => (ObjectPredicate::ObjectKind(ObjectKind::Permanent), true),
        other => (other.clone(), false),
    }
}

/// One program step creating a departure replacement: its one install
/// instruction, without declarations.
pub(in crate::oracle::canonical) fn lower_created_departure_replacement_step(
    cursor: &mut OracleTokenCursor<'_>,
    replacement: &DepartureReplacementNode,
) -> MachineResult<LoweredEffectSequence> {
    Ok(LoweredEffectSequence {
        decisions: Vec::new(),
        effects: vec![lower_created_departure_replacement(cursor, replacement)?],
    })
}

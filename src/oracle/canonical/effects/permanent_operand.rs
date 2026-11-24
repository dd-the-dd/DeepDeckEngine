use super::super::*;

/// Preserve a source reference exactly, or request the collection once at
/// resolution. A quantifier never creates a target declaration or a prompt.
pub(in crate::oracle::canonical) fn lower_nontargeted_permanent_operand(
    operand: &PermanentOperandNode,
) -> Result<Value, TargetlessLoweringError> {
    match operand {
        PermanentOperandNode::Targets(_) | PermanentOperandNode::BoundReference(_) => {
            Err(TargetlessLoweringError::RequiresTarget)
        }
        PermanentOperandNode::Reference(LocalObjectReference::SourceSelf) => Ok(self_ref()),
        PermanentOperandNode::Collection {
            objects,
            exclude_source,
        } => {
            let (predicate, controller_filter) = lower_fixed_controller_permanent_query(objects)?;
            let mut object = json!({"kind":"eachPermanent", "where":predicate});
            if let Some(player) = controller_filter {
                object["player"] = player;
            }
            if *exclude_source {
                object["excludeSource"] = json!(true);
            }
            Ok(object)
        }
    }
}

/// Shared predicate/controller capability for non-targeted permanent queries.
///
/// A universal collection (`all|each QUERY`) reads every member's controller,
/// so its controller slot is the one shared player reference of the query:
/// the ability controller, a population (`your opponents control`, CR 102.3
/// `you don't control`) or the player the triggering event records
/// (`defending player controls`). The collection consumer resolves that
/// reference as a population of controllers (`eachPermanent.player`); a
/// consumer that only proves a single player refuses it in its own validator.
pub(in crate::oracle::canonical) fn lower_fixed_controller_permanent_query(
    query: &ObjectQueryNode,
) -> Result<(Value, Option<Value>), TargetlessLoweringError> {
    if object_predicate_uses_x(&query.predicate) {
        return Err(TargetlessLoweringError::UnsupportedCollectionPredicate);
    }
    let controller_filter = query.controller.map(lower_object_controller_scope);
    Ok((
        lower_permanent_object_predicate(&query.predicate),
        controller_filter,
    ))
}

/// The player relation of a lowered non-target permanent query: one player
/// (a choice's candidates can only name one controller) or a population
/// (a universal collection reads every member's controller).
#[derive(Clone, Debug, PartialEq)]
pub(in crate::oracle::canonical) enum LoweredQueryController {
    Player(Value),
    Population(Value),
}

/// The shared non-target permanent query with every controller relation
/// and with numeric comparisons validated in the declaration context:
/// `with toughness X or less` reads X only when the face declares it or a
/// `where X is` definition of the same sentence supplies it (CR 107.3).
pub(in crate::oracle::canonical) fn lower_permanent_query_in_context(
    query: &ObjectQueryNode,
    declarations: TargetlessDeclarationSupport,
) -> Result<(Value, Option<LoweredQueryController>), TargetlessLoweringError> {
    validate_object_predicate_numeric_expressions(&query.predicate, declarations)?;
    let controller = query.controller.map(|scope| match scope {
        ObjectControllerScope::AbilityController
        | ObjectControllerScope::EventPlayer
        | ObjectControllerScope::Player(_) => {
            LoweredQueryController::Player(lower_object_controller_scope(scope))
        }
        ObjectControllerScope::OpponentOfAbilityController
        | ObjectControllerScope::NotAbilityController => {
            LoweredQueryController::Population(lower_object_controller_scope(scope))
        }
    });
    Ok((
        lower_permanent_object_predicate(&query.predicate),
        controller,
    ))
}

/// The placeholder kind a declared possessor (`target opponent controls`)
/// lowers to inside an action. The enclosing effect program replaces it with
/// the player target it declares (`declare_embedded_player_targets`); the
/// engine refuses any placeholder left in a rule.
pub(in crate::oracle::canonical) const EMBEDDED_PLAYER_TARGET: &str = "embeddedPlayerTarget";

/// The player one controller relation names (CR 108.4). A bound player is the
/// value the binding environment proved; a declared player is a placeholder
/// the program declaration pass resolves.
pub(in crate::oracle::canonical) fn lower_related_player(player: RelatedPlayer) -> Value {
    match player {
        // The selector admits only references with a context-free value.
        RelatedPlayer::Bound(reference) => bound_player_reference_value(reference, &[], &[])
            .unwrap_or_else(|| json!({ "kind": "unresolvedRelatedPlayer" })),
        RelatedPlayer::DeclaredTarget {
            binding,
            opponent,
            site,
        } => json!({
            "kind": EMBEDDED_PLAYER_TARGET,
            "binding": binding.0,
            "opponent": opponent,
            "site": site,
        }),
    }
}

/// Declares the player targets that possessors of queries (`tap all
/// creatures target player controls`, `for each creature target opponent
/// controls`) introduced in `effects`, in reading order: one `chooseTargets`
/// decision per possessor (CR 115.1, 601.2c), and each placeholder becomes
/// the chosen target. The declaration is recorded under its action's player
/// binding, so a later `that player` of the program reads the same player.
/// An action that already declared another player under that binding is
/// refused: one action has one declared player.
pub(in crate::oracle::canonical) fn declare_embedded_player_targets(
    effects: &mut [Value],
    decisions: &mut Vec<Value>,
    declared: &mut Vec<(PlayerTargetBinding, String)>,
) -> Result<(), &'static str> {
    fn collect(value: &Value, found: &mut Vec<(usize, bool, usize)>) {
        if value["kind"].as_str() == Some(EMBEDDED_PLAYER_TARGET) {
            if let (Some(binding), Some(opponent), Some(site)) = (
                value["binding"].as_u64(),
                value["opponent"].as_bool(),
                value["site"].as_u64(),
            ) {
                let entry = (binding as usize, opponent, site as usize);
                if !found.contains(&entry) {
                    found.push(entry);
                }
            }
            return;
        }
        match value {
            Value::Array(values) => values.iter().for_each(|value| collect(value, found)),
            Value::Object(values) => values.values().for_each(|value| collect(value, found)),
            _ => {}
        }
    }
    fn replace(value: &mut Value, site: usize, id: &str) {
        if value["kind"].as_str() == Some(EMBEDDED_PLAYER_TARGET)
            && value["site"].as_u64() == Some(site as u64)
        {
            *value = chosen_target(id);
            return;
        }
        match value {
            Value::Array(values) => values.iter_mut().for_each(|value| replace(value, site, id)),
            Value::Object(values) => values
                .values_mut()
                .for_each(|value| replace(value, site, id)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    effects
        .iter()
        .for_each(|effect| collect(effect, &mut found));
    for (binding, opponent, site) in found {
        let key = PlayerTargetBinding {
            effect_index: binding,
        };
        if declared.iter().any(|(declared, _)| *declared == key) {
            return Err("one declared player for one action of the program");
        }
        let selector = TargetSelector::Player(if opponent {
            PlayerPredicate::Opponent
        } else {
            PlayerPredicate::Any
        });
        let (_, candidates) = lower_target_selector_candidates(&selector)
            .ok_or("a supported player target selector")?;
        let used = |id: &str| {
            decisions
                .iter()
                .any(|decision| decision["id"].as_str() == Some(id))
        };
        let mut id = "possessorTargetPlayer".to_string();
        let mut ordinal = 1_usize;
        while used(&id) {
            ordinal += 1;
            id = format!("possessorTargetPlayer{ordinal}");
        }
        decisions.push(target_decision(&id, candidates, 1, 1));
        effects
            .iter_mut()
            .for_each(|effect| replace(effect, site, &id));
        declared.push((key, id));
    }
    Ok(())
}

/// One declared-count bound as a numeric IR value. A literal stays a
/// literal; `X` reads the face's X decision; other amounts use the shared
/// numeric lowering and fail when it cannot represent them.
pub(in crate::oracle::canonical) fn lower_target_count_amount(
    amount: &NumericExpression,
) -> Option<Value> {
    match amount {
        NumericExpression::Atom(atom) => Some(lower_target_cardinality_bound(*atom)),
        _ => lower_numeric_expression(amount),
    }
}

/// `any number of target ...` (CR 601.2c): no bound below the population
/// of the candidates' domain. The maximum is the declaration-time count of
/// every object of that domain, a superset of the legal candidates, so it
/// never limits the declaration below what the rules allow. Only domains
/// with a declaration-time population count are admitted.
pub(in crate::oracle::canonical) fn lower_unbounded_target_maximum(
    candidates: &Value,
) -> Option<Value> {
    match candidates["kind"].as_str()? {
        "permanents" => Some(json!({"kind":"countPermanents", "allPlayers":true})),
        // Every player in the game bounds any number of target players.
        "players" => Some(json!({"kind":"countPlayers"})),
        "cards"
            if matches!(
                candidates["zone"]["kind"].as_str(),
                Some("graveyard" | "anyGraveyard")
            ) =>
        {
            Some(json!({"kind":"countCards",
                "zone":{"kind":"graveyard", "player":controller()}, "allPlayers":true}))
        }
        _ => None,
    }
}

/// Overwrites the literal bounds of a lowered `chooseTargets` decision with
/// the slot's typed count. A slot without a count keeps its literal IR.
pub(in crate::oracle::canonical) fn apply_target_count_to_decision(
    decision: &mut Value,
    count: &TargetCount,
) -> Option<()> {
    decision["minimum"] = lower_target_count_amount(&count.minimum)?;
    decision["maximum"] = match &count.maximum {
        TargetCountBound::Amount(amount) => lower_target_count_amount(amount)?,
        TargetCountBound::Unbounded => lower_unbounded_target_maximum(&decision["candidates"])?,
    };
    Some(())
}

#[cfg(test)]
#[path = "permanent_operand_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "verb_operand_lowering_tests.rs"]
mod verb_operand_lowering_tests;

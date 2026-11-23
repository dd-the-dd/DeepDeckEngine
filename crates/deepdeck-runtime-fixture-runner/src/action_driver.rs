//! Stable dispatch of DATA actions through existing public engine APIs.
use super::*;
pub(super) fn rule_source(action: &FixtureAction) -> Option<(String, String)> {
    let action = match action {
        FixtureAction::ProbeFailure { action, .. } => action.as_ref(),
        other => other,
    };
    match action {
        FixtureAction::ActivateAbility {
            controller_id,
            source_definition_id,
            ..
        }
        | FixtureAction::ActivateManaAbility {
            controller_id,
            source_definition_id,
            ..
        }
        | FixtureAction::CastSpell {
            controller_id,
            source_definition_id,
            ..
        } => Some((controller_id.clone(), source_definition_id.clone())),
        _ => None,
    }
}
pub(super) fn execute(
    engine: &mut GameEngine,
    action: &FixtureAction,
    rule: &Value,
    provider: &mut ScriptedProvider,
) -> Result<Vec<String>, EngineError> {
    match action {
        FixtureAction::ProbeFailure {
            action,
            expected_error_contains,
            provider_error,
            retry,
        } => super::failure_probe::run(
            engine,
            action,
            rule,
            provider,
            expected_error_contains,
            provider_error.as_ref(),
            *retry,
        ),
        _ => {
            execute_plain(engine, action, rule, provider)?;
            Ok(vec![])
        }
    }
}
pub(super) fn execute_plain<P: DecisionProvider>(
    engine: &mut GameEngine,
    action: &FixtureAction,
    rule: &Value,
    provider: &mut P,
) -> Result<(), EngineError> {
    match action {
        FixtureAction::ActivateAbility {
            controller_id,
            source_definition_id,
            ability_rule_index,
            declaration_decisions,
            targets,
        } => engine.activate_canonical_ability_with_decisions(
            &CanonicalActivatedAbilityActivation {
                controller_id: controller_id.clone(),
                source_definition_id: source_definition_id.clone(),
                ability_rule_index: *ability_rule_index,
                targets: targets.clone(),
            },
            declaration_decisions,
            provider,
        ),
        FixtureAction::ActivateManaAbility {
            controller_id,
            source_definition_id,
            mana_symbols,
            objects,
            declaration_decisions,
        } => engine.activate_canonical_mana_ability_with_decisions(
            &CanonicalManaAbilityActivation {
                controller_id: controller_id.clone(),
                source_definition_id: source_definition_id.clone(),
                mana_symbols: mana_symbols.clone(),
            },
            objects,
            declaration_decisions,
            provider,
        ),
        FixtureAction::CastSpell {
            controller_id,
            source_definition_id,
            use_alternative_cost,
            alternative_exile_definition_id,
            declaration_decisions,
            source_zone,
        } => engine.cast_canonical_spell(
            &CanonicalSpellCast {
                source_zone: source_zone.clone(),
                controller_id: controller_id.clone(),
                source_definition_id: source_definition_id.clone(),
                use_alternative_cost: *use_alternative_cost,
                alternative_exile_definition_id: alternative_exile_definition_id.clone(),
                declaration_decisions: declaration_decisions.clone(),
            },
            provider,
        ),
        FixtureAction::PlayLand {
            player_id,
            source_definition_id,
            source_zone,
        } => engine.play_canonical_land(
            &CanonicalLandPlay {
                source_zone: source_zone.clone(),
                player_id: player_id.clone(),
                source_definition_id: source_definition_id.clone(),
            },
            provider,
        ),
        FixtureAction::RunMulligans {
            free_mulligans,
            maximum_mulligans,
        } => engine.run_mulligans(provider, *free_mulligans, *maximum_mulligans),
        FixtureAction::RunPostcombatMain {
            maximum_turn_number,
        } => engine
            .run_from_postcombat_main(provider, *maximum_turn_number)
            .map(|_| ()),
        FixtureAction::ResolveTopStack => engine.resolve_canonical_top_stack(provider),
        FixtureAction::ExecuteRule {
            execution,
            rule: override_rule,
        } => execute_rule(
            engine,
            execution,
            override_rule.as_ref().unwrap_or(rule),
            provider,
        ),
        FixtureAction::ProbeFailure { .. } => Err(EngineError::new(
            "nested probeFailure action is unsupported",
        )),
    }
}

/// Every spell rule whose source card is outside the battlefield resolves as a
/// spell through the engine's fixture seam, which alone decides whether the
/// card can resolve that way (an instant or sorcery card, CR 608.2) and
/// rejects anything else. A fixture that means to execute such a rule in place
/// says so with `executeInPlace`. A rule hosted by a permanent, and any
/// non-spell rule, executes in place: a permanent is never a spell.
pub(super) fn execute_rule<P: DecisionProvider>(
    engine: &mut GameEngine,
    execution: &FixtureExecution,
    rule: &Value,
    provider: &mut P,
) -> Result<(), EngineError> {
    let canonical = CanonicalRuleExecution {
        controller_id: execution.controller_id.clone(),
        source_definition_id: execution.source_definition_id.clone(),
        rule: rule.clone(),
        decisions: execution.decisions.clone(),
        targets: execution.targets.clone(),
    };
    let sources =
        engine.canonical_spell_sources(&execution.controller_id, &execution.source_definition_id);
    if rule.get("kind").and_then(Value::as_str) != Some("spellAbility")
        || execution.execute_in_place
        || sources.is_empty()
    {
        return engine.execute_canonical_rule(canonical, provider);
    }
    let source_instance_id = match (&execution.source_instance_id, sources.as_slice()) {
        (Some(chosen), _) => chosen.clone(),
        (None, [only]) => only.clone(),
        (None, many) => {
            return Err(EngineError::new(format!(
                "spell source {} has {} off-battlefield copies; name one with sourceInstanceId",
                execution.source_definition_id,
                many.len()
            )));
        }
    };
    engine.resolve_canonical_spell(canonical, &source_instance_id, provider)
}

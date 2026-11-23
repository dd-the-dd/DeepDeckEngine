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
            CanonicalRuleExecution {
                controller_id: execution.controller_id.clone(),
                source_definition_id: execution.source_definition_id.clone(),
                rule: override_rule.as_ref().unwrap_or(rule).clone(),
                decisions: execution.decisions.clone(),
                targets: execution.targets.clone(),
            },
            provider,
        ),
        FixtureAction::ProbeFailure { .. } => Err(EngineError::new(
            "nested probeFailure action is unsupported",
        )),
    }
}

/// A spell rule whose source is an instant or sorcery card outside the
/// battlefield resolves as that spell: the card is on the stack while it
/// resolves (CR 601.2a) and then goes where the rules send it (CR 608.2n), so
/// it can never choose or count itself in its old zone. Any other rule, and a
/// spell rule hosted by a permanent or a permanent card, executes in place as
/// an effect harness.
pub(super) fn execute_rule<P: DecisionProvider>(
    engine: &mut GameEngine,
    execution: CanonicalRuleExecution,
    provider: &mut P,
) -> Result<(), EngineError> {
    let is_spell_card = engine
        .state()
        .players
        .iter()
        .filter(|player| player.id == execution.controller_id)
        .flat_map(|player| {
            player
                .library
                .iter()
                .chain(&player.hand)
                .chain(&player.graveyard)
                .chain(&player.exile)
                .chain(&player.command_zone)
        })
        .any(|card| {
            card.definition.id == execution.source_definition_id
                && card
                    .definition
                    .type_line
                    .split(|character: char| !character.is_alphanumeric())
                    .any(|part| part == "Instant" || part == "Sorcery")
        });
    if is_spell_card && execution.rule.get("kind").and_then(Value::as_str) == Some("spellAbility") {
        engine.resolve_canonical_spell(execution, provider)
    } else {
        engine.execute_canonical_rule(execution, provider)
    }
}

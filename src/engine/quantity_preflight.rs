//! Conservative preflight for quantity and trigger-publication sources that can become active
//! during one program. Inspects structured rules/definitions, never Oracle
//! text. A false result is a proof over known data; uncertainty requests one
//! checkpoint instead of predicting a choice or resolving an effect.

use super::{CardDefinition, CardInstance, EngineError, GameEngine, LegalAction, value_kind};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const MAX_PREFLIGHT_DEPTH: usize = 64;
static BUILTIN_QUANTITY_FACTS: OnceLock<Option<BTreeMap<String, bool>>> = OnceLock::new();

fn builtin_fact(name: &str) -> Option<bool> {
    BUILTIN_QUANTITY_FACTS
        .get_or_init(|| {
            crate::game_piece_artifact::embedded_artifact()
                .ok()
                .map(|artifact| {
                    artifact
                        .definitions
                        .into_iter()
                        .map(|(name, definition)| {
                            (
                                name,
                                definition
                                    .printed_rules()
                                    .iter()
                                    .any(|rule| payload_risk(rule, None, 0)),
                            )
                        })
                        .collect()
                })
        })
        .as_ref()
        .and_then(|facts| facts.get(&name.trim().to_ascii_lowercase()).copied())
}

fn named_token_may_enable(name: &str, source: Option<&CardDefinition>, depth: usize) -> bool {
    if depth > MAX_PREFLIGHT_DEPTH {
        return true;
    }
    let Some(source) = source else {
        return true;
    };
    // Follow the executor's typed-related-definition precedence. The text
    // fallback is intentionally not compiled a second time during preflight.
    for rule in source.printed_rules() {
        for raw in rule["relatedTokenDefinitions"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if raw["name"]
                .as_str()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            {
                return serde_json::from_value::<CardDefinition>(raw.clone())
                    .ok()
                    .is_none_or(|definition| definition_risk(&definition, depth + 1));
            }
        }
        if rule["relatedTokenOracle"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|raw| {
                raw.get("displayName")
                    .or_else(|| raw.get("name"))
                    .and_then(Value::as_str)
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name))
            })
        {
            return true;
        }
    }
    builtin_fact(name).unwrap_or(true)
}

fn payload_risk(value: &Value, source: Option<&CardDefinition>, depth: usize) -> bool {
    if depth > MAX_PREFLIGHT_DEPTH {
        return true;
    }
    if super::permanent_presence_query::node_requires_checkpoint(value) {
        return true;
    }
    match value_kind(value) {
        Some(
            "triggeredAbility"
            | "keywordAbility"
            | "keywordAbilityGroup"
            | "prowess"
            | "firebending"
            | "grantKeyword"
            | "temporaryKeyword"
            | "ringEmblem"
            | "emblemTriggeredAbility"
            | "delayedTriggeredAbility"
            | "delayedStepTrigger"
            | "delayedSpellCastTrigger"
            | "delayedAttackTrigger"
            | "delayedOpponentAttackTrigger"
            | "delayedCombatDamageTrigger"
            | "delayedZoneChangeTrigger"
            | "paradigmDelayedTrigger"
            | "drawIfDiesThisTurn"
            | "grantFirstTapCounterTrigger"
            | "controlledCombatDamageDrawUntilEndOfTurn"
            | "attachedDamageDraw"
            | "attachedCombatDamageExileDamagedCreature"
            | "returnExiledCardWhenTokenLeaves"
            | "exileTokenWhenSourceLeaves"
            | "copyNextLessonThisTurn"
            | "dungeonRoom"
            | "ventureDungeon"
            | "temptWithRing"
            | "installAttackTrigger"
            | "installOpponentAttackTrigger"
            | "installDelayedTriggeredAbility"
            | "installCombatDamageTrigger"
            | "installDelayedSpellCastTrigger"
            | "installDelayedStepTrigger"
            | "installDelayedZoneChangeTrigger"
            | "replaceEventQuantity"
            | "multiplyTriggeredAbility"
            | "multiplyUnownedControlledTriggeredAbility"
            | "windcragSiegeMardu",
        ) => return true,
        Some("namedToken") => {
            return value["name"]
                .as_str()
                .is_none_or(|name| named_token_may_enable(name, source, depth + 1));
        }
        // This existing primitive imports a definition from outside the
        // current typed game data. No name lookup or parsing is done here.
        Some("conjureNamedCard") => return true,
        _ => {}
    }
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| payload_risk(value, source, depth + 1)),
        Value::Object(values) => values
            .values()
            .any(|value| payload_risk(value, source, depth + 1)),
        _ => false,
    }
}
fn definition_risk(definition: &CardDefinition, depth: usize) -> bool {
    definition
        .printed_rules()
        .iter()
        .any(|rule| payload_risk(rule, Some(definition), depth + 1))
}
pub(super) fn definition_may_enable_quantity(definition: &CardDefinition) -> bool {
    definition_risk(definition, 0)
}
fn card_risk(card: &CardInstance) -> bool {
    card.definition.is_game_piece
        || definition_may_enable_quantity(&card.definition)
        || card
            .printed_definition
            .as_ref()
            .is_some_and(definition_may_enable_quantity)
}

impl GameEngine {
    /// All known candidates are considered, including inactive conditions,
    /// phased sources, other printed faces, non-BF zones and saved snapshots.
    /// This intentionally overapproximates whether the current program will
    /// actually import a particular candidate; it never executes a selector.
    pub(super) fn quantity_known_sources_may_enable(&self) -> bool {
        self.state.players.iter().any(|player| {
            player
                .library
                .iter()
                .chain(&player.hand)
                .chain(&player.battlefield)
                .chain(&player.graveyard)
                .chain(&player.exile)
                .chain(&player.command_zone)
                .chain(&player.sideboard)
                .any(card_risk)
        }) || self
            .state
            .game_pieces
            .iter()
            .any(definition_may_enable_quantity)
            || self.state.stack.iter().any(|stack| {
                card_risk(&stack.card)
                    || stack
                        .ability_rule
                        .as_ref()
                        .is_some_and(|rule| payload_risk(rule, Some(&stack.card.definition), 0))
                    || stack
                        .decisions
                        .values()
                        .any(|value| payload_risk(value, None, 0))
            })
            || self
                .state
                .rule_modifiers
                .iter()
                .chain(&self.state.permissions)
                .any(|value| payload_risk(value, None, 0))
    }

    pub(super) fn quantity_action_payload_may_enable(&self, action: &LegalAction) -> bool {
        // Action DTO payloads can contain a granted rule or a typed snapshot.
        // Without its own definition context a named import stays uncertain.
        action
            .decisions
            .values()
            .any(|value| payload_risk(value, None, 0))
    }

    pub(super) fn with_quantity_program_transaction<T>(
        &mut self,
        program: &Value,
        source: Option<&CardDefinition>,
        work: impl FnOnce(&mut Self) -> Result<T, EngineError>,
    ) -> Result<T, EngineError> {
        if self.quantity_transaction_in_progress() || self.quantity_preflight_in_progress() {
            return self.with_quantity_preflight(false, work);
        }
        let extra_risk = payload_risk(program, source, 0)
            || source.is_some_and(definition_may_enable_quantity)
            || super::resolution_preflight::requires_checkpoint(program);
        self.with_quantity_preflight(extra_risk, work)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "quantity_preflight_tests.rs"]
mod tests;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use super::{default_max_hand_size, default_opening_hand_size, is_false};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardDefinition {
    pub id: String,
    pub name: String,
    pub type_line: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_commander: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_token: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_game_piece: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_sideboard: bool,
    #[serde(default)]
    pub mana_cost: String,
    #[serde(default)]
    pub power: Option<String>,
    #[serde(default)]
    pub toughness: Option<String>,
    #[serde(default)]
    pub rules: Vec<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerDeck {
    pub id: String,
    pub name: String,
    pub starting_life: i32,
    pub cards: Vec<CardDefinition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSetup {
    pub players: Vec<PlayerDeck>,
    #[serde(default = "default_opening_hand_size")]
    pub opening_hand_size: usize,
    #[serde(default)]
    pub starting_player: usize,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterministicPlayerPosition {
    pub player_id: String,
    pub life: i32,
    pub battlefield_definition_ids: Vec<String>,
    pub hand_definition_ids: Vec<String>,
    pub library_size: usize,
    #[serde(default)]
    pub mana_pool: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeterministicGamePosition {
    pub turn_number: u32,
    pub active_player_id: String,
    pub step: GameStep,
    pub players: Vec<DeterministicPlayerPosition>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GameStatus {
    InProgress,
    Completed,
    TurnLimitReached,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UntapBoundaryStatus {
    Reached,
    GameEnded,
    TurnLimitReached,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GameStep {
    Untap,
    Upkeep,
    Draw,
    PrecombatMain,
    BeginningOfCombat,
    DeclareAttackers,
    DeclareBlockers,
    CombatDamage,
    PostcombatMain,
    EndStep,
    Cleanup,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GameEndReason {
    LifeTotal,
    DrawFromEmptyLibrary,
    CommanderDamage,
    Poison,
    Simultaneous,
    MandatoryLoop,
    SimulationLimit,
    SpellOrAbility,
    UnpaidPact,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GameMode {
    #[default]
    Free,
    Legacy,
    Commander,
    DuelCommander,
    Training,
    Training2,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommanderState {
    pub card_instance_id: String,
    pub name: String,
    pub owner_id: String,
    pub casts_from_command_zone: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommanderDamage {
    pub commander_id: String,
    pub commander_name: String,
    pub owner_id: String,
    pub controller_id: String,
    pub amount: i32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameOutcome {
    pub winner: Option<String>,
    pub losers: Vec<String>,
    pub reason: GameEndReason,
    pub turn_number: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardInstance {
    pub instance_id: String,
    pub definition: CardDefinition,
    #[serde(skip)]
    pub printed_definition: Option<CardDefinition>,
    pub owner: String,
    pub controller: String,
    #[serde(default)]
    pub tapped: bool,
    #[serde(default)]
    pub summoning_sick: bool,
    #[serde(default)]
    pub damage_marked: i32,
    #[serde(default)]
    pub power_modifier: i32,
    #[serde(default)]
    pub toughness_modifier: i32,
    #[serde(default)]
    pub counters: BTreeMap<String, i32>,
    #[serde(default)]
    pub flags: BTreeMap<String, bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle_protector: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attached_to: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectivePowerToughness {
    pub current_power: i32,
    pub current_toughness: i32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloatingMana {
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spend_restriction: Option<Value>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnginePlayer {
    pub id: String,
    pub name: String,
    pub life: i32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub has_lost: bool,
    pub library: Vec<CardInstance>,
    pub hand: Vec<CardInstance>,
    pub battlefield: Vec<CardInstance>,
    pub graveyard: Vec<CardInstance>,
    pub exile: Vec<CardInstance>,
    pub sideboard: Vec<CardInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub command_zone: Vec<CardInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commander_damage: Vec<CommanderDamage>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mana_pool: Vec<FloatingMana>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counters: BTreeMap<String, i32>,
    pub land_plays_remaining: i32,
    #[serde(default = "default_max_hand_size")]
    pub max_hand_size: usize,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum TargetRef {
    Player { player_id: String },
    Permanent { instance_id: String },
    Card { instance_id: String },
    StackObject { stack_id: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredAttacker {
    pub attacker_id: String,
    pub defender: TargetRef,
    pub defending_player_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeclaredBlocker {
    pub attacker_id: String,
    pub blocker_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CombatState {
    #[serde(default)]
    pub attackers: Vec<DeclaredAttacker>,
    #[serde(default)]
    pub blockers: Vec<DeclaredBlocker>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackObject {
    pub id: String,
    pub controller: String,
    pub card: CardInstance,
    #[serde(default, skip_serializing_if = "is_false")]
    pub cant_be_countered: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub exile_on_leave_stack: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ability_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ability_rule: Option<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub decisions: BTreeMap<String, Value>,
    pub targets: BTreeMap<String, TargetRef>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEvent {
    pub sequence: usize,
    pub turn_number: u32,
    pub step: GameStep,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub player_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_instance_id: Option<String>,
    pub detail: Value,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedRule {
    pub card_id: String,
    pub card_name: String,
    pub rule_kind: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameState {
    pub schema_version: String,
    pub game_mode: GameMode,
    pub status: GameStatus,
    pub turn_number: u32,
    pub active_player: usize,
    pub priority_player: Option<usize>,
    pub step: GameStep,
    pub players: Vec<EnginePlayer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub game_pieces: Vec<CardDefinition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commanders: Vec<CommanderState>,
    pub stack: Vec<StackObject>,
    #[serde(default)]
    pub combat: CombatState,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub permissions: Vec<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rule_modifiers: Vec<Value>,
    pub events: Vec<GameEvent>,
    pub unsupported_rules: Vec<UnsupportedRule>,
    pub outcome: Option<GameOutcome>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ActionKind {
    PassPriority,
    PlayLand,
    CastSpell,
    ActivateAbility,
    ChooseOpeningHandCard,
    KeepHand,
    TakeMulligan,
    BottomCard,
    Discard,
    DeclareAttacker,
    CancelAttacker,
    FinishAttackers,
    DeclareBlocker,
    FinishBlockers,
    AssignCombatDamage,
    PayLife,
    DeclinePayment,
    ChooseResolution,
    MoveCommanderToCommandZone,
    LeaveCommanderInZone,
}

impl ActionKind {
    pub fn is_agent_selectable(&self) -> bool {
        !matches!(self, Self::CancelAttacker)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegalAction {
    pub id: String,
    pub kind: ActionKind,
    pub player_id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub payment_sources: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub decisions: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub targets: BTreeMap<String, TargetRef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attacker_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DecisionKind {
    OpeningHandSelection,
    Mulligan,
    MulliganBottom,
    Priority,
    Discard,
    Attackers,
    Blockers,
    CombatDamage,
    ReplacementChoice,
    ResolutionChoice,
    Sideboarding,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DecisionChoice {
    CardSelection {
        decision_id: String,
        candidate_card_instance_ids: Vec<String>,
        minimum: usize,
        maximum: usize,
        prompt: String,
    },
    CardOrder {
        decision_id: String,
        card_instance_ids: Vec<String>,
        prompt: String,
    },
    OptionSelection {
        decision_id: String,
        options: Vec<String>,
        prompt: String,
    },
    CardNameSelection {
        decision_id: String,
        suggestions: Vec<String>,
        prompt: String,
    },
    NumberSelection {
        decision_id: String,
        minimum: i32,
        maximum: i32,
        prompt: String,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineDecisionRequest {
    pub id: String,
    pub kind: DecisionKind,
    pub player_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_card: Option<CardInstance>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_card_instance_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<DecisionChoice>,
    pub options: Vec<LegalAction>,
}

impl EngineDecisionRequest {
    pub fn agent_facing(&self) -> Self {
        let mut request = self.clone();
        request
            .options
            .retain(|action| action.kind.is_agent_selectable());
        request
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(id: &str, kind: ActionKind) -> LegalAction {
        LegalAction {
            id: id.to_string(),
            kind,
            player_id: "player-1".to_string(),
            label: id.to_string(),
            card_instance_id: None,
            payment_sources: Vec::new(),
            decisions: BTreeMap::new(),
            targets: BTreeMap::new(),
            target_order: Vec::new(),
            attacker_id: None,
            blocker_id: None,
        }
    }

    #[test]
    fn agent_facing_decisions_exclude_ui_only_combat_revisions() {
        let request = EngineDecisionRequest {
            id: "attackers:1".to_string(),
            kind: DecisionKind::Attackers,
            player_id: "player-1".to_string(),
            source_card: None,
            source_card_instance_id: None,
            choice: None,
            options: vec![
                action("attack", ActionKind::DeclareAttacker),
                action("cancel", ActionKind::CancelAttacker),
                action("finish", ActionKind::FinishAttackers),
            ],
        };

        let agent_request = request.agent_facing();

        assert_eq!(request.options.len(), 3);
        assert_eq!(agent_request.options.len(), 2);
        assert!(
            agent_request
                .options
                .iter()
                .all(|action| action.kind != ActionKind::CancelAttacker)
        );
    }
}

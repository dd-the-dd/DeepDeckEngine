//! Adventure layout runtime (CR 716).
//!
//! An adventurer card carries its Adventure as an `adventureSpell` rule (the
//! playable-card adapter links the parsed Adventure part, like `prepareSpell`).
//! Casting it as an Adventure uses only the Adventure's characteristics
//! (CR 716.3b): name, mana cost, type line and rules. The ordinary stack
//! resolution then exiles it with an `adventureCastPermission` bound to that
//! exile incarnation (CR 716.3d, 400.7); if it is countered or otherwise leaves
//! the stack without resolving, it goes to the graveyard as usual. In every
//! other zone, and once it leaves the stack, the card has its normal
//! characteristics again (CR 716.4; the stack departure restores the printed
//! definition).
//!
//! The choice to cast the Adventure (CR 716.3) exists only where the
//! Adventure itself may be cast (CR 601.3e): from the hand, or from another
//! zone under a route that admits a card with the Adventure's
//! characteristics. A permission that admitted the creature does not, by
//! itself, admit its Adventure, and the reverse.
use super::*;

/// The decision that marks a cast as the card's Adventure (CR 716.3). Every
/// cast of an adventurer card carries it, `false` for the creature.
pub(super) const CAST_AS_ADVENTURE: &str = "castAsAdventure";
/// Casting-source flag set on the transient card that offers the Adventure.
pub(super) const CASTING_AS_ADVENTURE_FLAG: &str = "castingAsAdventure";
const ADVENTURE_PERMISSION: &str = "adventureCastPermission";

fn linked_adventure(card: &CardDefinition) -> Option<&Value> {
    card.rules
        .iter()
        .find(|rule| value_kind(rule) == Some("adventureSpell"))
        .map(|rule| &rule["spell"])
}

pub(super) fn has_adventure(card: &CardDefinition) -> bool {
    linked_adventure(card).is_some()
}

/// A linked spell part (`prepareSpell`, `adventureSpell`): one instant or
/// sorcery, of the required spell subtype if any, with at least one spell
/// ability, whose every rule the runtime supports.
pub(super) fn linked_spell_part_supported(spell: &Value, required_subtype: Option<&str>) -> bool {
    // A missing mana cost is not a free spell.
    if !spell["manaCost"].is_string() {
        return false;
    }
    let Ok(spell) = serde_json::from_value::<CardDefinition>(spell.clone()) else {
        return false;
    };
    (is_instant(&spell) || is_sorcery(&spell))
        && required_subtype.is_none_or(|subtype| type_line_contains(&spell.type_line, subtype))
        && spell
            .rules
            .iter()
            .any(|rule| value_kind(rule) == Some("spellAbility"))
        && spell.rules.iter().all(rule_supported)
}

pub(super) fn adventure_spell_rule_supported(rule: &Value) -> bool {
    value_kind(rule) == Some("adventureSpell")
        && linked_spell_part_supported(&rule["spell"], Some("Adventure"))
}

/// The card as its Adventure (CR 716.3b): only the Adventure's name, mana cost,
/// type line and rules, with no power or toughness. Its spell abilities exile
/// it on resolution with permission to cast the creature later (CR 716.3d).
pub(super) fn adventure_spell_definition(card: &CardDefinition) -> Option<CardDefinition> {
    let spell = linked_adventure(card)?;
    let mut definition = card.clone();
    definition.name = spell["name"].as_str()?.to_string();
    definition.type_line = spell["typeLine"].as_str()?.to_string();
    definition.mana_cost = spell["manaCost"].as_str()?.to_string();
    definition.power = None;
    definition.toughness = None;
    definition.rules = spell["rules"]
        .as_array()?
        .iter()
        .cloned()
        .map(|mut rule| {
            if value_kind(&rule) == Some("spellAbility") {
                rule["exileAfterResolution"] = Value::Bool(true);
                rule["adventureAfterResolution"] = Value::Bool(true);
            }
            rule
        })
        .collect();
    Some(definition)
}

pub(super) fn casting_as_adventure(card: &CardInstance) -> bool {
    card.flags
        .get(CASTING_AS_ADVENTURE_FLAG)
        .copied()
        .unwrap_or(false)
}

pub(super) fn cast_as_adventure(decisions: &BTreeMap<String, Value>) -> bool {
    decisions.get(CAST_AS_ADVENTURE).and_then(Value::as_bool) == Some(true)
}

/// The characteristics a cast uses: the Adventure's when the cast declares it.
pub(super) fn casting_definition(
    decisions: &BTreeMap<String, Value>,
    definition: CardDefinition,
) -> Result<CardDefinition, EngineError> {
    if !cast_as_adventure(decisions) {
        return Ok(definition);
    }
    adventure_spell_definition(&definition)
        .ok_or_else(|| EngineError::new("card has no Adventure to cast"))
}

/// The permission an Adventure resolution grants: cast the creature from that
/// exile incarnation only (CR 716.3d). Leaving exile ends it (CR 400.7).
pub(super) fn adventure_cast_permission(card: &CardInstance, controller: &str) -> Value {
    let mut permission = json!({
        "kind": ADVENTURE_PERMISSION,
        "action": "play",
        "cardInstanceId": card.instance_id,
        "playerId": controller,
        "whileInZone": "exile",
        "normalTimingApplies": true,
        "normalCostsApply": true,
    });
    if let Some(identity) = GameEngine::tracked_zone_identity(card) {
        permission["zoneIdentity"] = Value::from(identity);
    }
    permission
}

impl GameEngine {
    /// The Adventure casting sources of `player`: each adventurer card whose
    /// Adventure may be cast from its zone, with the Adventure's
    /// characteristics and the casting-source flag.
    ///
    /// Known gap, failing closed: exile is scanned only for cards `player`
    /// owns, so an Adventure on another player's exiled card (cast under a
    /// "you may cast that card" permission) is not offered, while the creature
    /// half still follows the ordinary permission route.
    pub(super) fn adventure_casting_sources(
        &self,
        player: &EnginePlayer,
    ) -> Vec<(CardInstance, &'static str)> {
        let library_top = player
            .library
            .last()
            .into_iter()
            .map(|card| (card, "library"));
        let exile = player
            .exile
            .iter()
            .filter(|card| card.owner == player.id)
            .map(|card| (card, "exile"));
        player
            .hand
            .iter()
            .map(|card| (card, "hand"))
            .chain(player.graveyard.iter().map(|card| (card, "graveyard")))
            .chain(exile)
            .chain(library_top)
            .filter_map(|(card, zone)| {
                let definition = adventure_spell_definition(&card.definition)?;
                let mut casting = card.clone();
                casting.definition = definition;
                casting
                    .flags
                    .insert(CASTING_AS_ADVENTURE_FLAG.to_string(), true);
                self.casting_route(player, &casting, zone, true)
                    .is_some()
                    .then_some((casting, zone))
            })
            .collect()
    }
}

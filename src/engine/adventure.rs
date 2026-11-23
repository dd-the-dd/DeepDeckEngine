//! Adventure layout runtime (CR 715).
//!
//! An adventurer card carries its Adventure as an `adventureSpell` rule (the
//! playable-card adapter links the parsed Adventure part, like `prepareSpell`).
//! Casting it as an Adventure uses only the Adventure's characteristics
//! (CR 715.3b): name, mana cost, type line and rules. The ordinary stack
//! resolution then exiles it with an `adventureCastPermission` (CR 715.4); if
//! it is countered or does not resolve, it goes to the graveyard as usual. In
//! every other zone, and once it leaves the stack, the card has its normal
//! characteristics again (the stack departure restores the printed definition).
use super::*;

/// The decision that marks a cast as the card's Adventure (CR 715.3a).
pub(super) const CAST_AS_ADVENTURE: &str = "castAsAdventure";
/// Casting-source flag set on the transient card that offers the Adventure.
pub(super) const CASTING_AS_ADVENTURE_FLAG: &str = "castingAsAdventure";

fn linked_adventure(card: &CardDefinition) -> Option<&Value> {
    card.rules
        .iter()
        .find(|rule| value_kind(rule) == Some("adventureSpell"))
        .map(|rule| &rule["spell"])
}

/// An `adventureSpell` rule links one instant or sorcery Adventure whose every
/// rule the runtime supports, with at least one spell ability.
pub(super) fn adventure_spell_rule_supported(rule: &Value) -> bool {
    if value_kind(rule) != Some("adventureSpell") {
        return false;
    }
    let Ok(spell) = serde_json::from_value::<CardDefinition>(rule["spell"].clone()) else {
        return false;
    };
    (is_instant(&spell) || is_sorcery(&spell))
        && type_line_contains(&spell.type_line, "Adventure")
        && spell
            .rules
            .iter()
            .any(|rule| value_kind(rule) == Some("spellAbility"))
        && spell.rules.iter().all(rule_supported)
}

/// The card as its Adventure (CR 715.3b): only the Adventure's name, mana cost,
/// type line and rules, with no power or toughness. Its spell abilities exile
/// it on resolution with permission to cast the creature later (CR 715.4).
pub(super) fn adventure_spell_definition(card: &CardDefinition) -> Option<CardDefinition> {
    let spell = linked_adventure(card)?;
    let mut definition = card.clone();
    definition.name = spell["name"].as_str()?.to_string();
    definition.type_line = spell["typeLine"].as_str()?.to_string();
    definition.mana_cost = spell["manaCost"].as_str().unwrap_or_default().to_string();
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

/// Whether this card in `source_zone` may be cast as its Adventure: it has
/// one, and it is not an adventurer exiled by its own Adventure, which may
/// only be cast as the creature (CR 715.4).
fn adventure_castable_from(card: &CardInstance, source_zone: &str, permissions: &[Value]) -> bool {
    !(source_zone == "exile"
        && permissions.iter().any(|permission| {
            value_kind(permission) == Some("adventureCastPermission")
                && permission["cardInstanceId"].as_str() == Some(card.instance_id.as_str())
        }))
}

/// The transient casting card that offers the Adventure from `source_zone`.
pub(super) fn adventure_casting_card(
    card: &CardInstance,
    source_zone: &str,
    permissions: &[Value],
) -> Option<CardInstance> {
    if !adventure_castable_from(card, source_zone, permissions) {
        return None;
    }
    let mut casting = card.clone();
    casting.definition = adventure_spell_definition(&card.definition)?;
    casting
        .flags
        .insert(CASTING_AS_ADVENTURE_FLAG.to_string(), true);
    Some(casting)
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

//! Linked spell parts: a spell printed on a card alongside its primary part
//! and cast through that card (a prepare spell; an Adventure, CR 716.2).
//! Only the shape and executability of the linked part live here; each
//! layout's casting rules stay in its own module.
use super::*;

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

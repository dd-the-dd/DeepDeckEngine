//! Whether a compiled rule is executable by this engine: the single gate
//! every coverage report, game setup and ability enumerator asks
//! (`rule_is_executable`). Each rule kind delegates to the support check of
//! its frame (spell, activated, triggered, static, keyword, marker); a rule
//! kind or shape not listed here is unsupported, never approximated.
use super::*;

pub(super) fn rule_supported(rule: &Value) -> bool {
    if !entry_observation::reference_scope_supported(rule) || !band_gate::band_gate_supported(rule)
    {
        return false;
    }
    match value_kind(rule) {
        Some("spellAbility") => spell_rule_supported(rule),
        Some("prepareSpell") => prepare_spell_rule_supported(rule),
        Some("adventureSpell") => adventure::adventure_spell_rule_supported(rule),
        Some("manaAbility") => mana_rule_supported(rule),
        Some("replacementEffect") => replacement_rule_supported(rule),
        Some("activatedAbility") => activated_rule_supported(rule),
        Some("triggeredAbility") => triggered_rule_supported(rule),
        Some("dungeonRoom") => {
            rule["room"].is_string()
                && rule["nextRooms"]
                    .as_array()
                    .is_some_and(|rooms| rooms.iter().all(Value::is_string))
                && rule["effects"].as_array().is_some_and(|effects| {
                    !effects.is_empty() && effects.iter().all(effect_supported)
                })
                && declaration_shape_supported(rule)
        }
        Some("keywordAbility" | "keywordAbilityGroup") => keyword_rule_supported(rule),
        Some("rulesMarker") => {
            let legacy_text_marker = rule["text"].is_string()
                && rule["splitFaces"].as_array().is_none_or(|faces| {
                    !faces.is_empty()
                        && faces.iter().all(|face| {
                            face["name"].is_string()
                                && face["typeLine"].is_string()
                                && face["manaCost"].is_string()
                                && face["rules"]
                                    .as_array()
                                    .is_some_and(|rules| rules.iter().all(rule_supported))
                        })
                });
            let typed_saga_progression = rule["marker"].as_str() == Some("sagaProgression")
                && value_kind(&rule["source"]) == Some("self")
                && numeric_expression_supported(&rule["finalChapter"]);
            rule.get("morbidSearchDestination").is_none()
                && (legacy_text_marker || typed_saga_progression)
        }
        Some("staticAbility") => {
            battlefield_static_rule_supported(rule)
                || casting_static_rule_supported(rule)
                || activated_rule_execution::cast_restriction_rule_supported(rule)
                || eminence_static_rule_supported(rule)
                // CR 604.3: statics that say they function from a graveyard,
                // hand or exile join the continuous-effect collector there.
                || zone_of_function::card_zone_static_rule_supported(rule)
                // CR 309.2: a Dungeon stays in the command zone; its entry
                // restriction is read there by the venture procedure.
                || (zone_of_function::static_zone_of_function(rule)
                    == Some(ZoneOfFunction::Command)
                    && rule["modifiers"].as_array().is_some_and(|modifiers| {
                        !modifiers.is_empty()
                            && modifiers.iter().all(|modifier| {
                                value_kind(modifier) == Some("dungeonEntryRestriction")
                                    && modifier["procedure"].is_string()
                            })
                    }))
                || (zone_of_function::static_zone_of_function(rule)
                    == Some(ZoneOfFunction::Stack)
                    && rule["modifiers"].as_array().is_some_and(|modifiers| {
                        !modifiers.is_empty()
                            && modifiers.iter().all(|modifier| {
                                value_kind(modifier) == Some("cantBeCountered")
                                    && value_kind(&modifier["object"]) == Some("self")
                            })
                    }))
        }
        Some(
            "cantBlock" | "deathtouch" | "defender" | "doubleStrike" | "firstStrike" | "flying"
            | "haste" | "hexproof" | "lifelink" | "menace" | "prowess" | "reach" | "trample"
            | "shroud" | "vigilance",
        ) => true,
        Some("firebending") => keyword_kind_supported(rule),
        _ => false,
    }
}

use super::*;
use crate::oracle::syntax::AbilityInput;

pub(crate) fn parse_canonical_rule(
    input: &AbilityInput<'_>,
    ability_kind: &str,
) -> Option<CanonicalRuleDraft> {
    let text = input.source.text.trim();

    if let Some(parsed) = parse_buyback_ability(text) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_cumulative_upkeep_ability(text) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_alternative_cost_ability(text) {
        return Some(parsed);
    }

    if input
        .face_type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("Dungeon"))
    {
        return parse_dungeon_room(text, input.face_name)
            .or_else(|| parse_common_static_ability(text, input.face_name));
    }

    if let Some(parsed) =
        parse_generalized_zone_and_combat_ability(text, ability_kind, input.face_name)
    {
        return Some(parsed);
    }

    let parsed = match ability_kind {
        "manaAbility" => parse_mana_ability(text).map(promote_activated_mana_ability),
        "replacementEffect" => parse_conditional_enter_tapped(text)
            .or_else(|| parse_multiversal_passage_replacement(text))
            .or_else(|| parse_shock_land_replacement(text))
            .or_else(|| parse_prepared_replacement(text))
            .or_else(|| parse_composed_entry_replacement(text, input.face_name))
            .or_else(|| parse_avatar_casting_instruction(text))
            .or_else(|| parse_special_static_ability(text))
            .or_else(|| parse_common_static_ability(text, input.face_name))
            .or_else(|| parse_keyword_ability(text, input.face_name)),
        "keywordAbility" | "keywordAbilityGroup" => parse_keyword_ability(text, input.face_name),
        "activatedAbility" => parse_mana_ability(text)
            .or_else(|| parse_avatar_activated_ability(text))
            .or_else(|| parse_special_activated_ability(text))
            .or_else(|| parse_simple_activated_ability_for_face(text, input.face_name))
            .or_else(|| parse_common_activated_ability(text))
            .map(promote_activated_mana_ability),
        "triggeredAbility" => parse_prepare_triggered_ability(text)
            .or_else(|| parse_special_triggered_ability(text))
            .or_else(|| parse_composed_entry_triggered(text))
            .or_else(|| parse_expansion_triggered(text, input.face_name)),
        "staticAbility" => parse_prepare_triggered_ability(text)
            .or_else(|| parse_mana_ability(text).map(promote_activated_mana_ability))
            .or_else(|| parse_special_static_ability(text))
            .or_else(|| parse_composed_entry_replacement(text, input.face_name))
            .or_else(|| parse_avatar_casting_instruction(text))
            .or_else(|| parse_own_casting_reduction(text))
            .or_else(|| parse_avatar_deck_static(text))
            .or_else(|| parse_direct_landfall_static_ability(text))
            .or_else(|| parse_landfall_static_ability(text))
            .or_else(|| parse_special_triggered_ability(text))
            .or_else(|| parse_composed_entry_triggered(text))
            .or_else(|| parse_expansion_triggered(text, input.face_name))
            .or_else(|| parse_common_static_ability(text, input.face_name))
            .or_else(|| parse_avatar_activated_ability(text))
            .or_else(|| parse_special_activated_ability(text))
            .or_else(|| parse_simple_activated_ability_for_face(text, input.face_name))
            .or_else(|| parse_common_activated_ability(text))
            .or_else(|| parse_keyword_ability(text, input.face_name))
            .or_else(|| parse_spell_cant_be_countered(text)),
        "spellAbility" => parse_simple_spell_ability(text)
            .or_else(|| parse_azula_spell_ability(text))
            .or_else(|| parse_common_spell_ability(text))
            .or_else(|| parse_expansion_spell(text))
            .or_else(|| parse_avatar_spell_ability(text))
            .or_else(|| parse_avatar_casting_instruction(text))
            .or_else(|| parse_avatar_deck_spell(text))
            .or_else(|| parse_remaining_deck_spell(text))
            .or_else(|| parse_choose_one_modal(text))
            .or_else(|| parse_general_modal_spell(text))
            .or_else(|| parse_tiered(text))
            .or_else(|| parse_spree(text))
            .or_else(|| parse_library_spell(text))
            .or_else(|| parse_ancient_vendetta(text))
            .or_else(|| parse_composed_spell(text))
            .or_else(|| parse_impractical_joke(text))
            .or_else(|| parse_global_destruction(text))
            .or_else(|| parse_simple_damage(text))
            .or_else(|| parse_counter_unless_paid(text))
            .or_else(|| parse_counter_spell(text))
            .or_else(|| parse_target_player_draw(text))
            .or_else(|| parse_common_zone_and_value_spell(text, input.face_name))
            .or_else(|| parse_special_static_ability(text))
            .or_else(|| parse_keyword_ability(text, input.face_name)),
        _ => None,
    };
    let mut parsed = parsed
        .or_else(|| {
            text.starts_with("(As this Saga enters and after your draw step, add a lore counter.")
                .then(|| {
                    draft(
                        json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
                        &["Recognize intrinsic Saga lore-counter progression"],
                    )
                })
        })
        .or_else(|| parse_remaining_kellan_ability(text, ability_kind))?;
    if input.face_type_line.contains("Room") && parsed.rule["kind"].as_str() != Some("rulesMarker")
    {
        parsed.rule["roomDoorIndex"] = Value::from(input.source.line_start / 2);
    }
    Some(parsed)
}

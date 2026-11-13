use super::*;
use crate::oracle::model::RecognizerProvenance;
use crate::oracle::syntax::AbilityInput;

fn parse_common_static_with_optional_trailing_reminder(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    parse_common_static_ability(text, face_name).or_else(|| {
        let (instruction, reminder) = text.rsplit_once(". (")?;
        reminder.ends_with(')').then(|| ())?;
        parse_common_static_ability(&format!("{instruction}."), face_name)
    })
}

pub(crate) enum CanonicalParseOutcome {
    Parsed {
        draft: CanonicalRuleDraft,
        recognizer: RecognizerProvenance,
    },
    ParsedLegacyAfterNativeRejection {
        draft: CanonicalRuleDraft,
        failure: MachineFailure,
    },
    Unsupported,
    Failed {
        failure: MachineFailure,
        recognizer: RecognizerProvenance,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CanonicalParsePolicy {
    Full,
    NativeOnly,
}

#[derive(Clone, Copy, Debug)]
enum AbilityKindConstraint<'a> {
    Exact(&'a str),
    AnyNative,
}

// Existing dispatch order is the native precedence contract. `AnyNative`
// opens only the kind gate when token classification has no answer; each
// family's structural `claims_*` predicate must still commit before parsing.
impl PartialEq<&str> for AbilityKindConstraint<'_> {
    fn eq(&self, expected: &&str) -> bool {
        match self {
            Self::Exact(actual) => actual == expected,
            Self::AnyNative => true,
        }
    }
}

pub(crate) fn parse_canonical_rule(
    input: &AbilityInput<'_>,
    ability_kind: &str,
) -> CanonicalParseOutcome {
    parse_canonical_rule_with_policy(
        input,
        AbilityKindConstraint::Exact(ability_kind),
        CanonicalParsePolicy::Full,
    )
}

pub(crate) fn parse_canonical_rule_native(
    input: &AbilityInput<'_>,
    promoted_ability_kind: Option<&str>,
) -> CanonicalParseOutcome {
    parse_canonical_rule_with_policy(
        input,
        promoted_ability_kind.map_or(
            AbilityKindConstraint::AnyNative,
            AbilityKindConstraint::Exact,
        ),
        CanonicalParsePolicy::NativeOnly,
    )
}

fn parse_canonical_rule_with_policy(
    input: &AbilityInput<'_>,
    ability_kind: AbilityKindConstraint<'_>,
    policy: CanonicalParsePolicy,
) -> CanonicalParseOutcome {
    let mut unpromoted_candidate_failure = None;
    if ability_kind == "staticAbility" && claims_promoted_attack_tax_family(input) {
        return match parse_promoted_attack_tax(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_no_maximum_hand_size_family(input) {
        return match parse_promoted_no_maximum_hand_size(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_block_only_family(input) {
        return match parse_promoted_block_only(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_scry_family(input) {
        return match parse_promoted_scry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_collection_destroy_family(input) {
        return match parse_promoted_collection_destroy(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_land_play_trigger_family(input) {
        return match parse_promoted_land_play_trigger(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_labeled_entry_reflexive_counter_family(input)
    {
        return match parse_promoted_labeled_entry_reflexive_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_optional_zone_action_trigger_family(input)
    {
        return match parse_promoted_optional_zone_action_trigger(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_attack_delayed_return_counter_family(input)
    {
        return match parse_promoted_attack_delayed_return_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_target_player_draw_family(input) {
        return match parse_promoted_target_player_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_card_type_partition_family(input) {
        return match parse_promoted_card_type_partition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility" && claims_promoted_earthbend_activation_family(input) {
        return match parse_promoted_earthbend_activation(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_mobilize_family(input) {
        return match parse_promoted_mobilize(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_cast_prohibition_family(input) {
        return match parse_promoted_cast_prohibition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_saga_family(input) {
        return match parse_promoted_saga(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_became_target_trigger_family(input) {
        return match parse_promoted_became_target_trigger(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_counter_prohibition_family(input) {
        return match parse_promoted_counter_prohibition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_additional_land_play_family(input) {
        return match parse_promoted_additional_land_play(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_static_riot_grant_family(input) {
        return match parse_promoted_static_riot_grant(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_delve_keyword_family(input) {
        return match parse_promoted_delve_keyword(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect" && claims_promoted_decision_entry_counters_family(input)
    {
        return match parse_promoted_decision_entry_counters(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_graveyard_leave_counter_family(input) {
        return match parse_promoted_graveyard_leave_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_player_spell_tax_draw_family(input) {
        return match parse_promoted_player_spell_tax_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_ordinal_spell_tax_draw_family(input) {
        return match parse_promoted_ordinal_spell_tax_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_card_draw_payment_family(input) {
        return match parse_promoted_card_draw_payment(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility"
        && claims_promoted_labeled_activated_zone_change_family(input)
    {
        return match parse_promoted_labeled_activated_zone_change(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility"
        && claims_promoted_activated_top_card_delayed_draw_family(input)
    {
        return match parse_promoted_activated_top_card_delayed_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility" && claims_promoted_loyalty_ability_family(input) {
        return match parse_promoted_loyalty_ability(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_phyrexian_mana_reminder_family(input) {
        return match parse_promoted_phyrexian_mana_reminder(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_parenthesized_activated_mana_family(input) {
        return match parse_promoted_parenthesized_activated_mana(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_optional_life_entry_family(input) {
        return match parse_promoted_optional_life_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility"
        && claims_promoted_temporary_source_animation_family(input)
    {
        return match parse_promoted_temporary_source_animation(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_source_cast_x_entry_family(input) {
        return match parse_promoted_source_cast_x_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_source_entry_surveil_family(input) {
        return match parse_promoted_source_entry_surveil(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_source_entry_earthbend_sequence_family(input)
    {
        return match parse_promoted_source_entry_earthbend_sequence(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_entry_or_repeat_trigger_family(input) {
        return match parse_promoted_entry_or_repeat_trigger(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_source_attack_mill_family(input) {
        return match parse_promoted_source_attack_mill(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_attack_investigate_family(input) {
        return match parse_promoted_attack_investigate(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_ordinal_draw_source_transform_family(input)
    {
        return match parse_promoted_ordinal_draw_source_transform(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_library_search_trigger_family(input) {
        return match parse_promoted_library_search_trigger(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_entry_zone_batch_family(input) {
        return match parse_promoted_entry_zone_batch(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_source_entry_card_zone_change_family(input)
    {
        return match parse_promoted_source_entry_card_zone_change(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_linked_entry_exile_family(input) {
        return match parse_promoted_linked_entry_exile(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_source_entry_effect_sequence_family(input)
    {
        return match parse_promoted_source_entry_effect_sequence(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" {
        match try_parse_promoted_permanent_entered_effect_sequence(input) {
            PermanentEnteredParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            PermanentEnteredParseAttempt::Unpromoted(failure) => {
                if policy == CanonicalParsePolicy::NativeOnly {
                    return CanonicalParseOutcome::Failed {
                        failure,
                        recognizer: RecognizerProvenance::Native,
                    };
                }
                if unpromoted_candidate_failure.is_none() {
                    unpromoted_candidate_failure = Some(failure);
                }
            }
            PermanentEnteredParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            PermanentEnteredParseAttempt::NotClaimed => {}
        }
    }

    if ability_kind == "triggeredAbility" {
        match try_parse_promoted_phase_trigger_sequence(input) {
            PhaseTriggerParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            PhaseTriggerParseAttempt::Unpromoted(failure) => {
                if policy == CanonicalParsePolicy::NativeOnly {
                    return CanonicalParseOutcome::Failed {
                        failure,
                        recognizer: RecognizerProvenance::Native,
                    };
                }
                if unpromoted_candidate_failure.is_none() {
                    unpromoted_candidate_failure = Some(failure);
                }
            }
            PhaseTriggerParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            PhaseTriggerParseAttempt::NotClaimed => {}
        }
    }

    if ability_kind == "triggeredAbility"
        && claims_promoted_permanent_died_effect_sequence_family(input)
    {
        return match parse_promoted_permanent_died_effect_sequence(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_opening_hand_battlefield_family(input) {
        return match parse_promoted_opening_hand_battlefield(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility"
        && claims_promoted_graveyard_destination_replacement_family(input)
    {
        return match parse_promoted_graveyard_destination_replacement(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_uncast_entry_replacement_family(input) {
        return match parse_promoted_uncast_entry_replacement(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_attached_keyword_grant_family(input) {
        return match parse_promoted_attached_keyword_grant(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_mana_equip_family(input) {
        return match parse_promoted_mana_equip(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_simple_keyword_family(input) {
        return match parse_promoted_simple_keyword(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect"
        && claims_promoted_chosen_creature_type_entry_family(input)
    {
        return match parse_promoted_chosen_creature_type_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_as_enters_named_object_choice_family(input) {
        return match parse_promoted_as_enters_named_object_choice(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_chosen_name_spell_tax_family(input) {
        return match parse_promoted_chosen_name_spell_tax(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    // The stored chosen-name predicate is a more specific native family than
    // the generic object-query casting tax. Give it precedence so `with the
    // chosen name` is not interpreted as a keyword qualifier by the shared
    // selector.
    if ability_kind == "staticAbility" && claims_promoted_spell_cost_increase_family(input) {
        return match parse_promoted_spell_cost_increase(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" {
        match try_parse_promoted_static_spell_reduction(input) {
            StaticSpellReductionParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            StaticSpellReductionParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            StaticSpellReductionParseAttempt::NotClaimed => {}
        }
    }

    if claims_promoted_chosen_name_activation_prohibition_family(input) {
        return match parse_promoted_chosen_name_activation_prohibition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_trigger_suppression_family(input) {
        return match parse_promoted_trigger_suppression(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect" && claims_promoted_early_turn_entry_family(input) {
        return match parse_promoted_early_turn_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect" && claims_promoted_controlled_count_entry_family(input) {
        return match parse_promoted_controlled_count_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect" && claims_promoted_chosen_land_type_entry_family(input) {
        return match parse_promoted_chosen_land_type_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect" && claims_promoted_conditional_entry_family(input) {
        return match parse_promoted_conditional_entry(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "replacementEffect" && claims_promoted_source_entry_state_family(input) {
        return match parse_promoted_source_entry_state(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_retain_chosen_land_type_family(input) {
        return match parse_promoted_retain_chosen_land_type(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility"
        && claims_promoted_permanent_scaled_casting_reduction_family(input)
    {
        return match parse_promoted_permanent_scaled_casting_reduction(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_alternative_additional_cost_family(input) {
        return match parse_promoted_alternative_additional_cost(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    // The complete casting-cost envelope establishes its own semantic kind,
    // including on permanent spells. Classification is only a dispatch hint.
    match try_parse_promoted_mandatory_additional_sacrifice(input) {
        MandatoryAdditionalSacrificeParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        MandatoryAdditionalSacrificeParseAttempt::Unpromoted(failure) => {
            if policy == CanonicalParsePolicy::NativeOnly {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            if unpromoted_candidate_failure.is_none() {
                unpromoted_candidate_failure = Some(failure);
            }
        }
        MandatoryAdditionalSacrificeParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        MandatoryAdditionalSacrificeParseAttempt::NotClaimed => {}
    }

    match try_parse_promoted_mandatory_additional_discard(input) {
        MandatoryAdditionalDiscardParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        MandatoryAdditionalDiscardParseAttempt::Unpromoted(failure) => {
            if policy == CanonicalParsePolicy::NativeOnly {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            if unpromoted_candidate_failure.is_none() {
                unpromoted_candidate_failure = Some(failure);
            }
        }
        MandatoryAdditionalDiscardParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        MandatoryAdditionalDiscardParseAttempt::NotClaimed => {}
    }

    if ability_kind == "spellAbility" && claims_promoted_optional_blight_cost_family(input) {
        return match parse_promoted_optional_blight_cost(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_additional_cost_paid_followup_family(input)
    {
        return match parse_promoted_additional_cost_paid_followup(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_mana_replicate_family(input) {
        return match parse_promoted_mana_replicate(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_mana_overload_family(input) {
        return match parse_promoted_mana_overload(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_conditional_free_cast_family(input) {
        return match parse_promoted_conditional_free_cast(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_compound_alternative_cost_family(input) {
        return match parse_promoted_compound_alternative_cost(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_alternative_return_cost_family(input) {
        return match parse_promoted_alternative_return_cost(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_library_search_partition_family(input) {
        return match parse_promoted_library_search_partition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_library_search_move_family(input) {
        return match parse_promoted_library_search_move(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_player_counter_payment_sweep_family(input)
    {
        return match parse_promoted_player_counter_payment_sweep(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility"
        && claims_promoted_conditional_top_cards_partition_family(input)
    {
        return match parse_promoted_conditional_top_cards_partition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_top_cards_partition_family(input) {
        return match parse_promoted_top_cards_partition(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_top_cards_reorder_family(input) {
        return match parse_promoted_top_cards_reorder(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_owner_library_reveal_family(input) {
        return match parse_promoted_owner_library_reveal(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_delayed_draw_family(input) {
        return match parse_promoted_delayed_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_draw_optional_card_deployment_family(input)
    {
        return match parse_promoted_draw_optional_card_deployment(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_plain_draw_family(input) {
        return match parse_promoted_plain_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_draw_move_family(input) {
        return match parse_promoted_draw_move(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_targeted_same_name_search_family(input) {
        return match parse_promoted_targeted_same_name_search(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_restricted_mana_family(input) {
        return match parse_promoted_restricted_mana(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_tap_scoped_mana_family(input) {
        return match parse_promoted_tap_scoped_mana(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_conditional_mana_activation_family(input) {
        return match parse_promoted_conditional_mana_activation(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_activated_mana_family(input) {
        return match parse_promoted_activated_mana(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_direct_mana_spell_family(input) {
        return match parse_promoted_direct_mana_spell(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility" && claims_promoted_activated_library_search_family(input)
    {
        return match parse_promoted_activated_library_search(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility"
        && claims_promoted_linked_exile_return_activation_family(input)
    {
        return match parse_promoted_linked_exile_return_activation(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if claims_promoted_activated_card_zone_change_family(input) {
        return match parse_promoted_activated_card_zone_change(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "activatedAbility" {
        match try_parse_promoted_activated_combat_restriction(input) {
            ActivatedCombatRestrictionParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            ActivatedCombatRestrictionParseAttempt::Unpromoted(failure) => {
                if policy == CanonicalParsePolicy::NativeOnly {
                    return CanonicalParseOutcome::Failed {
                        failure,
                        recognizer: RecognizerProvenance::Native,
                    };
                }
                if unpromoted_candidate_failure.is_none() {
                    unpromoted_candidate_failure = Some(failure);
                }
            }
            ActivatedCombatRestrictionParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            ActivatedCombatRestrictionParseAttempt::NotClaimed => {}
        }
    }

    match try_parse_promoted_compound_zone_change_activation(input) {
        CompoundZoneChangeActivationParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        CompoundZoneChangeActivationParseAttempt::Unpromoted(failure) => {
            if unpromoted_candidate_failure.is_none() {
                unpromoted_candidate_failure = Some(failure);
            }
        }
        CompoundZoneChangeActivationParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        CompoundZoneChangeActivationParseAttempt::NotClaimed => {}
    }

    if ability_kind == "spellAbility" && claims_promoted_color_guarded_modal_family(input) {
        return match parse_promoted_color_guarded_modal(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_primitive_modal_family(input) {
        return match parse_promoted_primitive_modal(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_effect_sequence_modal_family(input) {
        return match parse_promoted_effect_sequence_modal(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_set_ability_loss_sequence_family(input) {
        return match parse_promoted_set_ability_loss_sequence(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_counter_delayed_draw_family(input) {
        return match parse_promoted_counter_delayed_draw(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_counter_replacement_family(input) {
        return match parse_promoted_counter_replacement(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_counter_unless_paid_family(input) {
        return match parse_promoted_counter_unless_paid(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_stack_item_counter_family(input) {
        return match parse_promoted_stack_item_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_mana_value_counter_family(input) {
        return match parse_promoted_mana_value_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_colored_counter_family(input) {
        return match parse_promoted_colored_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_counter_descriptive_token_family(input) {
        return match parse_promoted_counter_descriptive_token(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_counter_named_token_family(input) {
        return match parse_promoted_counter_named_token(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_type_union_counter_family(input) {
        return match parse_promoted_type_union_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_simple_counter_family(input) {
        return match parse_promoted_simple_counter(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "keywordAbility" && claims_promoted_mana_kicker_family(input) {
        return match parse_promoted_mana_kicker(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_conditional_damage_family(input) {
        return match parse_promoted_conditional_damage(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_zone_change_search_family(input) {
        return match parse_promoted_zone_change_search(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_zone_change_token_family(input) {
        return match parse_promoted_zone_change_token(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_zone_change_life_family(input) {
        return match parse_promoted_zone_change_life(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_target_card_move_spell_family(input) {
        return match parse_promoted_target_card_move_spell(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_graveyard_card_entry_life_family(input) {
        return match parse_promoted_graveyard_card_entry_life(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_conditional_zone_change_family(input) {
        return match parse_promoted_conditional_zone_change(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility"
        && claims_promoted_conditional_zone_change_amendment_family(input)
    {
        return match parse_promoted_conditional_zone_change_amendment(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_destroy_regeneration_family(input) {
        return match parse_promoted_destroy_regeneration(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_atomic_zone_change_family(input) {
        return match parse_promoted_atomic_zone_change(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_exile_anaphora_family(input) {
        return match parse_promoted_exile_anaphora(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_player_compound_exile_family(input) {
        return match parse_promoted_player_compound_exile(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_targeted_hand_disruption_family(input) {
        return match parse_promoted_targeted_hand_disruption(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_controlled_keyword_grant_family(input) {
        return match parse_promoted_controlled_keyword_grant(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_temporary_power_toughness_family(input) {
        return match parse_promoted_temporary_power_toughness(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_damage_life_family(input) {
        return match parse_promoted_damage_life(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "spellAbility" && claims_promoted_plain_damage_family(input) {
        return match parse_promoted_plain_damage(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "staticAbility" && claims_promoted_graveyard_characteristic_family(input) {
        return match parse_promoted_graveyard_characteristic(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    if ability_kind == "triggeredAbility" && claims_promoted_optional_bound_mill_family(input) {
        return match parse_promoted_optional_bound_mill(input) {
            Ok(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            Err(failure) => CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            },
        };
    }

    // Specific native families above retain their semantic commitments.
    // This adapter admits programs structurally, then delegates all action
    // recognition and lowering to the shared typed sequence boundary.
    match try_parse_contextual_effect_program(input) {
        EffectProgramParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        EffectProgramParseAttempt::Unpromoted(failure) => {
            if policy == CanonicalParsePolicy::NativeOnly {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            if unpromoted_candidate_failure.is_none() {
                unpromoted_candidate_failure = Some(failure);
            }
        }
        EffectProgramParseAttempt::Failed(failure)
        | EffectProgramParseAttempt::FailedSupport(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        EffectProgramParseAttempt::NotApplicable => {}
    }

    if policy == CanonicalParsePolicy::NativeOnly {
        return CanonicalParseOutcome::Unsupported;
    }

    let AbilityKindConstraint::Exact(ability_kind) = ability_kind else {
        unreachable!("full canonical parsing always carries an exact ability kind")
    };
    if let Some(draft) = parse_canonical_rule_legacy(input, ability_kind) {
        return match unpromoted_candidate_failure {
            Some(failure) => {
                CanonicalParseOutcome::ParsedLegacyAfterNativeRejection { draft, failure }
            }
            None => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Legacy,
            },
        };
    }
    if let Some(failure) = unpromoted_candidate_failure {
        return CanonicalParseOutcome::Failed {
            failure,
            recognizer: RecognizerProvenance::Native,
        };
    }
    CanonicalParseOutcome::Unsupported
}

fn parse_canonical_rule_legacy(
    input: &AbilityInput<'_>,
    ability_kind: &str,
) -> Option<CanonicalRuleDraft> {
    let source_text = input.source.text.trim();
    // Expansion-specific parsers see the original label first. Generic
    // grammars later discard flavor labels while preserving ability words
    // whose labels carry rules meaning not repeated by the instruction.
    let text = source_text
        .strip_prefix("Vivid — ")
        .or_else(|| source_text.strip_prefix("Vivid â€” "))
        .or_else(|| source_text.strip_prefix("Vivid Ã¢â‚¬â€ "))
        .unwrap_or(source_text);

    if let Some(parsed) = parse_spm_leaf_ability(text, input.face_name) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_ecc_remaining_ability(text, ability_kind) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_tla_leaf_ability(text) {
        return Some(parsed);
    }

    let named_semantic_label = [
        "Landfall",
        "Power-up",
        "Boast",
        "Exhaust",
        "Imprint",
        "Threshold",
        "Domain",
        "Flurry",
    ]
    .iter()
    .any(|label| {
        text.get(..label.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(label))
    });
    let structural_label = text
        .split_once(" — ")
        .map(|(label, _)| label.trim())
        .is_some_and(|label| {
            label == "∞"
                || (!label.is_empty()
                    && label
                        .chars()
                        .all(|character| matches!(character, 'I' | 'V' | 'X' | ',' | ' ')))
        });
    let semantic_label = named_semantic_label || structural_label;
    let text = if semantic_label {
        text
    } else {
        strip_short_oracle_label(text)
    };

    if text.ends_with("can be your commander.") && text.contains("Enduring Bond") {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{ "kind": "commanderEligible" }],
            }),
            &["Recognize commander eligibility"],
        ));
    }

    if text.starts_with("Rulebreaker — ") {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "deckConstructionException": true,
            }),
            &["Record the commander deck-construction exception"],
        ));
    }

    if text == "A deck can have any number of cards named Sphinx's Approach." {
        return Some(draft(
            json!({ "kind": "rulesMarker", "text": text }),
            &["Recognize the named-card deck-construction exception"],
        ));
    }

    if let Some(parsed) = parse_buyback_ability(text) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_cumulative_upkeep_ability(text) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_alternative_cost_ability(text) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_ancient_vendetta(text) {
        return Some(parsed);
    }
    let lower_text = text.to_ascii_lowercase();
    if lower_text.starts_with("choose ")
        && lower_text.contains(" target players. each of them searches their library ")
        && let Some(parsed) = parse_common_zone_and_value_spell(text, input.face_name)
    {
        return Some(parsed);
    }
    if lower_text.starts_with("look at the top ")
        && let Some(parsed) = parse_library_spell(text)
    {
        return Some(parsed);
    }
    if text
        .to_ascii_lowercase()
        .starts_with("as an additional cost to cast this spell,")
        && let Some(parsed) = parse_common_zone_and_value_spell(text, input.face_name)
    {
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
            .or_else(|| parse_shock_land_replacement(text))
            .or_else(|| parse_prepared_replacement(text))
            .or_else(|| parse_composed_entry_replacement(text, input.face_name))
            .or_else(|| parse_avatar_casting_instruction(text))
            .or_else(|| parse_special_static_ability(text))
            .or_else(|| parse_common_static_with_optional_trailing_reminder(text, input.face_name))
            .or_else(|| parse_keyword_ability(text, input.face_name)),
        "keywordAbility" | "keywordAbilityGroup" => parse_keyword_ability(text, input.face_name),
        "activatedAbility" => parse_mana_ability(text)
            .or_else(|| parse_avatar_activated_ability(text))
            .or_else(|| parse_special_activated_ability(text))
            .or_else(|| parse_power_up_activated_ability(text, input.face_name))
            .or_else(|| parse_simple_activated_ability_for_face(text, input.face_name))
            .or_else(|| parse_common_activated_ability(text))
            // The historical static module still owns several executable
            // loyalty parsers. Classification is a dispatch hint; retain the
            // semantically correct rule while those leaves migrate.
            .or_else(|| parse_common_static_with_optional_trailing_reminder(text, input.face_name))
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
            .or_else(|| parse_power_up_activated_ability(text, input.face_name))
            .or_else(|| parse_simple_activated_ability_for_face(text, input.face_name))
            .or_else(|| parse_common_activated_ability(text))
            .or_else(|| parse_keyword_ability(text, input.face_name))
            .or_else(|| parse_spell_cant_be_countered(text)),
        "spellAbility" => parse_simple_spell_ability(text)
            .or_else(|| parse_own_casting_reduction(text))
            .or_else(|| parse_azula_spell_ability(text))
            .or_else(|| parse_common_spell_ability(text))
            .or_else(|| parse_expansion_spell(text))
            .or_else(|| parse_avatar_spell_ability(text))
            .or_else(|| parse_generic_effect_spell(text, input.face_name))
            .or_else(|| parse_avatar_casting_instruction(text))
            .or_else(|| parse_avatar_deck_spell(text))
            .or_else(|| parse_remaining_deck_spell(text))
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
    let parsed = parsed
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
    Some(parsed)
}

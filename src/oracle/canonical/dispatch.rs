use super::*;
use crate::oracle::model::RecognizerProvenance;
use crate::oracle::syntax::AbilityInput;

pub(crate) enum CanonicalParseOutcome {
    Parsed {
        draft: CanonicalRuleDraft,
        recognizer: RecognizerProvenance,
    },

    Unsupported,
    Failed {
        failure: MachineFailure,
        recognizer: RecognizerProvenance,
    },
    /// The first failure is owned. Compatibility feedback remains candidate
    /// evidence and does not become a successful or committed parse.
    FailedWithCandidateEvidence {
        failure: MachineFailure,
        candidate_failure: MachineFailure,
        recognizer: RecognizerProvenance,
    },
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
    parse_canonical_rule_native(input, Some(ability_kind))
}

pub(crate) fn parse_canonical_rule_native(
    input: &AbilityInput<'_>,
    promoted_ability_kind: Option<&str>,
) -> CanonicalParseOutcome {
    parse_native_rule(
        input,
        promoted_ability_kind.map_or(
            AbilityKindConstraint::AnyNative,
            AbilityKindConstraint::Exact,
        ),
    )
}

// The common syntax parent runs before specialized native compatibility paths.
// A failed candidate never commits merely because a child advanced its cursor.
fn parse_native_rule(
    input: &AbilityInput<'_>,
    ability_kind: AbilityKindConstraint<'_>,
) -> CanonicalParseOutcome {
    // CR 309.4: a labeled Dungeon room owns its frame before any ability-word
    // or flavor-label reading of the same em dash.
    if claims_dungeon_room_family(input) {
        return match parse_dungeon_room(input) {
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
    if claims_dungeon_entry_restriction(input) {
        return match parse_dungeon_entry_restriction(input) {
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
    let mut committed_generic_failure = None;
    if ability_kind == "triggeredAbility" {
        // This atomic native contract predates sequence-local decision IDs.
        // Preserve its public binding names until that contract is migrated;
        // only a fully consumed success takes precedence over composition.
        if claims_promoted_source_entry_surveil_family(input) {
            if let Ok(draft) = parse_promoted_source_entry_surveil(input) {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
        }
        // Preserve the established atomic graveyard-exile target identities
        // and primitive representation. Only full native success owns this
        // compatibility contract; additional actions stay with composition.
        if claims_promoted_source_entry_card_zone_change_family(input) {
            if let Ok(draft) = parse_promoted_source_entry_card_zone_change(input) {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
        }
        match try_parse_generic_trigger_program(input) {
            GenericTriggerProgramAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            GenericTriggerProgramAttempt::FailedCommitted(failure) => {
                committed_generic_failure = Some(failure);
            }
            GenericTriggerProgramAttempt::RejectedCandidate(_)
            | GenericTriggerProgramAttempt::NotApplicable => {}
        }
    }
    // Migration diagnostic: measure which historical families the shared
    // static and contextual program parents already supersede. Off by default.
    if generic_first_enabled()
        && let Some(draft) = try_shared_program_parents(input)
    {
        return CanonicalParseOutcome::Parsed {
            draft,
            recognizer: RecognizerProvenance::Native,
        };
    }
    let existing = parse_existing_native_rule(input, ability_kind);
    // A historical family that claimed a phrase and then failed must not hide a
    // complete shared parse. Successful family parses keep their exact IR.
    let existing = if matches!(
        existing,
        CanonicalParseOutcome::Failed { .. }
            | CanonicalParseOutcome::FailedWithCandidateEvidence { .. }
    ) {
        match try_shared_program_parents(input) {
            Some(draft) => CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            },
            None => existing,
        }
    } else {
        existing
    };
    preserve_committed_generic_failure(existing, committed_generic_failure)
}

/// The shared static-continuous and contextual-program parents, without any
/// historical family in front of them.
fn try_shared_program_parents(input: &AbilityInput<'_>) -> Option<CanonicalRuleDraft> {
    if let StaticContinuousParseAttempt::Parsed(draft) = try_parse_static_continuous_effect(input) {
        return Some(draft);
    }
    match try_parse_contextual_effect_program(input) {
        EffectProgramParseAttempt::Parsed(draft) => Some(draft),
        _ => None,
    }
}

/// `DEEPDECK_ORACLE_GENERIC_FIRST=1` runs the shared program parents before the
/// historical families (family-redundancy sieve). Never set in production.
fn generic_first_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("DEEPDECK_ORACLE_GENERIC_FIRST").is_ok_and(|value| value == "1")
    })
}

/// Fully consumed historical successes retain their primitive contract. If no
/// compatibility parser succeeds, preserve the failure of the owning parent.
fn preserve_committed_generic_failure(
    compatibility: CanonicalParseOutcome,
    committed: Option<MachineFailure>,
) -> CanonicalParseOutcome {
    match (compatibility, committed) {
        (parsed @ CanonicalParseOutcome::Parsed { .. }, _) => parsed,
        (
            CanonicalParseOutcome::Failed {
                failure: candidate_failure,
                ..
            },
            Some(failure),
        ) => CanonicalParseOutcome::FailedWithCandidateEvidence {
            failure,
            candidate_failure,
            recognizer: RecognizerProvenance::Native,
        },
        (_, Some(failure)) => CanonicalParseOutcome::Failed {
            failure,
            recognizer: RecognizerProvenance::Native,
        },
        (compatibility, None) => compatibility,
    }
}

fn parse_existing_native_rule(
    input: &AbilityInput<'_>,
    ability_kind: AbilityKindConstraint<'_>,
) -> CanonicalParseOutcome {
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

    if ability_kind == "spellAbility" {
        match try_parse_promoted_scry_family(input) {
            ScryFamilyParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            ScryFamilyParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            ScryFamilyParseAttempt::DeferredToProgram | ScryFamilyParseAttempt::NotApplicable => {}
        }
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

    match try_parse_promoted_zone_prohibition(input) {
        ZoneProhibitionProgramAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        ZoneProhibitionProgramAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        ZoneProhibitionProgramAttempt::NotClaimed => {}
    }

    // A complete typed quota establishes its static context independently
    // from the classifier hint. Unsupported envelopes remain unclaimed.
    match try_parse_promoted_cast_quota(input) {
        CastQuotaProgramAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        CastQuotaProgramAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        CastQuotaProgramAttempt::NotClaimed => {}
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

    if ability_kind == "replacementEffect" && claims_promoted_source_entry_counters_family(input) {
        return match parse_promoted_source_entry_counters(input) {
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
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
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
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
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

    if claims_promoted_typecycling_family(input) {
        return match parse_promoted_typecycling(input) {
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

    if claims_promoted_flashback_family(input) {
        return match parse_promoted_flashback(input) {
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

    if claims_promoted_cost_keyword_family(input) {
        return match parse_promoted_cost_keyword(input) {
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

    if claims_promoted_suspend_family(input) {
        return match parse_promoted_suspend(input) {
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

    if claims_promoted_enchant_family(input) {
        return match parse_promoted_enchant(input) {
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

    let source_combat_restriction_rejection =
        match try_parse_static_source_combat_restriction(input) {
            StaticSourceCombatRestrictionAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            StaticSourceCombatRestrictionAttempt::Rejected(failure) => Some(failure),
            StaticSourceCombatRestrictionAttempt::NotClaimed => None,
        };

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

    if claims_promoted_activation_prohibition_family(input) {
        return match parse_promoted_activation_prohibition(input) {
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
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
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
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
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

    // A complete shared action program supersedes the historical partition
    // recipe. A failed speculative program leaves other complete families live.
    if ability_kind == "spellAbility"
        && input
            .tokens
            .first()
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("look"))
        && let EffectProgramParseAttempt::Parsed(rule) = try_parse_contextual_effect_program(input)
    {
        return CanonicalParseOutcome::Parsed {
            draft: rule,
            recognizer: RecognizerProvenance::Native,
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
        match try_parse_promoted_activated_card_zone_change(input) {
            ActivatedCardZoneProgramAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            ActivatedCardZoneProgramAttempt::FailedCommitted(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            ActivatedCardZoneProgramAttempt::RejectedCandidate(_) => {}
        }
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

    if ability_kind == "spellAbility" && claims_promoted_damage_life_family(input) {
        match try_parse_promoted_damage_family(input, "effect-sequence") {
            DamageFamilyParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            DamageFamilyParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            // The shared contextual parent below owns this new node and its
            // producer scope. The compatibility adapter commits no draft.
            DamageFamilyParseAttempt::DeferredToContextualProgram => {}
        }
    }

    if ability_kind == "spellAbility" && claims_promoted_plain_damage_family(input) {
        match try_parse_promoted_damage_family(input, "plain-damage-effect") {
            DamageFamilyParseAttempt::Parsed(draft) => {
                return CanonicalParseOutcome::Parsed {
                    draft,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            DamageFamilyParseAttempt::Failed(failure) => {
                return CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                };
            }
            // The shared contextual parent below owns this new node and its
            // producer scope. The compatibility adapter commits no draft.
            DamageFamilyParseAttempt::DeferredToContextualProgram => {}
        }
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

    // This typed event attempt, rather than a classification hint or a second
    // lexical prefix scan, owns the quantity-replacement commitment.
    match try_parse_promoted_quantity_replacement(input) {
        QuantityReplacementParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        QuantityReplacementParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        QuantityReplacementParseAttempt::NotClaimed => {}
    }

    // A complete collection/keyword envelope proves the static context;
    // classification hints cannot substitute for this typed structural result.
    // Specific conditional, attached, Riot and temporary families run first.
    match try_parse_casting_permission(input) {
        CastingPermissionProgramAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        CastingPermissionProgramAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        CastingPermissionProgramAttempt::NotClaimed => {}
    }

    match try_parse_trigger_multiplicity(input) {
        TriggerMultiplicityParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        TriggerMultiplicityParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        TriggerMultiplicityParseAttempt::NotClaimed => {}
    }

    match try_parse_static_granted_activation(input) {
        StaticGrantedActivationParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        StaticGrantedActivationParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        StaticGrantedActivationParseAttempt::NotClaimed => {}
    }

    match try_parse_promoted_static_collection_keyword_grant(input) {
        StaticCollectionKeywordParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        StaticCollectionKeywordParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        StaticCollectionKeywordParseAttempt::NotClaimed => {}
    }

    // A committed static-frame failure never hides a later reader; it is
    // surfaced only if no reader below claims the ability.
    let static_continuous_rejection = match try_parse_static_continuous_effect(input) {
        StaticContinuousParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        StaticContinuousParseAttempt::Rejected(failure) => Some(failure),
        StaticContinuousParseAttempt::NotClaimed => None,
    };

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
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
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

    // Established native families above keep their precedence. A remaining
    // source transition delegates its complete body to the shared program.
    match try_parse_promoted_source_trigger_program(input) {
        SourceTriggerProgramParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        SourceTriggerProgramParseAttempt::Unpromoted(failure)
        | SourceTriggerProgramParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        SourceTriggerProgramParseAttempt::NotClaimed => {}
    }

    // Existing native event families retain priority. This late adapter
    // consumes one typed cast event and delegates its complete effect body.
    match try_parse_promoted_spell_cast_trigger_program(input) {
        SpellCastTriggerProgramParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        SpellCastTriggerProgramParseAttempt::Unpromoted(failure)
        | SpellCastTriggerProgramParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        SpellCastTriggerProgramParseAttempt::NotClaimed => {}
    }

    // Actor/object sacrifice event; the complete body remains shared.
    match try_parse_promoted_sacrifice_trigger_program(input) {
        SacrificeTriggerProgramParseAttempt::Parsed(draft) => {
            return CanonicalParseOutcome::Parsed {
                draft,
                recognizer: RecognizerProvenance::Native,
            };
        }
        SacrificeTriggerProgramParseAttempt::Unpromoted(failure)
        | SacrificeTriggerProgramParseAttempt::FailedSupport(failure)
        | SacrificeTriggerProgramParseAttempt::Failed(failure) => {
            return CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            };
        }
        SacrificeTriggerProgramParseAttempt::NotClaimed => {}
    }

    match static_continuous_rejection.or(source_combat_restriction_rejection) {
        Some(failure) => CanonicalParseOutcome::Failed {
            failure,
            recognizer: RecognizerProvenance::Native,
        },
        None => CanonicalParseOutcome::Unsupported,
    }
}

#[cfg(test)]
#[path = "generic_trigger_dispatch_tests.rs"]
mod generic_trigger_dispatch_tests;

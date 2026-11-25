mod ability_frame_claim;
mod ability_loss_destroy;
mod ability_signal;
mod activated_top_card_delayed_draw;
mod activated_zone_change;
mod activation_restriction;
mod additional_phase;
mod amass;
mod attack_tax;
mod block_restriction;
mod card_action;
mod card_type_partition;
mod cast_quota;
mod casting_reduction;
mod chosen_card_name;
mod chosen_creature_type;
mod chosen_land_type;
mod chosen_operand;
mod collection_destroy;
mod color_set_query;
mod condition;
mod conditional_entry;
mod conditional_top_card_partition;
mod conditional_zone_change;
mod conditional_zone_change_amendment;
mod contextual_effect_program;
mod controlled_count_entry;
mod counter;
mod counter_delayed_draw;
mod counter_descriptive_token;
mod counter_named_token;
mod counter_unless_paid;
mod damage;
mod damage_replacement;
mod delayed_blink;
mod delayed_draw;
mod delve;
mod draw_move;
mod early_turn_entry;
mod earthbend;
mod effect_program;
mod effect_sequence;
mod mana_result_riders;
mod stack_copy_lowering;
mod emblem;
mod empower_jace;
mod entry_counters;
mod entry_frame;
mod entry_or_repeat_trigger;
mod event_bound_payment;
mod exile;
mod explicit_token_creation;
mod explore_program;
mod graveyard_card_entry_life;
mod graveyard_card_return;
mod graveyard_characteristic;
mod graveyard_destination;
mod keyword_action_programs;
mod life_gain_replacement;
mod named_token_creation;
mod player_condition;
mod program_resolution;
mod rebound;
mod resolution_choice;
mod resolution_object_choice;
mod sacrifice;
mod sentence_condition;
mod trigger_event;
mod trigger_program;
mod zone_prohibition;

mod hand_disruption;
mod player_discard;
mod player_event_lowering;
mod player_sacrifice;
mod player_set;
mod quantified_player_action;

mod become_types;
mod continuous_type_change;
mod keyword_grant;
mod labeled_activated_zone_change;
mod land_play_trigger;
mod landfall_reflexive_counter;
mod library_partition;
mod library_search_move;
mod library_search_partition;
mod linked_exile_return;
mod loyalty_ability;

mod mana_lowering;
mod mana_program_support;
mod mobilize;
mod modal_color_guard;
mod modal_spell;
mod numeric;
mod object_predicate;
mod object_relation;
mod opening_hand_battlefield;
mod optional_bound_mill;
mod optional_card_deployment;
mod optional_life_entry;
mod optional_zone_action;
mod ordinal_spell_tax_draw;
mod owner_library_reveal;
mod permanent_died_effect_sequence;
mod permanent_entered_effect_sequence;
mod phase_trigger_sequence;
mod player_counter_sweep;
mod player_designation;

mod casting_permission;
mod damage_prevention;
mod permanent_operand;
mod permanent_selection;
mod player_permission;
mod player_spell_tax_draw;
mod protected_attack_trigger;
mod quantity_replacement;
mod retarget;
mod sacrifice_trigger_program;
mod same_name_search;
mod scoped_object_query;
mod scry;
mod selection;

mod replacement_event;
mod resource_payment;
mod simple_keyword;
mod source_cast_x_entry;
mod source_entry_card_zone_change;
mod source_entry_effect_sequence;
mod source_entry_tapped;
mod source_move;
mod source_trigger_program;
mod spell_cast_trigger_program;
mod spell_cost_increase;
mod spell_set_grant;
mod static_collection_keyword;
mod static_continuous;
mod static_damage_prevention;
mod static_granted_activation;
mod static_keyword_grant;
mod static_player_predicate;
mod static_source_combat_restriction;
mod static_spell_reduction;
mod targeting_trigger;
mod targetless;
mod temporary_power_toughness;
mod trigger_multiplicity;

mod top_card_partition;
mod top_cards_reorder;
mod trigger_suppression;
mod uncast_entry_replacement;
mod zone_change;
mod zone_change_search;
mod zone_change_token;

pub(super) use ability_frame_claim::*;
pub(super) use ability_loss_destroy::*;
pub(super) use ability_signal::*;
pub(super) use activated_top_card_delayed_draw::*;
pub(super) use activated_zone_change::*;
pub(super) use activation_restriction::*;
pub(super) use additional_phase::*;
pub(super) use amass::*;
pub(super) use attack_tax::*;
pub(super) use block_restriction::*;
pub(super) use card_action::*;
pub(super) use card_type_partition::*;
pub(super) use cast_quota::*;
pub(super) use casting_reduction::*;
pub(super) use chosen_card_name::*;
pub(super) use chosen_creature_type::*;
pub(super) use chosen_land_type::*;
pub(super) use collection_destroy::*;
pub(super) use condition::*;
pub(super) use conditional_entry::*;
pub(super) use conditional_top_card_partition::*;
pub(super) use conditional_zone_change::*;
pub(super) use conditional_zone_change_amendment::*;
pub(super) use contextual_effect_program::*;
pub(super) use controlled_count_entry::*;
pub(super) use counter::*;
pub(super) use counter_delayed_draw::*;
pub(super) use counter_descriptive_token::*;
pub(super) use counter_named_token::*;
pub(super) use counter_unless_paid::*;
pub(super) use damage::*;
pub(super) use damage_replacement::*;
pub(super) use delayed_blink::*;
pub(super) use delayed_draw::*;
pub(super) use delve::*;
pub(super) use draw_move::*;
pub(super) use early_turn_entry::*;
pub(super) use earthbend::*;
pub(super) use effect_program::*;
pub(super) use effect_sequence::*;
pub(super) use mana_result_riders::*;
pub(super) use stack_copy_lowering::*;
pub(super) use emblem::*;
pub(super) use empower_jace::*;
pub(super) use entry_counters::*;
pub(super) use entry_frame::*;
pub(super) use entry_or_repeat_trigger::*;
pub(super) use event_bound_payment::*;
pub(super) use exile::*;
pub(super) use explicit_token_creation::*;
pub(super) use explore_program::*;
pub(super) use graveyard_card_entry_life::*;
pub(super) use graveyard_card_return::*;
pub(super) use graveyard_characteristic::*;
pub(super) use graveyard_destination::*;
pub(super) use keyword_action_programs::*;
pub(super) use life_gain_replacement::*;
pub(super) use named_token_creation::*;
pub(super) use program_resolution::*;
pub(super) use rebound::*;
pub(super) use sacrifice::*;
pub(super) use zone_prohibition::*;

pub(super) use hand_disruption::*;
pub(super) use player_discard::*;
pub(super) use player_event_lowering::*;
pub(super) use player_sacrifice::*;
pub(super) use player_set::*;
pub(super) use quantified_player_action::*;

pub(super) use become_types::*;
pub(super) use continuous_type_change::*;
pub(super) use labeled_activated_zone_change::*;
pub(super) use land_play_trigger::*;
pub(super) use landfall_reflexive_counter::*;
pub(super) use library_partition::*;
pub(super) use library_search_move::*;
pub(super) use library_search_partition::*;
pub(super) use linked_exile_return::*;
pub(super) use loyalty_ability::*;

pub(super) use color_set_query::*;
pub(super) use mana_lowering::*;
pub(super) use mana_program_support::*;
pub(super) use mobilize::*;
pub(super) use modal_color_guard::*;
pub(super) use modal_spell::*;
pub(super) use numeric::*;
pub(super) use object_predicate::*;
pub(super) use object_relation::*;
pub(super) use opening_hand_battlefield::*;
pub(super) use optional_bound_mill::*;
pub(super) use optional_card_deployment::*;
pub(super) use optional_life_entry::*;
pub(super) use optional_zone_action::*;
pub(super) use ordinal_spell_tax_draw::*;
pub(super) use owner_library_reveal::*;
pub(super) use permanent_died_effect_sequence::*;
pub(super) use permanent_entered_effect_sequence::*;
pub(super) use phase_trigger_sequence::*;
pub(super) use player_counter_sweep::*;
pub(super) use player_designation::*;

pub(super) use casting_permission::*;
pub(super) use damage_prevention::*;
pub(super) use deck_construction_keyword::*;
pub(super) use permanent_operand::*;
pub(super) use permanent_selection::*;
pub(super) use player_permission::*;
pub(super) use player_spell_tax_draw::*;
pub(super) use protected_attack_trigger::*;
pub(super) use quantity_replacement::*;
pub(super) use retarget::*;
pub(super) use sacrifice_trigger_program::*;
pub(super) use same_name_search::*;
pub(super) use scoped_object_query::*;
pub(super) use scry::*;
pub(super) use selection::*;

pub(super) use replacement_event::*;
pub(super) use resource_payment::*;
pub(super) use simple_keyword::*;
pub(super) use source_cast_x_entry::*;
pub(super) use source_entry_card_zone_change::*;
pub(super) use source_entry_effect_sequence::*;
pub(super) use source_entry_tapped::*;
pub(super) use source_move::*;
pub(super) use source_trigger_program::*;
pub(super) use spell_cast_trigger_program::*;
pub(super) use spell_cost_increase::*;
pub(super) use spell_set_grant::*;
pub(super) use static_collection_keyword::*;
pub(super) use static_continuous::*;
pub(super) use static_damage_prevention::*;
pub(super) use static_granted_activation::*;
pub(super) use static_keyword_grant::*;
pub(super) use static_player_predicate::*;
pub(super) use static_source_combat_restriction::*;
pub(super) use static_spell_reduction::*;
pub(super) use targeting_trigger::*;
pub(super) use targetless::*;
pub(super) use temporary_power_toughness::*;
pub(super) use trigger_multiplicity::*;

pub(super) use top_card_partition::*;
pub(super) use top_cards_reorder::*;
pub(super) use trigger_suppression::*;
pub(super) use uncast_entry_replacement::*;
pub(super) use zone_change::*;
pub(super) use zone_change_search::*;
pub(super) use zone_change_token::*;

pub(super) use player_condition::*;
pub(super) use trigger_event::*;
pub(super) use trigger_program::*;

mod permanent_subject;
pub(in crate::oracle::canonical) use keyword_grant::lower_keyword_grant_to_object;
pub(in crate::oracle::canonical) use permanent_subject::{
    bound_permanent_reference_value, bound_player_reference_value, lower_bound_permanent_reference,
    lower_ordinary_permanent_subject, ordinary_action_targets,
};

#[cfg(test)]
#[path = "activated_combat_restriction_tests.rs"]
mod activated_combat_restriction_tests;
#[cfg(test)]
#[path = "choose_instruction_tests.rs"]
mod choose_instruction_tests;
#[cfg(test)]
#[path = "choose_object_choice_tests.rs"]
mod choose_object_choice_tests;
#[cfg(test)]
#[path = "destroy_regeneration_tests.rs"]
mod destroy_regeneration_tests;
#[cfg(test)]
#[path = "for_each_multiplier_tests.rs"]
mod for_each_multiplier_tests;
#[cfg(test)]
#[path = "league_reference_concepts_tests.rs"]
mod league_reference_concepts_tests;
#[cfg(test)]
#[path = "league_regression_concepts_tests.rs"]
mod league_regression_concepts_tests;
#[cfg(test)]
#[path = "player_permission_selection_tests.rs"]
mod player_permission_selection_tests;
#[cfg(test)]
#[path = "quantity_result_reuse_tests.rs"]
mod quantity_result_reuse_tests;
#[cfg(test)]
#[path = "regenerate_tests.rs"]
mod regenerate_tests;
#[cfg(test)]
#[path = "static_characteristics_concepts_tests.rs"]
mod static_characteristics_concepts_tests;
#[cfg(test)]
#[path = "storm_keyword_tests.rs"]
mod storm_keyword_tests;

#[cfg(test)]
#[path = "attach_tests.rs"]
mod attach_tests;
mod deck_construction_keyword;

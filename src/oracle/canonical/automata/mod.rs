//! Typed local recognizers for composable Oracle grammar concepts.
//!
//! These machines borrow one immutable token stream. They never reconstruct
//! or rematch an Oracle sentence and they emit typed semantic values rather
//! than card-shaped JSON.

mod ability_loss;
mod ability_word_envelope;
mod action_result;
mod activated;
mod activated_effect_sequence;
mod activated_library_search;
mod activated_top_card_delayed_draw;
mod activated_zone_change;
mod activation_reduction;
mod activation_restriction;
mod activation_timing_annotation;
mod actor_object_event;
mod additional_cost;
mod alternative_cost;
mod alternative_return_cost;
mod amass;
mod arithmetic_expression;
mod attach;
mod attack_tax;
mod become_types;
mod binding_environment;
mod block_restriction;
mod boolean_condition;
mod bound_collection_untap;
mod bound_object_condition;
mod card_action;
mod card_collection_operand;
mod card_library_top;
mod card_move_destination;
mod card_selection_operand;
mod card_target_operand;
mod card_type_partition;
mod card_zone_collection;
mod cast_prohibition;
mod cast_quota;
mod casting_permission;
mod casting_reduction;
mod causative;
mod characteristic;
mod choose_instruction;
mod chosen_card_name;
mod chosen_creature_type;
mod chosen_land_type;
mod clause;
mod collection;
mod color;
mod color_set;
mod color_set_mana;
mod color_set_query;
mod combat_restriction;
mod compound_activated;
mod compound_exile;
mod condition;
mod conditional;
mod conditional_activation;
mod conditional_entry;
mod conditional_zone_change;
mod conditional_zone_change_amendment;
mod connector;
mod container_body;
mod contextual_list;
mod control_change;
mod controlled_count_entry;
mod cost;
mod cost_keyword;
mod counter_delayed_draw;
mod counter_descriptive_token;
mod counter_event;
mod counter_name;
mod counter_named_token;
mod counter_prohibition;
mod counter_unless_paid;
mod damage_clause;
mod damage_event;
mod damage_prevention;
mod damage_replacement;
mod deck_construction_keyword;
mod delayed_blink;
mod delayed_draw;
mod delayed_step;
mod delayed_step_timing;
mod delve;
mod designation;
mod destroy_collection;
mod destroy_regeneration;
mod draw_move;
mod dungeon_room;
mod duration;
mod early_turn_entry;
mod earthbend;
mod effect_head;
mod effect_program;
mod effects;
mod embedded_delayed_trigger;
mod emblem;
mod empower_jace;
mod enchant;
mod entry_counters;
mod entry_frame;
mod entry_or_repeat_trigger;
pub(in crate::oracle::canonical) mod entry_replacement;
mod equip;
mod event_actor;
mod event_bound_payment;
mod event_clause;
mod event_modifiers;
mod event_quantity_amount;
mod flashback;
mod for_each_count;
mod graveyard_card_entry_life;
mod graveyard_card_return;
mod graveyard_characteristic;
mod graveyard_destination;
mod hand_disruption;
mod investigate;
mod keyword;
mod keyword_action;
mod keyword_action_reminder;
mod keyword_grant;
mod keyword_template;
mod labeled_activated_effect_sequence;
mod landfall_reflexive_counter;
mod library_search;
mod library_search_partition;
mod life_clause;
mod life_gain_replacement;
mod linked_exile_return;
mod loyalty;
mod loyalty_ability;
mod mana;
mod mana_cost_keyword;
mod mana_spend_constraint;
mod mana_spending_policy;
mod mandatory_additional_cost_header;
mod mill;
mod mobilize;
mod modal_color_guard;
mod modal_spell;
mod named_token_coordination;
mod numeric;
mod numeric_object_property;
mod object_argument;
mod object_predicate;
mod object_rule_clause;
mod object_set;
mod opening_hand_battlefield;
mod opponent_library_search;
mod optional_bound_mill;
mod optional_card_deployment;
mod optional_draw;
mod optional_life_entry;
mod optional_zone_action;
mod ordinal_spell_tax_draw;
mod ordinary_player_discard;
mod ordinary_sacrifice;
mod overload;
mod owner_library_reveal;
mod parameterized_keyword;
mod parenthetical;
mod payment_action;
mod permanent_died;
mod permanent_died_effect_sequence;
mod permanent_entered_effect_sequence;
mod permanent_operand;
mod permanent_selection;
mod phase_trigger_sequence;
mod phyrexian_mana;
mod player_action;
mod player_condition;
mod player_counter_sweep;
mod player_designation;
mod player_event;
mod player_numeric_fact;
mod player_permission;
mod player_sacrifice;
mod player_set;
mod player_spell_tax_draw;
mod power_toughness;
mod protected_attack_trigger;
mod protection;
mod put_counters;
mod qualified_effect;
mod quantified_event;
mod quantified_player_action;
mod quantity_event;
mod quantity_replacement;
mod rebound;
mod reference;
mod regenerate;
mod relative_clause;
mod repetition;
mod replacement_event;
mod replacing_conditional;
mod replicate;
mod resolution_cost;
mod resource_payment;
mod restricted_mana;
mod retarget_action;
mod riot;
mod sacrifice_event;
mod saga;
mod same_name_search;
mod scoped_mana;
mod scoped_object_query;
mod scry;
mod selector;
mod selector_atom;
mod sentence_condition;
mod sequence;
mod set_anaphora;
mod shuffle_into;
mod simple_keyword;
mod source_cast_x_entry;
mod source_entry;
mod source_entry_card_zone_change;
mod source_entry_effect_sequence;
mod source_event_union;
mod source_exclusion;
mod source_sacrifice;
mod source_state;
mod source_trigger_envelope;
mod source_zone_move;
mod spell_cast_event;
mod spell_cost_increase;
mod stack_copy;
mod static_collection_keyword;
mod static_condition;
mod static_granted_activation;
mod static_keyword_grant;
mod static_player_predicate;
mod static_spell_reduction;
mod surveil;
mod suspend;
mod target_card_zone_change;
mod target_domain;
mod target_list;
mod target_untap;
mod targeting_event;
mod temporary_power_toughness;
mod token;
mod token_creation_action;
mod token_descriptor;
mod token_subtype_list;
mod top_cards;
mod top_cards_reorder;
mod transform;
mod trigger_envelope;
mod trigger_event;
mod trigger_guard;
mod trigger_multiplicity;
mod trigger_program;
mod trigger_suppression;
mod turn_step_event;
mod type_change;
mod typecycling;
mod typed_target_reference;
mod unless_clause;
mod verb_lexicon;
mod where_x_definition;
mod zone_change_until;
mod zone_list;
mod zone_prohibition;

pub(super) use ability_loss::*;
pub(super) use ability_word_envelope::*;
pub(super) use action_result::*;
pub(super) use activated::*;
pub(super) use activated_effect_sequence::*;
pub(super) use activated_library_search::*;
pub(super) use activated_top_card_delayed_draw::*;
pub(super) use activated_zone_change::*;
pub(super) use activation_reduction::*;
pub(super) use activation_restriction::*;
pub(super) use activation_timing_annotation::*;
pub(super) use actor_object_event::*;
pub(super) use additional_cost::*;
pub(super) use alternative_cost::*;
pub(super) use alternative_return_cost::*;
pub(super) use amass::*;
pub(super) use arithmetic_expression::{
    equal_to_quantity_family_starts, recognize_equal_to_amount_expression,
    recognize_event_bound_numeric_expression, recognize_query_numeric_expression,
    recognize_variable_definition_expression,
};
pub(super) use attach::*;
pub(super) use attack_tax::*;
pub(super) use become_types::*;
pub(super) use binding_environment::*;
pub(super) use block_restriction::*;
pub(super) use bound_collection_untap::*;
pub(super) use bound_object_condition::*;
pub(super) use card_action::*;
pub(super) use card_collection_operand::*;
pub(super) use card_library_top::*;
pub(super) use card_move_destination::*;
pub(super) use card_selection_operand::*;
pub(super) use card_target_operand::*;
pub(super) use card_type_partition::*;
pub(super) use card_zone_collection::*;
pub(super) use cast_prohibition::*;
pub(super) use cast_quota::*;
pub(super) use casting_permission::*;
pub(super) use casting_reduction::*;
pub(in crate::oracle::canonical) use causative::causative_action_starts;
pub(super) use characteristic::*;
pub(super) use choose_instruction::*;
pub(super) use chosen_card_name::*;
pub(super) use chosen_creature_type::*;
pub(super) use chosen_land_type::*;
pub(super) use clause::*;
pub(super) use collection::*;
pub(super) use color::*;
pub(super) use color_set::*;
pub(super) use color_set_mana::*;
pub(super) use color_set_query::*;
pub(super) use combat_restriction::*;
pub(super) use compound_activated::*;
pub(super) use compound_exile::*;
pub(super) use condition::*;
pub(super) use conditional::*;
pub(super) use conditional_activation::*;
pub(super) use conditional_entry::*;
pub(super) use conditional_zone_change::*;
pub(super) use conditional_zone_change_amendment::*;
pub(super) use connector::*;
pub(super) use container_body::*;
pub(super) use contextual_list::*;
pub(super) use control_change::*;
pub(super) use controlled_count_entry::*;
pub(super) use cost::*;
pub(super) use cost_keyword::*;
pub(super) use counter_delayed_draw::*;
pub(super) use counter_descriptive_token::*;
pub(super) use counter_event::*;
pub(super) use counter_name::*;
pub(super) use counter_named_token::*;
pub(super) use counter_prohibition::*;
pub(super) use counter_unless_paid::*;
pub(super) use damage_clause::*;
pub(super) use damage_event::*;
pub(super) use damage_prevention::*;
pub(super) use damage_replacement::*;
pub(super) use delayed_blink::*;
pub(super) use delayed_draw::*;
pub(super) use delayed_step::*;
pub(super) use delayed_step_timing::*;
pub(super) use delve::*;
pub(super) use designation::*;
pub(super) use destroy_collection::*;
pub(super) use destroy_regeneration::*;
pub(super) use draw_move::*;
pub(super) use dungeon_room::*;
pub(super) use duration::*;
pub(super) use early_turn_entry::*;
pub(super) use earthbend::*;
pub(super) use effect_head::*;
pub(super) use effect_program::*;
pub(super) use effects::*;
pub(super) use embedded_delayed_trigger::*;
pub(super) use emblem::*;
pub(super) use empower_jace::*;
pub(super) use enchant::*;
pub(super) use entry_counters::*;
pub(super) use entry_frame::*;
pub(super) use entry_or_repeat_trigger::*;
pub(super) use equip::*;
pub(super) use event_actor::*;
pub(super) use event_bound_payment::*;
pub(super) use event_clause::*;
pub(super) use event_modifiers::*;
pub(super) use event_quantity_amount::*;
pub(super) use flashback::*;
pub(super) use for_each_count::*;
pub(super) use graveyard_card_entry_life::*;
pub(super) use graveyard_card_return::*;
pub(super) use graveyard_characteristic::*;
pub(super) use graveyard_destination::*;
pub(super) use hand_disruption::*;
pub(super) use investigate::*;
pub(super) use keyword::*;
pub(super) use keyword_action::*;
pub(super) use keyword_action_reminder::*;
pub(super) use keyword_grant::*;
pub(super) use keyword_template::*;
pub(super) use labeled_activated_effect_sequence::*;
pub(super) use landfall_reflexive_counter::*;
pub(super) use library_search::*;
pub(super) use library_search_partition::*;
pub(super) use life_clause::*;
pub(super) use life_gain_replacement::*;
pub(super) use linked_exile_return::*;
pub(super) use loyalty::*;
pub(super) use loyalty_ability::*;
pub(super) use mana::*;
pub(super) use mana_cost_keyword::*;
pub(super) use mana_spend_constraint::*;
pub(super) use mana_spending_policy::*;
pub(super) use mandatory_additional_cost_header::*;
pub(super) use mill::*;
pub(super) use mobilize::*;
pub(super) use modal_color_guard::*;
pub(super) use modal_spell::*;
pub(super) use named_token_coordination::*;
pub(super) use numeric::*;
pub(super) use numeric_object_property::*;
pub(super) use object_argument::*;
pub(super) use object_predicate::*;
pub(super) use object_rule_clause::*;
pub(super) use object_set::*;
pub(super) use opening_hand_battlefield::*;
pub(super) use opponent_library_search::*;
pub(super) use optional_bound_mill::*;
pub(super) use optional_card_deployment::*;
pub(super) use optional_draw::*;
pub(super) use optional_life_entry::*;
pub(super) use optional_zone_action::*;
pub(super) use ordinal_spell_tax_draw::*;
pub(super) use ordinary_player_discard::*;
pub(super) use ordinary_sacrifice::*;
pub(super) use overload::*;
pub(super) use owner_library_reveal::*;
pub(super) use parameterized_keyword::*;
pub(super) use parenthetical::*;
pub(super) use payment_action::*;
pub(super) use permanent_died::*;
pub(super) use permanent_died_effect_sequence::*;
pub(super) use permanent_entered_effect_sequence::*;
pub(super) use permanent_operand::*;
pub(super) use permanent_selection::*;
pub(super) use phase_trigger_sequence::*;
pub(super) use phyrexian_mana::*;
pub(super) use player_action::*;
pub(super) use player_counter_sweep::*;
pub(super) use player_designation::*;
pub(super) use player_event::*;
pub(super) use player_numeric_fact::*;
pub(super) use player_permission::*;
pub(super) use player_sacrifice::*;
pub(super) use player_set::*;
pub(super) use player_spell_tax_draw::*;
pub(super) use power_toughness::*;
pub(super) use protected_attack_trigger::*;
pub(super) use protection::*;
pub(super) use put_counters::*;
pub(super) use qualified_effect::*;
pub(super) use quantified_event::*;
pub(super) use quantified_player_action::*;
pub(super) use quantity_event::*;
pub(super) use quantity_replacement::*;
pub(super) use rebound::*;
pub(super) use reference::*;
pub(super) use regenerate::*;
pub(super) use relative_clause::*;
pub(super) use repetition::*;
pub(super) use replacement_event::*;
pub(super) use replacing_conditional::*;
pub(super) use replicate::*;
pub(super) use resolution_cost::*;
pub(super) use resource_payment::*;
pub(super) use restricted_mana::*;
pub(super) use retarget_action::*;
pub(super) use riot::*;
pub(super) use sacrifice_event::*;
pub(super) use saga::*;
pub(super) use same_name_search::*;
pub(super) use scoped_mana::*;
pub(super) use scoped_object_query::*;
pub(super) use scry::*;
pub(super) use selector::*;
pub(super) use selector_atom::*;
pub(super) use sequence::*;
pub(super) use set_anaphora::*;
pub(super) use shuffle_into::*;
pub(super) use simple_keyword::*;
pub(super) use source_cast_x_entry::*;
pub(super) use source_entry::*;
pub(super) use source_entry_card_zone_change::*;
pub(super) use source_entry_effect_sequence::*;
pub(super) use source_event_union::*;
pub(super) use source_exclusion::*;
pub(super) use source_sacrifice::atomic_action::player_subject::{
    ObjectPlayerRelation, ObjectSubjectAntecedent, defending_player_starts,
};
pub(super) use source_sacrifice::*;
pub(super) use source_state::*;
pub(super) use source_trigger_envelope::*;
pub(super) use source_zone_move::*;
pub(super) use spell_cast_event::*;
pub(super) use spell_cost_increase::*;
pub(super) use stack_copy::*;
pub(super) use static_collection_keyword::*;
pub(super) use static_granted_activation::*;
pub(super) use static_keyword_grant::*;
pub(super) use static_player_predicate::*;
pub(super) use static_spell_reduction::*;
pub(super) use surveil::*;
pub(super) use suspend::*;
pub(super) use target_card_zone_change::*;
pub(super) use target_domain::*;
pub(super) use target_list::*;
pub(super) use target_untap::*;
pub(super) use targeting_event::*;
pub(super) use temporary_power_toughness::*;
pub(super) use token::*;
pub(super) use token_creation_action::*;
pub(super) use token_descriptor::*;
pub(super) use top_cards::*;
pub(super) use top_cards_reorder::*;
pub(super) use transform::*;
pub(super) use trigger_event::*;
pub(super) use trigger_multiplicity::*;
pub(super) use trigger_suppression::*;
pub(super) use turn_step_event::*;
pub(super) use type_change::*;
pub(super) use typecycling::*;
pub(super) use typed_target_reference::*;
pub(super) use unless_clause::*;
pub(super) use verb_lexicon::*;
pub(super) use where_x_definition::*;
pub(super) use zone_change_until::*;
pub(super) use zone_list::*;
pub(super) use zone_prohibition::*;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalObjectReference {
    SourceSelf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacementObjectBinding {
    EventObject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct WouldEnterBattlefieldEventNode {
    pub object: ObjectPredicate,
    pub binding: ReplacementObjectBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ObjectCastStatus {
    NotCast,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ObjectCastConditionNode {
    pub object: ReplacementObjectBinding,
    pub status: ObjectCastStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ReplacementDestination {
    Exile,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ZoneReplacementNode {
    pub object: ReplacementObjectBinding,
    pub destination: ReplacementDestination,
}

/// Parent-owned bindings available while recognizing an independent effect.
///
/// Ownership and implicit anaphora are separate. The
/// ability owner remains available for explicit source names. A separate typed
/// event-subject binding proves source anaphora; local lexical scopes supersede
/// it. Numeric casting-X provenance remains an independent contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TypedObservedEventObject {
    EnteringPermanent,
    /// The one card this program just put onto the battlefield (CR 400.7:
    /// the effect that moved it can find the new object).
    MovedOntoBattlefield,
    /// The tokens the immediately preceding creation action created.
    CreatedTokens,
    /// The other combatant of a block event (`that creature` after `blocks
    /// a creature` / `becomes blocked by a creature`), bound by the runtime
    /// record with its object identity (CR 400.7).
    CombatCounterpart,
    /// The permanent the event record binds (the damaged permanent, the
    /// attacker, the dying permanent, the combat damage source), denoted only
    /// while it is the incarnation the event saw (CR 400.7).
    EventPermanent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentEffectReference {
    Source(LocalObjectReference),
    Target(TargetBinding),
    /// Every object one plural target slot declared (`those creatures`,
    /// `they` after `tap up to two target creatures`, CR 115.1, 608.2c).
    TargetSet(TargetBinding),
    Observed(TypedObservedEventObject),
    /// `enchanted creature` / `equipped creature`: the permanent the ability
    /// source is attached to (CR 301.5a, 303.4a), resolved at execution.
    Attached(AttachedObjectRole),
}

/// The parent that owns ability-restriction sentences (`Activate only ...`,
/// `This ability triggers only ...`). The shared sequence stops before such a
/// sentence only for its owner; any other parent must reject it explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AbilityRestrictionOwner {
    #[default]
    None,
    Activated,
    Triggered,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct EffectRecognitionContext {
    pub ability_restriction_owner: AbilityRestrictionOwner,
    pub activation_source_availability: ExplicitActivationSourceAvailability,
    pub permanent_subject_scope: PermanentSubjectScope,
    /// The declared selector of the singular target that `permanent_subject_scope`
    /// denotes. A nominal anaphor (`that creature`) is validated against it.
    pub permanent_subject_selector: Option<TargetSelector>,
    pub observed_source_relationship: ObservedSourceRelationship,
    pub source: Option<LocalObjectReference>,
    pub source_anaphor_binding: SourceAnaphorBinding,
    pub source_cast_x: bool,
    pub zone_change_domains: ZoneChangeDomains,
    pub target_reference_support: TargetReferenceSupport,
    pub observed_event_object: Option<TypedObservedEventObject>,
    /// Sentence-level duration (`Until end of turn, ...`) that each continuous
    /// effect action of the sentence must adopt instead of a trailing duration.
    pub preposed_duration: Option<EffectDurationNode>,
    /// Grammatical subject shared by a coordinated verb (`... and becomes`,
    /// `... and gains`). Set only by the sequence, after `and`, when the
    /// previous action is a continuous effect on one proven permanent.
    pub verb_chain_subject: Option<PermanentEffectReference>,
    /// Set only by parents whose lowering passes through the shared sequence
    /// lowerer, which owns sentence-conditional ranges. Family parents that
    /// destructure actions directly never receive a conditional sentence.
    pub sentence_conditionals: bool,
    /// Set only by the program parent that reads a replacing sentence
    /// (`If COND, instead BODY.`, `replacing_conditional.rs`) and gives its
    /// conditional the replacement form; every other parent keeps `instead`
    /// after a condition an explicit failure.
    pub replacing_conditionals: bool,
    /// Whether the parent lowers `unless` frames through the shared sequence
    /// lowering. Any other parent rejects `unless` at the sentence boundary.
    pub unless_frames: UnlessFrameSupport,
    /// The ability's source permanent, or its last-known information once it
    /// left the battlefield to pay a cost, can answer numeric reads such as
    /// its counters (CR 608.2h, 112.7a). Set only by a permanent's activation.
    pub source_numeric_reads: bool,
    /// Index of the action being read in the enclosing sequence, set only by
    /// the sequence machine. A subject target binds `its` to this action;
    /// other parents leave it unset and that binding fails explicitly.
    pub current_effect_index: Option<usize>,
    /// How the ability's source face is attached to another permanent: an
    /// Aura enchants it, an Equipment equips it (CR 301.5a, 303.4a). Set only
    /// by parents that read the face type line; `enchanted`/`equipped`
    /// references are rejected without it.
    pub source_attachment: Option<AttachedObjectRole>,
    /// Agreement of the verb that follows an explicit subject. The sequence
    /// sets `BareInfinitive` after a causative `have` (`you may have target
    /// player lose 3 life`); every subject-verb frame then reads its own verb
    /// in the bare form instead of the finite one.
    pub subject_verb_agreement: SubjectVerbAgreement,
    /// What the triggering event binds (`binding_environment.rs`): the
    /// event player, the defending player, the other combatant, the event
    /// object, the cast spell and the card a zone change made. Set only by
    /// the trigger parent from its typed event; the single source of truth
    /// every reference reader resolves against.
    pub frame: FrameBindings,
}

/// Surface agreement of an action verb with its explicit grammatical subject.
/// The action and its arguments are the same; only the verb form differs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SubjectVerbAgreement {
    /// `target player loses`, `it deals`, `creatures you control get`.
    #[default]
    Finite,
    /// The bare infinitive after a causative verb: `have target player lose`,
    /// `have it deal`.
    BareInfinitive,
}

/// Only a parent whose actions are lowered by the shared sequence lowering,
/// outside a `you may` permission, enables `EFFECT unless â€¦` frames.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum UnlessFrameSupport {
    #[default]
    Unsupported,
    SharedLowering,
}

/// A typed child-machine result together with the first token position not
/// owned by that child. Sentence punctuation and full-stream consumption stay
/// with the parent sequence machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct RecognizedEffectFragment<T> {
    pub node: T,
    pub end_position: usize,
}

impl EffectRecognitionContext {
    pub(in crate::oracle::canonical) fn with_observed_event_object(
        mut self,
        object: TypedObservedEventObject,
    ) -> Self {
        self.observed_event_object = Some(object);
        self
    }

    pub(in crate::oracle::canonical) fn with_target_reference_support(
        mut self,
        support: TargetReferenceSupport,
    ) -> Self {
        self.target_reference_support = support;
        self
    }

    /// Only a parent lowering its sequence with the shared lowering enables
    /// `unless` frames; the frame is then lowered with its guarded actions.
    pub(in crate::oracle::canonical) fn with_unless_frames(mut self) -> Self {
        self.unless_frames = UnlessFrameSupport::SharedLowering;
        self
    }

    /// Only a parent with a reviewed domain-aware lowerer enables stack Return.
    pub(in crate::oracle::canonical) fn with_zone_change_domains(
        mut self,
        domains: ZoneChangeDomains,
    ) -> Self {
        self.zone_change_domains = domains;
        self
    }

    /// The event child has proved that its observed subject is this exact source.
    /// Ownership-only `for_source` contexts deliberately do not set this binding.
    pub(in crate::oracle::canonical) fn with_source_event_subject(
        mut self,
        source: LocalObjectReference,
    ) -> Self {
        self.source_anaphor_binding = SourceAnaphorBinding::EventSubject(source);
        self
    }

    /// The parent read the source face type line: an Aura or Equipment source
    /// can name the permanent it is attached to.
    pub(in crate::oracle::canonical) fn with_source_attachment(
        mut self,
        role: Option<AttachedObjectRole>,
    ) -> Self {
        self.source_attachment = role;
        self
    }

    /// The parent lowers this sequence through the shared sequence lowerer.
    pub(in crate::oracle::canonical) fn with_sentence_conditionals(mut self) -> Self {
        self.sentence_conditionals = true;
        self
    }

    pub(in crate::oracle::canonical) fn for_source(source: LocalObjectReference) -> Self {
        Self {
            source: Some(source),
            source_cast_x: false,
            ..Self::default()
        }
    }

    pub(in crate::oracle::canonical) fn for_source_cast_x(source: LocalObjectReference) -> Self {
        Self {
            source: Some(source),
            source_cast_x: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalPlayerReference {
    Controller,
    ControllerOf(TargetBinding),
    ControllerOfQualified(TypedTargetReference),
    /// A player the binding environment names without a target slot of this
    /// reader (`that player` naming the event's player).
    Bound(BoundPlayerReference),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NumericAtom {
    Integer(i64),
    VariableX,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NumericComparisonOperator {
    Equal,
    LessThanOrEqual,
    GreaterThanOrEqual,
    LessThan,
    GreaterThan,
}

impl NumericComparisonOperator {
    /// The runtime `compare` operator.
    pub(in crate::oracle::canonical) fn ir_operator(self) -> &'static str {
        match self {
            Self::Equal => "==",
            Self::LessThanOrEqual => "<=",
            Self::GreaterThanOrEqual => ">=",
            Self::LessThan => "<",
            Self::GreaterThan => ">",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum NumericObjectProperty {
    Power,
    Toughness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SelectorNumericProperty {
    ManaValue,
    Power,
    Toughness,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TargetBinding {
    pub effect_index: usize,
    pub slot_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerTargetBinding {
    pub effect_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BoundObjectConditionNode {
    pub object: TargetBinding,
    pub predicate: ObjectPredicate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionalZoneChangeNode {
    pub zone_change: ZoneChangeEffectNode,
    pub condition: BoundObjectConditionNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ConditionalZoneChangeAmendmentNode {
    pub action: ZoneChangeAction,
    pub target_condition: BoundObjectConditionNode,
    pub amendment_condition: ConditionNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NumericExpression {
    Atom(NumericAtom),
    ObjectProperty {
        object: LocalObjectReference,
        property: NumericObjectProperty,
    },
    PowerOf(TargetBinding),
    ManaValueOf(TargetBinding),
    QualifiedTargetProperty {
        object: TargetBinding,
        required_nominal: ObjectPredicate,
        property: SelectorNumericProperty,
    },
    CountQuery(Box<ScopedObjectQueryNode>),
    CountCounters {
        object: LocalObjectReference,
        counter: crate::oracle::counter_name::CounterName,
    },
    /// Counters of one kind already on the object that receives the enclosing
    /// action (CR 701.10e doubling). It has no standalone lowering: only an
    /// action with one recipient per lowered effect may bind it.
    CountersOnRecipient {
        counter: crate::oracle::counter_name::CounterName,
    },
    /// Cardinality of a battlefield collection, or of the distinct colors
    /// among it (shared `for each` count child).
    PermanentCount(Box<PermanentCountNode>),
    SourceCastX,
    EventValue(EventNumericBinding),
    /// `card types among COLLECTION` (CR 205.2a): the distinct card types
    /// among the cards of a graveyard collection, read by the shared scoped
    /// query (`cards in your graveyard`, `creature cards in all graveyards`).
    CountDistinctCardTypes {
        zone: GraveyardScope,
        predicate: ObjectPredicate,
    },
    /// A quantity an earlier action of the program produced (`the total life
    /// lost this way`). `reader_effect` is the action that reads it; the
    /// program lowering binds the nearest earlier producing action.
    ActionResultQuantity {
        reader_effect: usize,
        quantity: ActionResultQuantity,
    },
    /// `your devotion to COLOR` (CR 700.5): the mana symbols of that color
    /// among the mana costs of permanents the ability controller controls.
    Devotion(crate::oracle::characteristic_vocabulary::CardColor),
    /// The number of the ability controller's opponents with one fact
    /// (`for each opponent you attacked with a creature this combat`).
    OpponentCount(OpponentFact),
    /// The number (or total amount) of recorded game events matching one
    /// history pattern (`spells you've cast this turn`, `creatures that died
    /// this turn`): the same pattern the condition language's EventHistory
    /// atom compares.
    EventHistoryCount(Box<EventHistoryCountNode>),
    Add {
        left: Box<NumericExpression>,
        right: Box<NumericExpression>,
    },
    Subtract {
        left: Box<NumericExpression>,
        right: Box<NumericExpression>,
    },
    Multiply {
        left: Box<NumericExpression>,
        right: Box<NumericExpression>,
    },
    CardsInZone {
        player: LocalPlayerReference,
        zone: LocalZone,
    },
    /// A numeric fact about one player's game state (CR 119 life facts).
    PlayerFact {
        player: LocalPlayerReference,
        fact: PlayerNumericFact,
    },
    Divide {
        left: Box<NumericExpression>,
        right: Box<NumericExpression>,
        rounding: NumericRounding,
    },
    /// A characteristic of the permanent sacrificed to pay a cost of this
    /// ability, as last known when the cost was paid. Its producer is
    /// resolved against the same face after card composition.
    SacrificedCostObjectProperty(NumericObjectProperty),
    /// The greatest power or toughness among the ability controller's
    /// permanents matching the predicate (zero when there are none).
    /// `the greatest|highest PROPERTY among COLLECTION`: a maximum over the
    /// counted battlefield collection (power, toughness or mana value).
    ///
    /// With `aggregate: Sum` it is the total of that property instead (`creatures
    /// you control have total power 8 or greater`), zero when there are none.
    GreatestPropertyAmong {
        property: SelectorNumericProperty,
        predicate: ObjectPredicate,
        aggregate: PropertyAggregate,
    },
    /// The number of objects an earlier action of the same program affected
    /// (`for each creature destroyed this way`, CR 608.2c). Only the program
    /// parent that resolved `effect_index` creates it; lowering reads the
    /// binding that parent installs on that action.
    ActionResultCount {
        effect_index: usize,
        filter: ActionResultFilter,
    },
    /// A characteristic of an object the binding environment names without
    /// a declaration (`that creature's power` after an event that binds it),
    /// read live while it is the same object, else as last known information
    /// (CR 400.7, 608.2h).
    BoundObjectProperty {
        object: PermanentEffectReference,
        property: SelectorNumericProperty,
    },
}

/// How a numeric property is aggregated over a collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PropertyAggregate {
    /// The greatest value (`the greatest power among`).
    Greatest,
    /// The sum of the values (`total power`).
    Sum,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PermanentCountNode {
    pub predicate: ObjectPredicate,
    pub controller: Option<ObjectControllerScope>,
    pub exclude_source: bool,
    pub measure: PermanentCountMeasure,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PlayerNumericFact {
    LifeGainedThisTurn,
    LifeTotal,
    /// The counters of one kind the player has (CR 122.1: experience,
    /// poison, energy).
    Counters(crate::oracle::counter_name::CounterName),
}

/// A fact about one opponent of the ability controller that a count reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpponentFact {
    /// The controller attacked that player with a creature during this
    /// combat (CR 506.2, 702.121a); attacking its planeswalkers or battles
    /// is not attacking the player.
    AttackedByControllerThisCombat,
    /// Every opponent of the controller still in the game (`the number of
    /// opponents you have`, CR 102.3).
    Any,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PermanentCountMeasure {
    Objects,
    DistinctColors,
    /// Distinct names among the counted permanents (`differently named`).
    DistinctNames,
    /// Distinct basic land types among the counted lands (`basic land types
    /// among lands you control`, the Domain count, CR 305.6).
    DistinctBasicLandTypes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventNumericBinding {
    DamageAmount,
    ReplacementQuantity,
    /// The mana value of the spell a cast record binds (`that spell's mana
    /// value`, CR 202.3), as recorded when it was cast.
    CastSpellManaValue,
    /// The life a life-gain event records (`whenever you gain life, ...
    /// that many`, CR 119.10): the runtime's `lifeGainedAmount`.
    LifeGained,
    /// The life a life-loss event records (CR 119.3): `lifeLostAmount`.
    LifeLost,
    /// The counters one counter-placement event put (CR 122.6):
    /// `counterCount`.
    CountersPut,
    /// The objects one `one or more` batch event matched (`Whenever one or
    /// more Dragons you control attack, draw that many cards`, CR 603.2c):
    /// `eventObjectCount`.
    EventObjectCount,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GraveyardScope {
    Controller,
    AllPlayers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NumericRounding {
    Down,
    Up,
}

impl NumericExpression {
    pub(super) fn requires_target_binding(&self) -> bool {
        match self {
            Self::CountQuery(query) => query.requires_target_binding(),
            Self::PowerOf(_) | Self::ManaValueOf(_) | Self::QualifiedTargetProperty { .. } => true,
            Self::Add { left, right }
            | Self::Subtract { left, right }
            | Self::Multiply { left, right }
            | Self::Divide { left, right, .. } => {
                left.requires_target_binding() || right.requires_target_binding()
            }
            _ => false,
        }
    }

    pub(super) fn is_target_mana_value(&self, binding: TargetBinding) -> bool {
        self.is_target_property(binding, SelectorNumericProperty::ManaValue)
    }

    /// Compatibility for historical consumers whose validated noun was card.
    /// New qualified nominals require the common original-slot validation.
    pub(super) fn is_target_property(
        &self,
        binding: TargetBinding,
        property: SelectorNumericProperty,
    ) -> bool {
        matches!((self, property), (Self::PowerOf(actual), SelectorNumericProperty::Power) | (Self::ManaValueOf(actual), SelectorNumericProperty::ManaValue) if *actual == binding)
            || matches!(self, Self::QualifiedTargetProperty {
                object, required_nominal: ObjectPredicate::ObjectKind(ObjectKind::Card),
                property: actual_property,
            } if *object == binding && *actual_property == property)
    }

    pub(super) fn add(left: Self, right: Self) -> Self {
        Self::Add {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub(super) fn subtract(left: Self, right: Self) -> Self {
        Self::Subtract {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub(super) fn multiply(left: Self, right: Self) -> Self {
        Self::Multiply {
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub(super) fn divide(
        left: Self,
        right: impl Into<Self>,
        rounding: NumericRounding,
    ) -> Option<Self> {
        let right = right.into();
        if matches!(right, Self::Atom(NumericAtom::Integer(value)) if value <= 0) {
            return None;
        }
        Some(Self::Divide {
            left: Box::new(left),
            right: Box::new(right),
            rounding,
        })
    }

    pub(super) fn divide_rounded_down(left: Self, right: impl Into<Self>) -> Option<Self> {
        Self::divide(left, right, NumericRounding::Down)
    }

    pub(super) fn uses_x(&self) -> bool {
        match self {
            Self::Atom(NumericAtom::VariableX) => true,
            Self::CountQuery(query) => query.uses_x(),
            Self::QualifiedTargetProperty {
                required_nominal, ..
            } => object_predicate_uses_x(required_nominal),
            Self::PermanentCount(count) => object_predicate_uses_x(&count.predicate),
            Self::GreatestPropertyAmong { predicate, .. } => object_predicate_uses_x(predicate),
            Self::Add { left, right }
            | Self::Subtract { left, right }
            | Self::Multiply { left, right }
            | Self::Divide { left, right, .. } => left.uses_x() || right.uses_x(),
            Self::Atom(NumericAtom::Integer(_))
            | Self::ObjectProperty { .. }
            | Self::PowerOf(_)
            | Self::ManaValueOf(_)
            | Self::CountCounters { .. }
            | Self::CountersOnRecipient { .. }
            | Self::SourceCastX
            | Self::EventValue(_)
            | Self::CountDistinctCardTypes { .. }
            | Self::OpponentCount(_)
            | Self::EventHistoryCount(_)
            | Self::Devotion(_)
            | Self::ActionResultQuantity { .. }
            | Self::PlayerFact { .. }
            | Self::SacrificedCostObjectProperty(_)
            | Self::BoundObjectProperty { .. }
            | Self::ActionResultCount { .. }
            | Self::CardsInZone { .. } => false,
        }
    }

    /// Replaces every arithmetic occurrence of X by its definition (CR 107.3)
    /// for a consumer that evaluates the expression continuously instead of
    /// binding it once. X inside a query predicate has no proved substitution,
    /// so it returns `None` rather than leaving an unbound variable.
    pub(super) fn substitute_x(&self, value: &Self) -> Option<Self> {
        Some(match self {
            Self::Atom(NumericAtom::VariableX) => value.clone(),
            Self::Add { left, right } => Self::Add {
                left: Box::new(left.substitute_x(value)?),
                right: Box::new(right.substitute_x(value)?),
            },
            Self::Subtract { left, right } => Self::Subtract {
                left: Box::new(left.substitute_x(value)?),
                right: Box::new(right.substitute_x(value)?),
            },
            Self::Multiply { left, right } => Self::Multiply {
                left: Box::new(left.substitute_x(value)?),
                right: Box::new(right.substitute_x(value)?),
            },
            Self::Divide {
                left,
                right,
                rounding,
            } => Self::Divide {
                left: Box::new(left.substitute_x(value)?),
                right: Box::new(right.substitute_x(value)?),
                rounding: *rounding,
            },
            other if other.uses_x() => return None,
            other => other.clone(),
        })
    }
}

impl From<NumericAtom> for NumericExpression {
    fn from(atom: NumericAtom) -> Self {
        Self::Atom(atom)
    }
}

impl From<i64> for NumericExpression {
    fn from(value: i64) -> Self {
        Self::Atom(NumericAtom::Integer(value))
    }
}

pub(super) use crate::oracle::characteristic_vocabulary::{
    AmassArmySubtype, CardSupertype, CardType as ObjectCardType, CreatureSubtype,
    ObjectDomain as ObjectKind,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CharacteristicBatch {
    Outlaw,
    /// CR 700.6.
    Historic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CharacteristicBatchReminderNode {
    pub batch: CharacteristicBatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayerPredicate {
    Any,
    Opponent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ObjectControllerScope {
    AbilityController,
    NotAbilityController,
    OpponentOfAbilityController,
    /// The player the triggering event records (`defending player`,
    /// `that player` after an event naming one), resolved by the C2
    /// binding environment.
    EventPlayer,
    /// One other player the query names (`target opponent controls`, `its
    /// controller controls`); lowered by `effects::lower_related_player`.
    Player(RelatedPlayer),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ObjectQueryNode {
    pub predicate: ObjectPredicate,
    pub controller: Option<ObjectControllerScope>,
    /// `other` before a collection: the bound ability source is not a member.
    /// Read once by the collection frame; a parent that cannot lower it
    /// refuses the query instead of dropping the exclusion.
    pub exclude_source: bool,
}

/// The player relation carried by an Oracle spell-casting subject. This is
/// deliberately distinct from [`ObjectControllerScope`]: a spell's caster and
/// its current controller can differ after the spell changes control on the
/// stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SpellCasterScope {
    AbilityController,
    NotAbilityController,
    OpponentOfAbilityController,
    /// The player the triggering event records.
    EventPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SpellCastQueryNode {
    pub predicate: ObjectPredicate,
    pub caster: Option<SpellCasterScope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ObjectSetNode {
    pub predicate: ObjectPredicate,
    pub controller: ObjectControllerScope,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DestroyPermanentCollectionEffectNode {
    pub objects: ObjectQueryNode,
    /// `destroy all other ...`: the bound ability source is not destroyed.
    pub exclude_source: bool,
    /// `They can't be regenerated.` read by the sentence parent (CR 701.19c).
    pub cannot_regenerate: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GrantedKeyword {
    /// Restriction and requirement markers of the shared combat clause
    /// (CR 508.1c-d, 509.1b-c). They are never read from a keyword list.
    AttackEachCombatIfAble,
    BlockEachCombatIfAble,
    MaxOneBlocker,
    MustBeBlockedIfAble,
    /// `assigns no combat damage` (CR 510.1): the object assigns no combat
    /// damage while the marker lasts.
    AssignsNoCombatDamage,
    /// `can attack as though it didn't have defender` (CR 702.3b): defender
    /// does not prevent the object from attacking while the marker lasts.
    AttackAsThoughNoDefender,
    /// `can't be regenerated` (CR 701.15): no regeneration shield replaces
    /// the object's destruction while the marker lasts.
    CantBeRegenerated,
    Cascade,
    CantAttack,
    CantBeBlocked,
    CantBlock,
    Changeling,
    /// CR 702.51: tapping creatures pays for this spell.
    Convoke,
    Deathtouch,
    Defender,
    /// CR 702.114: characteristic-defining colorlessness.
    Devoid,
    DoubleStrike,
    Exalted,
    Fear,
    FirstStrike,
    /// CR 702.25: a blocking creature without flanking gets -1/-1.
    Flanking,
    Flash,
    Flying,
    /// CR 702.102a: a split card cast from hand may be cast as both halves
    /// (the engine's fused split definition).
    Fuse,
    Haste,
    Hexproof,
    Horsemanship,
    /// CR 702.126: tapping artifacts pays for this spell.
    Improvise,
    Indestructible,
    Infect,
    Intimidate,
    Landwalk(LandwalkWord),
    Lifelink,
    Menace,
    Persist,
    Prowess,
    Reach,
    Shadow,
    Shroud,
    Skulk,
    /// CR 702.61a: while this spell is on the stack, players can't cast other
    /// spells or activate abilities that aren't mana abilities.
    SplitSecond,
    /// CR 702.40: a cast trigger copying the spell for each spell cast
    /// before it this turn.
    Storm,
    Sunburst,
    Trample,
    /// CR 702.89: Aura destruction replacement for the enchanted permanent.
    UmbraArmor,
    Undying,
    Vigilance,
    /// CR 702.80: damage to creatures is dealt as -1/-1 counters.
    Wither,
}

impl GrantedKeyword {
    pub(super) fn canonical_name(self) -> &'static str {
        match self {
            Self::AttackEachCombatIfAble => "attackEachCombatIfAble",
            Self::BlockEachCombatIfAble => "blockEachCombatIfAble",
            Self::MaxOneBlocker => "maxOneBlocker",
            Self::MustBeBlockedIfAble => "mustBeBlockedIfAble",
            Self::AssignsNoCombatDamage => "assignsNoCombatDamage",
            Self::AttackAsThoughNoDefender => "canAttackAsThoughNoDefender",
            Self::CantBeRegenerated => "cantBeRegenerated",
            Self::Cascade => "cascade",
            Self::CantAttack => "cantAttack",
            Self::CantBeBlocked => "cantBeBlocked",
            Self::CantBlock => "cantBlock",
            Self::Changeling => "changeling",
            Self::Convoke => "convoke",
            Self::Deathtouch => "deathtouch",
            Self::Defender => "defender",
            Self::Devoid => "devoid",
            Self::DoubleStrike => "doubleStrike",
            Self::Exalted => "exalted",
            Self::Fear => "fear",
            Self::FirstStrike => "firstStrike",
            Self::Flanking => "flanking",
            Self::Flash => "flash",
            Self::Flying => "flying",
            Self::Fuse => "fuse",
            Self::Haste => "haste",
            Self::Hexproof => "hexproof",
            Self::Horsemanship => "horsemanship",
            Self::Improvise => "improvise",
            Self::Indestructible => "indestructible",
            Self::Infect => "infect",
            Self::Intimidate => "intimidate",
            Self::Landwalk(word) => word.keyword,
            Self::Lifelink => "lifelink",
            Self::Menace => "menace",
            Self::Persist => "persist",
            Self::Prowess => "prowess",
            Self::Reach => "reach",
            Self::Shadow => "shadow",
            Self::Shroud => "shroud",
            Self::Skulk => "skulk",
            Self::SplitSecond => "splitSecond",
            Self::Storm => "storm",
            Self::Sunburst => "sunburst",
            Self::Trample => "trample",
            Self::UmbraArmor => "umbraArmor",
            Self::Undying => "undying",
            Self::Vigilance => "vigilance",
            Self::Wither => "wither",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum EffectDurationNode {
    UntilEndOfCurrentTurn,
    UntilNextTurn {
        player: LocalPlayerReference,
    },
    /// Through the cleanup of the player's next turn (CR 514.2). Produced only
    /// by `recognize_play_window_duration`; the continuous-effect readers keep
    /// the narrower `recognize_effect_duration` and never see this window.
    UntilEndOfNextTurn {
        player: LocalPlayerReference,
    },
    /// `for as long as you control SOURCE` (CR 611.2b): ends when the
    /// ability's controller no longer controls that source object. Produced
    /// only by `recognize_source_control_duration` for consumers opting in.
    WhileSourceControlled,
    /// No stated duration (CR 611.2a): the effect lasts indefinitely, and an
    /// effect on an object ends when that object becomes a new object
    /// (CR 400.7). Produced only at the end of a continuous-effect clause.
    Indefinite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum TargetSelector {
    Any,
    Union(Vec<TargetSelector>),
    Object(ObjectPredicate),
    ControlledObject {
        predicate: ObjectPredicate,
        controller: ObjectControllerScope,
    },
    Player(PlayerPredicate),
    StackItem(StackItemPredicate),
}

impl TargetSelector {
    pub(in crate::oracle::canonical) fn guarantees_card_type(
        &self,
        expected: ObjectCardType,
    ) -> bool {
        match self {
            Self::Object(predicate) | Self::ControlledObject { predicate, .. } => {
                predicate.guarantees_card_type(expected)
            }
            Self::Union(branches) => {
                !branches.is_empty()
                    && branches
                        .iter()
                        .all(|branch| branch.guarantees_card_type(expected))
            }
            Self::Any | Self::Player(_) | Self::StackItem(_) => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum StackAbilityKind {
    Activated,
    Triggered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum StackItemPredicate {
    Ability(StackAbilityKind),
    Spell,
    Colorless,
    Intersection(Vec<StackItemPredicate>),
    Union(Vec<StackItemPredicate>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TargetCardinality {
    pub minimum: u32,
    pub maximum: u32,
}

/// A target collection whose bounds are Oracle numeric expressions rather
/// than compile-time integers. This stays distinct from `TargetCardinality`
/// while older target consumers still require literal bounds; new grammars
/// can share `X` without baking the variable into an effect-specific parser.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExpressedTargetCardinality {
    pub minimum: NumericAtom,
    pub maximum: NumericAtom,
}

impl ExpressedTargetCardinality {
    pub(super) const EXACTLY_ONE: Self = Self {
        minimum: NumericAtom::Integer(1),
        maximum: NumericAtom::Integer(1),
    };

    pub(super) fn uses_x(&self) -> bool {
        self.minimum == NumericAtom::VariableX || self.maximum == NumericAtom::VariableX
    }
}

impl TargetCardinality {
    pub(super) const EXACTLY_ONE: Self = Self {
        minimum: 1,
        maximum: 1,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TargetSlot {
    pub selector: TargetSelector,
    pub cardinality: TargetCardinality,
    pub exclude_source: bool,
    /// The authoritative declared count when it is not two literals (`X
    /// target`, `any number of target`); `cardinality` is then
    /// `TargetCardinality::EXPRESSED`. Only count-aware parents produce it.
    pub count: Option<TargetCount>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TargetListNode {
    pub slots: Vec<TargetSlot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DamageRecipientNode {
    Targets(TargetListNode),
    Player(LocalPlayerReference),
    PlayerSet(PlayerSetReferenceNode),
    EachPermanent(DamageCollectionNode),
    /// `to each creature and each player`: every group receives the same
    /// amount from the same source (CR 120.2). Members are never nested.
    All(Vec<DamageRecipientNode>),
    /// `to that player`, `to defending player`, `to that creature's
    /// controller`: a player the binding environment names.
    BoundPlayer(BoundPlayerReference),
    /// `to it`, `to that creature`: an object the binding environment names.
    BoundObject(PermanentEffectReference),
}

impl DamageRecipientNode {
    /// A recipient resolved through the binding environment; its lowering
    /// needs the program's target bindings.
    pub(super) fn is_bound(&self) -> bool {
        match self {
            Self::BoundPlayer(_) | Self::BoundObject(_) => true,
            Self::All(groups) => groups.iter().any(Self::is_bound),
            _ => false,
        }
    }
}

/// `each [other] OBJECTS [CONTROLLER]`: the shared object query of a damage
/// recipient collection, with the optional source exclusion of `other`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DamageCollectionNode {
    pub objects: ObjectQueryNode,
    pub exclude_source: bool,
}

/// One damage frame: a source object, a shared numeric amount, and a
/// recipient query. The source is the ability source (`Reference`) or an
/// object subject such as a target or a bound referent (CR 120.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DamageEffectNode {
    pub source: PermanentOperandNode,
    pub amount: NumericExpression,
    pub recipient: DamageRecipientNode,
}

impl DamageEffectNode {
    /// The fixed ability-source reference, when the source is not an object
    /// subject that requires a declaration or a lexical binding.
    pub(in crate::oracle::canonical) fn fixed_source(&self) -> Option<LocalObjectReference> {
        match self.source {
            PermanentOperandNode::Reference(reference) => Some(reference),
            _ => None,
        }
    }
}

/// `A fights B` (CR 701.14): each permanent deals damage equal to its power
/// to the other. Both operands use the shared permanent operand grammar.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FightEffectNode {
    pub first: PermanentOperandNode,
    pub second: PermanentOperandNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CastingCostReductionNode {
    pub subject: CastingReductionSubject,
    pub amount: NumericExpression,
    /// Shared sentence condition program gating the adjustment (`if you
    /// control ...`, `if an opponent has ...`), read without antecedents.
    pub condition: Option<SentenceConditionNode>,
    /// Object query matched against the spell's chosen targets (`if it targets ...`).
    pub target_query: Option<ObjectQueryNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CastingReductionSubject {
    CurrentSpell,
    SpellSet(SpellCastQueryNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EarlyTurnEntryNode {
    pub maximum_controller_turn: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CounterEffectNode {
    pub target: TargetListNode,
    /// `counter that spell`: the spell the triggering cast record binds,
    /// not a declared target (the target list is then empty).
    pub event_spell: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OptionalDrawCardsEffectNode {
    pub player: LocalPlayerReference,
    pub maximum: NumericExpression,
    pub up_to: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GainLifeEffectNode {
    pub player: LocalPlayerReference,
    pub amount: NumericExpression,
}

/// Whether a counter action puts counters on its object or removes them
/// from it (CR 122.1, 122.8): the same operands, the opposite change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CounterChangeDirection {
    #[default]
    Put,
    Remove,
    /// `distribute N counters among TARGETS` (CR 601.2d): `count` is divided
    /// among the declared targets, at least one each.
    Divided,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PutCountersOnObjectEffectNode {
    pub permanent: PermanentOperandNode,
    pub counter: crate::oracle::counter_name::CounterName,
    pub count: NumericExpression,
    pub direction: CounterChangeDirection,
    /// Further placements of the same action, or a choice among kinds
    /// (`put_counters.rs`); `Single` is exactly `count` `counter` counters.
    pub kinds: CounterKindsNode,
}

/// CR 122.5: moving counters removes them from one object and puts them on
/// another; counters that could not be removed are not put.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MoveCountersEffectNode {
    /// The counter action in the removal direction.
    pub removal: PutCountersOnObjectEffectNode,
    pub destination: PermanentOperandNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AmassEffectNode {
    pub player: LocalPlayerReference,
    pub army_subtype: AmassArmySubtype,
    pub count: NumericExpression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct InvestigateEffectNode {
    pub player: LocalPlayerReference,
    /// `investigate twice` / `investigate X times`: the number of separate
    /// investigations, 1 for the bare action.
    pub times: NumericExpression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SurveilEffectNode {
    pub player: LocalPlayerReference,
    pub count: NumericExpression,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TransformPermanentEffectNode {
    pub permanent: LocalObjectReference,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceZone {
    Battlefield,
    Exile,
    Graveyard,
    Hand,
    /// Its owner's library, shuffled after the move (CR 701.24b).
    Library,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceFaceChange {
    Preserve,
    Transform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceMoveController {
    Preserve,
    Owner,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct MoveSourceEffectNode {
    pub source: LocalObjectReference,
    pub from: SourceZone,
    pub to: SourceZone,
    pub face_change: SourceFaceChange,
    pub controller: SourceMoveController,
    /// `to the battlefield tapped` (CR 110.5): the source enters tapped.
    pub tapped: bool,
    /// `to the battlefield attached to OBJECT` (CR 303.4f, 301.5c): the
    /// permanent the entering Aura or Equipment becomes attached to.
    pub attached_to: Option<PermanentOperandNode>,
    /// `to the battlefield with N COUNTER counters on it` (CR 122.6): the
    /// source enters with these counters.
    pub entering_counters: Option<EnteringCountersNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LoseLifeEffectNode {
    pub player: LocalPlayerReference,
    pub amount: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetPlayerLifeAction {
    Gain,
    Lose,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetPlayerLifeEffectNode {
    pub player: PlayerTargetBinding,
    pub declares_target: bool,
    pub action: TargetPlayerLifeAction,
    pub amount: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TapSelfCostNode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PayLifeCostNode {
    pub amount: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LoyaltyCostNode {
    pub delta: i64,
}

pub(super) use crate::oracle::characteristic_vocabulary::CardColor;
pub(super) use crate::oracle::characteristic_vocabulary::ColorQuantity;
pub(super) use crate::oracle::characteristic_vocabulary::LandwalkWord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ExileQualifiedCardFromHandCostNode {
    pub color: CardColor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompoundCostNode {
    PayLife(PayLifeCostNode),
    ExileQualifiedCardFromHand(ExileQualifiedCardFromHandCostNode),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CostConnector {
    And,
    Comma,
}

/// An ordered list of cost atoms and the connectors between them. Activation
/// and resolution parents read `CostSequenceNode<PaymentActionNode>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CostSequenceNode<A> {
    pub costs: Vec<A>,
    pub connectors: Vec<CostConnector>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CurrentSpellManaCostReferenceNode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AlternativeManaCostEnvelopeNode {
    pub condition: Option<ConditionNode>,
    pub costs: CostSequenceNode<CompoundCostNode>,
    pub replaces: CurrentSpellManaCostReferenceNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CounterDestinationReplacement {
    Exile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CounterReplacementSequenceNode {
    pub counter: CounterEffectNode,
    pub destination: CounterDestinationReplacement,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddFixedManaEffectNode {
    pub mana: ManaCostNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddCountedManaEffectNode {
    pub symbol: char,
    pub amount: NumericExpression,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AddAnyColorManaEffectNode {
    pub amount: i64,
    pub distribution: ManaColorDistribution,
}

/// A color-choice production whose amount is a shared numeric expression
/// (`X mana of any one color, where X is ...`, `that much mana of any one
/// color`). A literal amount stays `AddAnyColorManaEffectNode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddAnyColorAmountManaEffectNode {
    pub amount: NumericExpression,
    pub distribution: ManaColorDistribution,
}

impl AddAnyColorAmountManaEffectNode {
    /// Runtime mana-value kind for this distribution.
    pub(super) fn runtime_mana_kind(&self) -> &'static str {
        match self.distribution {
            ManaColorDistribution::OneColor => "chooseColor",
            ManaColorDistribution::AnyCombination => "chooseColors",
        }
    }
}

/// How the colors of a multi-mana production are chosen (CR 106.1b wording):
/// `of any [one] color` chooses one color for every mana, while `in any
/// combination of colors` chooses a color independently for each mana.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManaColorDistribution {
    OneColor,
    AnyCombination,
}

impl AddAnyColorManaEffectNode {
    /// Runtime mana-value kind for this distribution.
    pub(super) fn runtime_mana_kind(&self) -> &'static str {
        match self.distribution {
            ManaColorDistribution::OneColor => "chooseColor",
            ManaColorDistribution::AnyCombination => "chooseColors",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddManaChoiceEffectNode {
    pub options: Vec<ManaCostNode>,
}

/// `[FIXED or] N mana of the chosen color`: the color the source stored as
/// it entered (CR 614.12), optionally after fixed alternatives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AddStoredColorManaEffectNode {
    pub amount: i64,
    pub alternatives: Vec<ManaCostNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddManaEffectNode {
    Qualified {
        production: Box<AddManaEffectNode>,
        policy: ManaSpendingPolicyNode,
    },
    Fixed(AddFixedManaEffectNode),
    /// `Add {G} for each COUNTABLE`: one whole-mana symbol whose amount is the
    /// shared numeric expression (literal base times the count).
    Counted(AddCountedManaEffectNode),
    Choice(AddManaChoiceEffectNode),
    AnyColor(AddAnyColorManaEffectNode),
    /// A color choice whose amount is a shared numeric expression.
    AnyColorAmount(AddAnyColorAmountManaEffectNode),
    ScopedColor(AddScopedManaEffectNode),
    EachColor(AddEachColorManaNode),
    StoredColor(AddStoredColorManaEffectNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ActivatedManaAbilityNode {
    pub costs: ActivationCostSequenceNode,
    pub effect: AddManaEffectNode,
    pub followup: Option<EffectSequenceNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneChangeAction {
    Exile,
    Destroy,
    Move(ZoneChangeDestination),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneChangeDestination {
    OwnerHand,
    /// `on top of` / `on the bottom of its owner's library` (CR 401.4).
    OwnerLibrary(LibraryPosition),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneChangeRegenerationPolicy {
    Allowed,
    Prohibited {
        referenced_type: Option<ObjectCardType>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ZoneChangeEffectNode {
    pub action: ZoneChangeAction,
    pub target: TargetListNode,
    pub regeneration: ZoneChangeRegenerationPolicy,
    /// `until EVENT` (CR 610.3), attached by the enclosing sequence.
    pub until: Option<ZoneChangeUntilNode>,
}

/// The event ending a one-shot `until` zone change (CR 610.3). Its second
/// one-shot effect returns the object to its previous zone under its owner's
/// control (CR 610.3c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ZoneChangeUntilNode {
    /// `until SOURCE leaves the battlefield`.
    SourceLeavesBattlefield,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CreatureTokenSpecNode {
    pub power: i64,
    pub toughness: i64,
    pub colors: Vec<CardColor>,
    pub subtypes: Vec<CreatureSubtype>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CreateTokensEffectNode {
    pub player: LocalPlayerReference,
    pub quantity: NumericAtom,
    pub token: CreatureTokenSpecNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchDestination {
    Battlefield {
        tapped: bool,
    },
    Zone(CardZoneName),
    LibraryTop,
    /// The search ends at its selection: the found cards are bound for the
    /// ordinary actions that follow it (`library_search.rs`).
    Bound,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SearchShuffleOrder {
    MoveThenShuffle,
    ShuffleThenMove,
    /// The searched player's own later instruction shuffles (`Then that
    /// player shuffles.`); the search itself does not.
    ByHolderInstruction,
    /// `If you search your library this way, shuffle.` after a search of
    /// the library and other zones: the searcher chooses whether the
    /// library is searched, and only then is it shuffled (CR 701.23a).
    IfLibrarySearched,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibrarySearchEffectNode {
    pub player: LocalPlayerReference,
    /// The searched player's zones (`library_search.rs`).
    pub origin: SearchOrigin,
    /// The predicate every found card satisfies: the one card phrase, or
    /// the union of the slots below.
    pub filter: ObjectPredicate,
    /// `a Forest card and a Plains card`: one card for each listed phrase,
    /// each chosen among the cards no earlier slot took. Empty for a search
    /// with one card phrase and one quantity.
    pub slots: Vec<ObjectPredicate>,
    pub minimum: u32,
    pub maximum: u32,
    /// The shared typed count (`up to X`, `X`, `that many`) when the count
    /// is not two positive literals; `minimum`/`maximum` are then only the
    /// expressed envelope, and the lowering reads this count instead.
    pub count: Option<TargetCount>,
    /// `up to X basic land cards, where X is EXPR, put them ...`: the
    /// postposed definition read right after the counted phrase it defines
    /// (CR 107.3). The sentence parent takes it and owns its scope.
    pub where_x: Option<NumericExpression>,
    pub destination: SearchDestination,
    pub reveal: bool,
    pub shuffle_order: SearchShuffleOrder,
    pub binding: CollectionBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibrarySearchCollectionNode {
    pub player: LocalPlayerReference,
    pub zone: LocalZone,
    pub minimum: u32,
    pub maximum: u32,
    pub filter: ObjectPredicate,
    pub binding: CollectionBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RevealCollectionNode {
    pub binding: CollectionBinding,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CollectionDestination {
    Battlefield { tapped: bool },
    Hand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SearchCollectionPartitionNode {
    pub binding: CollectionBinding,
    pub selected_count: u32,
    pub selected_destination: CollectionDestination,
    pub remainder_destination: CollectionDestination,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ShuffleCollectionZoneNode {
    pub player: LocalPlayerReference,
    pub zone: LocalZone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LibrarySearchPartitionSequenceNode {
    pub search: LibrarySearchCollectionNode,
    pub reveal: RevealCollectionNode,
    pub partition: SearchCollectionPartitionNode,
    pub shuffle: ShuffleCollectionZoneNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EntryObjectKind {
    Land,
    Permanent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ChoiceBinding {
    BasicLandType,
    CardName,
    OptionalLifePayment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct BasicLandTypeChoiceNode {
    pub player: LocalPlayerReference,
    pub binding: ChoiceBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OptionalLifePaymentNode {
    pub player: LocalPlayerReference,
    pub amount: i64,
    pub binding: ChoiceBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SetEnteringStateNode {
    pub object: LocalObjectReference,
    pub tapped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConditionalEnteringStateNode {
    pub condition: ConditionNode,
    pub effect: SetEnteringStateNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ChosenLandTypeEntryNode {
    pub object_kind: EntryObjectKind,
    pub choice: BasicLandTypeChoiceNode,
    pub payment: OptionalLifePaymentNode,
    pub unpaid_entry: ConditionalEnteringStateNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OptionalLifeEntryNode {
    pub object_kind: EntryObjectKind,
    pub payment: OptionalLifePaymentNode,
    pub unpaid_entry: ConditionalEnteringStateNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct RetainChosenLandTypeNode {
    pub object_kind: EntryObjectKind,
    pub choice: ChoiceBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum LocalEffectNode {
    CardAction(CardActionNode),
    OptionalSequence(EffectSequenceNode),
    QuantifiedPlayerAction(QuantifiedPlayerActionNode),
    CounterSpell(CounterEffectNode),
    Retarget(RetargetActionNode),
    TemporaryKeywordGrant(TemporaryKeywordGrantNode<PermanentOperandNode>),
    AddMana(AddManaEffectNode),
    DealDamage(DamageEffectNode),
    Fight(FightEffectNode),
    DrawCards(DrawCardsEffectNode),
    PlayerAction(PlayerActionNode),
    DiscardCards(PlayerDiscardEffectNode),
    OptionalDrawCards(OptionalDrawCardsEffectNode),
    MillCards(MillCardsEffectNode),
    Amass(AmassEffectNode),
    EmpowerJace(EmpowerJaceEffectNode),
    Investigate(InvestigateEffectNode),
    ControllerKeywordAction(ControllerKeywordActionNode),
    Scry(ScryEffectNode),
    Surveil(SurveilEffectNode),
    Earthbend(EarthbendActionNode),
    TransformPermanent(TransformPermanentEffectNode),
    PutCounters(PutCountersOnObjectEffectNode),
    /// `move COUNT KIND counter(s) from OBJECT onto OBJECT` (CR 122.5): the
    /// removal, then the placement of what was removed.
    MoveCounters(MoveCountersEffectNode),
    MoveSource(MoveSourceEffectNode),
    MoveCards(MoveCardsEffectNode),
    ZoneBatch(ZoneBatchEffectNode),
    ZoneChange(ZoneChangeEffectNode),
    TargetCardZoneChange(TargetCardZoneChangeEffectNode),
    CreateTokens(CreateTokensEffectNode),
    CreateExplicitTokens(ExplicitTokenCreationNode),
    CreateNamedTokens(CreateNamedTokensEffectNode),
    OptionalLibrarySearch(LibrarySearchEffectNode),
    ControllerLibrarySearch(LibrarySearchEffectNode),
    OptionalCardDeployment(OptionalCardDeploymentEffectNode),
    GainLife(GainLifeEffectNode),
    LoseLife(LoseLifeEffectNode),
    TargetPlayerLife(TargetPlayerLifeEffectNode),
    LoseAbilities(SetAbilityLossEffectNode),
    DestroyReferencedSet(DestroyReferencedSetEffectNode),
    /// `destroy` a bound permanent reference (`destroy that creature`).
    ReferencedPermanentAction(ReferencedPermanentActionNode),
    DestroyCollection(DestroyPermanentCollectionEffectNode),
    Sacrifice(SacrificeEffectNode),
    ModifyStats(TemporaryPowerToughnessNode<PermanentOperandNode>),
    BecomeTypes(BecomeTypesNode<PermanentOperandNode>),
    UntapTarget(UntapTargetEffectNode),
    SelectedPermanents(SelectedPermanentsEffectNode),
    TapPermanents(TapPermanentsEffectNode),
    /// `OBJECTS don't untap during their controller's next untap step`.
    UntapRestriction(UntapRestrictionNode),
    /// An action another player performs (`that player creates ...`).
    Performed(PerformedEffectNode),
    PermanentKeywordAction(PermanentKeywordActionNode),
    /// `gain control of OBJECT` / `exchange control of OBJECT`.
    ControlChange(ControlChangeEffectNode),
    /// `copy TARGET` over a stack object.
    CopyStackObject(CopyStackObjectEffectNode),
    PreventDamage(PreventDamageEffectNode),
    Attach(AttachEffectNode),
    TemporaryPlayerPermission(TemporaryPlayerPermissionNode),
    /// `choose DOMAIN`: a typed decision whose binding later references use.
    Choose(ChooseInstructionNode),
}

/// Verb frame `tap OBJECT`: the object slot is the shared permanent operand
/// (targets with cardinality, a queried collection, the source or a bound
/// reference). No tap-specific noun grammar exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TapPermanentsEffectNode {
    pub targets: PermanentOperandNode,
    /// `untap OBJECT` sets the opposite tapped state over the same operand.
    pub untap: bool,
}

/// Keyword actions whose operand is a permanent (CR 2026-11-06 numbering):
/// `regenerate OBJECT` (701.19), `goad OBJECT` (701.15), `suspect OBJECT`
/// (701.60), `detain OBJECT` (701.35), `SUBJECT explores` (701.44),
/// `SUBJECT connives [N]` (701.50), `SUBJECT endures N` (701.63) and the
/// self-referential `adapt N` (701.46) and `monstrosity N` (701.37). The
/// object slot is the shared permanent operand; each action lowers to
/// ordinary primitives or one engine designation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PermanentKeywordAction {
    Regenerate,
    Goad,
    Suspect,
    Detain,
    Explore,
    Connive,
    Endure,
    Adapt,
    Monstrosity,
    /// `OBJECT becomes DESIGNATION` (CR 701.37b, 702.112b): the designating
    /// half of monstrosity and renown, read from the shared registry.
    BecomeDesignated(PermanentDesignation),
    /// `OBJECT phases out` (CR 702.26b): the permanent's phasing status.
    PhaseOut,
    /// `remove OBJECT from combat` (CR 506.4).
    RemoveFromCombat,
    /// `remove all|up to N|any number of counters from OBJECT`: counters of
    /// any kinds (CR 122.8), chosen by the remover while it resolves.
    RemoveUnnamedCounters(UnnamedCounterRemoval),
    /// `put its|those counters on OBJECT`: as many counters of each kind as
    /// the referenced object has, or had as it last existed (CR 608.2h).
    PutCountersOf(PermanentEffectReference),
}

impl PermanentKeywordAction {
    /// Third-person intransitive verb forms whose grammatical subject is the
    /// acting permanent (`it explores`, `target creature connives`).
    pub(super) fn from_subject_verb(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "explores" => Some(Self::Explore),
            "connives" => Some(Self::Connive),
            "endures" => Some(Self::Endure),
            _ => None,
        }
    }

    /// Imperative verbs whose object is an explicit permanent operand.
    pub(super) fn from_imperative_object_verb(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "suspect" => Some(Self::Suspect),
            "detain" => Some(Self::Detain),
            _ => None,
        }
    }

    /// Imperative verbs defined over `this permanent` (CR 701.46a, 701.37a).
    pub(super) fn from_self_quantity_verb(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "adapt" => Some(Self::Adapt),
            "monstrosity" => Some(Self::Monstrosity),
            _ => None,
        }
    }

    /// Whether the action carries a numeric argument, and whether it may be omitted.
    pub(super) fn quantity_policy(self) -> KeywordQuantityPolicy {
        match self {
            Self::Regenerate
            | Self::Goad
            | Self::Suspect
            | Self::Detain
            | Self::Explore
            | Self::BecomeDesignated(_)
            | Self::PhaseOut
            | Self::RemoveFromCombat
            | Self::RemoveUnnamedCounters(
                UnnamedCounterRemoval::Every | UnnamedCounterRemoval::AnyNumber,
            )
            | Self::PutCountersOf(_) => KeywordQuantityPolicy::None,
            Self::RemoveUnnamedCounters(UnnamedCounterRemoval::UpTo) => {
                KeywordQuantityPolicy::Required
            }
            Self::Connive => KeywordQuantityPolicy::Optional,
            Self::Endure | Self::Adapt | Self::Monstrosity => KeywordQuantityPolicy::Required,
        }
    }

    /// Procedures that act on exactly one object at a time and have an
    /// ordering rule for simultaneous instructions (CR 701.44d, 701.50c).
    pub(super) fn requires_singular_object(self) -> bool {
        matches!(
            self,
            Self::Explore | Self::Connive | Self::Endure | Self::Adapt | Self::Monstrosity
        )
    }

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Regenerate => "regenerate",
            Self::Goad => "goad",
            Self::Suspect => "suspect",
            Self::Detain => "detain",
            Self::Explore => "explore",
            Self::Connive => "connive",
            Self::Endure => "endure",
            Self::Adapt => "adapt",
            Self::Monstrosity => "monstrosity",
            Self::BecomeDesignated(designation) => designation.flag(),
            Self::PhaseOut => "phase out",
            Self::RemoveFromCombat => "remove from combat",
            Self::RemoveUnnamedCounters(_) => "remove counters",
            Self::PutCountersOf(_) => "put counters of",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KeywordQuantityPolicy {
    None,
    Optional,
    Required,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PermanentKeywordActionNode {
    pub action: PermanentKeywordAction,
    pub targets: PermanentOperandNode,
    /// `connive N`, `endure N`, `adapt N`, `monstrosity N`; `None` is the
    /// unparameterized form (connive = connive 1, CR 701.50a/d).
    pub quantity: Option<NumericExpression>,
}

/// Keyword actions performed by the ability controller without an object
/// operand: `populate` (CR 701.36a), `bolster N` (CR 701.39a), `discover N`
/// (CR 701.57a), `learn` (CR 701.48a), `manifest` over the shared
/// library-top operand (CR 701.40a, the count is the quantity) and
/// `manifest dread` (CR 701.62a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControllerKeywordAction {
    Populate,
    Bolster,
    Discover,
    Learn,
    Manifest,
    ManifestDread,
}

impl ControllerKeywordAction {
    pub(super) fn from_imperative_verb(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "populate" => Some(Self::Populate),
            "bolster" => Some(Self::Bolster),
            "discover" => Some(Self::Discover),
            "learn" => Some(Self::Learn),
            "manifest" => Some(Self::Manifest),
            _ => None,
        }
    }

    pub(super) fn takes_quantity(self) -> bool {
        matches!(self, Self::Bolster | Self::Discover)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ControllerKeywordActionNode {
    pub action: ControllerKeywordAction,
    pub quantity: Option<NumericExpression>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EffectConnector {
    And,
    Then,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct EffectSequenceNode {
    pub effects: Vec<LocalEffectNode>,
    pub connectors: Vec<EffectConnector>,
    /// Postposed `where X is EXPR` definitions, each scoped to the actions of
    /// the sentence that carries it (CR 107.3).
    pub where_x: Vec<WhereXBindingNode>,
    /// Sentence-level `If CONDITION, EFFECTS.` instructions, each governing
    /// only the action range of its own sentence (and `Otherwise` sentence).
    pub conditionals: Vec<SentenceConditionalNode>,
    /// `EFFECT unless PAYER COST` / `unless CONDITION` frames (CR 118.12),
    /// each over the guarded actions of the sentence that carries it.
    pub unless: Vec<UnlessFrameNode>,
    /// Action ranges performed later by a delayed triggered ability
    /// (CR 603.7), each inside the sentence that states its timing.
    pub delayed: Vec<DelayedFrameNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum LocalZone {
    Library,
    Hand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CollectionBinding(pub usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ControlledCountDirection {
    OrMore,
    OrFewer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ControlledPermanentCountConditionNode {
    pub player: LocalPlayerReference,
    pub threshold: i64,
    pub direction: ControlledCountDirection,
    pub predicate: ObjectPredicate,
    pub exclude_source: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BoundCollectionUntapEffectNode {
    pub binding: CollectionBinding,
    pub object_type: ObjectCardType,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CollectionOrder {
    Chosen,
    Random,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TopCardsEffectNode {
    pub count: i64,
    pub zone: LocalZone,
    pub owner: TopCardsLibraryOwnerNode,
    pub binding: CollectionBinding,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PartitionCollectionNode {
    pub binding: CollectionBinding,
    pub chosen_count: i64,
    pub chosen_destination: LocalZone,
    pub remainder_destination: LocalZone,
    pub order: CollectionOrder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TopCardsPartitionSequenceNode {
    pub top: TopCardsEffectNode,
    pub partition: PartitionCollectionNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReorderBoundCollectionNode {
    pub binding: CollectionBinding,
    pub destination: LocalZone,
    pub position: LibraryPosition,
    pub order: CollectionOrder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct OptionalShuffleCollectionNode {
    pub player: LocalPlayerReference,
    pub zone: LocalZone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TopCardsReorderSequenceNode {
    pub top: TopCardsEffectNode,
    pub reorder: ReorderBoundCollectionNode,
    pub shuffle: OptionalShuffleCollectionNode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ControlledPermanentChoiceNode {
    pub player: TargetBinding,
    pub predicate: SelectorPredicate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PlayerZoneKind {
    Graveyard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PossessivePlayerZoneNode {
    pub player: TargetBinding,
    pub zone: PlayerZoneKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompoundObjectConnector {
    And,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PlayerCompoundExileNode {
    pub subject: TargetListNode,
    pub subject_binding: TargetBinding,
    pub permanent: ControlledPermanentChoiceNode,
    pub zone: PossessivePlayerZoneNode,
    pub connector: CompoundObjectConnector,
}

pub(super) use player_condition::*;
pub(super) use sentence_condition::*;
pub(super) use static_condition::*;
pub(super) use trigger_envelope::*;
pub(super) use trigger_guard::*;
pub(super) use trigger_program::*;

pub(super) use boolean_condition::*;
pub(super) use deck_construction_keyword::*;
pub(super) use token_window::*;
pub(super) use unsupported_word::*;

#[cfg(test)]
#[path = "target_card_sequence_tests.rs"]
mod target_card_sequence_tests;

mod performer;
mod permanent_action;
mod referenced_permanent_action;
mod untap_restriction;
pub(in crate::oracle::canonical) use performer::{
    PerformedEffectNode, PerformerContext, PerformerNode, controller_reference_failure,
    object_relation_declaration_starts, performer_description, performer_modality,
    permission_sentence_end, person_shifted_permission_tokens, person_shifted_tokens,
    recognize_declaring_performer, recognize_object_relation_declaration, recognize_performer,
};
pub(in crate::oracle::canonical) use referenced_permanent_action::{
    OwnerLibraryPlacement, ReferencedPermanentAction, ReferencedPermanentActionAttempt,
    ReferencedPermanentActionNode, recognize_destroy_reference_effect,
    recognize_referenced_permanent_zone_action,
};
pub(in crate::oracle::canonical) use untap_restriction::{
    SubjectNumber, UntapRestrictionDuration, UntapRestrictionNode, recognize_untap_restriction,
    untap_restriction_starts,
};
mod permanent_subject;
mod token_window;
mod unsupported_word;
pub(in crate::oracle::canonical) use permanent_action::{
    PermanentActionAttempt, ordinary_keyword_subject, permanent_operand_is_plural,
    try_recognize_permanent_action,
};
pub(in crate::oracle::canonical) use permanent_subject::{
    ObservedSourceRelationship, PermanentSubjectScope, SourceAnaphorBinding,
    recognize_contextual_permanent_operand, selector_guarantees_permanent,
};

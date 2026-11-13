mod ability_splitter;
mod class;
mod classifier;
mod context;
mod counter_name_resolver;
mod name_resolver;
mod named_token_resolver;
mod normalize;
mod tokenizer;

pub(crate) use ability_splitter::{documents, split_abilities};
pub(crate) use class::{
    apply_class_level_requirement, class_level_header, class_level_header_from_tokens,
    is_class_type_line,
};
pub(crate) use classifier::{
    activation_parts, classify_ability, classify_promoted_ability_from_tokens,
    has_root_activation_effect_head, is_promoted_spell_cost_increase_envelope,
    is_promoted_static_spell_reduction_envelope, root_activation_effect_start,
};
pub(crate) use context::AbilityInput;
pub(crate) use counter_name_resolver::resolve_counter_name_spans;
#[allow(unused_imports)]
pub(crate) use name_resolver::{
    ExactNameTokenIndex, NameLinkKind, NameResolution, NameResolutionContext,
    NameResolutionFailure, NameResolverMemory, NameResolverState, NameResolverTransition,
    NamedCardIndex, ResolvedNameLink, UnresolvedName, resolve_unknown_names,
};
pub(crate) use named_token_resolver::resolve_named_token_spans;
pub(crate) use normalize::strip_short_oracle_label;
pub(crate) use tokenizer::recognize_entities;
pub(crate) use tokenizer::recognize_oracle_entities;

#[cfg(test)]
pub(crate) use tokenizer::{oracle_tokenization_count, reset_oracle_tokenization_count};

#[cfg(test)]
mod name_resolver_tests;

#[cfg(test)]
mod counter_name_resolver_tests;

#[cfg(test)]
mod named_token_resolver_tests;

#[cfg(test)]
mod tokenizer_tests;

#[cfg(test)]
mod classifier_tests;

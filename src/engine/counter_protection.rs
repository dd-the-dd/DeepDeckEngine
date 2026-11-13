use super::{GameEngine, StackObject, matches_card_filter, stack_object_is_spell, value_kind};

impl GameEngine {
    /// Evaluates counter prohibitions at the moment an object would be countered.
    ///
    /// The intrinsic flag belongs to the stack object. Continuous battlefield
    /// effects are deliberately re-evaluated here so a source entering or
    /// leaving after the spell was cast immediately changes the result.
    pub(super) fn stack_object_cant_be_countered(&self, object: &StackObject) -> bool {
        object.cant_be_countered
            || self.battlefield_static_modifiers().iter().any(
                |(source, modifier)| match value_kind(modifier) {
                    Some("allStackObjectsCantBeCountered") => true,
                    Some("cantBeCountered") if stack_object_is_spell(object) => {
                        let selector = &modifier["objects"];
                        value_kind(selector) == Some("spells")
                            && selector.get("controller").is_none_or(|controller| {
                                self.event_player_reference_matches(
                                    controller,
                                    source,
                                    &object.controller,
                                )
                            })
                            && selector.get("where").is_none_or(|filter| {
                                matches_card_filter(&object.card.definition, filter)
                            })
                    }
                    _ => false,
                },
            )
    }
}

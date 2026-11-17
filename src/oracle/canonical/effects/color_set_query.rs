//! One preserved color-set query for mana iteration and numeric cardinality.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_color_set_query(query: &ColorSetQueryNode) -> Value {
    match query {
        ColorSetQueryNode::PermanentsControlledByAbilityController => {
            json!({"kind":"eachColorAmongPermanents","player":controller()})
        }
    }
}

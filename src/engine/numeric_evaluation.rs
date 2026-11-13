use super::source_lki::{self, SourceNumericProperty, SourcePropertyInput};
use super::{CardInstance, StackObject};

/// Evaluates a numeric property from the live ability source, captured
/// last-known information, or the source snapshot selected by `source_lki`.
pub(super) fn evaluate_source_property(
    property_kind: &str,
    live_source: Option<&CardInstance>,
    stack_object: &StackObject,
    power: impl Fn(&CardInstance) -> i32,
    toughness: impl Fn(&CardInstance) -> i32,
) -> Option<i32> {
    let property = match property_kind {
        "powerOf" => SourceNumericProperty::Power,
        "toughnessOf" => SourceNumericProperty::Toughness,
        _ => return None,
    };
    match source_lki::property_input(stack_object, live_source, property)? {
        SourcePropertyInput::Captured(value) => Some(value),
        SourcePropertyInput::Live(source) | SourcePropertyInput::Snapshot(source) => {
            Some(match property {
                SourceNumericProperty::Power => power(source),
                SourceNumericProperty::Toughness => toughness(source),
            })
        }
    }
}

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PIXI_REPLAY_SCHEMA_VERSION: &str = "mtg-pixi-replay/v2";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservationDelta {
    pub sequence: u64,
    pub previous_sequence: u64,
    pub patch: Value,
}

pub fn merge_patch(previous: &Value, current: &Value) -> Value {
    if previous == current {
        return Value::Object(Map::new());
    }
    match (previous, current) {
        (Value::Object(before), Value::Object(after)) => {
            let mut patch = Map::new();
            for key in before.keys() {
                if !after.contains_key(key) {
                    patch.insert(key.clone(), Value::Null);
                }
            }
            for (key, value) in after {
                if before.get(key) != Some(value) {
                    patch.insert(
                        key.clone(),
                        before
                            .get(key)
                            .map_or_else(|| value.clone(), |old| merge_patch(old, value)),
                    );
                }
            }
            Value::Object(patch)
        }
        _ => current.clone(),
    }
}

pub fn apply_merge_patch(target: &mut Value, patch: &Value) {
    let Value::Object(changes) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let object = target
        .as_object_mut()
        .expect("target was normalized to an object");
    for (key, value) in changes {
        if value.is_null() {
            object.remove(key);
        } else {
            apply_merge_patch(object.entry(key.clone()).or_insert(Value::Null), value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_merge_patch, merge_patch};
    use serde_json::json;

    #[test]
    fn generated_patch_reconstructs_the_current_observation() {
        let previous = json!({"state":{"turnNumber":1,"players":[{"life":20}]},"old":true});
        let current = json!({"state":{"turnNumber":2,"players":[{"life":17}]}});
        let patch = merge_patch(&previous, &current);
        let mut reconstructed = previous;
        apply_merge_patch(&mut reconstructed, &patch);
        assert_eq!(reconstructed, current);
    }
}

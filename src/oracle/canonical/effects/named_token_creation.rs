//! Lower one typed token expression into existing independent IR operations.
use super::super::*;

pub(in crate::oracle::canonical) fn lower_named_token_creation(
    cursor: &mut OracleTokenCursor<'_>,
    creation: &NamedTokenCreationPattern,
    player: Value,
    existing_single_x: bool,
) -> MachineResult<Value> {
    if let NamedTokenCreationPattern::OneOf(specs) = creation {
        cursor.require(specs.len() >= 2, "at least two token alternatives")?;
        cursor.enter("lower.named-token-alternatives.quantity");
        cursor.require(
            specs.iter().all(|spec| {
                matches!(spec.quantity,
                NumericAtom::Integer(n) if n > 0 && n <= i64::from(i32::MAX))
            }),
            "positive literal quantities within the engine i32 range for every token alternative",
        )?;
    }
    let leaf = |spec: &NamedTokenSpec, allow_x: bool| -> MachineResult<Value> {
        cursor.require(
            allow_x || !matches!(spec.quantity, NumericAtom::VariableX),
            "a literal named-token quantity; X requires a declaration",
        )?;
        let mut effect = json!({"kind":"createTokens", "controller":player,
            "quantity":lower_numeric_atom(spec.quantity.clone()),
            "token":{"kind":"namedToken","name":spec.token_name}});
        if spec.tapped {
            effect["tapped"] = json!(true);
        }
        Ok(effect)
    };
    match creation {
        NamedTokenCreationPattern::Single(spec) => leaf(spec, existing_single_x),
        NamedTokenCreationPattern::OneOf(specs) => {
            let options = specs
                .iter()
                .map(|spec| {
                    let quantity =
                        match spec.quantity {
                            NumericAtom::Integer(n) => n,
                            NumericAtom::VariableX => return Err(cursor.fail(
                                "a literal alternative quantity without an unproved X declaration",
                            )),
                        };
                    Ok(json!({
                        "label":format!("Create {} {} {}", quantity, spec.token_name,
                            if quantity == 1 { "token" } else { "tokens" }),
                        "effects":[leaf(spec, false)?]
                    }))
                })
                .collect::<MachineResult<Vec<Value>>>()?;
            Ok(json!({"kind":"chooseOneEffect", "player":player, "options":options}))
        }
    }
}

#[cfg(test)]
#[path = "named_token_option_bounds_tests.rs"]
mod bounds_tests;

//! Typed evaluation of existing mana-value IR into exact type-support witnesses.
//! Type support and executable output enumeration share one typed value walk.
//! The two wrapper results are deliberately distinct.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueEvaluationMode {
    TypeSupport,
    Executable,
}

enum ManaValueFacts {
    NoMana(NoManaReason),
    Symbols(Vec<Vec<String>>),
}

impl ManaProjectionContext<'_> {
    fn amount(
        &self,
        value: &Value,
        controller: &str,
        source: &CardInstance,
        mode: ValueEvaluationMode,
        runtime_scalar: Option<&dyn Fn(&Value) -> Option<i32>>,
    ) -> Result<usize, ManaProjectionContextError> {
        let binding_state = if mode == ValueEvaluationMode::Executable {
            "mana.execution.numeric-binding"
        } else {
            "mana.projection.numeric-binding"
        };
        if requires_activation_binding(value) && runtime_scalar.is_none() {
            return Err(ManaProjectionContextError::new(
                binding_state,
                &source.instance_id,
                format!("activation/trigger binding is not closed in this value frame: {value}"),
            ));
        }
        let n = integer_value(value)
            .or_else(|| match runtime_scalar {
                Some(resolve) => resolve(value),
                None => self
                    .engine
                    .numeric_expression_with_source(value, controller, Some(source)),
            })
            .ok_or_else(|| {
                ManaProjectionContextError::new(
                    binding_state,
                    &source.instance_id,
                    format!("unresolved amount: {value}"),
                )
            })?;
        usize::try_from(n.max(0)).map_err(|_| {
            ManaProjectionContextError::new(
                "mana.projection.numeric-range",
                &source.instance_id,
                n.to_string(),
            )
        })
    }

    pub(super) fn value_projection(
        &self,
        value: &Value,
        controller: &str,
        source: &CardInstance,
        table: &ManaTypeTable,
    ) -> Result<ValueProjection, ManaProjectionContextError> {
        match self.evaluate_value(
            value,
            controller,
            source,
            Some(table),
            ValueEvaluationMode::TypeSupport,
            None,
        )? {
            ManaValueFacts::NoMana(reason) => Ok(ValueProjection::NoMana(reason)),
            ManaValueFacts::Symbols(witnesses) => Ok(ValueProjection::TypeWitnesses(witnesses)),
        }
    }

    pub(in crate::engine) fn executable_outputs(
        &self,
        value: &Value,
        controller: &str,
        source: &CardInstance,
    ) -> Result<ExecutableManaOutputs, ManaProjectionContextError> {
        self.executable_outputs_in_frame(value, controller, source, None)
    }

    pub(in crate::engine) fn executable_outputs_with_scalar(
        &self,
        value: &Value,
        controller: &str,
        source: &CardInstance,
        scalar: &dyn Fn(&Value) -> Option<i32>,
    ) -> Result<ExecutableManaOutputs, ManaProjectionContextError> {
        self.executable_outputs_in_frame(value, controller, source, Some(scalar))
    }

    fn executable_outputs_in_frame(
        &self,
        value: &Value,
        controller: &str,
        source: &CardInstance,
        runtime_scalar: Option<&dyn Fn(&Value) -> Option<i32>>,
    ) -> Result<ExecutableManaOutputs, ManaProjectionContextError> {
        match self.evaluate_value(
            value,
            controller,
            source,
            None,
            ValueEvaluationMode::Executable,
            runtime_scalar,
        )? {
            ManaValueFacts::NoMana(reason) => Ok(ExecutableManaOutputs::NoMana(reason)),
            ManaValueFacts::Symbols(choices) => Ok(ExecutableManaOutputs::Choices(choices)),
        }
    }

    fn evaluate_value(
        &self,
        value: &Value,
        controller: &str,
        source: &CardInstance,
        table: Option<&ManaTypeTable>,
        mode: ValueEvaluationMode,
        runtime_scalar: Option<&dyn Fn(&Value) -> Option<i32>>,
    ) -> Result<ManaValueFacts, ManaProjectionContextError> {
        self.require_player(controller)?;
        let fail =
            |state, detail| ManaProjectionContextError::new(state, &source.instance_id, detail);
        if let Some(text) = value.as_str() {
            let symbols = parse_mana_symbols(text);
            if symbols.is_empty() || symbols.iter().any(|s| ManaType::from_symbol(s).is_none()) {
                return Err(fail("mana.projection.production-symbol", text.to_string()));
            }
            return Ok(ManaValueFacts::Symbols(vec![symbols]));
        }
        match value_kind(value) {
            Some("chooseOne") => {
                let options = value["options"]
                    .as_array()
                    .ok_or_else(|| fail("mana.projection.options", value.to_string()))?;
                if options.is_empty() {
                    return Err(fail(
                        "mana.projection.options",
                        "empty option list".to_string(),
                    ));
                }
                let mut produced = Vec::new();
                let mut reason = NoManaReason::ZeroAmount;
                for option in options {
                    match self.evaluate_value(
                        option,
                        controller,
                        source,
                        table,
                        mode,
                        runtime_scalar,
                    )? {
                        ManaValueFacts::Symbols(v) => produced.extend(v),
                        ManaValueFacts::NoMana(r) => {
                            if mode == ValueEvaluationMode::Executable {
                                produced.push(Vec::new());
                            }
                            if r == NoManaReason::UndefinedType {
                                reason = r;
                            }
                        }
                    }
                }
                Ok(if produced.iter().all(Vec::is_empty) {
                    ManaValueFacts::NoMana(reason)
                } else {
                    ManaValueFacts::Symbols(produced)
                })
            }
            Some(
                "manaTypesLandsYouControlCouldProduce"
                | "manaColorsLandsOpponentsControlCouldProduce",
            ) => {
                let amount =
                    self.amount(&value["amount"], controller, source, mode, runtime_scalar)?;
                if amount == 0 {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::ZeroAmount));
                }
                let opponents =
                    value_kind(value) == Some("manaColorsLandsOpponentsControlCouldProduce");
                let queried = match table {
                    Some(snapshot) => self.query_types(snapshot, controller, opponents)?,
                    None => {
                        let context = self.projected_context()?;
                        context.query_types(&context.projected, controller, opponents)?
                    }
                };
                match queried {
                    ManaTypeProjection::NoMana(r) => Ok(ManaValueFacts::NoMana(r)),
                    ManaTypeProjection::Types(types) => Ok(ManaValueFacts::Symbols(
                        types
                            .into_iter()
                            .map(|kind| {
                                vec![
                                    symbol(kind).to_string();
                                    if mode == ValueEvaluationMode::TypeSupport {
                                        1
                                    } else {
                                        amount
                                    }
                                ]
                            })
                            .collect(),
                    )),
                }
            }
            Some("fixedMana" | "chooseColor" | "chooseColors") => {
                if value_kind(&value["amount"]) == Some("devotionToChosenColor") {
                    if value_kind(value) != Some("chooseColor") {
                        return Err(fail("mana.projection.devotion-domain", value.to_string()));
                    }
                    return exact_options(
                        self.engine.mana_options_for_value(
                            value,
                            controller,
                            &BTreeSet::new(),
                            Some(source),
                        ),
                        source,
                    );
                }
                let amount =
                    self.amount(&value["amount"], controller, source, mode, runtime_scalar)?;
                if amount == 0 {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::ZeroAmount));
                }
                if value_kind(value) == Some("fixedMana") {
                    let output = value["symbol"]
                        .as_str()
                        .and_then(ManaType::from_symbol)
                        .ok_or_else(|| fail("mana.projection.fixed-symbol", value.to_string()))?;
                    let copies = match mode {
                        ValueEvaluationMode::TypeSupport => 1,
                        ValueEvaluationMode::Executable => amount,
                    };
                    let symbols = vec![symbol(output).to_string(); copies];
                    return Ok(ManaValueFacts::Symbols(vec![symbols]));
                }
                if mode == ValueEvaluationMode::Executable
                    && value_kind(value) == Some("chooseColors")
                {
                    if amount > 5 {
                        return Err(fail("mana.execution.choice-cardinality",
                            "more than five independently selected mana colors need a non-enumerating decision protocol".to_string()));
                    }
                    let mut bound = value.clone();
                    bound["amount"] = json!(amount);
                    return exact_options(
                        self.engine.mana_options_for_value(
                            &bound,
                            controller,
                            &BTreeSet::new(),
                            Some(source),
                        ),
                        source,
                    );
                }
                // Type support is the same for any positive unrestricted choice.
                // Quantity-dependent type changes are capped during certification;
                // one witness per possible type is exact in this type domain.
                Ok(ManaValueFacts::Symbols(
                    ["W", "U", "B", "R", "G"]
                        .into_iter()
                        .map(|s| {
                            vec![
                                s.to_string();
                                if mode == ValueEvaluationMode::TypeSupport {
                                    1
                                } else {
                                    amount
                                }
                            ]
                        })
                        .collect(),
                ))
            }
            Some("storedColor") => {
                let id = value["decisionId"]
                    .as_str()
                    .ok_or_else(|| fail("mana.projection.stored-choice", value.to_string()))?;
                let amount =
                    self.amount(&value["amount"], controller, source, mode, runtime_scalar)?;
                if amount == 0 {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::ZeroAmount));
                }
                let Some(color) = stored_decision_value(&source.definition, id) else {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::UndefinedType));
                };
                let kind = ManaType::from_symbol(color)
                    .filter(|t| *t != ManaType::Colorless)
                    .ok_or_else(|| fail("mana.projection.stored-choice-type", color.to_string()))?;
                Ok(ManaValueFacts::Symbols(vec![vec![
                    symbol(kind).to_string(),
                ]]))
            }
            Some("chooseCommanderColor") => {
                let amount =
                    self.amount(&value["amount"], controller, source, mode, runtime_scalar)?;
                if amount == 0 {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::ZeroAmount));
                }
                let player = &self.engine.state.players[self.require_player(controller)?];
                let mut colors = BTreeSet::new();
                for commander in player
                    .library
                    .iter()
                    .chain(&player.hand)
                    .chain(&player.battlefield)
                    .chain(&player.graveyard)
                    .chain(&player.exile)
                    .chain(&player.sideboard)
                    .chain(&player.command_zone)
                    .filter(|c| is_commander_definition(&c.definition))
                {
                    let identity = commander
                        .definition
                        .printed_rules()
                        .iter()
                        .filter(|r| value_kind(r) == Some("rulesMarker"))
                        .find_map(|r| r["colorIdentity"].as_array())
                        .ok_or_else(|| {
                            fail(
                                "mana.projection.commander-identity",
                                format!("missing explicit identity for {}", commander.instance_id),
                            )
                        })?;
                    for color in identity {
                        let kind = color
                            .as_str()
                            .and_then(ManaType::from_symbol)
                            .filter(|t| *t != ManaType::Colorless)
                            .ok_or_else(|| {
                                fail("mana.projection.commander-identity", color.to_string())
                            })?;
                        colors.insert(kind);
                    }
                }
                if colors.is_empty() {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::UndefinedType));
                }
                Ok(ManaValueFacts::Symbols(
                    colors
                        .into_iter()
                        .map(|k| {
                            vec![
                                symbol(k).to_string();
                                if mode == ValueEvaluationMode::TypeSupport {
                                    1
                                } else {
                                    amount
                                }
                            ]
                        })
                        .collect(),
                ))
            }
            Some("manaTypesOfSacrificedPermanent") => Err(fail(
                "mana.projection.sacrificed-binding",
                "hypothetical sacrificed-object binding is not closed".to_string(),
            )),
            Some("eachColorAmongPermanents") => {
                // Retain the existing mana producer's bounded literal domain.
                // One output contains N of EACH present color; this is not a
                // choose-one-color decision or a commander-identity query.
                let amount = match value.get("amount") {
                    None => 1,
                    Some(amount) => integer_value(amount)
                        .filter(|amount| (1..=10).contains(amount))
                        .ok_or_else(|| {
                            fail("mana.projection.color-quantity", amount.to_string())
                        })?,
                };
                let colors = self
                    .engine
                    .checked_color_set_query(value, controller)
                    .map_err(|error| fail(error.state, error.detail))?;
                if colors.cardinality() == 0 {
                    return Ok(ManaValueFacts::NoMana(NoManaReason::UndefinedType));
                }
                let repeats = if mode == ValueEvaluationMode::TypeSupport {
                    1
                } else {
                    amount as usize
                };
                let output = colors
                    .symbols()
                    .into_iter()
                    .flat_map(|symbol| std::iter::repeat_n(symbol, repeats))
                    .collect();
                Ok(ManaValueFacts::Symbols(vec![output]))
            }
            Some("linkedExiledCardColors") => Err(fail(
                "mana.projection.color-source-context",
                "effective color/player binding needs a shared closed evaluator".to_string(),
            )),
            _ => Err(fail("mana.projection.value-kind", value.to_string())),
        }
    }
}

fn symbol(kind: ManaType) -> &'static str {
    match kind {
        ManaType::White => "W",
        ManaType::Blue => "U",
        ManaType::Black => "B",
        ManaType::Red => "R",
        ManaType::Green => "G",
        ManaType::Colorless => "C",
    }
}

fn exact_options(
    options: Vec<Vec<String>>,
    source: &CardInstance,
) -> Result<ManaValueFacts, ManaProjectionContextError> {
    if options
        .iter()
        .flatten()
        .any(|s| ManaType::from_symbol(s).is_none())
    {
        return Err(ManaProjectionContextError::new(
            "mana.projection.output-symbol",
            &source.instance_id,
            "noncanonical output type",
        ));
    }
    if options.iter().all(Vec::is_empty) {
        Ok(ManaValueFacts::NoMana(NoManaReason::ZeroAmount))
    } else {
        Ok(ManaValueFacts::Symbols(options))
    }
}

pub(super) fn context_free_mana_value(value: &Value) -> bool {
    if value.is_string() {
        return true;
    }
    match value_kind(value) {
        Some("chooseOne") => value["options"]
            .as_array()
            .is_some_and(|v| v.iter().all(context_free_mana_value)),
        Some("chooseColor" | "chooseColors" | "fixedMana") => {
            integer_value(&value["amount"]).is_some()
        }
        _ => false,
    }
}

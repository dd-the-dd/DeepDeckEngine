use serde_json::Value;

use crate::oracle::model::{
    OracleCardParseRequest, ParsedOracleAbility, ParserDiagnostic, RecognizerProvenance,
};

const X_VALUE: &str = "xValue";

/// Prefix of the `bind` ids that carry a postposed `where X is EXPR` value
/// (CR 107.3). The sentence that defines X is lowered without knowing its
/// face; this validator rejects a face that also supplies X by another
/// producer, so the two definitions are never merged or borrowed.
pub(crate) const WHERE_X_BINDING_ID: &str = "whereX";

/// Decision id of a creature type chosen while an effect program resolves
/// (CR 608.2d). Distinct from the `As ... enters, choose` replacement choice
/// stored on a permanent: only a `chooseCreatureType` action of the same rule
/// produces it, and the validator below rejects a rule without that producer.
pub(crate) const RESOLUTION_CREATURE_TYPE_DECISION: &str = "resolutionCreatureType";

/// Decision id of a color chosen while an effect program resolves (CR
/// 608.2d). Only a `chooseColor` step of the same rule produces it; the
/// engine reads it through the `chosenColor` object predicate.
pub(crate) const RESOLUTION_COLOR_DECISION: &str = "resolutionColor";

/// Decision id of a card name chosen while an effect program resolves (CR
/// 608.2d). Only a `chooseCardName` step of the same rule produces it; the
/// engine reads it through the `chosenCardName` object predicate. Distinct
/// from a name stored on a permanent by an `As ... enters` choice.
pub(crate) const RESOLUTION_CARD_NAME_DECISION: &str = "resolutionCardName";

/// The cost that records a cost-paid object binding when it is paid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CostObjectProducer {
    /// A `sacrificePermanent` cost (CR 601.2h, 602.2b).
    Sacrifice,
    /// A `tap` cost of a chosen permanent (`Tap another untapped creature
    /// you control`, CR 702.184a); the source's own `{T}` taps no other
    /// object.
    TapChosen,
}

impl CostObjectProducer {
    fn produces(self, cost: &Value) -> bool {
        match self {
            Self::Sacrifice => cost["kind"].as_str() == Some("sacrificePermanent"),
            Self::TapChosen => {
                cost["kind"].as_str() == Some("tap")
                    && cost["object"]["kind"].as_str() == Some("chosenTarget")
            }
        }
    }
}

/// Stack values bound when a cost is paid, keyed by the cost field that asks
/// the engine to record them (CR 608.2h last known information). A tapped
/// cost object records only its power: the runtime tap cost has no
/// toughness or mana-value binding, so the reader refuses those reads.
const COST_OBJECT_BINDINGS: [(&str, &str, CostObjectProducer); 4] = [
    (
        "sacrificedPermanentPower",
        "bindPowerAs",
        CostObjectProducer::Sacrifice,
    ),
    (
        "sacrificedPermanentToughness",
        "bindToughnessAs",
        CostObjectProducer::Sacrifice,
    ),
    (
        "sacrificedPermanentManaValue",
        "bindManaValueAs",
        CostObjectProducer::Sacrifice,
    ),
    (
        "tappedPermanentPower",
        "bindPowerAs",
        CostObjectProducer::TapChosen,
    ),
];

pub(crate) fn validate_native_contextual_bindings(
    request: &OracleCardParseRequest,
    abilities: &mut [ParsedOracleAbility],
) {
    crate::oracle::entry_choice_binding::bind_entry_choices(abilities);
    bind_cost_paid_objects(abilities);
    for ability in abilities {
        if ability.status != "canonical" || ability.recognizer != RecognizerProvenance::Native {
            continue;
        }
        let Some(rule) = ability.rule.as_ref() else {
            continue;
        };
        // CR 601.2h: the colors of mana spent to cast a spell are recorded
        // with that spell's own payment. Another ability of the card (an
        // enters trigger reading `this spell`) resolves without it.
        // The source's own entry replacement applies while its spell
        // resolves (CR 614.12), with that cast's payment.
        let own_entry_replacement = rule["kind"].as_str() == Some("replacementEffect")
            && rule["event"]["kind"].as_str() == Some("wouldEnterBattlefield")
            && rule["event"]["object"]["kind"].as_str() == Some("self");
        if rule["kind"].as_str() != Some("spellAbility")
            && !own_entry_replacement
            && references_kind(rule, "colorsOfManaSpentToCastSource")
        {
            let found = rule["kind"].as_str().unwrap_or("unknown rule").to_string();
            let source_span_end = ability.source.text.len();
            ability.status = "unsupported".to_string();
            ability.rule = None;
            ability.iterations.clear();
            ability.diagnostics.push(ParserDiagnostic {
                code: "oracle_context_binding_failure".to_string(),
                message: "The colors of mana spent to cast a spell are read only by that spell's own resolution.".to_string(),
                severity: "unsupported".to_string(),
                automaton: Some("card-context-binding".to_string()),
                state: Some("binding.mana-spent-colors.source".to_string()),
                token_index: None,
                span_start: Some(0),
                span_end: Some(source_span_end),
                expected: Some("a spell ability whose own cast payment records the colors".to_string()),
                found: Some(found),
                trace: Vec::new(),
                owner: None,
            });
            continue;
        }
        if binds_where_x(rule) {
            let face_mana_cost = mana_cost_for_source(request, ability.source.face_id.as_deref());
            let mana_x = face_mana_cost.is_some_and(|mana_cost| mana_cost.contains("{X}"));
            if mana_x || declares_number(rule, X_VALUE) {
                let source_span_end = ability.source.text.len();
                ability.status = "unsupported".to_string();
                ability.rule = None;
                ability.iterations.clear();
                ability.diagnostics.push(ParserDiagnostic {
                    code: "oracle_context_binding_failure".to_string(),
                    message: "A `where X is` definition binds X, but this face also supplies X through {X} in its mana cost or an explicit `xValue` declaration; the two producers are not merged.".to_string(),
                    severity: "unsupported".to_string(),
                    automaton: Some("card-context-binding".to_string()),
                    state: Some("binding.x-value.definition-conflict".to_string()),
                    token_index: None,
                    span_start: Some(0),
                    span_end: Some(source_span_end),
                    expected: Some(
                        "a face whose only producer of X is the `where X is` definition"
                            .to_string(),
                    ),
                    found: Some(if mana_x {
                        face_mana_cost.unwrap_or_default().to_string()
                    } else {
                        "an explicit xValue declaration".to_string()
                    }),
                    trace: Vec::new(),
                    owner: None,
                });
                continue;
            }
        }
        if let Some((state, expected)) = resolution_creature_type_binding_failure(rule) {
            let source_span_end = ability.source.text.len();
            ability.status = "unsupported".to_string();
            ability.rule = None;
            ability.iterations.clear();
            ability.diagnostics.push(ParserDiagnostic {
                code: "oracle_context_binding_failure".to_string(),
                message: "The canonical rule reads a creature type chosen on resolution without a proven producer or executable consumer.".to_string(),
                severity: "unsupported".to_string(),
                automaton: Some("card-context-binding".to_string()),
                state: Some(state.to_string()),
                token_index: None,
                span_start: Some(0),
                span_end: Some(source_span_end),
                expected: Some(expected.to_string()),
                found: Some("a chosen creature type reference".to_string()),
                trace: Vec::new(),
                owner: None,
            });
            continue;
        }
        if let Some((state, expected)) = resolution_color_binding_failure(rule) {
            let source_span_end = ability.source.text.len();
            ability.status = "unsupported".to_string();
            ability.rule = None;
            ability.iterations.clear();
            ability.diagnostics.push(ParserDiagnostic {
                code: "oracle_context_binding_failure".to_string(),
                message: "A color chosen on resolution needs exactly one producing `choose a color` step and a consumer of the same rule.".to_string(),
                severity: "unsupported".to_string(),
                automaton: Some("card-context-binding".to_string()),
                state: Some(state.to_string()),
                token_index: None,
                span_start: Some(0),
                span_end: Some(source_span_end),
                expected: Some(expected.to_string()),
                found: Some("a chosen color reference".to_string()),
                trace: Vec::new(),
                owner: None,
            });
            continue;
        }
        if let Some((state, expected)) = resolution_card_name_binding_failure(rule) {
            let source_span_end = ability.source.text.len();
            ability.status = "unsupported".to_string();
            ability.rule = None;
            ability.iterations.clear();
            ability.diagnostics.push(ParserDiagnostic {
                code: "oracle_context_binding_failure".to_string(),
                message: "A card name chosen on resolution needs exactly one producing `choose a card name` step and a consumer of the same rule.".to_string(),
                severity: "unsupported".to_string(),
                automaton: Some("card-context-binding".to_string()),
                state: Some(state.to_string()),
                token_index: None,
                span_start: Some(0),
                span_end: Some(source_span_end),
                expected: Some(expected.to_string()),
                found: Some("a chosen card name reference".to_string()),
                trace: Vec::new(),
                owner: None,
            });
            continue;
        }
        if !references_decision(rule, X_VALUE) || declares_number(rule, X_VALUE) {
            continue;
        }

        let face_mana_cost = mana_cost_for_source(request, ability.source.face_id.as_deref());
        if rule["kind"].as_str() == Some("spellAbility")
            && face_mana_cost.is_some_and(|mana_cost| mana_cost.contains("{X}"))
        {
            continue;
        }

        let found = face_mana_cost
            .map(ToOwned::to_owned)
            .or_else(|| {
                ability
                    .source
                    .face_id
                    .as_ref()
                    .map(|face_id| format!("missing face `{face_id}`"))
            })
            .unwrap_or_else(|| "no mana cost".to_string());
        let source_span_end = ability.source.text.len();
        ability.status = "unsupported".to_string();
        ability.rule = None;
        ability.iterations.clear();
        ability.diagnostics.push(ParserDiagnostic {
            code: "oracle_context_binding_failure".to_string(),
            message: "The canonical rule consumes `xValue`, but this spell face has neither an explicit `xValue` declaration nor {X} in its mana cost.".to_string(),
            severity: "unsupported".to_string(),
            automaton: Some("card-context-binding".to_string()),
            state: Some("binding.x-value.source".to_string()),
            token_index: None,
            span_start: Some(0),
            span_end: Some(source_span_end),
            expected: Some(
                "an explicit xValue declaration or {X} in this spell face mana cost"
                    .to_string(),
            ),
            found: Some(found),
            trace: Vec::new(),
            owner: None,
        });
    }
}

fn mana_cost_for_source<'a>(
    request: &'a OracleCardParseRequest,
    face_id: Option<&str>,
) -> Option<&'a str> {
    match face_id {
        Some(face_id) => request
            .faces
            .iter()
            .find(|face| face.id == face_id)
            .and_then(|face| face.mana_cost.as_deref()),
        None => request.mana_cost.as_deref(),
    }
}

fn binds_where_x(value: &Value) -> bool {
    if value["kind"].as_str() == Some("bind")
        && value["id"]
            .as_str()
            .is_some_and(|id| id.starts_with(WHERE_X_BINDING_ID))
    {
        return true;
    }
    match value {
        Value::Array(values) => values.iter().any(binds_where_x),
        Value::Object(values) => values.values().any(binds_where_x),
        _ => false,
    }
}

fn references_decision(value: &Value, decision_id: &str) -> bool {
    if value["kind"].as_str() == Some("decisionResult")
        && value["decisionId"].as_str() == Some(decision_id)
    {
        return true;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| references_decision(value, decision_id)),
        Value::Object(values) => values
            .values()
            .any(|value| references_decision(value, decision_id)),
        _ => false,
    }
}

fn is_resolution_creature_type(value: &Value) -> bool {
    value["kind"].as_str() == Some("chosenCreatureType")
        && value["decisionId"].as_str() == Some(RESOLUTION_CREATURE_TYPE_DECISION)
}

/// Counts references to the resolution creature-type choice, and those read
/// by a battlefield query the engine evaluates against resolution choices: a
/// permanent count, or the collection a one-shot battlefield action reads
/// once as it resolves (`destroy all creatures that aren't of the chosen
/// type`). The collection of an action with a duration is not such a reader.
fn resolution_creature_type_references(value: &Value, inside_count: bool) -> (usize, usize) {
    fn visit(value: &Value, inside_count: bool, durable: bool) -> (usize, usize) {
        if is_resolution_creature_type(value) {
            return (1, usize::from(inside_count));
        }
        let durable = durable || value.get("duration").is_some();
        let counted = match value["kind"].as_str() {
            Some("countPermanents") => true,
            Some("eachPermanent") => !durable,
            _ => false,
        };
        let children: Box<dyn Iterator<Item = (&String, &Value)>> = match value {
            Value::Object(values) => Box::new(values.iter()),
            Value::Array(values) => {
                return values.iter().fold((0, 0), |(total, read), value| {
                    let (more, more_read) = visit(value, inside_count, durable);
                    (total + more, read + more_read)
                });
            }
            _ => return (0, 0),
        };
        children.fold((0, 0), |(total, read), (key, value)| {
            let (more, more_read) =
                visit(value, inside_count || (counted && key == "where"), durable);
            (total + more, read + more_read)
        })
    }
    visit(value, inside_count, false)
}

/// Whether a lowered value reads the color chosen on resolution.
pub(crate) fn references_resolution_color(value: &Value) -> bool {
    if value["kind"].as_str() == Some("chosenColor")
        && value["decisionId"].as_str() == Some(RESOLUTION_COLOR_DECISION)
    {
        return true;
    }
    match value {
        Value::Object(values) => values.values().any(references_resolution_color),
        Value::Array(values) => values.iter().any(references_resolution_color),
        _ => false,
    }
}

/// CR 608.2d: a color chosen while the rule resolves is produced once and
/// read by the same rule. A reference without its producer, or a choice no
/// instruction reads, is refused rather than executed as a no-op.
fn resolution_color_binding_failure(rule: &Value) -> Option<(&'static str, &'static str)> {
    let produced = produces_resolution_color(rule);
    if references_resolution_color(rule) {
        (produced != 1).then_some((
            "binding.chosen-color.source",
            "exactly one `choose a color` action of the same rule",
        ))
    } else {
        (produced > 0).then_some((
            "binding.chosen-color.consumer",
            "an instruction of the same rule that reads the chosen color",
        ))
    }
}

fn produces_resolution_color(value: &Value) -> usize {
    let own = usize::from(
        value["kind"].as_str() == Some("chooseColor")
            && value["id"].as_str() == Some(RESOLUTION_COLOR_DECISION),
    );
    own + match value {
        Value::Object(values) => values.values().map(produces_resolution_color).sum(),
        Value::Array(values) => values.iter().map(produces_resolution_color).sum(),
        _ => 0,
    }
}

/// Counts the `kind` values carrying `decision_id` under `field`.
fn count_decision_kind(value: &Value, kind: &str, field: &str, decision_id: &str) -> usize {
    let own = usize::from(
        value["kind"].as_str() == Some(kind) && value[field].as_str() == Some(decision_id),
    );
    own + match value {
        Value::Object(values) => values
            .values()
            .map(|value| count_decision_kind(value, kind, field, decision_id))
            .sum(),
        Value::Array(values) => values
            .iter()
            .map(|value| count_decision_kind(value, kind, field, decision_id))
            .sum(),
        _ => 0,
    }
}

/// CR 608.2d: a card name chosen while the rule resolves is produced once
/// and read by the same rule. A reference without its producer, or a choice
/// no instruction reads, is refused rather than executed as a no-op.
fn resolution_card_name_binding_failure(rule: &Value) -> Option<(&'static str, &'static str)> {
    let produced = count_decision_kind(rule, "chooseCardName", "id", RESOLUTION_CARD_NAME_DECISION);
    let read = count_decision_kind(
        rule,
        "chosenCardName",
        "decisionId",
        RESOLUTION_CARD_NAME_DECISION,
    );
    if read > 0 {
        (produced != 1).then_some((
            "binding.chosen-card-name.source",
            "exactly one `choose a card name` action of the same rule",
        ))
    } else {
        (produced > 0).then_some((
            "binding.chosen-card-name.consumer",
            "an executable binding consumer of the same rule that reads the chosen card name",
        ))
    }
}

pub(crate) fn references_resolution_creature_type(value: &Value) -> bool {
    resolution_creature_type_references(value, false).0 > 0
}

fn produces_resolution_creature_type(value: &Value) -> usize {
    let own = usize::from(
        value["kind"].as_str() == Some("chooseCreatureType")
            && value["id"].as_str() == Some(RESOLUTION_CREATURE_TYPE_DECISION),
    );
    own + match value {
        Value::Object(values) => values.values().map(produces_resolution_creature_type).sum(),
        Value::Array(values) => values.iter().map(produces_resolution_creature_type).sum(),
        _ => 0,
    }
}

/// CR 608.2d: a creature type chosen while the rule resolves is read only by
/// the same rule, after its one producing choice, through a consumer that the
/// engine evaluates against resolution choices.
fn resolution_creature_type_binding_failure(rule: &Value) -> Option<(&'static str, &'static str)> {
    let (references, counted) = resolution_creature_type_references(rule, false);
    if references == 0 {
        return None;
    }
    if produces_resolution_creature_type(rule) != 1 {
        return Some((
            "binding.chosen-creature-type.source",
            "exactly one `choose a creature type` action of the same rule",
        ));
    }
    (counted != references).then_some((
        "binding.chosen-creature-type.consumer",
        "a battlefield count or one-shot collection reading the chosen creature type",
    ))
}

/// Whether an IR node of `kind` occurs anywhere under `value`.
fn references_kind(value: &Value, kind: &str) -> bool {
    match value {
        Value::Object(map) => {
            map.get("kind").and_then(Value::as_str) == Some(kind)
                || map.values().any(|child| references_kind(child, kind))
        }
        Value::Array(items) => items.iter().any(|child| references_kind(child, kind)),
        _ => false,
    }
}

fn declares_number(rule: &Value, decision_id: &str) -> bool {
    rule["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|decision| {
            decision["kind"].as_str() == Some("chooseNumber")
                && decision["id"].as_str() == Some(decision_id)
        })
}

fn reject_binding(ability: &mut ParsedOracleAbility, state: &str, expected: &str, message: &str) {
    let source_span_end = ability.source.text.len();
    ability.status = "unsupported".to_string();
    ability.rule = None;
    ability.iterations.clear();
    ability.diagnostics.push(ParserDiagnostic {
        code: "oracle_context_binding_failure".to_string(),
        message: message.to_string(),
        severity: "unsupported".to_string(),
        automaton: Some("card-context-binding".to_string()),
        state: Some(state.to_string()),
        token_index: None,
        span_start: Some(0),
        span_end: Some(source_span_end),
        expected: Some(expected.to_string()),
        found: Some("a cost-paid object reference".to_string()),
        trace: Vec::new(),
        owner: None,
    });
}

/// The `costs` of an activated rule and a spell declaration's
/// `additionalCosts`: the costs a rule pays itself.
fn own_costs_mut(rule: &mut Value) -> Vec<&mut Value> {
    let mut costs = Vec::new();
    let Some(fields) = rule.as_object_mut() else {
        return costs;
    };
    let mut declaration_costs = None;
    let mut activation_costs = None;
    for (key, value) in fields.iter_mut() {
        match key.as_str() {
            "costs" => activation_costs = Some(value),
            "declaration" => declaration_costs = value.get_mut("additionalCosts"),
            _ => {}
        }
    }
    for list in [activation_costs, declaration_costs].into_iter().flatten() {
        if let Some(list) = list.as_array_mut() {
            costs.extend(list.iter_mut());
        }
    }
    costs
}

/// The number of own costs of `rule` that `producer` names.
fn producing_cost_count(rule: &Value, producer: CostObjectProducer) -> usize {
    let count = |costs: &Value| {
        costs
            .as_array()
            .into_iter()
            .flatten()
            .filter(|cost| producer.produces(cost))
            .count()
    };
    count(&rule["costs"]) + count(&rule["declaration"]["additionalCosts"])
}

fn install_cost_object_bindings(
    rule: &mut Value,
    producer: CostObjectProducer,
    fields: &[(&str, &str)],
) {
    for cost in own_costs_mut(rule) {
        if producer.produces(cost) {
            for (binding, field) in fields {
                cost[*field] = Value::String((*binding).to_string());
            }
        }
    }
}

/// The cost-object bindings a rule reads, grouped by producing cost kind.
type CostObjectReads = Vec<(CostObjectProducer, Vec<(&'static str, &'static str)>)>;

fn read_cost_object_bindings(rule: &Value) -> CostObjectReads {
    let mut groups: CostObjectReads = Vec::new();
    for (binding, field, producer) in COST_OBJECT_BINDINGS {
        if !references_decision(rule, binding) {
            continue;
        }
        match groups.iter_mut().find(|(kind, _)| *kind == producer) {
            Some((_, fields)) => fields.push((binding, field)),
            None => groups.push((producer, vec![(binding, field)])),
        }
    }
    groups
}

/// `the power of the card you exiled` (CR 702.97a) reads the source card an
/// own `exileSource` cost exiled: once exactly one such cost is proven, the
/// symbolic read becomes the source's characteristic (the exiled card keeps
/// its printed characteristics). Returns false when a read has no unique
/// producing cost.
fn resolve_exiled_source_card_reads(rule: &mut Value) -> bool {
    const READS: [(&str, &str); 2] = [
        ("exiledSourceCardPower", "powerOf"),
        ("exiledSourceCardToughness", "toughnessOf"),
    ];
    fn rewrite(value: &mut Value, decision: &str, kind: &str) {
        if value["kind"].as_str() == Some("decisionResult")
            && value["decisionId"].as_str() == Some(decision)
        {
            *value = serde_json::json!({ "kind": kind, "object": { "kind": "self" } });
            return;
        }
        match value {
            Value::Array(values) => values
                .iter_mut()
                .for_each(|value| rewrite(value, decision, kind)),
            Value::Object(values) => values
                .values_mut()
                .for_each(|value| rewrite(value, decision, kind)),
            _ => {}
        }
    }
    let read = READS
        .iter()
        .any(|(decision, _)| references_decision(rule, decision));
    if !read {
        return true;
    }
    let exiles_source = rule["costs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|cost| {
            cost["kind"].as_str() == Some("exileSource")
                && cost["object"]["kind"].as_str() == Some("self")
        })
        .count()
        == 1;
    if !exiles_source {
        return false;
    }
    for (decision, kind) in READS {
        rewrite(rule, decision, kind);
    }
    true
}

/// A keyword expansion (CR 702) carries its compiled rules: each one reads
/// only its own costs, like a printed activated ability. Returns whether
/// every read binding found its unique own producer.
fn bind_keyword_expansion_cost_objects(rule: &mut Value) -> bool {
    let Some(parts) = rule.get_mut("rules").and_then(Value::as_array_mut) else {
        return true;
    };
    for part in parts {
        if !resolve_exiled_source_card_reads(part) {
            return false;
        }
        for (producer, fields) in read_cost_object_bindings(part) {
            if producing_cost_count(part, producer) != 1 {
                return false;
            }
            install_cost_object_bindings(part, producer, &fields);
        }
    }
    true
}

/// The unique other spell rule of the same face whose additional costs
/// produce the sacrificed object a spell ability reads.
fn same_face_spell_producer(
    abilities: &[ParsedOracleAbility],
    index: usize,
    producer: CostObjectProducer,
) -> Option<usize> {
    let face = &abilities[index].source.face_id;
    let producers = abilities
        .iter()
        .enumerate()
        .filter(|(other, candidate)| {
            *other != index
                && &candidate.source.face_id == face
                && candidate.status == "canonical"
                && candidate.rule.as_ref().is_some_and(|rule| {
                    rule["kind"].as_str() == Some("spellAbility")
                        && producing_cost_count(rule, producer) > 0
                })
        })
        .map(|(other, candidate)| {
            (
                other,
                candidate
                    .rule
                    .as_ref()
                    .map_or(0, |rule| producing_cost_count(rule, producer)),
            )
        })
        .collect::<Vec<_>>();
    match producers.as_slice() {
        [(other, 1)] => Some(*other),
        _ => None,
    }
}

/// A rule reading `the sacrificed NOUN's power|toughness` or `the tapped
/// NOUN's power` (CR 601.2h, 602.2b) needs exactly one producing cost: its
/// own activation cost, or for a sacrifice the additional cost of the same
/// spell face (a separate spell rule). The producing cost records the value
/// when it is paid; a missing or ambiguous producer is a binding failure,
/// never a zero.
fn bind_cost_paid_objects(abilities: &mut [ParsedOracleAbility]) {
    for index in 0..abilities.len() {
        let ability = &abilities[index];
        if ability.status != "canonical" || ability.recognizer != RecognizerProvenance::Native {
            continue;
        }
        let Some(rule) = ability.rule.as_ref() else {
            continue;
        };
        if rule["kind"].as_str() == Some("keywordExpansion") {
            let bound = abilities[index]
                .rule
                .as_mut()
                .is_some_and(bind_keyword_expansion_cost_objects);
            if !bound {
                reject_binding(
                    &mut abilities[index],
                    "binding.cost-object.source",
                    "exactly one producing cost of the same expanded ability",
                    "The keyword definition reads a cost-paid object, but no unique cost of the same expanded ability produces it.",
                );
            }
            continue;
        }
        let spell = rule["kind"].as_str() == Some("spellAbility");
        let exiled_reads_resolved = abilities[index]
            .rule
            .as_mut()
            .is_some_and(resolve_exiled_source_card_reads);
        if !exiled_reads_resolved {
            reject_binding(
                &mut abilities[index],
                "binding.exiled-source-card.source",
                "exactly one cost of this ability exiling its source card",
                "The canonical rule reads the card a cost exiled, but no unique exile-this-card cost of the same ability produces it.",
            );
            continue;
        }
        let Some(rule) = abilities[index].rule.as_ref() else {
            continue;
        };
        for (producer, fields) in read_cost_object_bindings(rule) {
            let own = abilities[index]
                .rule
                .as_ref()
                .map_or(0, |rule| producing_cost_count(rule, producer));
            let target = if own > 0 {
                (own == 1).then_some(index)
            } else if spell && producer == CostObjectProducer::Sacrifice {
                same_face_spell_producer(abilities, index, producer)
            } else {
                None
            };
            let Some(target) = target else {
                reject_binding(
                    &mut abilities[index],
                    "binding.sacrificed-cost-object.source",
                    "exactly one producing cost of this ability or, for a sacrifice, of the same spell face",
                    "The canonical rule reads a cost-paid permanent, but no unique same-face cost produces it.",
                );
                break;
            };
            if let Some(rule) = abilities[target].rule.as_mut() {
                install_cost_object_bindings(rule, producer, &fields);
            }
        }
    }
}

#[cfg(test)]
#[path = "equal_amount_card_tests.rs"]
mod equal_amount_card_tests;

#[cfg(test)]
#[path = "control_copy_players_card_tests.rs"]
mod control_copy_players_card_tests;

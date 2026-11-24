use serde_json::{Value, json};

use crate::oracle::model::{OracleCardParseRequest, ParsedOracleAbility, ParserDiagnostic};

use super::automata::{
    ObjectTypeHead, lowered_candidate_domains, lowered_candidate_type_heads, type_heads_name,
    type_heads_overlap, type_line_type_heads,
};

const OVERLOAD_MODE: &str = "overload";

/// The `link` value of `cards exiled with this NOUN`: every exile of the
/// object (CR 607.2a links each exiling ability with the reader).
const FACE_LINK: &str = "face";
/// The `link` value of `the exiled card(s)`: the one exile it is paired with.
const PAIR_LINK: &str = "pair";

/// Marks every exile of permanents that does not already return them when
/// the source leaves (that duration records the link itself).
fn link_permanent_exiles(value: &mut Value) -> usize {
    match value {
        Value::Object(map) => {
            let own = usize::from(
                map.get("kind").and_then(Value::as_str) == Some("exilePermanent")
                    && !map.contains_key("until"),
            );
            if own == 1 {
                map.insert("linkToSource".to_string(), Value::Bool(true));
            }
            own + map.values_mut().map(link_permanent_exiles).sum::<usize>()
        }
        Value::Array(items) => items.iter_mut().map(link_permanent_exiles).sum(),
        _ => 0,
    }
}

fn reject(ability: &mut ParsedOracleAbility, state: &str, message: &str, expected: &str) {
    ability.status = "unsupported".to_string();
    ability.rule = None;
    ability.diagnostics.push(ParserDiagnostic {
        code: "oracle_card_composition_failure".to_string(),
        message: message.to_string(),
        severity: "unsupported".to_string(),
        automaton: Some("linked-exile-composition".to_string()),
        state: Some(state.to_string()),
        token_index: None,
        span_start: Some(0),
        span_end: Some(ability.source.text.len()),
        expected: Some(expected.to_string()),
        found: None,
        trace: Vec::new(),
        owner: None,
    });
}

/// Linked abilities (CR 607.2a): an ability that exiles and an ability of the
/// same object that refers to `the exiled card(s)` / cards `exiled with` it.
/// The reader is paired with the exile of its own ability, else with the one
/// other ability of its face that exiles permanents; only that exile records
/// the link (`linkToSource`). Two candidate abilities make the pairing
/// ambiguous and the reader fails explicitly; a reader lowered from `the
/// exiled card(s)` without any producer fails too. An exile of another kind
/// on a reader's face records no source relation and is rejected.
fn link_exiles_to_linked_readers(abilities: &mut [ParsedOracleAbility]) {
    let mentions = |rule: &Value, needle: &str| rule.to_string().contains(needle);
    let reads_linked = |rule: &Value| mentions(rule, "\"cardsExiledWithSource\"");
    let reads_face_link =
        |rule: &Value| reads_linked(rule) && mentions(rule, &format!("\"link\":\"{FACE_LINK}\""));
    let reads_pair_link =
        |rule: &Value| reads_linked(rule) && mentions(rule, &format!("\"link\":\"{PAIR_LINK}\""));
    let exiles_permanents = |rule: &Value| mentions(rule, "\"kind\":\"exilePermanent\"");
    let other_exiles = |rule: &Value| {
        mentions(rule, "\"exileTargetCards\"") || mentions(rule, "\"kind\":\"exile\"")
    };
    let faces_with_readers = abilities
        .iter()
        .filter(|ability| ability.rule.as_ref().is_some_and(reads_linked))
        .map(|ability| ability.source.face_id.clone())
        .collect::<Vec<_>>();
    for ability in abilities.iter_mut() {
        let Some(rule) = ability.rule.as_ref() else {
            continue;
        };
        if faces_with_readers.contains(&ability.source.face_id)
            && !reads_linked(rule)
            && other_exiles(rule)
        {
            reject(
                ability,
                "composition.linked-exile.relation",
                "A same-face ability reads cards exiled with this object (CR 607), but this exile records no source relation.",
                "an exile that records its source relation for the linked reader",
            );
        }
    }
    let readers = (0..abilities.len())
        .filter(|index| abilities[*index].rule.as_ref().is_some_and(reads_linked))
        .collect::<Vec<_>>();
    for reader in readers {
        // The reader's own exile is its pair.
        if abilities[reader]
            .rule
            .as_ref()
            .is_some_and(exiles_permanents)
        {
            if let Some(rule) = abilities[reader].rule.as_mut() {
                link_permanent_exiles(rule);
            }
            continue;
        }
        let face = abilities[reader].source.face_id.clone();
        let candidates = (0..abilities.len())
            .filter(|other| {
                *other != reader
                    && abilities[*other].source.face_id == face
                    && abilities[*other]
                        .rule
                        .as_ref()
                        .is_some_and(|rule| exiles_permanents(rule) && !reads_linked(rule))
            })
            .collect::<Vec<_>>();
        let pair = abilities[reader].rule.as_ref().is_some_and(reads_pair_link);
        match candidates.as_slice() {
            // `cards exiled with this` reads every exile of the object.
            producers if !producers.is_empty() && !pair => {
                for producer in producers {
                    if let Some(rule) = abilities[*producer].rule.as_mut() {
                        link_permanent_exiles(rule);
                    }
                }
            }
            [producer] => {
                if let Some(rule) = abilities[*producer].rule.as_mut() {
                    link_permanent_exiles(rule);
                }
            }
            [] if abilities[reader]
                .rule
                .as_ref()
                .is_some_and(|rule| reads_face_link(rule) || pair) =>
            {
                reject(
                    &mut abilities[reader],
                    "binding.linked-exile.producer",
                    "The ability refers to cards exiled with this object, but no ability of this face exiles permanents with it (CR 607.2a).",
                    "an ability of this face that exiles permanents with this object",
                )
            }
            [] => {}
            _ => reject(
                &mut abilities[reader],
                "binding.linked-exile.ambiguous",
                "The ability refers to cards exiled with this object, but several abilities of this face exile permanents; the linked pair is ambiguous (CR 607.2a).",
                "exactly one ability of this face that exiles permanents with this object",
            ),
        }
    }
}

/// What `another` can contrast with besides earlier target slots: the
/// source object, described by its type heads when its type line is known.
#[derive(Clone, Debug, PartialEq, Eq)]
enum OtherSource {
    /// A spell (or another nonpermanent source): never a permanent target.
    Spell,
    /// A permanent whose printed type heads are known.
    Permanent(Vec<ObjectTypeHead>),
    /// A permanent source whose type line was not supplied: it may be
    /// named by any permanent slot.
    UnknownPermanent,
}

impl OtherSource {
    /// Whether a slot naming `heads` could name this source, so `another`
    /// may contrast with it.
    fn nameable_by(&self, heads: &[ObjectTypeHead]) -> bool {
        match self {
            Self::Spell => false,
            Self::UnknownPermanent => true,
            Self::Permanent(source) => type_heads_name(source, heads),
        }
    }
}

/// Resolves the target-list child's typed `otherThanEarlierTargets` relation
/// (`another target X`, `each of two other target X`) in one declaration.
/// An earlier target slot counts when it is declared under the same mode
/// condition (a modal mode's `another` relates to its own mode's slots) and
/// its domain can overlap the later slot's (a player slot never does). With
/// such slots the relation records `distinctFrom` (compared by owning slot
/// in the engine, whatever the slot sizes) and `another` names them, not
/// the source (Wayta: `Target creature you control fights another target
/// creature` may name Wayta).
///
/// When the later slot could also name the source, `another` contrasts
/// with an earlier slot only when their type heads overlap in the shared
/// object-query hierarchy (`type_heads_overlap`): `target Dinosaur ...
/// another target creature` (a Dinosaur is a creature head, Itzquinth),
/// `target Aura ... another target enchantment`, `target Equipment ...
/// another target artifact`, and a bare earlier `permanent`, which every
/// permanent type is a kind of. After `target artifact or enchantment` or
/// `Equipment`, `other target creatures` contrasts with the source (Rhino,
/// Jaheira, Kinetic Ooze, Amy Rose). A source the later slot cannot name
/// (a Saga for `another target creature`, The Curse of Fenric; a spell)
/// leaves only the earlier slots to contrast with. Without such a slot the
/// later one keeps its source exclusion (`another target creature you
/// control`; Stiltzkin after a player slot; a mode-2 `another` when only
/// mode 2 is chosen).
fn resolve_other_target_relation(decisions: &mut [Value], source: &OtherSource) {
    for index in 0..decisions.len() {
        let later = &decisions[index];
        if later["kind"].as_str() != Some("chooseTargets")
            || later["candidates"]["otherThanEarlierTargets"].as_bool() != Some(true)
        {
            continue;
        }
        let later_domains = lowered_candidate_domains(&later["candidates"]);
        let later_heads = lowered_candidate_type_heads(&later["candidates"]);
        let source_nameable = source.nameable_by(&later_heads);
        let condition = later.get("condition").cloned();
        let earlier = decisions[..index]
            .iter()
            .filter(|earlier| earlier["kind"].as_str() == Some("chooseTargets"))
            .filter(|earlier| earlier.get("condition").cloned() == condition)
            .filter(|earlier| {
                lowered_candidate_domains(&earlier["candidates"])
                    .iter()
                    .any(|domain| later_domains.contains(domain))
                    && (!source_nameable
                        || type_heads_overlap(
                            &later_heads,
                            &lowered_candidate_type_heads(&earlier["candidates"]),
                        ))
            })
            .filter_map(|earlier| earlier["id"].as_str().map(str::to_string))
            .collect::<Vec<_>>();
        if earlier.is_empty() {
            continue;
        }
        let later = &mut decisions[index];
        later["distinctFrom"] = json!(earlier);
        if let Some(candidates) = later["candidates"].as_object_mut() {
            candidates.remove("excludeSource");
        }
    }
}

/// Resolves the relation in every declaration of the rule, nested ones
/// (reflexive triggers, granted abilities) included, then consumes the
/// marker. `get_mut`, not indexing: a rule without a declaration stays
/// without one. A spell ability's source is the spell; any other rule's
/// source is the permanent described by `source_heads` when known.
fn mark_distinct_other_targets(rule: &mut Value, source_heads: Option<&[ObjectTypeHead]>) {
    match rule {
        Value::Object(map) => {
            let source = if map.get("kind").and_then(Value::as_str) == Some("spellAbility") {
                OtherSource::Spell
            } else {
                match source_heads {
                    Some(heads) if !heads.iter().any(|head| *head == ObjectTypeHead::Permanent) => {
                        // An instant or sorcery face's non-spell rule (a
                        // granted or reflexive ability) still has a spell
                        // or card source, never a permanent target.
                        OtherSource::Spell
                    }
                    Some(heads) => OtherSource::Permanent(heads.to_vec()),
                    None => OtherSource::UnknownPermanent,
                }
            };
            if let Some(decisions) = map
                .get_mut("declaration")
                .and_then(|declaration| declaration.get_mut("decisions"))
                .and_then(Value::as_array_mut)
            {
                resolve_other_target_relation(decisions, &source);
            }
            for (key, child) in map.iter_mut() {
                if key != "declaration" {
                    mark_distinct_other_targets(child, source_heads);
                }
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| mark_distinct_other_targets(item, source_heads)),
        _ => {}
    }
}

/// Composition entry without a known source type line: a non-spell source
/// is a permanent any permanent slot may name.
#[cfg(test)]
fn compose_other_target_relation(rule: &mut Value) {
    compose_other_target_relation_for_source(rule, None);
}

fn compose_other_target_relation_for_source(
    rule: &mut Value,
    source_heads: Option<&[ObjectTypeHead]>,
) {
    mark_distinct_other_targets(rule, source_heads);
    strip_other_target_relation(rule);
}

fn strip_other_target_relation(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("otherThanEarlierTargets");
            for child in map.values_mut() {
                strip_other_target_relation(child);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_other_target_relation),
        _ => {}
    }
}

/// Composes the card's rules with each ability's source described by the
/// type line of the face it is printed on.
pub(crate) fn compose_card_rules_for_request(
    request: &OracleCardParseRequest,
    abilities: &mut [ParsedOracleAbility],
) {
    let face_type_line = |face_id: Option<&str>| match face_id {
        Some(face_id) => request
            .faces
            .iter()
            .find(|face| face.id == face_id)
            .map(|face| face.type_line.clone()),
        None => Some(request.type_line.clone()),
    };
    compose_card_rules_with_sources(abilities, &face_type_line);
}

pub(crate) fn compose_card_rules_native(abilities: &mut [ParsedOracleAbility]) {
    compose_card_rules_with_sources(abilities, &|_| None);
}

fn compose_card_rules_with_sources(
    abilities: &mut [ParsedOracleAbility],
    face_type_line: &dyn Fn(Option<&str>) -> Option<String>,
) {
    for ability in abilities.iter_mut() {
        let source_heads = face_type_line(ability.source.face_id.as_deref())
            .map(|type_line| type_line_type_heads(&type_line));
        if let Some(rule) = ability.rule.as_mut() {
            compose_other_target_relation_for_source(rule, source_heads.as_deref());
        }
    }
    link_exiles_to_linked_readers(abilities);
    let directives = abilities
        .iter()
        .enumerate()
        .filter_map(|(index, ability)| {
            let rule = ability.rule.as_ref()?;
            (rule["kind"].as_str() == Some("keywordAbility")
                && rule["ability"]["kind"].as_str() == Some("alternativeCost")
                && rule["ability"]["mode"].as_str() == Some(OVERLOAD_MODE)
                && rule["ability"]["compileDirective"]["kind"].as_str()
                    == Some("replaceTargetWithEach"))
            .then_some(index)
        })
        .collect::<Vec<_>>();

    for directive_index in directives {
        let mut composed = abilities.to_vec();
        match compile_target_to_each(&mut composed) {
            Ok(()) => {
                if let Some(rule) = composed[directive_index].rule.as_mut()
                    && let Some(ability) = rule["ability"].as_object_mut()
                {
                    ability.remove("compileDirective");
                }
                abilities.clone_from_slice(&composed);
            }
            Err(failure) => {
                // A diagnostic's recognizer is not proof that its spell body
                // was compiled. Preserve the independently recognized keyword
                // while a same-face sibling remains unsupported, whether no
                // child claimed it or a native child reports its exact limit.
                // The card itself remains unsupported in either case.
                let missing_unsupported_sibling =
                    abilities.iter().enumerate().any(|(index, ability)| {
                        index != directive_index
                            && ability.source.face_id == abilities[directive_index].source.face_id
                            && ability.status == "unsupported"
                            && ability.rule.is_none()
                    });
                if failure.state == "composition.target-rule" && missing_unsupported_sibling {
                    if let Some(rule) = abilities[directive_index].rule.as_mut()
                        && let Some(ability) = rule["ability"].as_object_mut()
                    {
                        ability.remove("compileDirective");
                    }
                    continue;
                }
                let source = &abilities[directive_index].source;
                abilities[directive_index].status = "unsupported".to_string();
                abilities[directive_index].rule = None;
                abilities[directive_index]
                    .diagnostics
                    .push(ParserDiagnostic {
                        code: "oracle_card_composition_failure".to_string(),
                        message: failure.message.clone(),
                        severity: "unsupported".to_string(),
                        automaton: Some("card-target-to-each-composition".to_string()),
                        state: Some(failure.state.to_string()),
                        token_index: None,
                        span_start: Some(0),
                        span_end: Some(source.text.len()),
                        expected: Some(failure.expected.to_string()),
                        found: failure.found,
                        trace: Vec::new(),
                        owner: None,
                    });
            }
        }
    }
}

struct CompositionFailure {
    state: &'static str,
    expected: &'static str,
    found: Option<String>,
    message: String,
}

fn compile_target_to_each(abilities: &mut [ParsedOracleAbility]) -> Result<(), CompositionFailure> {
    let overload_paid = json!({
        "kind": "wasCastWithAlternativeCost",
        "mode": OVERLOAD_MODE,
    });
    let normal_cast = json!({
        "kind": "not",
        "operand": overload_paid.clone(),
    });
    let mut transformed_rules = 0_usize;

    for ability in abilities {
        let Some(rule) = ability.rule.as_mut() else {
            continue;
        };
        if rule["kind"].as_str() != Some("spellAbility") {
            continue;
        }
        let Some(decisions) = rule["declaration"]["decisions"].as_array_mut() else {
            continue;
        };
        let mut substitutions = Vec::new();
        for decision in decisions
            .iter_mut()
            .filter(|decision| decision["kind"].as_str() == Some("chooseTargets"))
        {
            let decision_id = decision["id"].as_str().ok_or_else(|| CompositionFailure {
                state: "composition.target-decision.id",
                expected: "a stable target decision id",
                found: None,
                message: "Overload found a target decision without a stable id.".to_string(),
            })?;
            let collection = target_candidates_to_collection(&decision["candidates"])?;
            substitutions.push((decision_id.to_string(), collection));
            let previous = decision.get("condition").cloned();
            decision["condition"] = previous.map_or_else(
                || normal_cast.clone(),
                |condition| {
                    json!({
                        "kind": "and",
                        "operands": [condition, normal_cast.clone()],
                    })
                },
            );
        }
        if substitutions.is_empty() {
            continue;
        }

        let normal_effects = rule["effects"].clone();
        let mut overloaded_effects = normal_effects.clone();
        for (decision_id, collection) in &substitutions {
            if replace_target_references(&mut overloaded_effects, decision_id, collection) == 0 {
                return Err(CompositionFailure {
                    state: "composition.target-reference",
                    expected: "an effect reference to every transformed target decision",
                    found: Some(decision_id.clone()),
                    message: format!(
                        "Overload could not link target decision `{decision_id}` to a structured effect reference."
                    ),
                });
            }
        }
        rule["effects"] = json!([{
            "kind": "conditionalEffect",
            "condition": overload_paid.clone(),
            "then": overloaded_effects,
            "else": normal_effects,
        }]);
        transformed_rules += 1;
    }

    if transformed_rules == 0 {
        return Err(CompositionFailure {
            state: "composition.target-rule",
            expected: "at least one structured targeted spell rule",
            found: None,
            message: "Overload requires a structured targeted spell rule on the same card."
                .to_string(),
        });
    }
    Ok(())
}

fn target_candidates_to_collection(candidates: &Value) -> Result<Value, CompositionFailure> {
    if candidates["kind"].as_str() != Some("permanents") {
        return Err(CompositionFailure {
            state: "composition.selector-domain",
            expected: "a permanent target selector with a reusable set representation",
            found: candidates["kind"].as_str().map(ToOwned::to_owned),
            message: "This target domain does not yet have a target-to-each collection lowering."
                .to_string(),
        });
    }

    let mut collection = json!({ "kind": "eachPermanent" });
    if let Some(where_filter) = candidates.get("where").filter(|filter| !filter.is_null()) {
        collection["where"] = where_filter.clone();
    }
    if let Some(controller_scope) = candidates.get("controller") {
        match controller_scope["kind"].as_str() {
            Some("controllerOf") => collection["player"] = controller_scope.clone(),
            Some("playersExcept")
                if controller_scope["player"]["kind"].as_str() == Some("controllerOf") =>
            {
                collection["excludeSourceController"] = Value::Bool(true);
            }
            other => {
                return Err(CompositionFailure {
                    state: "composition.selector-controller",
                    expected: "controllerOf(self) or playersExcept(controllerOf(self))",
                    found: other.map(ToOwned::to_owned),
                    message:
                        "This controller relation does not yet have an exact target-to-each lowering."
                            .to_string(),
                });
            }
        }
    }
    Ok(collection)
}

fn replace_target_references(value: &mut Value, decision_id: &str, replacement: &Value) -> usize {
    if matches!(
        value["kind"].as_str(),
        Some("chosenTarget" | "chosenTargets")
    ) && value["id"].as_str() == Some(decision_id)
    {
        *value = replacement.clone();
        return 1;
    }

    match value {
        Value::Array(values) => values
            .iter_mut()
            .map(|value| replace_target_references(value, decision_id, replacement))
            .sum(),
        Value::Object(values) => values
            .values_mut()
            .map(|value| replace_target_references(value, decision_id, replacement))
            .sum(),
        _ => 0,
    }
}

pub(crate) fn compose_card_rules(abilities: &mut [ParsedOracleAbility]) {
    compose_card_rules_native(abilities);
}

#[cfg(test)]
#[path = "card_composition_tests.rs"]
mod card_composition_tests;

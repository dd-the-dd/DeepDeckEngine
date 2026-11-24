//! Compiles a template keyword (CR 702) through the ordinary ability parser.
//!
//! The keyword line is read by `automata/keyword_template.rs`, which returns
//! the keyword's rules definition instantiated with the printed parameter.
//! Each instantiated ability is then compiled by the caller's ordinary
//! per-ability pipeline in the same face context, so the keyword inherits every
//! grammar and runtime capability of printed text. The result is one
//! `keywordExpansion` rule carrying the keyword's provenance and the compiled
//! rules; `keyword_expansion_rules` flattens it into the playable rules.
//!
//! A part that the ordinary parser does not compile fails the whole keyword
//! explicitly (`keyword.template.expansion`), with the part's own failure kept
//! as candidate evidence. Nothing is approximated or silently dropped.
use super::super::*;
use crate::oracle::model::RecognizerProvenance;
use crate::oracle::syntax::AbilityInput;

/// The IR kind of a compiled template keyword, before flattening.
pub(in crate::oracle::canonical) const KEYWORD_EXPANSION_KIND: &str = "keywordExpansion";
/// The field naming the keyword and its parameters on every flattened rule.
pub(in crate::oracle::canonical) const KEYWORD_PROVENANCE_FIELD: &str = "keywordProvenance";

/// Fields owned by the expansion itself. Every other top-level field of the
/// expansion (placement such as `minimumClassLevel` or `roomDoorIndex`, added
/// by the pipeline after parsing) applies to each expanded rule.
const EXPANSION_FIELDS: &[&str] = &[
    "kind",
    "source",
    "keyword",
    "parameters",
    "definition",
    "namedAction",
    "rules",
];

fn parameters(invocation: &KeywordTemplateInvocation) -> Value {
    let mut parameters = serde_json::Map::new();
    if let Some(count) = invocation.arguments.count {
        parameters.insert("N".to_string(), Value::from(count));
    }
    if let Some(cost) = &invocation.arguments.cost {
        parameters.insert("cost".to_string(), Value::from(cost.as_str()));
    }
    Value::Object(parameters)
}

fn definition(invocation: &KeywordTemplateInvocation) -> Value {
    let document = keyword_template_document();
    json!({
        "rulesSection": invocation.template.rules_section,
        "rulesClause": invocation.template.rules_clause,
        "templateVersion": invocation.template.template_version,
        "catalogVersion": document.catalog_version,
        "rulesEffectiveDate": document.rules_effective_date,
    })
}

fn expansion_failure(
    input: &AbilityInput<'_>,
    invocation: &KeywordTemplateInvocation,
    part: usize,
    reason: String,
) -> MachineFailure {
    let mut cursor = OracleTokenCursor::new("keyword-template", input.tokens);
    cursor.enter("keyword.template.expansion");
    let (span_start, span_end) = match (input.tokens.first(), input.tokens.last()) {
        (Some(first), Some(last)) => (first.span_start, last.span_end),
        _ => (0, 0),
    };
    cursor.fail_with_span(
        &format!(
            "the {} definition (CR {}) compiled by the shared ability parser; part {} \"{}\" {}",
            invocation.template.canonical_id,
            invocation.template.rules_section,
            part + 1,
            invocation.abilities[part],
            reason
        ),
        0,
        span_start,
        span_end,
        invocation.abilities[part].clone(),
    )
}

fn intrinsic_failure(
    input: &AbilityInput<'_>,
    state: &'static str,
    expected: &str,
) -> MachineFailure {
    let mut cursor = OracleTokenCursor::new("keyword-template", input.tokens);
    cursor.enter(state);
    let (span_start, span_end) = match (input.tokens.first(), input.tokens.last()) {
        (Some(first), Some(last)) => (first.span_start, last.span_end),
        _ => (0, 0),
    };
    let found = input
        .tokens
        .first()
        .map(|token| token.raw.clone())
        .unwrap_or_default();
    cursor.fail_with_span(expected, 0, span_start, span_end, found)
}

/// The typed rule of an intrinsic keyword definition (CR 702.1). A keyword
/// ability intrinsic is the runtime keyword itself (its state check or
/// entry choice belongs to the engine rule that reads the keyword); an
/// activated intrinsic reads its quoted costs with the shared activation-cost
/// sequence and composes them with an ordinary zone-change effect.
fn lower_intrinsic_definition(
    input: &AbilityInput<'_>,
    invocation: &KeywordTemplateInvocation,
    intrinsic: KeywordIntrinsic,
) -> Result<Value, MachineFailure> {
    // CR 702.131a: ascend on an instant or sorcery is a spell ability, one
    // instruction of the spell's program; CR 702.131b: on a permanent it is
    // the static state check the runtime keyword performs.
    if intrinsic == KeywordIntrinsic::Ascend && spell_face(input.face_type_line) {
        return Ok(json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [{ "kind": "ascend", "player": controller() }],
        }));
    }
    if intrinsic == KeywordIntrinsic::Ascend
        && !crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type(
            input.face_type_line,
        )
    {
        return Err(intrinsic_failure(
            input,
            "keyword.template.intrinsic.face",
            "a permanent, instant or sorcery face for ascend (CR 702.131a-b)",
        ));
    }
    if let Some(kind) = intrinsic.keyword_kind() {
        return Ok(json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": { "kind": kind },
        }));
    }
    match intrinsic {
        KeywordIntrinsic::Ninjutsu => lower_ninjutsu(input, invocation),
        KeywordIntrinsic::Ascend | KeywordIntrinsic::Storied | KeywordIntrinsic::ReadAhead => {
            Err(intrinsic_failure(
                input,
                "keyword.template.intrinsic",
                "a keyword-ability intrinsic with a runtime keyword kind",
            ))
        }
    }
}

/// CR 702.49a: `[Cost], Reveal this card from your hand, Return an
/// unblocked attacking creature you control to its owner's hand: Put this
/// card onto the battlefield from your hand tapped and attacking.` The costs
/// are the row's quoted CR costs read by the shared activation-cost sequence;
/// the effect moves the ability's source from hand onto the battlefield
/// tapped, attacking what the returned creature was attacking (CR 702.49c).
fn lower_ninjutsu(
    input: &AbilityInput<'_>,
    invocation: &KeywordTemplateInvocation,
) -> Result<Value, MachineFailure> {
    let text = invocation.activation_costs.as_deref().ok_or_else(|| {
        intrinsic_failure(
            input,
            "keyword.template.intrinsic.costs",
            "the intrinsic definition's quoted activation costs",
        )
    })?;
    let tokens = crate::oracle::syntax::recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("keyword-template-intrinsic", &tokens);
    cursor.enter("keyword.template.intrinsic.costs");
    let costs = recognize_activation_cost_sequence(&mut cursor)?;
    cursor.enter("keyword.template.intrinsic.costs-complete");
    cursor.finish()?;
    let lowered = costs
        .costs
        .iter()
        .flat_map(lower_local_activation_costs)
        .collect::<Vec<_>>();
    cursor.enter("keyword.template.intrinsic.returned-attacker");
    let returned = lowered
        .iter()
        .filter(|cost| cost["kind"] == "returnPermanentToOwnersHand")
        .filter_map(|cost| {
            (cost["permanent"]["kind"] == "chosenObject")
                .then(|| cost["permanent"]["id"].as_str())
                .flatten()
        })
        .collect::<Vec<_>>();
    let [returned] = returned.as_slice() else {
        return Err(cursor.fail("exactly one chosen attacker returned to its owner's hand"));
    };
    let returned = (*returned).to_string();
    let decisions = lower_activation_cost_decisions(&costs);
    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": lowered,
        "declaration": {
            "kind": "castingDeclaration",
            "decisions": decisions,
        },
        "effects": [{
            "kind": "moveAbilitySourceToBattlefield",
            "from": "hand",
            "tapped": true,
            "attacking": {
                "kind": "sameDefenderAs",
                "object": { "kind": "chosenObject", "id": returned },
            },
        }],
    });
    // CR 702.49a: the ability functions only while the card is in a hand.
    rule["activationZone"] = json!("hand");
    Ok(rule)
}

/// Expands a template keyword line, or `None` when no template keyword owns
/// it. `compile` runs the ordinary per-ability parser on one ability text in
/// the face context of `input`.
pub(crate) fn expand_keyword_template_line(
    input: &AbilityInput<'_>,
    compile: &mut dyn FnMut(&str) -> CanonicalParseOutcome,
) -> Option<CanonicalParseOutcome> {
    let invocation = match recognize_keyword_template_line(
        input.tokens,
        &input.source.text,
        input.face_type_line,
    )? {
        Ok(invocation) => invocation,
        Err(failure) => {
            return Some(CanonicalParseOutcome::Failed {
                failure,
                recognizer: RecognizerProvenance::Native,
            });
        }
    };
    let mut rules = Vec::with_capacity(invocation.abilities.len().max(1));
    if let Some(intrinsic) = invocation.template.intrinsic {
        match lower_intrinsic_definition(input, &invocation, intrinsic) {
            Ok(rule) => rules.push(rule),
            Err(failure) => {
                return Some(CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                });
            }
        }
    }
    for (part, ability) in invocation.abilities.iter().enumerate() {
        let failed = |reason: String, candidate: Option<MachineFailure>| {
            let failure = expansion_failure(input, &invocation, part, reason);
            Some(match candidate {
                Some(candidate_failure) => CanonicalParseOutcome::FailedWithCandidateEvidence {
                    failure,
                    candidate_failure,
                    recognizer: RecognizerProvenance::Native,
                },
                None => CanonicalParseOutcome::Failed {
                    failure,
                    recognizer: RecognizerProvenance::Native,
                },
            })
        };
        // A definition never starts with a keyword line (checked for every
        // row), so this recursion is one level deep.
        match compile(ability) {
            CanonicalParseOutcome::Parsed { draft, .. } => rules.push(draft.rule),
            CanonicalParseOutcome::Failed { failure, .. }
            | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => {
                let reason = format!("stopped in {}", failure.state);
                return failed(reason, Some(failure));
            }
            CanonicalParseOutcome::Unsupported => {
                return failed("was claimed by no ability reader".to_string(), None);
            }
        }
    }
    let mut rule = json!({
        "kind": KEYWORD_EXPANSION_KIND,
        "source": self_ref(),
        "keyword": invocation.template.canonical_id,
        "parameters": parameters(&invocation),
        "definition": definition(&invocation),
        "rules": rules,
    });
    if let Some(action) = &invocation.template.named_action {
        rule["namedAction"] = json!({
            "lemma": action.lemma,
            "performedBy": action.performed_by.as_str(),
        });
    }
    let expansion = format!(
        "Instantiate the {} definition (CR {}, template v{}) with its printed parameter{}",
        invocation.template.canonical_id,
        invocation.template.rules_section,
        invocation.template.template_version,
        if invocation.template.intrinsic.is_some() {
            " and lower its intrinsic rule to typed primitives"
        } else {
            ""
        }
    );
    let reminder = if invocation.reminder {
        "Validate the printed reminder against the keyword's reminder renderings"
    } else {
        "No printed reminder"
    };
    Some(CanonicalParseOutcome::Parsed {
        draft: draft(
            rule,
            &[
                "Recognize the template keyword lexeme from the shared token stream",
                "Read its parameter with the shared count or mana-cost child",
                reminder,
                &expansion,
                "Compile every instantiated ability with the ordinary ability parser",
                &invocation.trace,
            ],
        ),
        recognizer: RecognizerProvenance::Native,
    })
}

/// The playable rules of one parsed ability rule: the rule itself, or every
/// rule of a keyword expansion with the keyword's provenance attached.
pub(crate) fn keyword_expansion_rules(rule: &Value) -> Vec<Value> {
    if rule["kind"].as_str() != Some(KEYWORD_EXPANSION_KIND) {
        return vec![rule.clone()];
    }
    let Some(parts) = rule["rules"].as_array() else {
        // A malformed expansion stays unexecutable instead of vanishing.
        return vec![rule.clone()];
    };
    let placement = rule
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(key, _)| !EXPANSION_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    parts
        .iter()
        .enumerate()
        .map(|(part, inner)| {
            let mut inner = inner.clone();
            if let Some(fields) = inner.as_object_mut() {
                for (key, value) in &placement {
                    fields.insert(key.clone(), value.clone());
                }
                let mut provenance = json!({
                    "keyword": rule["keyword"],
                    "parameters": rule["parameters"],
                    "rulesSection": rule["definition"]["rulesSection"],
                    "templateVersion": rule["definition"]["templateVersion"],
                    "part": part,
                });
                if let Some(action) = rule.get("namedAction") {
                    provenance["namedAction"] = action.clone();
                }
                fields.insert(KEYWORD_PROVENANCE_FIELD.to_string(), provenance);
            }
            inner
        })
        .collect()
}

#[cfg(test)]
#[path = "keyword_template_expansion_tests.rs"]
mod tests;

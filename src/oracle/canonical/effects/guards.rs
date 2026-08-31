use super::super::*;

pub(in crate::oracle::canonical) fn permanent_target_candidates(
    description: &str,
    face_name: &str,
) -> Option<Value> {
    let mut description = description.trim().trim_end_matches('.').trim();
    let mut controller_filter = None;
    let mut ownership_filter = None;
    let embedded_controller_re =
        Regex::new(r"(?i)^(.+?) (you don't control|an opponent controls)(.*)$")
            .expect("embedded target-controller qualifier regex compiles");
    let reconstructed_description;
    if let Some(captures) = embedded_controller_re.captures(description) {
        reconstructed_description = format!("{}{}", &captures[1], &captures[3]);
        description = reconstructed_description.trim();
        controller_filter = Some(json!({
            "kind": "opponentsOf",
            "player": controller(),
        }));
    } else if let Some(captures) = Regex::new(r"(?i)^(.+?) you control(.*)$")
        .expect("embedded controller qualifier regex compiles")
        .captures(description)
    {
        reconstructed_description = format!("{}{}", &captures[1], &captures[2]);
        description = reconstructed_description.trim();
        controller_filter = Some(controller());
    }
    for (suffix, controller) in [
        (" you control", controller()),
        (
            " an opponent controls",
            json!({ "kind": "opponentsOf", "player": controller() }),
        ),
    ] {
        if let Some(rest) = description.strip_suffix(suffix) {
            description = rest.trim();
            controller_filter = Some(controller);
            break;
        }
    }
    if let Some(rest) = description.strip_suffix(" you own") {
        description = rest.trim();
        ownership_filter = Some("owned");
    }
    let exclude_source = description
        .strip_prefix("another ")
        .or_else(|| description.strip_prefix("other "))
        .is_some();
    if exclude_source {
        description = description
            .strip_prefix("another ")
            .or_else(|| description.strip_prefix("other "))
            .unwrap_or(description)
            .trim();
    }
    let mut candidates = json!({
        "kind": "permanents",
        "where": parse_permanent_criteria(description, face_name)?,
    });
    if let Some(controller_filter) = controller_filter {
        candidates["controller"] = controller_filter;
    }
    if let Some(ownership_filter) = ownership_filter {
        candidates["ownership"] = Value::String(ownership_filter.to_string());
    }
    if exclude_source {
        candidates["excludeSource"] = Value::Bool(true);
    }
    Some(candidates)
}

pub(in crate::oracle::canonical) fn parse_mana_value_guarded_destroy_instruction(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let guarded_destroy_re = Regex::new(&format!(
        r"(?i)^Destroy (target|that) (.+?) if it has mana value ({}) or less\.?$",
        numeric_expression_pattern(),
    ))
    .expect("mana-value guarded destroy instruction regex compiles");
    let captures = guarded_destroy_re.captures(instruction.trim())?;
    let reference_kind = captures.get(1)?.as_str();
    let description = captures.get(2)?.as_str();
    let maximum = parse_numeric_expression_text(captures.get(3)?.as_str())?;
    let criteria = parse_permanent_criteria(description, face_name)?;
    let decisions = if reference_kind.eq_ignore_ascii_case("target") {
        vec![target_decision(
            "targetPermanent",
            json!({
                "kind": "permanents",
                "where": criteria,
            }),
            1,
            1,
        )]
    } else {
        Vec::new()
    };
    Some((
        vec![json!({
            "kind": "conditionalEffect",
            "condition": compare(
                "<=",
                json!({
                    "kind": "manaValueOf",
                    "object": chosen_target("targetPermanent"),
                }),
                maximum,
            ),
            "then": [{
                "kind": "destroyPermanent",
                "permanent": chosen_target("targetPermanent"),
            }],
            "else": [],
        })],
        decisions,
    ))
}

pub(in crate::oracle::canonical) fn parse_conditional_effect_amendment(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let instruction = strip_short_oracle_label(instruction);
    let clause_re = Regex::new(r"(?i)^(.+?) instead if (.+?)\.?$")
        .expect("conditional effect-amendment clause regex compiles");
    let captures = clause_re.captures(instruction.trim())?;
    let alternate_instruction = format!("{}.", captures.get(1)?.as_str().trim_end_matches('.'));
    let condition = parse_condition_text(captures.get(2)?.as_str())?;
    let inherited_recipient_damage_re = Regex::new(&format!(
        r"(?i)^(.+?) deals ({}) damage\.$",
        count_word_pattern(),
    ))
    .expect("conditional replacement damage regex compiles");
    let parsed_damage = inherited_recipient_damage_re
        .captures(&alternate_instruction)
        .filter(|captures| source_reference_matches(&captures[1], face_name))
        .and_then(|captures| {
            Some((
                vec![json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(parse_number_word(&captures[2])?),
                    "recipient": chosen_target("targetPermanent"),
                })],
                Vec::new(),
            ))
        });
    let (effects, decisions) = parsed_damage.or_else(|| {
        parse_mana_value_guarded_destroy_instruction(&alternate_instruction, face_name)
    })?;
    Some((
        vec![json!({
            "kind": "conditionalEffect",
            "condition": condition,
            "then": effects,
            "else": [],
        })],
        decisions,
    ))
}

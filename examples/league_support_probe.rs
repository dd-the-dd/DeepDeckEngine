//! League support probe: evaluates each card through the public
//! `POST /oracle/rules` route (same measure as `compact_oracle_rules`).
//! Usage: league_support_probe <input.jsonl> <output.jsonl>
//!
//! Each unexecutable rule also reports its failing support branches
//! (`unexecutableBranches`): the rule part (event, condition, declaration,
//! cost, static modifier or effect) whose isolation keeps the rule
//! unexecutable while every other part is replaced by a neutral supported
//! part, recursing into nested effect lists. The diagnosis only calls the
//! public `rule_is_executable`; it never widens support.
use mtg_engine::engine::rule_is_executable;
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};

#[derive(Deserialize)]
struct Row {
    oracle_id: String,
    name: String,
    request: Value,
}

/// A supported trigger event that observes only the source.
fn neutral_event() -> Value {
    json!({"kind": "enterBattlefield", "object": {"kind": "self"}})
}

/// A supported effect with no reference other than the source's controller.
fn neutral_effect() -> Value {
    json!({
        "kind": "gainLife",
        "amount": {"kind": "integer", "value": 1},
        "player": {"kind": "controllerOf", "object": {"kind": "self"}},
    })
}

/// A supported activation cost.
fn neutral_cost() -> Value {
    json!({"kind": "tap", "object": {"kind": "self"}})
}

fn with_effects(rule: &Value, effects: Vec<Value>) -> Value {
    let mut probe = rule.clone();
    probe["effects"] = Value::Array(effects);
    probe
}

/// Effect lists a control effect nests.
const NESTED_EFFECT_FIELDS: [&str; 5] =
    ["effects", "then", "else", "declineEffects", "onPerformed"];

/// `effects` with the effect at `index` kept and every other one neutral.
fn isolate(effects: &[Value], index: usize, kept: &Value) -> Vec<Value> {
    (0..effects.len())
        .map(|position| {
            if position == index {
                kept.clone()
            } else {
                neutral_effect()
            }
        })
        .collect()
}

/// Diagnoses one effect list of a rule; `wrap` rebuilds the whole rule from
/// a replacement list at this position. A failure inside a nested list is
/// reported at its deepest failing position.
fn diagnose_effect_list(
    effects: &[Value],
    path: &str,
    wrap: &dyn Fn(Vec<Value>) -> Value,
    branches: &mut Vec<Value>,
) {
    for (index, effect) in effects.iter().enumerate() {
        if rule_is_executable(&wrap(isolate(effects, index, effect))) {
            continue;
        }
        let effect_path = format!("{path}[{index}]");
        let mut nested_failure = false;
        for field in NESTED_EFFECT_FIELDS {
            let Some(nested) = effect[field].as_array().filter(|nested| !nested.is_empty()) else {
                continue;
            };
            let with_nested = |replacement: Vec<Value>| {
                let mut parent = effect.clone();
                parent[field] = Value::Array(replacement);
                wrap(isolate(effects, index, &parent))
            };
            if rule_is_executable(&with_nested(vec![neutral_effect()])) {
                nested_failure = true;
                diagnose_effect_list(
                    nested,
                    &format!("{effect_path}.{field}"),
                    &with_nested,
                    branches,
                );
            }
        }
        if !nested_failure {
            branches.push(json!({
                "part": "effect",
                "path": effect_path,
                "kind": effect["kind"],
                "node": effect,
            }));
        }
    }
}

/// The failing support branches of an unexecutable rule.
fn diagnose_rule(rule: &Value) -> Vec<Value> {
    let mut branches = Vec::new();
    if rule["kind"] == "staticAbility" {
        for (index, modifier) in rule["modifiers"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let mut probe = rule.clone();
            probe["modifiers"] = json!([modifier]);
            if !rule_is_executable(&probe) {
                branches.push(json!({
                    "part": "modifier",
                    "path": format!("modifiers[{index}]"),
                    "kind": modifier["kind"],
                    "node": modifier,
                }));
            }
        }
        return branches;
    }
    // The rule frame with neutral effects and no declaration or condition.
    let mut frame = with_effects(rule, vec![neutral_effect()]);
    if let Some(fields) = frame.as_object_mut() {
        fields.remove("declaration");
        fields.remove("condition");
    }
    if rule.get("event").is_some() && !rule_is_executable(&frame) {
        branches
            .push(json!({"part": "event", "kind": rule["event"]["kind"], "node": rule["event"]}));
    }
    let mut neutral_frame = frame.clone();
    if rule.get("event").is_some() {
        neutral_frame["event"] = neutral_event();
    }
    if rule["kind"] == "activatedAbility" {
        neutral_frame["costs"] = json!([neutral_cost()]);
        let mut with_costs = neutral_frame.clone();
        with_costs["costs"] = rule["costs"].clone();
        if !rule_is_executable(&with_costs) {
            branches.push(json!({"part": "costs", "node": rule["costs"]}));
        }
    }
    if !rule_is_executable(&neutral_frame) {
        branches.push(json!({"part": "frame", "kind": rule["kind"]}));
        return branches;
    }
    for part in ["condition", "declaration"] {
        if let Some(value) = rule.get(part) {
            let mut probe = neutral_frame.clone();
            probe[part] = value.clone();
            if !rule_is_executable(&probe) {
                branches.push(json!({"part": part, "kind": value["kind"], "node": value}));
            }
        }
    }
    let mut effect_frame = rule.clone();
    if rule.get("event").is_some() {
        effect_frame["event"] = neutral_event();
    }
    if let Some(fields) = effect_frame.as_object_mut() {
        fields.remove("condition");
    }
    if rule["kind"] == "activatedAbility" {
        effect_frame["costs"] = json!([neutral_cost()]);
    }
    let effects = rule["effects"].as_array().cloned().unwrap_or_default();
    diagnose_effect_list(
        &effects,
        "effects",
        &|replacement: Vec<Value>| with_effects(&effect_frame, replacement),
        &mut branches,
    );
    branches
}

fn evaluate(row: Row) -> Value {
    let (oracle_id, name) = (row.oracle_id.clone(), row.name.clone());
    let body = row.request.to_string();
    let r = std::panic::catch_unwind(move || {
        mtg_engine::http::route_json("POST", "/oracle/rules", &body)
    });
    let v = match r {
        Ok(response) if response.status == 200 => {
            serde_json::from_str::<Value>(&response.body).unwrap_or(Value::Null)
        }
        _ => {
            return json!({"oracle_id":oracle_id,"name":name,"supported":false,"state":"panic","failures":[]});
        }
    };
    let abilities = v["abilities"].as_array().cloned().unwrap_or_default();
    let status = v["status"].as_str().unwrap_or("").to_string();
    let unexec = v["unexecutableRuleCount"].as_u64().unwrap_or(0);
    let all_canonical = abilities.iter().all(|a| a["status"] == "canonical");
    let canonical = status == "canonical" && all_canonical;
    let executable = v["engineStatus"] == "executable" && unexec == 0;
    let supported = canonical && executable;
    let mut failures = Vec::new();
    for a in &abilities {
        if a["status"] != "canonical" {
            let d = a["diagnostics"]
                .as_array()
                .and_then(|d| d.first())
                .cloned()
                .unwrap_or(Value::Null);
            failures.push(json!({"status":a["status"],"text":a["source"]["text"],
                "code":d["code"],"state":d["state"],"owner":d["owner"],
                "automaton":d["automaton"],"message":d["message"]}));
        }
    }
    let unexec_kinds: Vec<Value> = v["ruleCoverage"]
        .as_array()
        .map(|rules| {
            rules
                .iter()
                .filter(|r| r["executable"] == false)
                .map(|r| r["kind"].clone())
                .collect()
        })
        .unwrap_or_default();
    // The failing support branches of each unexecutable rule, so a cohort is
    // clustered by its engine gap rather than by its ability kind.
    let unexec_branches: Vec<Value> = v["ruleCoverage"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|coverage| coverage["executable"] == false)
        .filter_map(|coverage| {
            let index = usize::try_from(coverage["index"].as_u64()?).ok()?;
            let rule = v["candidateRules"].as_array()?.get(index)?;
            Some(json!({
                "index": index,
                "kind": coverage["kind"],
                "branches": diagnose_rule(rule),
            }))
        })
        .collect();
    let state = if supported {
        "supported"
    } else if !canonical {
        "non_canonical"
    } else {
        "unexecutable"
    };
    // Stable digest of the emitted IR, to count cards whose IR changed.
    let ir_digest = {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        v["candidateRules"].to_string().hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    };
    json!({"oracle_id":oracle_id,"name":name,"abilities":abilities.len(),"canonical":canonical,"irDigest":ir_digest,
      "executable":executable,"supported":supported,"state":state,"parseStatus":status,
      "engineStatus":v["engineStatus"],"unexecutableRuleCount":unexec,
      "unexecutableKinds":unexec_kinds,"unexecutableBranches":unexec_branches,"failures":failures})
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = std::fs::File::open(&args[1]).unwrap();
    let rows: Vec<Row> = std::io::BufReader::new(input)
        .lines()
        .map(|l| l.unwrap())
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(&l).unwrap())
        .collect();
    let total = rows.len();
    let queue = Arc::new(Mutex::new(rows.into_iter().enumerate().collect::<Vec<_>>()));
    let out = Arc::new(Mutex::new(Vec::<(usize, Value)>::new()));
    std::panic::set_hook(Box::new(|_| {}));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let (q, o) = (queue.clone(), out.clone());
            std::thread::Builder::new()
                .stack_size(256 << 20)
                .spawn(move || {
                    loop {
                        let item = q.lock().unwrap().pop();
                        let Some((i, row)) = item else { break };
                        let v = evaluate(row);
                        let mut g = o.lock().unwrap();
                        g.push((i, v));
                        if g.len() % 500 == 0 {
                            eprintln!("{}/{}", g.len(), total);
                        }
                    }
                })
                .unwrap()
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let mut res = out.lock().unwrap().clone();
    res.sort_by_key(|(i, _)| *i);
    let mut w = std::io::BufWriter::new(std::fs::File::create(&args[2]).unwrap());
    let supported = res.iter().filter(|(_, v)| v["supported"] == true).count();
    for (_, v) in res {
        writeln!(w, "{}", v).unwrap();
    }
    eprintln!("supported {supported}/{total}");
}

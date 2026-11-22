//! League support probe: evaluates each card through the public
//! `POST /oracle/rules` route (same measure as `compact_oracle_rules`).
//! Usage: league_support_probe <input.jsonl> <output.jsonl>
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
                "code":d["code"],"state":d["state"],"automaton":d["automaton"],"message":d["message"]}));
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
      "unexecutableKinds":unexec_kinds,"failures":failures})
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

use super::mana::{
    mana_cost_generic_amount, mana_cost_mana_amount, parse_mana_requirement, parse_mana_symbols,
    reduce_generic_mana_cost, reduce_mana_cost_by_mana_cost,
};

#[test]
fn parses_normalized_mana_symbols() {
    assert_eq!(
        parse_mana_symbols("{2}{w}{G/U}{B/P}{S}{X}"),
        ["2", "W", "G/U", "B/P", "S", "X"]
    );
}

#[test]
fn validates_supported_requirement_families() {
    let requirement = parse_mana_requirement("{2}{W}{G/U}{B/P}{S}{X}")
        .expect("normalized requirement should be supported");
    assert_eq!(requirement.generic, 2);
    assert_eq!(requirement.colored.get("W"), Some(&1));
    assert_eq!(requirement.hybrid, [vec!["G".to_string(), "U".to_string()]]);
    assert_eq!(requirement.phyrexian.get("B"), Some(&1));
    assert_eq!(requirement.snow, 1);
    assert!(parse_mana_requirement("{Q}").is_none());
}

#[test]
fn computes_and_reduces_costs_without_runtime_state() {
    assert_eq!(mana_cost_mana_amount("{2}{W}{G/U}{X}"), 5);
    assert_eq!(mana_cost_generic_amount("{2}{W}{3}"), 5);
    assert_eq!(reduce_generic_mana_cost("{6}{R}", 2), "{4}{R}");
    assert_eq!(reduce_mana_cost_by_mana_cost("{6}{R}", "{2}{R}"), "{4}");
    assert_eq!(
        reduce_mana_cost_by_mana_cost("{C}{W}{U}{B}{R}{G}", "{R}{W}{B}"),
        "{U}{G}{C}"
    );
    assert_eq!(reduce_mana_cost_by_mana_cost("{4}{G/U}", "{1}{G/U}"), "{3}");
    assert_eq!(reduce_mana_cost_by_mana_cost("{X}{U}{U}", "{U}"), "{U}");
}

#[test]
fn activation_mana_x_requires_its_own_bounded_producer() {
    use super::mana::bind_mana_cost_x;
    assert_eq!(
        bind_mana_cost_x("{X}{X}{G}", Some(2)),
        Some("{2}{2}{G}".into())
    );
    assert_eq!(bind_mana_cost_x("{X}", Some(0)), Some("{0}".into()));
    assert_eq!(bind_mana_cost_x("{G}", None), Some("{G}".into()));
    assert_eq!(bind_mana_cost_x("{X}", None), None);
    assert_eq!(bind_mana_cost_x("{X}", Some(-1)), None);
    assert_eq!(bind_mana_cost_x("{X}", Some(i64::MAX)), None);
}

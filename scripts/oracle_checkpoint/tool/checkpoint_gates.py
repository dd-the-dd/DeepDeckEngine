"""Closed adapters count real results. Wrapper passed flags are necessary, never sufficient."""
import re
from checkpoint_core import count, keys, require


def expected_counts(actual, expected):
    require(isinstance(expected, dict) and bool(expected), 'gate expected counts missing')
    for name, value in expected.items():
        require(name in actual, 'unknown expected gate metric: ' + name)
        count(value, name)
        require(actual[name] == value, f'gate {name}: expected {value}, actual {actual[name]}')


def validate_gate(evidence, gate, ledger=None):
    keys(gate, ['id','format','evidence','expected'], ['wrapperReport','artifactId','mode'])
    fmt = gate['format']
    if 'wrapperReport' in gate:
        report = evidence.json(gate['wrapperReport'])
        require(report.get('passed') is True, 'wrapper did not pass')
        if 'exit_code' in report:
            require(type(report['exit_code']) is int and report['exit_code'] == 0, 'wrapper exit code is not zero')
        require(report.get('error') is None, 'wrapper reports an error')
    if fmt == 'rust-test-log':
        text = evidence.text(gate['evidence'],log=True)
        results = re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed;', text, re.M)
        require(bool(results), 'no Rust test-result lines')
        require(all(status == 'ok' and int(failed) == 0 for status, _, failed in results), 'Rust harness failed')
        passed = sum(int(value) for _, value, _ in results)
        count(passed, 'Rust executed tests', positive=True)
        named = re.findall(r'^test .+ \.\.\. ok\s*$', text, re.M)
        if named:
            # Concurrent harness output can interleave a named result with stderr.
            # The harness summaries remain authoritative executed-test counts.
            require(len(named) <= passed, 'Rust named rows exceed executed summaries')
        actual = {'passed':passed,'harnesses':len(results)}
    elif fmt in ('parser-log','contract-log','runtime-log'):
        rows = evidence.jsonl(gate['evidence'], mixed=True)
        require(bool(rows), 'empty DATA gate')
        require(not any(row.get('type') == 'fixtureError' for row in rows), 'fixtureError in DATA gate')
        if fmt == 'runtime-log':
            require(all(row.get('schemaVersion') == 'deepdeck-runtime-fixture-result/v1' for row in rows), 'unexpected runtime result schema')
            identities = [row.get('fixtureId') for row in rows]
            require(len(set(identities)) == len(rows) and all(isinstance(i,str) and i for i in identities), 'duplicate/missing runtime identities')
            assertions = 0
            for row in rows:
                require(row.get('status') == 'passed' and not row.get('error'), 'runtime scenario failed')
                values = row.get('assertions')
                require(isinstance(values,list) and bool(values), 'runtime scenario executed zero assertions')
                require(all(isinstance(item,dict) and item.get('passed') is True for item in values), 'runtime assertion failed')
                assertions += len(values)
            actual = {'passed':len(rows),'assertions':assertions}
        else:
            kind = 'caseResult' if fmt == 'parser-log' else 'fixtureResult'
            cases = [row for row in rows if row.get('type') == kind]
            summaries = [row for row in rows if row.get('type') == 'summary']
            require(len(summaries) == 1 and len(cases) > 0 and len(rows) == len(cases)+1, 'DATA gate missing/extra rows or zero cases')
            require(all(row.get('status') == 'passed' for row in cases), 'DATA case failed')
            ids = [(row.get('fixturePath'),row.get('caseId',row.get('fixtureId'))) for row in cases]
            require(len(set(ids)) == len(ids), 'duplicate DATA case results')
            summary = summaries[0]
            total_key = 'caseCount' if fmt == 'parser-log' else 'fixtureCount'
            require(type(summary.get(total_key)) is int and summary[total_key] == len(cases), 'summary total mismatch')
            require(summary.get('passed') == len(cases) and type(summary.get('passed')) is int
                    and summary.get('failed') == 0 and type(summary.get('failed')) is int
                    and summary.get('invalidFixtures') == 0 and type(summary.get('invalidFixtures')) is int
                    and summary.get('status') == 'passed', 'summary failed/invalid counts')
            if fmt == 'parser-log':
                require(summary.get('parserMode') == gate.get('mode'), 'wrong parser mode')
            else:
                require(all(type(row.get('actualExecutable')) is bool
                    and row.get('actualExecutable') is row.get('expectedExecutable') for row in cases), 'contract executable mismatch')
            actual = {'passed':len(cases)}
    elif fmt == 'strict-audit':
        report = evidence.json(gate['evidence'])
        require(report.get('complete') is True and report.get('failure_stage') is None, 'incomplete strict audit')
        cards = report.get('cards');require(isinstance(cards,list) and bool(cards), 'zero strict cards')
        require(len({c['oracle_id'] for c in cards}) == len(cards), 'duplicate strict Oracle IDs')
        for card in cards:
            for field, expected in [('parser_status','canonical'),('recognizer_status','native'),
                ('migration_status','native-primitive-executable'),('ir_status','primitive'),
                ('contract_status','executable'),('runtime_status','passed'),('proof_status','verified')]:
                require(card.get(field) == expected, 'strict card failed '+field)
            require(card.get('gap') is False and count(card.get('unexecutable_rule_count'),'unexecutable rules') == 0, 'strict gap/unexecutable rule')
        summary = report['summary']
        require(count(summary['total'],'strict total',True) == len(cards), 'strict total mismatch')
        require(count(summary['proof_cache_hits'],'strict cache') == 0 and report['source']['refresh_proofs'] is True, 'strict proofs were cached')
        require(count(summary['gaps'],'gaps') == 0, 'strict gaps')
        proofs = count(report['run_metrics']['proof_count'],'fresh proof count',True)
        if ledger is not None:
            by_id = {row['oracle_id']:row for row in ledger['cards']}
            expected = 0
            for card in cards:
                require(card['oracle_id'] in by_id, 'strict card absent from proof ledger')
                spec = by_id[card['oracle_id']]
                require(card['semantic_ir_digest'] == spec['expected_semantic_ir_digest'], 'strict semantic pin mismatch')
                for kind in ['contract','runtime']:
                    entries = spec[kind] if isinstance(spec[kind],list) else [spec[kind]]
                    require(bool(entries) and all(isinstance(entry,dict) and entry for entry in entries), 'empty proof specification')
                    expected += len(entries)
            require(proofs == expected, 'strict fresh-proof count differs from per-card proof specifications')
        actual = {'cards':len(cards),'freshProofs':proofs,'cachedCards':0}
    elif fmt == 'architecture':
        report = evidence.json(gate['evidence'])
        require(report.get('violations') == [] and report.get('warnings') == [], 'architecture has violations/warnings')
        require(isinstance(report.get('fingerprint'),str) and len(report['fingerprint']) == 64, 'architecture source fingerprint missing')
        actual = {'violations':0,'warnings':0}
    else:
        raise ValueError('unknown gate adapter '+str(fmt))
    expected_counts(actual, gate['expected'])
    return {'id':gate['id'],'format':fmt,'counts':actual}

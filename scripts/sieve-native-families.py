"""Family-redundancy sieve for the native Oracle dispatcher.

Runs the release analyzer twice over one deduplicated corpus (one printing per
oracle_id from the local Scryfall bulk file):
  * baseline: the normal native dispatch order;
  * generic-first: DEEPDECK_ORACLE_GENERIC_FIRST=1, where the shared static and
    contextual program parents run before the historical families.
Each baseline ability is attributed to the family that built it (its first
construction operation). For every family it reports how many of its abilities
the shared grammar reproduces with identical IR (redundant), with different IR
(review), or not at all (family still needed), plus abilities the shared path
newly parses. Measures parser recognition only, not runtime behavior.

Examples:
    python scripts/sieve-native-families.py --label sieve-1
    python scripts/sieve-native-families.py --label sieve-1 --reuse   # skip rebuild/reparse
Outputs: .tmp/family-sieve/<label>/{families.json, summary.md, divergent.jsonl}
"""
from __future__ import annotations

import argparse
import collections
import gzip
import importlib.util
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / '.tmp/family-sieve'
_spec = importlib.util.spec_from_file_location('audit_latest_sets', ROOT / 'scripts/audit-latest-sets.py')
audit = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(audit)

PAPER_LAYOUTS_EXCLUDED = {'token', 'double_faced_token', 'emblem', 'art_series', 'vanguard', 'scheme',
                          'planar'}


def build_corpus(path: Path) -> int:
    groups = collections.defaultdict(list)
    with gzip.open(audit.BULK, 'rt', encoding='utf-8') as stream:
        for line in stream:
            card = json.loads(line)
            if card.get('oracle_id') and card.get('layout') not in PAPER_LAYOUTS_EXCLUDED:
                groups[card['oracle_id']].append(card)
    chosen = [audit.representative(group) for group in groups.values()]
    chosen.sort(key=lambda c: (c['name'], c['oracle_id']))
    path.write_text(json.dumps(chosen, ensure_ascii=False), encoding='utf-8')
    return len(chosen)


def run(exe: Path, corpus: Path, output: Path, generic_first: bool) -> float:
    env = dict(os.environ, DEEPDECK_ORACLE_NATIVE_ONLY='1', RUST_MIN_STACK='33554432')
    if generic_first:
        env['DEEPDECK_ORACLE_GENERIC_FIRST'] = '1'
    else:
        env.pop('DEEPDECK_ORACLE_GENERIC_FIRST', None)
    started = time.perf_counter()
    result = subprocess.run([str(exe), str(corpus), str(output)], env=env, capture_output=True)
    if result.returncode:
        sys.exit(result.stderr.decode(errors='replace'))
    return time.perf_counter() - started


def load(path: Path) -> dict:
    table = {}
    for line in path.read_text(encoding='utf-8').splitlines():
        card = json.loads(line)
        for index, ability in enumerate(card['abilities']):
            table[(card['oracle_id'], index)] = (card['name'], ability)
    return table


def family_of(ability: dict) -> str:
    # Operation 0 is the shared classification step; depths 1-2 name the
    # family (or shared parent) that built the rule.
    operations = [op.split(': ', 1)[-1] for op in (ability.get('operations') or [])[1:3]]
    return ' / '.join(operations) if operations else '(no construction trace)'


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--label', required=True)
    parser.add_argument('--reuse', action='store_true', help='reuse existing corpus and analyzer outputs')
    args = parser.parse_args()
    out = OUT / args.label
    out.mkdir(parents=True, exist_ok=True)
    corpus, base_path, generic_path = out / 'corpus.json', out / 'baseline.jsonl', out / 'generic-first.jsonl'
    if not args.reuse:
        print('corpus cards:', build_corpus(corpus), flush=True)
        exe, build_seconds = audit.build_analyzer()
        print(f'build {build_seconds:.1f}s; baseline {run(exe, corpus, base_path, False):.1f}s; '
              f'generic-first {run(exe, corpus, generic_path, True):.1f}s', flush=True)
    base, generic = load(base_path), load(generic_path)

    families = collections.defaultdict(lambda: collections.Counter())
    examples = collections.defaultdict(dict)
    newly_parsed, lost = [], []
    with (out / 'divergent.jsonl').open('w', encoding='utf-8') as divergent:
        for key, (name, b) in base.items():
            g = generic[key][1]
            b_ok, g_ok = audit.is_native(b), audit.is_native(g)
            if not b_ok:
                if g_ok:
                    newly_parsed.append(dict(card=name, text=b['text'], by=family_of(g)))
                continue
            family = family_of(b)
            counts = families[family]
            counts['abilities'] += 1
            if not g_ok:
                counts['lostByGenericFirst'] += 1
                lost.append(dict(card=name, text=b['text'], family=family))
            elif (g.get('operations') or []) == (b.get('operations') or []):
                counts['familyStillNeeded'] += 1
                examples[family].setdefault('needed', f'{name}: {b["text"][:120]}')
            elif g['rule'] == b['rule']:
                counts['redundantIdenticalIr'] += 1
            else:
                counts['divergentIr'] += 1
                examples[family].setdefault('divergent', f'{name}: {b["text"][:120]}')
                divergent.write(json.dumps(dict(family=family, card=name, text=b['text'], baseline=b['rule'],
                                                generic=g['rule'], genericBy=family_of(g)),
                                           ensure_ascii=False) + '\n')
    rows = []
    for family, counts in families.items():
        verdict = ('redundant' if counts['abilities'] == counts['redundantIdenticalIr']
                   else 'needed' if counts['familyStillNeeded'] == counts['abilities']
                   else 'mixed')
        stats = {key: counts[key] for key in ('abilities', 'redundantIdenticalIr', 'divergentIr',
                                               'familyStillNeeded', 'lostByGenericFirst')}
        rows.append(dict(family=family, verdict=verdict, **stats, **examples.get(family, {})))
    rows.sort(key=lambda r: ({'redundant': 0, 'mixed': 1, 'needed': 2}[r['verdict']], -r['abilities']))
    summary = dict(label=args.label, families=len(rows), verdicts=dict(collections.Counter(r['verdict'] for r in rows)),
                   newlyParsedByGenericFirst=len(newly_parsed), lostByGenericFirst=len(lost))
    (out / 'families.json').write_text(json.dumps(dict(summary=summary, families=rows, newlyParsed=newly_parsed,
                                                       lost=lost), ensure_ascii=False, indent=2), encoding='utf-8')
    lines = [f'# Family sieve `{args.label}`', '', '```', json.dumps(summary, indent=2), '```', '',
             '| Verdict | Abilities | Identical IR | Divergent IR | Still needed | Lost | Family (first operation) |',
             '|---|---:|---:|---:|---:|---:|---|']
    for r in rows:
        lines.append(f"| {r['verdict']} | {r['abilities']} | {r['redundantIdenticalIr']} | {r['divergentIr']} | "
                     f"{r['familyStillNeeded']} | {r['lostByGenericFirst']} | {r['family'][:90]} |")
    (out / 'summary.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()

"""Audit the native Oracle parser on the most recent Magic sets.

Selects the N latest released, non-digital sets of the requested Scryfall set
types from the cached set index, builds one deduplicated input per set from the
local Scryfall bulk file (one representative printing per oracle_id), parses
them with the release `analyze_local_abilities` example in native-only mode,
and writes per-set results, totals, failure clusters and an optional
before/after comparison.

Examples:
    python scripts/audit-latest-sets.py --label before
    python scripts/audit-latest-sets.py --label after --compare-to before
    python scripts/audit-latest-sets.py --count 5 --set-types expansion core --label quick
    python scripts/audit-latest-sets.py --sets trk fra --label two-sets

Outputs go to .tmp/latest-sets-audit/<label>/ (summary.json, summary.md,
clusters.json, <set>.jsonl, and comparison.json when --compare-to is given).
Measures parser recognition only: a complete card here is not a runtime proof.
"""
from __future__ import annotations

import argparse
import collections
import datetime as dt
import gzip
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CACHE = ROOT / '.tmp/card-migration-cache'
BULK = ROOT / '.tmp/default-cards.jsonl.gz'
OUT = ROOT / '.tmp/latest-sets-audit'
TARGET = ROOT / '.tmp/targets/card-migration'


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest().upper()


def latest_set_index() -> Path:
    candidates = sorted(CACHE.glob('scryfall-set-index-*.json'))
    if not candidates:
        sys.exit(f'No cached Scryfall set index under {CACHE}')
    return candidates[-1]


def select_sets(args) -> list[dict]:
    index_path = latest_set_index()
    data = json.loads(index_path.read_text(encoding='utf-8-sig'))
    data = data.get('data', data)
    by_code = {s['code']: s for s in data}
    if args.sets:
        missing = [code for code in args.sets if code not in by_code]
        if missing:
            sys.exit(f'Unknown set codes in {index_path.name}: {missing}')
        return [by_code[code] for code in args.sets]
    today = args.as_of or dt.date.today().isoformat()
    eligible = [s for s in data
                if not s.get('digital')
                and s['set_type'] in args.set_types
                and s.get('released_at', '9999') <= today]
    eligible.sort(key=lambda s: (s['released_at'], s['code']), reverse=True)
    return eligible[:args.count]


def representative(group: list[dict]) -> dict:
    # Same rule as the historical audits: keep the most complete printing.
    return sorted(group, key=lambda c: (-(len(c.get('oracle_text', '')) + 10000 * len(c.get('card_faces', []))),
                                        c.get('collector_number', ''), c['id']))[0]


def build_inputs(sets: list[dict], directory: Path) -> dict[str, Path]:
    if not BULK.exists():
        sys.exit(f'Missing local Scryfall bulk file {BULK}')
    wanted = {s['code'] for s in sets}
    cards = collections.defaultdict(lambda: collections.defaultdict(list))
    with gzip.open(BULK, 'rt', encoding='utf-8') as stream:
        for line in stream:
            card = json.loads(line)
            if card.get('set') in wanted and card.get('oracle_id'):
                cards[card['set']][card['oracle_id']].append(card)
    paths = {}
    for s in sets:
        groups = cards.get(s['code'])
        if not groups:
            sys.exit(f"No local cards for {s['code']} in {BULK.name}; refresh the bulk file first.")
        chosen = [representative(group) for group in groups.values()]
        chosen.sort(key=lambda c: (c.get('collector_number', ''), c['name'], c['oracle_id']))
        path = directory / f"{s['code']}-input.json"
        path.write_text(json.dumps(chosen, ensure_ascii=False), encoding='utf-8')
        paths[s['code']] = path
    return paths


def build_analyzer() -> tuple[Path, float]:
    started = time.perf_counter()
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    run = subprocess.run(['cargo', 'build', '--locked', '--release', '-p', 'deepdeck-oracle',
                          '--example', 'analyze_local_abilities'], cwd=ROOT, env=env)
    if run.returncode:
        sys.exit('cargo build of the analyzer failed')
    exe = TARGET / 'release/examples' / ('analyze_local_abilities.exe' if os.name == 'nt' else 'analyze_local_abilities')
    return exe, time.perf_counter() - started


def is_native(ability: dict) -> bool:
    return ability['status'] == 'canonical' and ability['recognizer'] == 'native'


def cluster_key(ability: dict) -> str:
    diagnostic = (ability['diagnostics'] or [{}])[0]
    if ability['recognizer'] == 'unclaimed':
        text = re.sub(r'^[^—]{1,30} — ', '', ability['text'])
        text = re.sub(r'\{[^}]+\}', '{M}', text)
        return 'unclaimed: ' + ' '.join(text.split()[:3]).lower()
    start = diagnostic.get('spanStart', 0)
    words = ' '.join(ability['text'][start:].split()[:2]).lower()
    return f"{diagnostic.get('state', diagnostic.get('code', '?'))} @ '{words}'"


def audit(args) -> None:
    out = OUT / args.label
    out.mkdir(parents=True, exist_ok=True)
    sets = select_sets(args)
    inputs = build_inputs(sets, out)
    exe, build_seconds = build_analyzer()
    env = dict(os.environ, DEEPDECK_ORACLE_NATIVE_ONLY='1', RUST_MIN_STACK='33554432')
    rows, union = [], {}
    clusters, unlocks, examples = collections.Counter(), collections.Counter(), {}
    for s in sets:
        code = s['code']
        output = out / f'{code}.jsonl'
        started = time.perf_counter()
        run = subprocess.run([str(exe), str(inputs[code]), str(output)], env=env, capture_output=True)
        if run.returncode:
            sys.exit(run.stderr.decode(errors='replace'))
        elapsed = time.perf_counter() - started
        counts = collections.Counter()
        for line in output.read_text(encoding='utf-8').splitlines():
            card = json.loads(line)
            abilities = card['abilities']
            native = sum(map(is_native, abilities))
            category = ('empty' if not abilities else 'complete' if native == len(abilities)
                        else 'partial' if native else 'none')
            counts[category] += 1
            counts['abilities'] += len(abilities)
            counts['nativeAbilities'] += native
            union[card['oracle_id']] = category
            failing = [a for a in abilities if not is_native(a)]
            for ability in failing:
                key = cluster_key(ability)
                clusters[key] += 1
                examples.setdefault(key, f"{card['name']}: {ability['text'][:140]}")
            if len(failing) == 1:
                unlocks[cluster_key(failing[0])] += 1
        rows.append(dict(code=code, name=s['name'], releasedAt=s['released_at'],
                         cards=sum(counts[k] for k in ('complete', 'partial', 'none', 'empty')),
                         **{k: counts[k] for k in ('complete', 'partial', 'none', 'empty', 'abilities', 'nativeAbilities')},
                         seconds=round(elapsed, 3), inputSha256=sha256(inputs[code])))
        print(f"{code:>5}  {counts['complete']:>4}/{rows[-1]['cards']:<4} cards  "
              f"{counts['nativeAbilities']:>5}/{counts['abilities']:<5} abilities  {elapsed:.2f}s", flush=True)
    keys = ('cards', 'complete', 'partial', 'none', 'empty', 'abilities', 'nativeAbilities')
    totals = {k: sum(r[k] for r in rows) for k in keys}
    summary = dict(label=args.label, generatedAt=dt.datetime.now().isoformat(timespec='seconds'),
                   setIndex=latest_set_index().name, bulkSha256=sha256(BULK),
                   analyzerSha256=sha256(exe), buildSeconds=round(build_seconds, 1),
                   sets=rows, totals=totals, uniqueOracleIds=len(union),
                   uniquePartition=dict(collections.Counter(union.values())))
    (out / 'summary.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    (out / 'clusters.json').write_text(json.dumps(
        [dict(key=k, abilities=n, singleBlockerCards=unlocks[k], example=examples[k])
         for k, n in sorted(clusters.items(), key=lambda item: (-unlocks[item[0]], -item[1]))],
        ensure_ascii=False, indent=2), encoding='utf-8')
    lines = [f'# Native parser audit `{args.label}`', '',
             '| Set | Released | Complete cards | Partial | None | Native abilities |',
             '|---|---|---:|---:|---:|---:|']
    for r in rows:
        lines.append(f"| {r['name']} ({r['code']}) | {r['releasedAt']} | {r['complete']}/{r['cards']} | "
                     f"{r['partial']} | {r['none']} | {r['nativeAbilities']}/{r['abilities']} |")
    lines.append(f"| **Total** | | **{totals['complete']}/{totals['cards']}** | {totals['partial']} | "
                 f"{totals['none']} | **{totals['nativeAbilities']}/{totals['abilities']}** |")
    (out / 'summary.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    print('TOTAL', json.dumps(totals))
    if args.compare_to:
        compare(OUT / args.compare_to, out)


def load_abilities(directory: Path) -> dict:
    # Keyed by ability text (with its occurrence rank), not by position: a
    # reminder-only line that stops being an ability must not shift the keys
    # of the abilities after it.
    result = {}
    for path in directory.glob('*.jsonl'):
        for line in path.read_text(encoding='utf-8').splitlines():
            card = json.loads(line)
            seen = {}
            for ability in card['abilities']:
                occurrence = seen.get(ability['text'], 0)
                seen[ability['text']] = occurrence + 1
                result[(card['oracle_id'], ability['text'], occurrence)] = (card['name'], ability)
    return result


def compare(before_dir: Path, after_dir: Path) -> None:
    before, after = load_abilities(before_dir), load_abilities(after_dir)
    shared = before.keys() & after.keys()
    # A native ability whose text no longer appears is lost, not ignored.
    regressions = [k for k in before.keys() - after.keys() if is_native(before[k][1])]
    regressions += [k for k in shared if is_native(before[k][1]) and not is_native(after[k][1])]
    gains = [k for k in shared if is_native(after[k][1]) and not is_native(before[k][1])]
    changed = [k for k in shared if is_native(before[k][1]) and is_native(after[k][1])
               and before[k][1]['rule'] != after[k][1]['rule']]
    entry = lambda table, k: dict(card=table[k][0], text=table[k][1]['text'])
    report = dict(before=before_dir.name, after=after_dir.name, regressions=[entry(before, k) for k in regressions],
                  gains=[entry(after, k) for k in gains], irChanged=[entry(after, k) for k in changed])
    (after_dir / 'comparison.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding='utf-8')
    print(f'COMPARE {before_dir.name} -> {after_dir.name}: +{len(gains)} gained, '
          f'{len(regressions)} regressed, {len(changed)} IR changed')
    if regressions:
        sys.exit(1)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--label', required=True, help='output folder name under .tmp/latest-sets-audit')
    parser.add_argument('--count', type=int, default=10)
    parser.add_argument('--set-types', nargs='+', default=['expansion'])
    parser.add_argument('--sets', nargs='+', help='explicit set codes instead of the latest-N selection')
    parser.add_argument('--as-of', help='release-date cutoff YYYY-MM-DD (default: today)')
    parser.add_argument('--compare-to', help='label of an earlier run; exits 1 on any ability regression')
    audit(parser.parse_args())


if __name__ == '__main__':
    main()

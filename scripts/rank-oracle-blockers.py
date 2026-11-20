"""Rank Oracle-compiler blockers from a support probe run.

Input: the JSONL written by `examples/league_support_probe.rs` (one card per
line: oracle_id, name, supported, failures[{text, state, code, message}]).
Output: a Markdown ranking of parser states (and the token the parser found)
by the number of complete cards each one blocks alone, plus the structural
frame of abilities that no automaton claimed. This is a measurement tool for
scheduling themed work; it never feeds the parser.

Example:
    python scripts/rank-oracle-blockers.py .tmp/main-corpus-probe.jsonl \
        --out .tmp/blocker-ranking.md --compare .tmp/previous-probe.jsonl
"""
from __future__ import annotations

import argparse
import collections
import json
from pathlib import Path

FRAME_PREFIXES = (
    ('trigger', ('when ', 'whenever ', 'at the beginning', 'at end of')),
    ('replacement or static', ('if ', 'as long as', 'as an additional', 'as ', 'you may ',
                               'each ', 'all ', 'creatures ', 'other ', 'enchanted ', 'equipped ')),
)


def load(path: Path) -> list[dict]:
    return [json.loads(line) for line in path.read_text(encoding='utf-8').splitlines() if line.strip()]


def state_of(failure: dict) -> str:
    return failure.get('state') or failure.get('code') or '?'


def found_token(failure: dict) -> str:
    message = failure.get('message', '')
    marker = ' got '
    if marker not in message:
        return ''
    return message.split(marker, 1)[1].split()[0].strip('.,').lower() if message.split(marker, 1)[1].split() else ''


def body(text: str) -> str:
    # Drop a leading ability word ("Landfall — ") without interpreting it.
    head, sep, rest = text.partition(' — ')
    return rest if sep and len(head) <= 40 else text


def frame(text: str) -> str:
    lowered = body(text).strip().lower()
    for name, prefixes in FRAME_PREFIXES:
        if lowered.startswith(prefixes):
            return name
    colon, period = lowered.find(':'), lowered.find('.')
    if 0 < colon < 80 and (period == -1 or colon < period):
        return 'activated'
    return 'spell, keyword or other'


def rank(cards: list[dict]) -> dict:
    failing = [card for card in cards if not card['supported']]
    single = [card for card in failing if len(card.get('failures', [])) == 1]
    by_state, by_token, unclaimed = collections.Counter(), collections.Counter(), collections.Counter()
    touching, examples = collections.Counter(), {}
    for card in failing:
        for state in {state_of(f) for f in card.get('failures', [])}:
            touching[state] += 1
    for card in single:
        failure = card['failures'][0]
        state = state_of(failure)
        by_state[state] += 1
        key = (state, found_token(failure))
        by_token[key] += 1
        examples.setdefault(key, f"{card['name']}: {failure['text'][:120]}")
        if state == 'unsupported_oracle_ability':
            unclaimed[frame(failure['text'])] += 1
    return dict(total=len(cards), supported=len(cards) - len(failing), failing=len(failing),
                singleBlocker=len(single), byState=by_state, byToken=by_token,
                touching=touching, unclaimed=unclaimed, examples=examples)


def markdown(result: dict, label: str, limit: int) -> str:
    lines = [f'# Oracle blocker ranking — {label}', '',
             f"Supported {result['supported']} / {result['total']}; failing {result['failing']}, "
             f"of which {result['singleBlocker']} fail on exactly one ability.", '',
             '| Parser state | Cards blocked alone | Cards touched |', '|---|---:|---:|']
    for state, count in result['byState'].most_common(limit):
        lines.append(f"| `{state}` | {count} | {result['touching'][state]} |")
    lines += ['', '## Unclaimed abilities by structural frame', '', '| Frame | Cards |', '|---|---:|']
    for name, count in result['unclaimed'].most_common():
        lines.append(f'| {name} | {count} |')
    lines += ['', '## State and found token', '', '| Cards | State | Token | Example |', '|---:|---|---|---|']
    for (state, token), count in result['byToken'].most_common(limit):
        example = result['examples'][(state, token)].replace('|', '/').replace('\n', ' ')
        lines.append(f'| {count} | `{state}` | `{token}` | {example} |')
    return '\n'.join(lines) + '\n'


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('probe', type=Path)
    parser.add_argument('--out', type=Path)
    parser.add_argument('--limit', type=int, default=60)
    parser.add_argument('--compare', type=Path, help='earlier probe run; reports gained and lost cards')
    args = parser.parse_args()
    cards = load(args.probe)
    text = markdown(rank(cards), args.probe.name, args.limit)
    if args.compare:
        before = {c['oracle_id']: c['supported'] for c in load(args.compare)}
        after = {c['oracle_id']: c['supported'] for c in cards}
        gained = sum(1 for k, v in after.items() if v and not before.get(k, False))
        lost = sum(1 for k, v in before.items() if v and not after.get(k, False))
        text += f'\nCompared with `{args.compare.name}`: +{gained} gained, {lost} lost.\n'
    if args.out:
        args.out.write_text(text, encoding='utf-8')
    else:
        print(text)


if __name__ == '__main__':
    main()

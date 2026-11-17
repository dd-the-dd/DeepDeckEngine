"""Typed baseline preservation, explicit exact rewrites, and distinct set/dedup totals."""
from collections import Counter
from checkpoint_core import count, keys, require, typed

CLASSES = ['completeNonempty','partial','none','emptyCanonical','emptyUnsupported']


def native(ability):
    return ability.get('status') == 'canonical' and ability.get('recognizer') == 'native'


def classification(card):
    abilities = card.get('abilities') or []
    if not abilities:
        return 'emptyCanonical' if card.get('status') == 'canonical' else 'emptyUnsupported'
    n = sum(native(ability) for ability in abilities)
    return 'completeNonempty' if n == len(abilities) and card.get('status') == 'canonical' else 'partial' if n else 'none'


def identity(row):
    require(isinstance(row.get('oracle_id'),str) and bool(row['oracle_id']), 'missing Oracle identity')
    return (row['oracle_id'],row.get('set'),row.get('collector_number'))


def compare_corpus(evidence, spec, changes):
    keys(spec,['id','before','after','input'],['artifactId'])
    before = evidence.jsonl(spec['before']);after = evidence.jsonl(spec['after'])
    inputs = evidence.json(spec['input'])
    require(isinstance(inputs,list) and bool(inputs), 'expected nonempty Scryfall input array')
    require(len(before) == len(after) == len(inputs), 'corpus/input row count mismatch')
    require([identity(row) for row in before] == [identity(row) for row in after]
            == [identity(row) for row in inputs], 'ordered corpus/input identities mismatch')
    require(len({identity(row) for row in after}) == len(after), 'duplicate complete corpus identity')
    preserved = rewritten = gained = retired = 0
    seen_changes = set()
    for old, new, source in zip(before,after,inputs):
        require(old.get('name') == new.get('name') == source.get('name'), 'source card name drift')
        aa = old.get('abilities') or [];bb = new.get('abilities') or []
        require(len(aa) == len(bb), 'ability-boundary change requires a separate explicit protocol')
        oracle_texts = [source.get('oracle_text') or ''] + [(face.get('oracle_text') or '') for face in source.get('card_faces') or []]
        for index,(a,b) in enumerate(zip(aa,bb)):
            require(type(a.get('text')) is str and a['text'] == b.get('text'), 'Oracle ability text drift')
            require(any(a['text'] in text for text in oracle_texts), 'ability text not present in exact source/face')
            if native(a):
                change_key = (old['oracle_id'],index)
                if change_key in changes:
                    change = changes[change_key]
                    require(typed(a.get('rule')) == typed(change['beforeRule']), 'allowlisted rewrite BEFORE is not exact')
                    require(typed(b.get('rule')) == typed(change['afterRule']) and b.get('status') == change['afterStatus'], 'allowlisted rewrite AFTER is not exact')
                    if change['afterStatus'] == 'canonical':
                        require(native(b), 'rewrite lost native recognizer')
                        rewritten += 1
                    else:
                        require(not native(b), 'false-positive retirement stayed native')
                        retired += 1
                    seen_changes.add(change_key)
                else:
                    require(native(b), 'prior native ability became unsupported without exact declared retirement')
                    require(typed(a.get('rule')) == typed(b.get('rule')), 'unapproved prior native IR change')
                    preserved += 1
            elif native(b):
                gained += 1
    def metrics(rows):
        partition = {name:0 for name in CLASSES}
        for row in rows:partition[classification(row)] += 1
        require(sum(partition.values()) == len(rows), 'card partition does not sum')
        return {'cards':len(rows),'partition':partition,'nativeAbilities':sum(sum(map(native,row.get('abilities') or [])) for row in rows)}
    old_metrics=metrics(before);new_metrics=metrics(after)
    return {'id':spec['id'],'before':old_metrics,'after':new_metrics,'priorNativeExact':preserved,
        'intendedRewrites':rewritten,'declaredRetirements':retired,'newNativeAbilities':gained,
        'newCompleteCards':new_metrics['partition']['completeNonempty']-old_metrics['partition']['completeNonempty']},after,seen_changes


def load_changes(evidence, specs):
    result={}
    for spec in specs:
        keys(spec,['oracleId','abilityIndex','beforeRule','afterRule','afterStatus','reason'])
        key=(spec['oracleId'],count(spec['abilityIndex'],'rewrite ability index'))
        require(key not in result and isinstance(spec['reason'],str) and bool(spec['reason'].strip()), 'duplicate/unjustified IR exception')
        require(spec['afterStatus'] in ['canonical','unsupported'], 'invalid rewrite status')
        result[key]={'beforeRule':evidence.json(spec['beforeRule']),'afterRule':evidence.json(spec['afterRule']),
                     'afterStatus':spec['afterStatus'],'reason':spec['reason']}
    return result


def set_totals(corpora, identifiers, expected=None):
    require(isinstance(identifiers,list) and bool(identifiers) and len(set(identifiers)) == len(identifiers), 'empty/duplicate set selection')
    occurrences={name:0 for name in CLASSES};unique={};rows=[]
    for label in identifiers:
        require(label in corpora, 'unknown set corpus '+label)
        selected=corpora[label]
        partition=Counter(map(classification,selected))
        require(sum(partition.values()) == len(selected), 'set partition mismatch')
        rows.append({'id':label,'cards':len(selected),'partition':dict(partition)})
        for name,value in partition.items():occurrences[name] += value
        for card in selected:
            oracle_id=card['oracle_id'];kind=classification(card)
            # Explicit policy: best-supported occurrence per Oracle identity.
            if oracle_id not in unique or CLASSES.index(kind) < CLASSES.index(unique[oracle_id]):unique[oracle_id]=kind
    result={'selection':identifiers,'setRows':rows,'occurrences':sum(occurrences.values()),'partition':occurrences,
        'uniqueOracleIds':len(unique),'uniquePartition':{name:sum(value==name for value in unique.values()) for name in CLASSES},
        'dedupPolicy':'best-supported occurrence per oracle_id; occurrence totals remain separate'}
    require(result['occurrences'] == sum(row['cards'] for row in rows), 'set total not derived from rows')
    require(sum(result['uniquePartition'].values()) == result['uniqueOracleIds'], 'unique partition mismatch')
    if expected is not None:
        keys(expected,['occurrences','partition','uniqueOracleIds','uniquePartition'])
        require(typed(expected) == typed({key:result[key] for key in expected}), 'declared set totals/partition inconsistent with actual rows')
    return result

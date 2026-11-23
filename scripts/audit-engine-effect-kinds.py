"""Read-only, role-bounded runtime effect census; write refreshes only under .tmp."""
from pathlib import Path
from dataclasses import dataclass
import argparse,collections,hashlib,json,re,sys,time
RAW_STRING=re.compile(r'(?:br|r)(#*)"')
CHAR_OR_WORD=re.compile(r"(?:b)?'(?:\\u\{[0-9A-Fa-f_]+\}|\\.|[^'\\\n])'|[A-Za-z_][A-Za-z0-9_]*|[0-9][A-Za-z0-9_]*")

@dataclass(frozen=True)
class Token:
    text:str
    start:int
    end:int

def lex(source):
    result=[];i=0
    while i<len(source):
        if source[i].isspace():i+=1;continue
        if source.startswith('//',i):
            end=source.find('\n',i);i=len(source) if end<0 else end;continue
        if source.startswith('/*',i):
            depth=1;i+=2
            while depth:
                if i>=len(source):raise ValueError('unterminated Rust comment')
                if source.startswith('/*',i):depth+=1;i+=2
                elif source.startswith('*/',i):depth-=1;i+=2
                else:i+=1
            continue
        start=i
        raw=RAW_STRING.match(source,i)
        if raw:
            marker='"'+raw[1];end=source.find(marker,raw.end())
            if end<0:raise ValueError('unterminated raw string')
            i=end+len(marker)
        elif source[i]=='"' or source.startswith('b"',i):
            i+=2 if source[i]=='b' else 1
            while i<len(source):
                if source[i]=='\\':i+=2
                elif source[i]=='"':i+=1;break
                else:i+=1
        else:
            match=CHAR_OR_WORD.match(source,i)
            i=match.end() if match else i+1
        result.append(Token(source[start:i],start,i))
    return result

class RustSource:
    def __init__(self,path,raw):
        self.path=path;self.raw=raw;self.text=raw.decode('utf-8');self.tokens=lex(self.text)
        self.pairs={};stack=[]
        for i,token in enumerate(self.tokens):
            if token.text in ('(','[','{'):stack.append(i)
            elif token.text in (')',']','}'):
                if not stack:raise ValueError(f'{path}: unmatched closing delimiter')
                opening=stack.pop()
                if (self.tokens[opening].text,token.text) not in [('(',')'),('[',']'),('{','}')]:raise ValueError(f'{path}: mismatched delimiter')
                self.pairs[opening]=i
        if stack:raise ValueError(f'{path}: unclosed delimiter')
        self.functions=collections.defaultdict(list)
        for i,token in enumerate(self.tokens[:-2]):
            if token.text!='fn' or not re.fullmatch('[A-Za-z_][A-Za-z0-9_]*',self.tokens[i+1].text):continue
            j=i+2
            while j<len(self.tokens) and self.tokens[j].text not in ('{',';'):
                j=self.pairs[j]+1 if j in self.pairs else j+1
            if j<len(self.tokens) and self.tokens[j].text=='{':self.functions[self.tokens[i+1].text].append((i,j,self.pairs[j]))

    def function(self,name):
        sites=self.functions.get(name,[])
        if len(sites)!=1:raise ValueError(f'{self.path}: expected one function {name}, found {len(sites)}')
        return sites[0]

    def line(self,offset):return self.text.count('\n',0,offset)+1

    def arms(self,name,role):
        start,opening,ending=self.function(name);ts=self.tokens
        target=['match','value_kind','(','effect',')','{'];match=None
        for i in range(opening+1,ending-len(target)+1):
            if [t.text for t in ts[i:i+len(target)]]==target:match=i+5;break
        if match is None:return []
        stop=self.pairs[match];i=match+1;result=[]
        while i<stop:
            if ts[i].text==',':i+=1;continue
            pattern_start=i;j=i
            while j<stop and not(ts[j].text=='=' and ts[j+1].text=='>'):
                j=self.pairs[j]+1 if j in self.pairs else j+1
            if j>=stop:raise ValueError(f'{self.path}:{name}: no arm arrow')
            body_start=j+2;k=body_start
            if ts[k].text=='{':k=self.pairs[k]+1
            else:
                while k<stop and ts[k].text!=',':k=self.pairs[k]+1 if k in self.pairs else k+1
            body_end=ts[k-1].end
            pattern=ts[pattern_start:j]
            kinds=[]
            # Only the Some pattern supplies effect kinds; strings in guards are not names.
            if pattern and pattern[0].text=='Some' and ts[pattern_start+1].text=='(':
                for token in ts[pattern_start+2:self.pairs[pattern_start+1]]:
                    if token.text.startswith('"'):
                        kind=json.loads(token.text)
                        if not re.fullmatch('[A-Za-z][A-Za-z0-9_]*',kind):raise ValueError('unexpected kind pattern')
                        kinds.append(kind)
            if kinds:
                result.append({'kinds':kinds,'pattern':self.text[ts[pattern_start].start:ts[j].start].strip(),
                               'body':self.text[ts[body_start].start:body_end], 'file':self.path,
                               'line':self.line(ts[pattern_start].start),'function':name,'actualRole':role})
            i=k
        return result

    def whole_function(self,name,kinds,role):
        start,opening,end=self.function(name)
        return {'kinds':kinds,'pattern':'delegated selective validator' if role=='delegated-validator' else 'delegated domain executor',
                'body':self.text[self.tokens[start].start:self.tokens[end].end],'file':self.path,
                'line':self.line(self.tokens[start].start),'function':name,'actualRole':role}

def sha(raw):return hashlib.sha256(raw).hexdigest().upper()
def engine_files(repo):
    return sorted(p for p in (repo/'src/engine').rglob('*.rs') if 'test' not in p.stem and 'tests' not in p.parts)

def retired_list(path,raw=None):
    data=json.loads(raw.decode('utf-8-sig') if raw is not None else path.read_text(encoding='utf-8-sig'))
    values=data if isinstance(data,list) else data.get('retiredOracleKinds',data.get('kinds'))
    if not isinstance(values,list) or not all(isinstance(k,str) and k for k in values):raise ValueError('retired kinds must be an array or retiredOracleKinds/kinds array')
    if len(values)!=len(set(values)):raise ValueError('duplicate retired kind')
    return values

def census(sources):
    accepted=collections.defaultdict(list);executed=collections.defaultdict(list)
    def add(target,path,name,role):
        for arm in sources[path].arms(name,role):
            for kind in arm['kinds']:target[kind].append(arm)
    # Explicit function roles; never a repository-wide Some/string census.
    for path,name in [('src/engine/mod.rs','effect_supported'),('src/engine/life.rs','direct_life_effect_supported'),
                      ('src/engine/validation/damage.rs','damage_effect_supported'),('src/engine/validation/tokens.rs','token_copy_effect_supported'),
                      ('src/engine/zone_change_triggers.rs','zone_change_effect_supported')]:
        add(accepted,path,name,'validator-branch')
    for path,source in sources.items():
        if Path(path).parent.as_posix()=='src/engine/effect_domains' and 'supports_known_effect' in source.functions:
            add(accepted,path,'supports_known_effect','domain-validator-branch')
            for name in source.functions:
                if re.fullmatch('execute_[A-Za-z0-9_]+_known',name):add(executed,path,name,'domain-executor-branch')
    for path,name in [*((ROOT,name) for name in ROOT_EXECUTORS if name in sources[ROOT].functions),
                      ('src/engine/zone_change_triggers.rs','execute_zone_change_effect')]:
        add(executed,path,name,'executor-branch')
    for kind,path,name in [('forEachPlayer','src/engine/player_iteration.rs','player_iteration_supported'),
                           ('emitAbilitySignal','src/engine/ability_signals.rs','ability_signal_effect_supported'),
                           ('sacrificePermanents','src/engine/sacrifice.rs','explicit_object_sacrifice_supported'),
                           ('modifySubtypeSet','src/engine/subtypes.rs','effect_supported')]:
        accepted[kind].append(sources[path].whole_function(name,[kind],'delegated-validator'))
    for kinds,path,name in [(['forEachPlayer'],'src/engine/player_iteration.rs','execute_player_iteration'),
                            (['emitAbilitySignal'],'src/engine/ability_signals.rs','execute_ability_signal_effect'),
                            (['gainLife','loseLife'],'src/engine/life.rs','execute_direct_life_effect'),
                            (['sacrificePermanents'],'src/engine/sacrifice.rs','execute_explicit_object_sacrifice')]:
        arm=sources[path].whole_function(name,kinds,'delegated-executor')
        for kind in kinds:executed[kind].append(arm)
    return accepted,executed

ROOT='src/engine/mod.rs'
# The root executor opens one simultaneous event batch (CR 603.2c) and routes inside it.
ROOT_EXECUTORS=('execute_effect','execute_effect_in_batch')
EVENT_BATCH_SCOPE={'begin_event_batch','end_event_batch'}
# Kind-transparent envelopes: each strips one envelope field and re-enters the root role, so it
# accepts or executes no kind the census does not already attribute. The check is lexical, not a
# semantic proof: a wrapper must not name the "kind" field or call a kind reader, may call only
# its own allowlisted helpers, and must re-enter the root role. Anything else fails closed and
# must be registered as a validator/executor role instead.
IDENT=re.compile('[A-Za-z_][A-Za-z0-9_]*')
KIND_READERS={'value_kind'}
# Paths that name the engine root from a child module (`super::effect_supported`).
ROOT_FUNCTION_PATHS={'','super','crate::engine'}
ROOT_METHOD_RECEIVERS={'self','Self','GameEngine','super::GameEngine','crate::engine::GameEngine'}
# Value plumbing shared by both envelopes; none of these dispatches on an effect kind.
ENVELOPE_VALUE_METHODS={'get','clone','as_object_mut','remove','is_ok_and'}
TRANSPARENT_VALIDATORS={'effect_performer::performer_effect_supported':{
    'path':'src/engine/effect_performer.rs','function':'performer_effect_supported','reentry':('function','effect_supported'),
    'helpers':{('function','singular_player_reference_supported')}}}
TRANSPARENT_EXECUTORS={'execute_performed_effect':{
    'path':'src/engine/effect_performer.rs','function':'execute_performed_effect','reentry':('method','execute_effect_in_batch'),
    'helpers':{('method','resolve_player'),('method','player_index')}}}
# Non-delegating calls the root executors make (the unsupported-effect error message).
ROOT_EXECUTOR_HELPERS={('function','value_kind'),('function','EngineError::new'),('value','unwrap_or')}

def call_sites(tokens):
    """Every call in a body as (form, name): `method` for self/Self/GameEngine methods,
    `function` for free or path-qualified functions (root paths normalised away),
    `value` for methods on any other receiver. Constructors (Some, Ok) are skipped."""
    result=set()
    for i in range(len(tokens)-1):
        name=tokens[i].text
        if tokens[i+1].text!='(' or not IDENT.fullmatch(name) or name[0].isupper():continue
        before=tokens[i-1].text if i else ''
        if before=='fn':continue
        if before=='.':
            result.add(('method' if i>=2 and tokens[i-2].text=='self' else 'value',name));continue
        segments=[];j=i
        while j>=2 and tokens[j-1].text==':' and tokens[j-2].text==':' and j>=3 and IDENT.fullmatch(tokens[j-3].text):
            segments.insert(0,tokens[j-3].text);j-=3
        prefix='::'.join(segments)
        if prefix in ROOT_METHOD_RECEIVERS-{'self'}:result.add(('method',name))
        elif prefix in ROOT_FUNCTION_PATHS:result.add(('function',name))
        else:result.add(('function',prefix+'::'+name))
    return result

def transparent_wrapper(sources,name,spec):
    path=spec['path']
    if path not in sources:raise ValueError(f'kind-transparent wrapper {name}: {path} is missing')
    body=lex(sources[path].whole_function(spec['function'],[],'transparent')['body'])
    if any(t.text in KIND_READERS or t.text=='"kind"' for t in body):
        raise ValueError(f'{path}:{spec["function"]} inspects effect kinds; register it as a validator/executor role instead')
    found=call_sites(body);reentry=spec['reentry']
    if reentry not in found:raise ValueError(f'{path}:{spec["function"]} no longer re-enters {reentry[1]}; it is not kind-transparent')
    extra=sorted(f'{form}:{call}' for form,call in found-{reentry}-spec['helpers'] if not(form=='value' and call in ENVELOPE_VALUE_METHODS))
    if extra:raise ValueError(f'{path}:{spec["function"]} calls outside its envelope allowlist: {extra}')

def role_conformance(sources):
    """Fail closed when the root adds a delegation the census does not know."""
    root=sources[ROOT]
    arm=root.whole_function('effect_supported',[],'validator')
    ts=lex(arm['body']);qualified=set()
    for i in range(len(ts)-4):
        if [t.text for t in ts[i+1:i+3]]==[':',':'] and ts[i+4].text=='(':
            qualified.add(ts[i].text+'::'+ts[i+3].text)
    known={'player_iteration::player_iteration_supported','ability_signals::ability_signal_effect_supported',
           'life::direct_life_effect_supported','validation::damage_effect_supported','validation::token_copy_effect_supported',
           'sacrifice::explicit_object_sacrifice_supported','zone_change_triggers::zone_change_effect_supported',
           'subtypes::effect_supported','effect_domains::effect_domain_supported'}
    if qualified-known-TRANSPARENT_VALIDATORS.keys():raise ValueError(f'unregistered root validator delegation: {sorted(qualified-known-TRANSPARENT_VALIDATORS.keys())}')
    for name in qualified&TRANSPARENT_VALIDATORS.keys():transparent_wrapper(sources,name,TRANSPARENT_VALIDATORS[name])
    known_runtime={'execute_player_iteration','execute_ability_signal_effect','execute_direct_life_effect',
                   'execute_explicit_object_sacrifice','execute_zone_change_effect','execute_effect_domain'}
    allowed={('method',n) for n in known_runtime|TRANSPARENT_EXECUTORS.keys()|EVENT_BATCH_SCOPE|set(ROOT_EXECUTORS)}|ROOT_EXECUTOR_HELPERS
    # Every call form counts: `self.f(`, `Self::f(`, free and path-qualified functions, other receivers.
    pending=['execute_effect'];seen=set();executor_calls=set()
    while pending:
        name=pending.pop()
        if name in seen:continue
        seen.add(name);found=call_sites(lex(root.whole_function(name,[],'executor')['body']));executor_calls|=found
        pending.extend(n for form,n in found if form=='method' and n in ROOT_EXECUTORS)
    unregistered=sorted(n if form=='method' else f'{form}:{n}' for form,n in executor_calls-allowed)
    if unregistered:raise ValueError(f'unregistered root executor delegation: {unregistered}')
    for form,name in executor_calls:
        if form=='method' and name in TRANSPARENT_EXECUTORS:transparent_wrapper(sources,name,TRANSPARENT_EXECUTORS[name])
    catalog_path='src/engine/effect_domains/mod.rs'
    if catalog_path in sources:
        routed={k for a in sources[catalog_path].arms('domain','routing-only') for k in a['kinds']}
        local=set()
        for path,source in sources.items():
            if Path(path).parent.as_posix()!='src/engine/effect_domains':continue
            for name in source.functions:
                if re.fullmatch('execute_[A-Za-z0-9_]+_known',name):
                    local.update(k for a in source.arms(name,'executor') for k in a['kinds'])
        if routed!=local:raise ValueError(f'catalogue/executor role mismatch: catalogOnly={sorted(routed-local)}, executorOnly={sorted(local-routed)}')
    else:routed=set()
    return {'rootDelegationsRegistered':True,'catalogueMatchesDomainExecutors':True,'catalogueKinds':len(routed)}

def records(accepted,executed,sources,baseline):
    manual={row['kind']:row for row in baseline.get('records',[])};result=[]
    # GameEngine-only method definitions avoid the old false record_event link to DamageEventCollector.
    methods=collections.defaultdict(list)
    for path,source in sources.items():
        ts=source.tokens
        for i in range(len(ts)-3):
            if [t.text for t in ts[i:i+3]]!=['impl','GameEngine','{']:continue
            end=source.pairs[i+2]
            for name,sites in source.functions.items():
                for start,opening,closing in sites:
                    if i+2<start<end:
                        line_start=source.text.rfind('\n',0,ts[start].start)+1
                        prefix=source.text[line_start:ts[start].start]
                        if len(prefix)-len(prefix.lstrip())==4:methods[name].append({'method':name,'file':path,'line':source.line(ts[start].start)})
    for kind in sorted(set(accepted)|set(executed)):
        def locations(arms):return [{k:a[k] for k in ['file','line','function','pattern','actualRole']} for a in arms]
        delegates=[]
        for arm in executed[kind]:
            tokens=lex(arm['body'])
            for i in range(len(tokens)-3):
                if tokens[i].text=='self' and tokens[i+1].text=='.' and tokens[i+3].text=='(':
                    name=tokens[i+2].text;sites=methods.get(name,[])
                    # A root method wins over a same-named helper on a different struct.
                    sites=[s for s in sites if s['file']=='src/engine/mod.rs'] or sites
                    for site in sites:
                        if site['file']!='src/engine/mod.rs' and site not in delegates:delegates.append(site)
        row=dict(manual.get(kind,{}))
        row.update({'kind':kind,'accepted':bool(accepted[kind]),'executed':bool(executed[kind]),
                    'acceptedBy':locations(accepted[kind]),'executedBy':locations(executed[kind]),'delegatedDomains':delegates,
                    'sharedExecutionBranchKinds':list(dict.fromkeys(k for a in executed[kind] for k in a['kinds'])),
                    'contractExcerpt':accepted[kind][0]['body'].strip()[:320] if accepted[kind] else None,
                    'executionExcerpt':executed[kind][0]['body'].strip()[:320] if executed[kind] else None})
        row.setdefault('shortDescription',kind+'.');row.setdefault('descriptionBasis','New technical label; semantic review required.')
        row.setdefault('category','non-classe-a-revoir');row.setdefault('categoryConfidence','classification indicative, not a primitive-coverage claim')
        row['contractLink']=f"{row['acceptedBy'][0]['file']}:{row['acceptedBy'][0]['line']}" if row['acceptedBy'] else None
        row['executionLink']=f"{row['executedBy'][0]['file']}:{row['executedBy'][0]['line']}" if row['executedBy'] else None
        result.append(row)
    return result

def main(argv=None):
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--repo-root',type=Path,default=Path.cwd())
    ap.add_argument('--baseline',type=Path,help='Existing manual inventory required only for a detailed refresh; never needed by --check-only.')
    ap.add_argument('--output',type=Path);ap.add_argument('--check-only',action='store_true')
    ap.add_argument('--retired-kinds-json',type=Path);ap.add_argument('--expected-mod-sha')
    ap.add_argument('--expect-accepted',type=int);ap.add_argument('--expect-executed',type=int)
    args=ap.parse_args(argv);repo=args.repo_root.resolve();started=time.perf_counter()
    paths=engine_files(repo);raw={p.relative_to(repo).as_posix():p.read_bytes() for p in paths}
    if args.expected_mod_sha and sha(raw['src/engine/mod.rs'])!=args.expected_mod_sha.upper():raise ValueError('mod.rs SHA differs from required source epoch')
    sources={p:RustSource(p,b) for p,b in raw.items()};conformance=role_conformance(sources);accepted,executed=census(sources)
    counts={'acceptedKinds':len(accepted),'executedKinds':len(executed),'both':len(accepted.keys()&executed.keys()),
            'acceptedWithoutExecutor':sorted(accepted.keys()-executed.keys()),'executorWithoutAcceptance':sorted(executed.keys()-accepted.keys())}
    retired_raw=args.retired_kinds_json.read_bytes() if args.retired_kinds_json else None
    retired=retired_list(args.retired_kinds_json,retired_raw) if args.retired_kinds_json else []
    guard={'checked':bool(args.retired_kinds_json),'retiredKinds':retired,
           'stillAccepted':sorted(set(retired)&accepted.keys()),'stillExecuted':sorted(set(retired)&executed.keys())}
    guard['passed']=not(guard['stillAccepted'] or guard['stillExecuted'])
    if retired_raw is not None:guard['definition']={'path':str(args.retired_kinds_json.resolve()),'sha256':sha(retired_raw)}
    if args.expect_accepted is not None and counts['acceptedKinds']!=args.expect_accepted:raise ValueError(f'accepted count differs: {counts}')
    if args.expect_executed is not None and counts['executedKinds']!=args.expect_executed:raise ValueError(f'executed count differs: {counts}')
    baseline={};baseline_raw=None;baseline_path=None
    if not args.check_only:
        if args.output is None:raise ValueError('--output is required unless --check-only')
        if args.baseline is None:raise ValueError('--baseline is required for a detailed refresh to preserve manual descriptions')
        output=args.output.resolve()
        if not output.is_relative_to((repo/'.tmp').resolve()):raise ValueError('staging output must remain within repo/.tmp')
        if output.exists() and any(output.iterdir()):raise ValueError('output must be absent or empty; historical captures are immutable')
        baseline_path=args.baseline.resolve();baseline_raw=baseline_path.read_bytes();baseline=json.loads(baseline_raw.decode('utf-8-sig'))
        rows=records(accepted,executed,sources,baseline)
    # Fail closed if integration/pruning happened during the capture.
    if paths!=engine_files(repo) or any((repo/p).read_bytes()!=b for p,b in raw.items()):raise ValueError('source changed during capture; retry after root freezes the lane')
    if baseline_path and baseline_path.read_bytes()!=baseline_raw:raise ValueError('manual inventory changed during capture')
    if retired_raw is not None and args.retired_kinds_json.read_bytes()!=retired_raw:raise ValueError('retired definition changed during capture')
    report={'schemaVersion':'deepdeck-engine-effect-census/v1','scope':'Static role-bounded effect contract/executor census; not atom count or behavioral/native-card coverage',
            'generatedBy':'refresh.py: explicit root/delegated roles and effect_domains supports_known_effect/execute_*_known',
            'sourceHashes':{p:sha(b) for p,b in raw.items()},'counts':counts,'retiredGuard':guard,'roleConformance':conformance,
            'seconds':time.perf_counter()-started,'productionChanged':False,'cargoInvoked':False}
    if not args.check_only:
        old={r['kind'] for r in baseline.get('records',[])};new={r['kind'] for r in rows}
        report.update({'records':rows,'categoryCounts':dict(collections.Counter(r['category'] for r in rows if r['accepted'])),
                       'aliasNotes':baseline.get('aliasNotes'),'limits':baseline.get('limits',[]),
                       'refreshedFrom':{'path':str(baseline_path),'sha256':sha(baseline_raw)},
                       'deltaFromBaseline':{'addedKinds':sorted(new-old),'removedKinds':sorted(old-new)},
                       'sourceEpochStable':True})
        output.mkdir(parents=True,exist_ok=True)
        (output/'inventory.json').write_text(json.dumps(report,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
        (output/'capture.json').write_text(json.dumps({k:v for k,v in report.items() if k!='records'},indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps({'schemaVersion':report['schemaVersion'],'counts':counts,'retiredGuard':guard,'sourceFiles':len(raw),'sourceEpochStable':True,'seconds':report['seconds']},ensure_ascii=False))
    return 0 if guard['passed'] else 2

if __name__=='__main__':
    try:raise SystemExit(main())
    except (ValueError,KeyError,OSError) as error:
        print(json.dumps({'error':str(error),'productionChanged':False,'cargoInvoked':False}),file=sys.stderr);raise SystemExit(3)

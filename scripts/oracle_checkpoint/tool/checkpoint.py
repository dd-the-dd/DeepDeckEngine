"""Generate a reviewable DRAFT from pinned evidence. Never accepts or installs a checkpoint."""
from pathlib import Path
import argparse
import json
import subprocess
import tempfile
import time

from checkpoint_core import Evidence, EvidenceError, count, digest, fresh_output, keys, require, strict_load, typed
from checkpoint_gates import validate_gate
from checkpoint_ir import compare_corpus, load_changes, set_totals, native

HERE = Path(__file__).resolve().parent


def official_helper(evidence, work, mode, payload):
    request=work/(mode+'-'+str(len(list(work.iterdir())))+'-input.json')
    output=request.with_name(request.stem+'-output.json')
    request.write_text(json.dumps(payload,ensure_ascii=False),encoding='utf-8')
    args=['powershell','-NoLogo','-NoProfile','-File',str(HERE/'Invoke-OfficialCheckpointHelpers.ps1'),
          '-Mode',mode,'-RepoRoot',str(evidence.repo),'-InputJson',str(request),'-OutputJson',str(output)]
    started=time.perf_counter()
    result=subprocess.run(args,cwd=evidence.repo,capture_output=True,text=True,encoding='utf-8')
    require(result.returncode == 0, 'official PowerShell helper failed: '+result.stderr)
    response=strict_load(output.read_text(encoding='utf-8-sig'))
    return response,time.perf_counter()-started


def source_epoch(evidence, plan):
    snapshot=evidence.json(plan['sourceFreeze'])
    require(isinstance(snapshot.get('sources'),dict) and bool(snapshot['sources']), 'source snapshot is empty')
    if plan['sourceMode']=='live':
        evidence.hash_map(snapshot['sources'])
        if snapshot.get('fixtureInputs'):evidence.hash_map(snapshot['fixtureInputs'])
    else:
        require(plan['sourceMode']=='root-attested-history', 'unknown source mode')
        approval=evidence.json(plan['historicalCertificate'])
        require(approval.get('accepted') is True and approval.get('sourceFreezeSHA256')==plan['sourceFreeze']['sha256'],
                'historical source snapshot lacks its pinned accepted ROOT certificate')
        require(approval.get('sourceInputs')==len(snapshot['sources']), 'historical source count mismatch')
    return snapshot


def validate_artifact(evidence, spec, plan, snapshot, work):
    keys(spec,['id','profile'],['binary','metadata','wrapperScript','commonScript','attestation','gateReport','gateLog'])
    profile=spec['profile']
    if profile=='root-attested-gate':
        require(plan['sourceMode']=='root-attested-history', 'historical gate attestation cannot prove a fresh candidate')
        cert=evidence.json(spec['attestation'])
        require(cert.get('accepted') is True and cert.get('sourceFreezeSHA256')==plan['sourceFreeze']['sha256'],'wrong gate attestation epoch')
        proofs=cert.get('evidence',{})
        for field in ['gateReport','gateLog']:
            ref=spec[field];path=evidence.ref(ref)
            keys_to_try=[path.name,ref['path'],ref['path'].replace('/','\\')]
            require(any(proofs.get(key)==ref['sha256'] for key in keys_to_try),'ROOT certificate does not pin gate evidence '+field)
        return {'id':spec['id'],'profile':profile,'freshRunnerVerified':False,'historicalRootAttestationVerified':True}
    binary=evidence.ref(spec['binary'])
    if profile=='root-attested-analyzer':
        cert=evidence.json(spec['attestation'])
        require(cert.get('accepted') is True and cert.get('sourceFreezeSHA256')==plan['sourceFreeze']['sha256']
                and cert.get('analyzerSHA256')==spec['binary']['sha256'],'analyzer lacks accepted source/binary binding')
        return {'id':spec['id'],'profile':profile,'binarySHA256':digest(binary),'freshRunnerVerified':False,
                'historicalRootAttestationVerified':True}
    require(profile in ['runtime','parser','contract'],'unsupported artifact profile')
    require(plan['sourceMode']=='live','official current-source fingerprint requires live mode; archive/attest history explicitly')
    expected_script={'runtime':'scripts/run-runtime-fixture.ps1','parser':'scripts/run-parser-fixture.ps1','contract':'scripts/run-contract-fixture.ps1'}[profile]
    require(evidence.ref(spec['wrapperScript'])==evidence.path(expected_script),'wrong official runner wrapper')
    require(evidence.ref(spec['commonScript'])==evidence.path('scripts/runner-fingerprint.ps1'),'wrong official fingerprint helper')
    metadata=evidence.json(spec['metadata'])
    require(metadata.get('executable_hash')==digest(binary),'binary/metadata hash mismatch')
    response,seconds=official_helper(evidence,work,'RunnerFingerprint',{'profile':profile})
    require(response['fingerprint']==metadata.get('input_fingerprint'),'official current-source fingerprint mismatch')
    # Parser v1 metadata intentionally stores the aggregate fingerprint without
    # a separate file count. Its official enumeration still supplies the count
    # and every input must match the complete source snapshot below.
    legacy_parser_count = (
        profile == 'parser'
        and metadata.get('schema_version') == 'deepdeck-parser-runner-fingerprint/v1'
        and 'input_file_count' not in metadata
    )
    if not legacy_parser_count:
        require(type(metadata.get('input_file_count')) is int
                and response['inputCount']==metadata['input_file_count'],
                'official runner input count mismatch')
    evidence.hash_map(response['inputs'])
    for path,actual in response['inputs'].items():
        require(path in snapshot['sources'],'official artifact input missing from complete checkpoint source freeze: '+path)
        require(snapshot['sources'][path]==actual,'artifact input differs from checkpoint source epoch')
    return {'id':spec['id'],'profile':profile,'binarySHA256':digest(binary),'fingerprint':response['fingerprint'],
            'sourceInputCount':len(response['inputs']),'sourceInputs':response['inputs'],
            'inputCountSource':'official enumeration (parser v1 metadata omits count)' if legacy_parser_count else 'official enumeration and metadata',
            'helperSeconds':seconds,'freshRunnerVerified':True}


def validate_ledger(evidence, spec, corpora, work, audit_script):
    keys(spec,['before','after','appendOracleIds','repinOracleIds','corpus','beforeAudit','fixtureHashes'])
    before=evidence.json(spec['before']);after=evidence.json(spec['after'])
    require(before.get('schema_version')=='deepdeck-native-card-proofs/v1' and after.get('schema_version')==before['schema_version'],'wrong proof ledger schema')
    aa=before['cards'];bb=after['cards'];require(isinstance(aa,list) and isinstance(bb,list),'ledger cards must be arrays')
    require(len({row['oracle_id'] for row in aa})==len(aa) and len({row['oracle_id'] for row in bb})==len(bb),'duplicate ledger IDs')
    require(len(bb)==len(aa)+len(spec['appendOracleIds']),'ledger is not declared append-only')
    require([row['oracle_id'] for row in bb[:len(aa)]]==[row['oracle_id'] for row in aa],'ledger prefix/order drift')
    require([row['oracle_id'] for row in bb[len(aa):]]==spec['appendOracleIds'],'ledger appended identities differ')
    repins=set(spec['repinOracleIds']);require(len(repins)==len(spec['repinOracleIds']),'duplicate declared repin')
    changed=set()
    for old,new in zip(aa,bb):
        if old['oracle_id'] in repins:
            left={k:v for k,v in old.items() if k!='expected_semantic_ir_digest'}
            right={k:v for k,v in new.items() if k!='expected_semantic_ir_digest'}
            require(typed(left)==typed(right),'repin changed proof specification or name')
            require(old['expected_semantic_ir_digest']!=new['expected_semantic_ir_digest'],'declared repin did not change pin')
            changed.add(old['oracle_id'])
        else:require(typed(old)==typed(new),'undeclared prior ledger row change')
    require(changed==repins,'declared repin absent from old prefix')
    require(spec['corpus'] in corpora,'ledger corpus missing')
    current={row['oracle_id']:row for row in corpora[spec['corpus']]}
    old_audit={row['oracle_id']:row for row in evidence.jsonl(spec['beforeAudit'])}
    entries=[]
    for row in aa:
        oracle_id=row['oracle_id'];require(oracle_id in old_audit,'old ledger ID missing from before audit')
        card=old_audit[oracle_id]
        require(card.get('status')=='canonical' and card.get('abilities') and all(map(native,card['abilities'])),'old ledger card not fully native in actual before audit')
        entries.append({'oracleId':oracle_id,'phase':'before','rulesJson':json.dumps([a['rule'] for a in card['abilities']],sort_keys=True,ensure_ascii=False,separators=(',',':'),allow_nan=False)})
    for row in bb:
        oracle_id=row['oracle_id'];require(oracle_id in current,'ledger ID missing from actual audit')
        card=current[oracle_id]
        require(card.get('status')=='canonical' and card.get('abilities') and all(map(native,card['abilities'])),'ledger card not fully native in actual audit')
        entries.append({'oracleId':oracle_id,'phase':'after','rulesJson':json.dumps([a['rule'] for a in card['abilities']],sort_keys=True,ensure_ascii=False,separators=(',',':'),allow_nan=False)})
    require(evidence.ref(audit_script)==evidence.path('scripts/audit-card-set.ps1'),'wrong official semantic digest implementation')
    response,seconds=official_helper(evidence,work,'SemanticPins',{'entries':entries})
    actual={(row['phase'],row['oracleId']):row['digest'] for row in response['pins']}
    for phase,rows in [('before',aa),('after',bb)]:
        for row in rows:require(actual[(phase,row['oracle_id'])]==row['expected_semantic_ir_digest'],'official semantic IR pin mismatch: '+phase+' '+row['oracle_id'])
    fixture_hashes=evidence.json(spec['fixtureHashes']);evidence.hash_map(fixture_hashes)
    fixture_count=0
    for card in bb:
        for kind in ['contract','runtime']:
            proofs=card[kind] if isinstance(card[kind],list) else [card[kind]]
            require(bool(proofs),'empty card proof list')
            for proof in proofs:
                require(isinstance(proof,dict) and bool(proof),'empty proof specification')
                path=proof.get('fixture_path')
                if path is not None:
                    require(path in fixture_hashes,'proof fixture lacks explicit hash '+path)
                    fixture=evidence.json({'path':path,'sha256':fixture_hashes[path]})
                    require(fixture.get('id')==proof.get('fixture_id'),'proof fixture ID mismatch')
                    fixture_count+=1
                else:
                    require(isinstance(proof.get('test_name'),str) and proof['test_name'],'unsupported legacy proof shape')
    return {'beforeCards':len(aa),'afterCards':len(bb),'appended':spec['appendOracleIds'],'repinned':sorted(repins),
            'officialPinsVerified':len(aa)+len(bb),'proofFixtureReferencesVerified':fixture_count,'officialHelperSeconds':seconds},after


def validate(evidence, plan, work):
    keys(plan,['schemaVersion','id','parent','sourceFreeze','sourceMode','artifacts','gates','corpora','allowedIRChanges',
               'setCorpora','setsExpected','ledger','officialDigestScript','knownGaps','notes'],['historicalCertificate'])
    require(plan['schemaVersion']=='deepdeck-checkpoint-plan/v1','wrong plan schema')
    require(isinstance(plan['id'],str) and bool(plan['id']),'missing checkpoint ID')
    parent=evidence.json(plan['parent']);require(parent.get('accepted') is True,'parent checkpoint is not accepted')
    snapshot=source_epoch(evidence,plan)
    require(isinstance(plan['artifacts'],list) and bool(plan['artifacts']),'no artifact provenance')
    artifacts={};artifact_specs={}
    for spec in plan['artifacts']:
        require(spec['id'] not in artifacts,'duplicate artifact ID')
        artifacts[spec['id']]=validate_artifact(evidence,spec,plan,snapshot,work)
        artifact_specs[spec['id']]=spec
    changes=load_changes(evidence,plan['allowedIRChanges']);seen_changes=set();comparisons=[];corpora={}
    for spec in plan['corpora']:
        require(spec['id'] not in corpora,'duplicate corpus ID')
        require(spec.get('artifactId') in artifacts,'corpus lacks stable analyzer provenance')
        result,after,used=compare_corpus(evidence,spec,changes)
        comparisons.append(result);corpora[spec['id']]=after;seen_changes|=used
    require(seen_changes==set(changes),'unused IR exception: explicit exact exceptions must be exercised')
    totals=set_totals(corpora,plan['setCorpora'],plan['setsExpected'])
    ledger_result,ledger=validate_ledger(evidence,plan['ledger'],corpora,work,plan['officialDigestScript'])
    require(isinstance(plan['gates'],list) and bool(plan['gates']),'no execution gates')
    gates=[];ids=set()
    for gate in plan['gates']:
        require(gate['id'] not in ids,'duplicate gate ID');ids.add(gate['id'])
        if gate['format'] in ['parser-log','contract-log','runtime-log']:
            require(gate.get('artifactId') in artifacts,'DATA gate lacks artifact provenance')
            artifact=artifacts[gate['artifactId']];artifact_spec=artifact_specs[gate['artifactId']]
            if artifact['profile']=='root-attested-gate':
                require(typed(gate['evidence'])==typed(artifact_spec['gateLog'])
                        and typed(gate.get('wrapperReport'))==typed(artifact_spec['gateReport']), 'gate evidence is not bound to its ROOT attestation')
            else:
                require(artifact['profile']=={'parser-log':'parser','contract-log':'contract','runtime-log':'runtime'}[gate['format']], 'wrong runner artifact for gate')
                require('wrapperReport' in gate,'live DATA gate needs its wrapper execution report')
                report=evidence.json(gate['wrapperReport'])
                require(report.get('input_fingerprint')==artifact.get('fingerprint'),'gate report used a different source/binary fingerprint')
        gates.append(validate_gate(evidence,gate,ledger))
    gaps=[]
    for gap in plan['knownGaps']:
        keys(gap,['id','description','blocking','evidence'])
        require(type(gap['blocking']) is bool and isinstance(gap['description'],str) and bool(gap['description']),'invalid known gap')
        evidence.ref(gap['evidence']);gaps.append(gap)
    evidence.final_check()
    fresh=plan['sourceMode']=='live' and all(artifact.get('freshRunnerVerified') is True for artifact in artifacts.values())
    return {'schemaVersion':'deepdeck-checkpoint-draft/v1','id':plan['id'],'accepted':False,'goalComplete':False,
        'rootApprovalRequired':True,'evidenceValidation':'passed','freshCandidateEpochVerified':fresh,
        'readyForRootReview':True,'readyForRootAcceptance':fresh and not any(gap['blocking'] for gap in gaps),
        'sourceInputs':len(snapshot['sources']),'fixtureEpochInputs':len(snapshot.get('fixtureInputs',{})),
        'parent':plan['parent'],'sourceFreeze':plan['sourceFreeze'],'artifacts':list(artifacts.values()),
        'gates':gates,'corpora':comparisons,'sets':totals,'proofLedger':ledger_result,'knownGaps':gaps,
        'limits':['Draft only: root must review and explicitly accept proofs; no authoritative files or pointers are modified.',
                  'A successful recorded gate does not prove absence of untested semantic defects.',
                  'Historical accepted reports are distinct from revalidated current binary/source fingerprints.',
                  'No general runtime or compilation speed improvement is inferred from certification.'],
        'notes':plan['notes']}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo',type=Path,default=Path.cwd())
    parser.add_argument('--manifest',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args();evidence=Evidence(args.repo);output=fresh_output(evidence.repo,args.output)
    manifest=args.manifest.resolve();require(manifest.is_relative_to(evidence.repo),'manifest outside repository')
    plan=strict_load(manifest.read_text(encoding='utf-8-sig'))
    evidence.seen[manifest]=digest(manifest)
    output.mkdir(parents=True)
    work=output/'helper-evidence';work.mkdir()
    started=time.perf_counter()
    try:
        result=validate(evidence,plan,work);exit_code=0
    except (EvidenceError,KeyError,TypeError,ValueError,OSError) as error:
        result={'schemaVersion':'deepdeck-checkpoint-draft/v1','id':plan.get('id'),'accepted':False,
            'rootApprovalRequired':True,'evidenceValidation':'failed','readyForRootReview':False,
            'readyForRootAcceptance':False,'error':str(error)};exit_code=2
    result['manifestSHA256']=digest(manifest)
    result['validationSeconds']=time.perf_counter()-started
    result['cargoInvoked']=False;result['authoritativeFilesChanged']=False
    (output/'checkpoint-draft.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps({key:result[key] for key in ['accepted','evidenceValidation','readyForRootAcceptance','validationSeconds']}))
    return exit_code


if __name__=='__main__':
    raise SystemExit(main())

"""Capture the union of OFFICIAL wrapper inputs; no alternative fingerprint algorithm."""
from pathlib import Path
import argparse,json,tempfile
from checkpoint import official_helper
from checkpoint_core import Evidence,digest,fresh_output,require


def collect_complete_source_snapshot(evidence,work,additional_sources=None,fixture_inputs=None):
    sources={};profiles={}
    for profile in ('parser','contract','runtime'):
        response,seconds=official_helper(evidence,work,'RunnerFingerprint',{'profile':profile})
        require(isinstance(response.get('inputs'),dict) and bool(response['inputs']),'empty official input enumeration')
        for path,value in response['inputs'].items():
            require(path not in sources or sources[path]==value,'source changed between official wrapper enumerations: '+path)
            sources[path]=value
        profiles[profile]={'fingerprint':response['fingerprint'],'inputCount':response['inputCount'],'helperSeconds':seconds}
    for path,value in (additional_sources or {}).items():
        require(path not in sources or sources[path]==value,'additional source differs from wrapper source: '+path)
        sources[path]=value
    evidence.hash_map(sources)
    if fixture_inputs:evidence.hash_map(fixture_inputs)
    evidence.final_check()
    return {'schemaVersion':'deepdeck-complete-source-snapshot/v1','sources':dict(sorted(sources.items())),
        'fixtureInputs':dict(sorted((fixture_inputs or {}).items())),'officialProfiles':profiles,
        'scope':'Current inputs only; this cannot establish historical bytes or prove tests ran.'}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo',default=str(Path.cwd()));parser.add_argument('--output',required=True)
    parser.add_argument('--additional-sources',help='Explicit JSON path/hash map, e.g. analyzer-specific inputs')
    parser.add_argument('--fixture-inputs',help='Explicit JSON path/hash map of the fixtures about to execute')
    args=parser.parse_args();evidence=Evidence(args.repo);output=fresh_output(evidence.repo,args.output)
    output.mkdir(parents=True)
    def load_map(path):return json.loads(Path(path).read_text(encoding='utf-8-sig')) if path else None
    snapshot=collect_complete_source_snapshot(evidence,output,load_map(args.additional_sources),load_map(args.fixture_inputs))
    path=output/'source-snapshot.json';path.write_text(json.dumps(snapshot,indent=2)+'\n',encoding='utf-8')
    print(json.dumps({'path':str(path),'sha256':digest(path),'sourceInputs':len(snapshot['sources']),'cargoInvocations':0}))

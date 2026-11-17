from pathlib import Path
import copy,importlib.util,json,subprocess,sys,tempfile,unittest
from unittest.mock import patch
HERE=Path(__file__).resolve().parent
ROOT=HERE.parent
REPO=ROOT.parent.parent
sys.path.insert(0,str(ROOT/'tool'))
from checkpoint_core import Evidence,EvidenceError,digest,fresh_output,strict_load,typed
from checkpoint_gates import validate_gate
from checkpoint_ir import compare_corpus,set_totals
from checkpoint import official_helper,validate_ledger,validate_artifact
from collect_complete_source_snapshot import collect_complete_source_snapshot


class CheckpointTests(unittest.TestCase):
    def artifact_with_count(self,profile,metadata_count,include_count=True):
        binary=self.ref('artifact.bin',{'test':'binary'})
        source=self.ref('artifact-input.rs',{'test':'source'})
        metadata={'schema_version':f'deepdeck-{profile}-runner-fingerprint/v1',
                  'executable_hash':binary['sha256'],'input_fingerprint':'F'*64}
        if include_count:metadata['input_file_count']=metadata_count
        spec={'id':profile,'profile':profile,'binary':binary,'metadata':self.ref('metadata.json',metadata),
              'wrapperScript':{'path':f'scripts/run-{profile}-fixture.ps1','sha256':digest(REPO/f'scripts/run-{profile}-fixture.ps1')},
              'commonScript':{'path':'scripts/runner-fingerprint.ps1','sha256':digest(REPO/'scripts/runner-fingerprint.ps1')}}
        response={'fingerprint':'F'*64,'inputCount':1,'inputs':{source['path']:source['sha256']}}
        with patch('checkpoint.official_helper',return_value=(response,0)):
            return validate_artifact(self.evidence,spec,{'sourceMode':'live'},
                                     {'sources':response['inputs']},self.folder)

    def test_parser_v1_metadata_omits_count_but_official_inputs_are_verified(self):
        result=self.artifact_with_count('parser',None,False)
        self.assertTrue(result['freshRunnerVerified'])
        self.assertEqual(result['sourceInputCount'],1)
        self.assertIn('parser v1 metadata omits count',result['inputCountSource'])

    def test_runtime_metadata_cannot_omit_its_required_count(self):
        with self.assertRaisesRegex(EvidenceError,'input count'):
            self.artifact_with_count('runtime',None,False)

    def test_present_parser_count_must_match_official_enumeration(self):
        with self.assertRaisesRegex(EvidenceError,'input count'):
            self.artifact_with_count('parser',2)

    def test_metadata_boolean_is_not_an_integer_input_count(self):
        with self.assertRaisesRegex(EvidenceError,'input count'):
            self.artifact_with_count('runtime',True)

    def setUp(self):
        sandbox=REPO/'.tmp/checkpoint-test-sandboxes';sandbox.mkdir(parents=True,exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='checkpoint-',dir=sandbox)
        self.folder=Path(self.temp.name).resolve();self.evidence=Evidence(REPO)
    def tearDown(self):self.temp.cleanup()
    def ref(self,name,value,jsonl=False):
        path=self.folder/name
        data='\n'.join(json.dumps(row) for row in value) if jsonl else json.dumps(value)
        path.write_bytes(data.encode('utf-8'))
        return {'path':path.relative_to(REPO).as_posix(),'sha256':digest(path)}
    def text(self,name,value):
        path=self.folder/name;path.write_bytes(value.encode('utf-8'))
        return {'path':path.relative_to(REPO).as_posix(),'sha256':digest(path)}
    def gate(self,fmt,rows,expected=None,**other):
        return {'id':'test','format':fmt,'evidence':self.ref('gate.jsonl',rows,True),'expected':expected or {'passed':1},**other}
    def parser_rows(self):
        return [{'type':'caseResult','fixturePath':'fixture.json','caseId':'a','status':'passed'},
            {'type':'summary','caseCount':1,'passed':1,'failed':0,'invalidFixtures':0,'status':'passed','parserMode':'nativeOnly'}]
    def corpus(self,value=True):
        return {'oracle_id':'id','set':'tst','collector_number':'1','name':'Card','status':'canonical',
            'abilities':[{'status':'canonical','recognizer':'native','text':'Flying',
                'rule':{'kind':'keywordAbility','source':{'kind':'self'},'ability':{'kind':'flying'},'probe':value}}]}
    def corpus_spec(self,before,after):
        return {'id':'global','before':self.ref('before.jsonl',[before],True),'after':self.ref('after.jsonl',[after],True),
            'input':self.ref('input.json',[{'oracle_id':'id','set':'tst','collector_number':'1','name':'Card','oracle_text':'Flying'}])}

    def test_duplicate_json_keys_are_not_silently_overwritten(self):
        with self.assertRaises(EvidenceError):strict_load('{"passed":false,"passed":true}')
    def test_complete_snapshot_unions_official_profiles_without_fixed_count(self):
        one=self.ref('one.rs',{'source':1});two=self.ref('two.rs',{'source':2})
        responses=[({'inputs':{one['path']:one['sha256']},'fingerprint':'p','inputCount':1},0),
            ({'inputs':{two['path']:two['sha256']},'fingerprint':'c','inputCount':1},0),
            ({'inputs':{one['path']:one['sha256'],two['path']:two['sha256']},'fingerprint':'r','inputCount':2},0)]
        with patch('collect_complete_source_snapshot.official_helper',side_effect=responses):
            result=collect_complete_source_snapshot(self.evidence,self.folder)
        self.assertEqual(len(result['sources']),2)
    def test_complete_snapshot_rejects_cross_profile_mutation(self):
        one=self.ref('one.rs',{'source':1})
        responses=[({'inputs':{one['path']:one['sha256']},'fingerprint':'p','inputCount':1},0),
            ({'inputs':{one['path']:'0'*64},'fingerprint':'c','inputCount':1},0)]
        with patch('collect_complete_source_snapshot.official_helper',side_effect=responses):
            with self.assertRaisesRegex(EvidenceError,'changed between'):collect_complete_source_snapshot(self.evidence,self.folder)
    def test_case_and_json_type_are_exact(self):
        self.assertNotEqual(typed({'x':True}),typed({'x':1}))
        self.assertNotEqual(typed({'x':1}),typed({'x':1.0}))
        self.assertNotEqual(typed({'counter':'Quest'}),typed({'counter':'quest'}))
    def test_hash_mutation_fails(self):
        ref=self.ref('evidence.json',{'passed':True});(REPO/ref['path']).write_text('{}',encoding='utf-8')
        with self.assertRaisesRegex(EvidenceError,'hash mismatch'):self.evidence.ref(ref)
    def test_evidence_mutated_after_first_read_fails_final_check(self):
        ref=self.ref('evidence.json',{'count':1});self.evidence.json(ref)
        (REPO/ref['path']).write_text('{}',encoding='utf-8')
        with self.assertRaises(EvidenceError):self.evidence.final_check()
    def test_output_cannot_overwrite_or_escape_temporary_workspace(self):
        with self.assertRaises(EvidenceError):fresh_output(REPO,'docs/proof.json')
        with self.assertRaises(EvidenceError):fresh_output(REPO,self.folder)
    def test_zero_rust_tests_do_not_pass(self):
        gate={'id':'zero','format':'rust-test-log','evidence':self.text('rust.log','test result: ok. 0 passed; 0 failed; 0 ignored;\n'),'expected':{'passed':0}}
        with self.assertRaisesRegex(EvidenceError,'executed tests'):validate_gate(self.evidence,gate)
    def test_interleaved_named_rows_do_not_override_positive_harness_count(self):
        gate={'id':'interleaved','format':'rust-test-log','evidence':self.text('rust.log','test first ... ok\ninterleaved second output\ntest result: ok. 2 passed; 0 failed; 0 ignored;\n'),'expected':{'passed':2}}
        self.assertEqual(validate_gate(self.evidence,gate)['counts']['passed'],2)
    def test_powershell_utf16_bom_transcript_is_read_without_changing_source_encoding_rules(self):
        path=self.folder/'rust.log';path.write_bytes('test result: ok. 1 passed; 0 failed; 0 ignored;\n'.encode('utf-16'))
        ref={'path':path.relative_to(REPO).as_posix(),'sha256':digest(path)}
        result=validate_gate(self.evidence,{'id':'bom','format':'rust-test-log','evidence':ref,'expected':{'passed':1}})
        self.assertEqual(result['counts']['passed'],1)
        with self.assertRaises(EvidenceError):self.evidence.text(ref,log=False)
    def test_passed_flag_without_rust_results_is_not_evidence(self):
        gate={'id':'flag','format':'rust-test-log','evidence':self.text('rust.log','passed=true\n'),'expected':{'passed':1}}
        with self.assertRaises(EvidenceError):validate_gate(self.evidence,gate)
    def test_runtime_assertion_failure_is_not_hidden_by_passed_row(self):
        row={'schemaVersion':'deepdeck-runtime-fixture-result/v1','fixtureId':'x','status':'passed','assertions':[{'passed':False}]}
        with self.assertRaises(EvidenceError):validate_gate(self.evidence,self.gate('runtime-log',[row]))
    def test_zero_runtime_assertions_do_not_pass(self):
        row={'schemaVersion':'deepdeck-runtime-fixture-result/v1','fixtureId':'x','status':'passed','assertions':[]}
        with self.assertRaises(EvidenceError):validate_gate(self.evidence,self.gate('runtime-log',[row]))
    def test_runtime_counts_are_derived_from_actual_rows(self):
        row={'schemaVersion':'deepdeck-runtime-fixture-result/v1','fixtureId':'x','status':'passed','assertions':[{'passed':True},{'passed':True}]}
        result=validate_gate(self.evidence,self.gate('runtime-log',[row],{'passed':1,'assertions':2}))
        self.assertEqual(result['counts'],{'passed':1,'assertions':2})
    def test_fixture_error_cannot_be_hidden_by_summary(self):
        rows=self.parser_rows();rows.insert(0,{'type':'fixtureError','error':'bad expectation'})
        with self.assertRaisesRegex(EvidenceError,'fixtureError'):validate_gate(self.evidence,self.gate('parser-log',rows,mode='nativeOnly'))
    def test_parser_summary_must_match_real_cases(self):
        rows=self.parser_rows();rows[-1]['caseCount']=2
        with self.assertRaises(EvidenceError):validate_gate(self.evidence,self.gate('parser-log',rows,mode='nativeOnly'))
    def test_parser_mode_is_not_inferred_from_success(self):
        with self.assertRaisesRegex(EvidenceError,'mode'):validate_gate(self.evidence,self.gate('parser-log',self.parser_rows(),mode='full'))
    def test_valid_parser_case_is_counted(self):
        result=validate_gate(self.evidence,self.gate('parser-log',self.parser_rows(),mode='nativeOnly'))
        self.assertEqual(result['counts']['passed'],1)
    def test_inconsistent_set_partition_is_rejected(self):
        rows={'set':[self.corpus()]}
        expected={'occurrences':1,'partition':{'completeNonempty':0,'partial':0,'none':1,'emptyCanonical':0,'emptyUnsupported':0},
            'uniqueOracleIds':1,'uniquePartition':{'completeNonempty':1,'partial':0,'none':0,'emptyCanonical':0,'emptyUnsupported':0}}
        with self.assertRaises(EvidenceError):set_totals(rows,['set'],expected)
    def test_set_occurrences_and_dedup_are_separate(self):
        result=set_totals({'a':[self.corpus()],'b':[self.corpus()]},['a','b'])
        self.assertEqual(result['occurrences'],2);self.assertEqual(result['uniqueOracleIds'],1)
    def test_unapproved_bool_to_integer_ir_change_is_rejected(self):
        with self.assertRaisesRegex(EvidenceError,'unapproved'):compare_corpus(self.evidence,self.corpus_spec(self.corpus(True),self.corpus(1)),{})
    def test_allowlist_before_must_be_typed_exact(self):
        old=self.corpus(True);new=self.corpus(2)
        changes={('id',0):{'beforeRule':self.corpus(1)['abilities'][0]['rule'],'afterRule':new['abilities'][0]['rule'],'afterStatus':'canonical'}}
        with self.assertRaisesRegex(EvidenceError,'BEFORE'):compare_corpus(self.evidence,self.corpus_spec(old,new),changes)
    def test_exact_rewrite_is_counted_without_global_normalization(self):
        old=self.corpus(True);new=self.corpus(2)
        changes={('id',0):{'beforeRule':old['abilities'][0]['rule'],'afterRule':new['abilities'][0]['rule'],'afterStatus':'canonical'}}
        result,_,_=compare_corpus(self.evidence,self.corpus_spec(old,new),changes)
        self.assertEqual(result['intendedRewrites'],1)
    def test_strict_gap_is_rejected_despite_complete_flag(self):
        report={'complete':True,'cards':[{'oracle_id':'id','parser_status':'canonical','recognizer_status':'native',
            'migration_status':'native-primitive-executable','ir_status':'primitive','contract_status':'executable',
            'runtime_status':'passed','proof_status':'verified','gap':True,'unexecutable_rule_count':0}]}
        gate={'id':'strict','format':'strict-audit','evidence':self.ref('strict.json',report),'expected':{'cards':1}}
        with self.assertRaisesRegex(EvidenceError,'gap'):validate_gate(self.evidence,gate)
    def test_ledger_prior_row_mutation_cannot_hide_in_append(self):
        old={'schema_version':'deepdeck-native-card-proofs/v1','cards':[{'oracle_id':'id','name':'Card','expected_semantic_ir_digest':'A'*64}]}
        new=copy.deepcopy(old);new['cards'][0]['name']='card'
        spec={'before':self.ref('ledger-before.json',old),'after':self.ref('ledger-after.json',new),'appendOracleIds':[],'repinOracleIds':[],
              'corpus':'global','beforeAudit':self.ref('old.jsonl',[self.corpus()],True),'fixtureHashes':self.ref('fixtures.json',{'x':'A'*64})}
        with self.assertRaisesRegex(EvidenceError,'undeclared prior ledger'):validate_ledger(self.evidence,spec,{'global':[self.corpus()]},self.folder,{})
    def test_official_powershell_semantic_pin_distinguishes_case_and_type(self):
        entries=[{'oracleId':str(i),'phase':'test','rulesJson':json.dumps([{'kind':'gainLife','probe':value}])} for i,value in enumerate([True,1,'Quest','quest'])]
        result,_=official_helper(self.evidence,self.folder,'SemanticPins',{'entries':entries})
        self.assertEqual(result['implementation'],'scripts/audit-card-set.ps1::Get-SemanticIrDigest')
        self.assertEqual(len({row['digest'] for row in result['pins']}),4)
    def test_official_runtime_fingerprint_enumerates_real_inputs_without_cargo(self):
        response,_=official_helper(self.evidence,self.folder,'RunnerFingerprint',{'profile':'runtime'})
        self.assertEqual(response['inputCount'],len(response['inputs']))
        self.assertGreater(response['inputCount'],0)
        self.assertIn('src/engine/color_set_query.rs',response['inputs'])
        self.assertEqual(len(response['fingerprint']),64)
    def test_official_profiles_track_shared_domain_and_vocabulary(self):
        required = ['crates/deepdeck-card-domain/Cargo.toml',
                    'data/oracle-vocabulary/creature-subtypes-cr-205.3m-2026-11-06.json',
                    'data/oracle-vocabulary/noncreature-subtypes-cr-205.3-2026-11-06.json',
                    'data/oracle-vocabulary/subtype-orthographic-aliases-2026-11-15.json']
        combined = {}
        for profile in ('parser', 'runtime', 'contract'):
            response, _ = official_helper(self.evidence, self.folder, 'RunnerFingerprint', {'profile': profile})
            self.assertEqual(response['inputCount'], len(response['inputs']))
            for path in required:
                self.assertIn(path, response['inputs'])
            self.assertTrue(any(path.startswith('crates/deepdeck-card-domain/src/') and path.endswith('.rs')
                                for path in response['inputs']))
            for path, digest_value in response['inputs'].items():
                if path in combined:
                    self.assertEqual(combined[path], digest_value)
                combined[path] = digest_value

    def test_powershell_helper_rejects_authoritative_output_path(self):
        request=self.folder/'request.json';request.write_text('{"entries":[]}',encoding='utf-8')
        result=subprocess.run(['powershell','-NoLogo','-NoProfile','-File',str(ROOT/'tool/Invoke-OfficialCheckpointHelpers.ps1'),
            '-Mode','SemanticPins','-RepoRoot',str(REPO),'-InputJson',str(request),'-OutputJson',str(REPO/'docs/should-never-be-written.json')],capture_output=True,text=True,encoding='utf-8')
        self.assertNotEqual(result.returncode,0);self.assertFalse((REPO/'docs/should-never-be-written.json').exists())


if __name__=='__main__':unittest.main(verbosity=2)

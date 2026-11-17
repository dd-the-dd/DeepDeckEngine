"""Compile-free integration of the architecture script and role-bounded census."""
from pathlib import Path
import json,shutil,subprocess,tempfile,unittest

ROOT=Path(__file__).resolve().parents[2]
POWERSHELL=shutil.which('powershell') or shutil.which('pwsh')

@unittest.skipUnless(POWERSHELL,'PowerShell is required for architecture integration tests')
class ArchitectureCensusIntegrationTests(unittest.TestCase):
    def setUp(self):
        scratch=ROOT/'.tmp';scratch.mkdir(exist_ok=True)
        self.temp=tempfile.TemporaryDirectory(prefix='effect-census-architecture-',dir=scratch)
        self.repo=Path(self.temp.name).resolve()
        self.assertEqual(self.repo.parent,scratch.resolve())
        (self.repo/'scripts').mkdir()
        for name in ['audit-engine-effect-kinds.py','check-card-migration-architecture.ps1','card-migration-architecture-baseline.json']:
            shutil.copyfile(ROOT/'scripts'/name,self.repo/'scripts'/name)
        (self.repo/'scripts/tests').mkdir()
        for name in ['test_audit_engine_effect_kinds.py','test_engine_effect_census_architecture.py']:
            shutil.copyfile(ROOT/'scripts/tests'/name,self.repo/'scripts/tests'/name)
        baseline=json.loads((self.repo/'scripts/card-migration-architecture-baseline.json').read_text(encoding='utf-8-sig'))
        self.retired=baseline['retiredOracleKinds'][0]
        self.write_source('src/engine/mod.rs','''
pub fn effect_supported(effect: &Value) -> bool { false }
fn execute_effect(effect: &Value) -> Result<(), Error> { Ok(()) }
fn keyword_metadata(effect: &Value) -> bool {
 match value_kind(effect) { Some("amass") => true, _ => false }
}
''')
        simple={
            'life.rs':['direct_life_effect_supported','execute_direct_life_effect'],
            'validation/damage.rs':['damage_effect_supported'],
            'validation/tokens.rs':['token_copy_effect_supported'],
            'zone_change_triggers.rs':['zone_change_effect_supported','execute_zone_change_effect'],
            'player_iteration.rs':['player_iteration_supported','execute_player_iteration'],
            'ability_signals.rs':['ability_signal_effect_supported','execute_ability_signal_effect'],
            'sacrifice.rs':['explicit_object_sacrifice_supported','execute_explicit_object_sacrifice'],
            'subtypes.rs':['effect_supported'],
        }
        for path,functions in simple.items():
            self.write_source('src/engine/'+path,'\n'.join('fn '+fn+'(effect: &Value) -> bool { match value_kind(effect) { _ => false } }' for fn in functions))

    def tearDown(self):
        self.temp.cleanup()

    def write_source(self,path,text):
        target=self.repo/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(text,encoding='utf-8')

    def handlers(self,kind=None):
        literal=json.dumps(kind) if kind else None
        catalog='Some('+literal+') => Some(Domain::Fixture),' if kind else ''
        validator='Some('+literal+') => true,' if kind else ''
        executor='Some('+literal+') => {},' if kind else ''
        self.write_source('src/engine/effect_domains/mod.rs','fn domain(effect: &Value) -> Option<Domain> { match value_kind(effect) { '+catalog+' _ => None } }')
        self.write_source('src/engine/effect_domains/fixture.rs',
            'fn supports_known_effect(effect: &Value) -> bool { match value_kind(effect) { '+validator+' _ => false } }\n'
            'fn execute_fixture_domain_known(effect: &Value) -> Result<(), Error> { match value_kind(effect) { '+executor+' _ => {} } Ok(()) }')

    def run_architecture(self,label):
        report=self.repo/(label+'.json')
        command=[POWERSHELL,'-NoProfile','-ExecutionPolicy','Bypass','-File',str(self.repo/'scripts/check-card-migration-architecture.ps1'),
                 '-Enforce','-SummaryOnly','-ReportJson',str(report)]
        process=subprocess.run(command,cwd=self.repo,capture_output=True,text=True,encoding='utf-8',errors='replace')
        self.assertTrue(report.exists(),process.stdout+'\n'+process.stderr)
        return process,json.loads(report.read_text(encoding='utf-8-sig'))

    def test_retired_handler_is_rejected_even_without_serialized_kind_literal(self):
        self.handlers(self.retired)
        process,report=self.run_architecture('retired')
        self.assertEqual(process.returncode,1,process.stdout+'\n'+process.stderr)
        self.assertEqual(report['runtime_effect_census']['retiredGuard']['stillAccepted'],[self.retired])
        self.assertEqual(report['runtime_effect_census']['retiredGuard']['stillExecuted'],[self.retired])
        self.assertTrue(any('runtime effect contract' in v for v in report['violations']))
        self.assertTrue(any('runtime effect executor' in v for v in report['violations']))

    def test_metadata_is_ignored_generic_additions_allowed_and_helper_invalidates_cache(self):
        self.handlers()
        process,first=self.run_architecture('first')
        self.assertEqual(process.returncode,0,process.stdout+'\n'+process.stderr)
        self.assertFalse(first['cache_hit'])
        self.assertEqual(first['runtime_effect_census']['retiredGuard']['stillAccepted'],[])
        process,cached=self.run_architecture('cached')
        self.assertEqual(process.returncode,0,process.stdout+'\n'+process.stderr)
        self.assertTrue(cached['cache_hit'])
        helper=self.repo/'scripts/audit-engine-effect-kinds.py'
        helper.write_text(helper.read_text(encoding='utf-8')+'\n# Cache-input mutation test.\n',encoding='utf-8')
        process,changed=self.run_architecture('helper-changed')
        self.assertEqual(process.returncode,0,process.stdout+'\n'+process.stderr)
        self.assertFalse(changed['cache_hit'])
        test_file=self.repo/'scripts/tests/test_audit_engine_effect_kinds.py'
        test_file.write_text(test_file.read_text(encoding='utf-8')+'\n# Test-input cache mutation.\n',encoding='utf-8')
        process,test_changed=self.run_architecture('test-changed')
        self.assertEqual(process.returncode,0,process.stdout+'\n'+process.stderr)
        self.assertFalse(test_changed['cache_hit'])
        self.handlers('drawCards')
        process,generic=self.run_architecture('generic-added')
        self.assertEqual(process.returncode,0,process.stdout+'\n'+process.stderr)
        self.assertEqual(generic['summary']['accepted_effect_kind_count'],first['summary']['accepted_effect_kind_count']+1)
        self.assertEqual(generic['summary']['executed_effect_kind_count'],first['summary']['executed_effect_kind_count']+1)
        self.assertNotIn('primitive IR kinds:',process.stdout)

    def test_extraction_failure_is_a_violation_and_never_a_fake_zero_census(self):
        self.handlers()
        helper=self.repo/'scripts/audit-engine-effect-kinds.py'
        helper.write_text('import sys\nprint("fixture extraction failure",file=sys.stderr)\nraise SystemExit(3)\n',encoding='utf-8')
        process,report=self.run_architecture('extractor-failed')
        self.assertEqual(process.returncode,1,process.stdout+'\n'+process.stderr)
        self.assertTrue(any('runtime effect census extraction failed' in v for v in report['violations']))
        self.assertIsNone(report['runtime_effect_census'])
        self.assertIsNone(report['summary']['accepted_effect_kind_count'])
        self.assertIsNone(report['summary']['executed_effect_kind_count'])

if __name__=='__main__':unittest.main(verbosity=2)

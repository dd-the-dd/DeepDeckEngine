"""Bounded IO/integrity tests for the proof linker, without Cargo or runtime."""
from pathlib import Path
import copy
import importlib.util
import json
import subprocess
import tempfile
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "link-native-oracle-fixtures.py"
spec = importlib.util.spec_from_file_location("native_fixture_linker", SCRIPT)
linker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(linker)


class LinkerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name)
        self.source = self.folder / "source.json"
        self.expected = self.folder / "rules.json"
        self.fixture = self.folder / "fixture.json"
        self.analyzer = self.folder / "analyzer.exe"
        self.analyzer.write_bytes(b"immutable-test-analyzer")
        self.card = {"id":"printing", "oracle_id":"oracle", "name":"Source Card",
                     "type_line":"Creature — Wizard", "mana_cost":"{2}", "power":"2", "toughness":"3",
                     "oracle_text":"First.\nSecond.", "layout":"normal", "set":"tst", "collector_number":"1"}
        self.rules = [{"kind":"activatedAbility", "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]},
                      {"kind":"activatedAbility", "effects":[{"kind":"putCounter","counter":"CaseSensitive","count":2}]}]
        self.value = {"schemaVersion":"deepdeck-runtime-fixture/v1", "id":"source-fixture", "rule":self.rules[0],
                      "initialState":{"players":[{"id":"p0", "battlefield":[{"id":"executor","name":self.card["name"],
                       "typeLine":self.card["type_line"],"manaCost":self.card["mana_cost"],"power":"2","toughness":"3","rules":self.rules}]}]},
                      "actions":[], "assertions":[]}
        linker.write_json(self.source, [self.card])
        linker.write_json(self.expected, self.rules)
        linker.write_json(self.fixture, self.value)
        self.args = linker.parser().parse_args([
            "--source-input",str(self.source),"--oracle-id","oracle","--analyzer",str(self.analyzer),
            "--expected-analyzer-sha",linker.digest(self.analyzer),"--expected-rules",str(self.expected),
            "--fixture",str(self.fixture),"--source-instance-id","executor","--output",str(self.folder/"out")])

    def emit(self, command, **kwargs):
        self.assertEqual(kwargs["env"]["DEEPDECK_ORACLE_NATIVE_ONLY"], "1")
        self.assertTrue(linker.strict_equal(linker.read_json(Path(command[1])), [self.card]))
        row = {key:self.card.get(key) for key in ("oracle_id","name","set","collector_number")}
        row.update(status="canonical", abilities=[{"status":"canonical","recognizer":"native","text":str(i),"rule":rule}
                                                  for i,rule in enumerate(self.rules)])
        Path(command[2]).write_text(json.dumps(row)+"\n",encoding="utf-8")
        return subprocess.CompletedProcess(command, 0, b"", b"")

    def run_link(self, emitter=None):
        with mock.patch.object(linker.subprocess, "run", side_effect=emitter or self.emit):
            return linker.link(self.args)

    def failure(self, code, emitter=None):
        with self.assertRaises(linker.LinkError) as error:
            self.run_link(emitter)
        self.assertEqual(error.exception.code, code)

    def test_success_preserves_inputs_and_links_all_rules_without_runtime(self):
        originals={p:p.read_bytes() for p in (self.source,self.fixture,self.expected,self.analyzer)}
        result=self.run_link()
        self.assertFalse(result["runtimeExecuted"])
        self.assertFalse(result["coverageCertified"])
        self.assertEqual(result["completeRuleCount"],2)
        output=linker.read_json(Path(result["fixtures"][0]["outputPath"]))
        self.assertTrue(linker.strict_equal(output,self.value))
        self.assertEqual(originals,{p:p.read_bytes() for p in originals})

    def test_source_changed_by_analyzer_is_rejected(self):
        def mutate(command, **kwargs):
            result=self.emit(command,**kwargs)
            self.source.write_text("[]",encoding="utf-8")
            return result
        self.failure("input_changed",mutate)

    def test_selected_input_changed_is_rejected(self):
        def mutate(command, **kwargs):
            result=self.emit(command,**kwargs)
            Path(command[1]).write_text("[]",encoding="utf-8")
            return result
        self.failure("input_changed",mutate)

    def test_preexisting_source_mutation_fails_optional_trusted_pin(self):
        self.args.expected_source_input_sha=linker.digest(self.source)
        mutated=copy.deepcopy(self.card);mutated["oracle_text"]="Mutated input."
        linker.write_json(self.source,[mutated])
        self.failure("source_hash_mismatch")

    def test_bad_alias_rejected(self):
        self.args.source_instance_id="wrong-alias"
        self.failure("source_alias_missing")

    def test_every_alias_copy_is_checked_including_hand(self):
        other=copy.deepcopy(self.value["initialState"]["players"][0]["battlefield"][0])
        other["name"]="Fake source"
        self.value["initialState"]["players"][0]["hand"]=[other]
        linker.write_json(self.fixture,self.value)
        self.failure("source_metadata_mismatch")

    def test_partial_source_rule_array_rejected(self):
        self.value["initialState"]["players"][0]["battlefield"][0]["rules"]=self.rules[:1]
        linker.write_json(self.fixture,self.value)
        self.failure("source_rules_incomplete")

    def test_modified_fixture_rule_rejected(self):
        self.value["rule"]=copy.deepcopy(self.rules[0]);self.value["rule"]["effects"]=[]
        linker.write_json(self.fixture,self.value)
        self.failure("fixture_rule_mismatch")

    def test_boolean_is_not_integer_in_expected_ir(self):
        altered=copy.deepcopy(self.rules);altered[0]["effects"][0]["amount"]=True
        linker.write_json(self.expected,altered)
        self.failure("expected_rules_mismatch")

    def test_case_is_preserved_in_expected_ir(self):
        altered=copy.deepcopy(self.rules);altered[1]["effects"][0]["counter"]="casesensitive"
        linker.write_json(self.expected,altered)
        self.failure("expected_rules_mismatch")

    def test_numeric_representation_type_is_exact(self):
        self.assertFalse(linker.strict_equal(1,1.0))
        self.assertFalse(linker.strict_equal({"A":1},{"a":1}))

    def test_other_layout_has_tool_limit_not_factory_diagnostic(self):
        self.card["layout"]="transform"
        linker.write_json(self.source,[self.card])
        self.failure("tool_layout_unsupported")

    def test_normal_with_parts_is_not_flattened(self):
        self.card["card_faces"]=[{"name":"Back"}]
        linker.write_json(self.source,[self.card])
        self.failure("tool_layout_unsupported")

    def test_duplicate_selected_oracle_identity_rejected(self):
        linker.write_json(self.source,[self.card,self.card])
        self.failure("source_identity_ambiguous")

    def test_analyzer_hash_mismatch_prevents_execution(self):
        self.args.expected_analyzer_sha="0"*64
        with mock.patch.object(linker.subprocess,"run") as process:
            with self.assertRaises(linker.LinkError):linker.link(self.args)
            process.assert_not_called()

    def test_nested_execute_rule_override_must_be_real(self):
        self.value["actions"]=[{"kind":"probeFailure","action":{"kind":"executeRule","rule":{"kind":"fake"}}}]
        linker.write_json(self.fixture,self.value)
        self.failure("fixture_rule_mismatch")

    def test_duplicate_json_keys_rejected(self):
        self.expected.write_text('[{"kind":"A","kind":"B"}]',encoding="utf-8")
        self.failure("invalid_json")

    def test_fixture_mutated_by_analyzer_is_rejected(self):
        def mutate(command, **kwargs):
            result=self.emit(command,**kwargs)
            self.fixture.write_text("{}",encoding="utf-8")
            return result
        self.failure("input_changed",mutate)

    def test_partial_expected_rule_array_rejected(self):
        linker.write_json(self.expected,self.rules[:1])
        self.failure("expected_rules_mismatch")

    def test_native_failure_diagnostics_are_not_a_complete_card(self):
        def diagnostics(command, **kwargs):
            result=self.emit(command,**kwargs)
            path=Path(command[2]);row=json.loads(path.read_text(encoding="utf-8"))
            row["abilities"][0]["diagnostics"]=[{"severity":"unsupported"}]
            path.write_text(json.dumps(row)+"\n",encoding="utf-8")
            return result
        self.failure("incomplete_native_card",diagnostics)

    def test_legacy_ability_cannot_be_linked(self):
        def legacy(command, **kwargs):
            result=self.emit(command,**kwargs)
            path=Path(command[2]);row=json.loads(path.read_text(encoding="utf-8"))
            row["abilities"][0]["recognizer"]="legacy"
            path.write_text(json.dumps(row)+"\n",encoding="utf-8")
            return result
        self.failure("incomplete_native_card",legacy)

    def test_invalid_faces_metadata_is_not_accepted_as_no_faces(self):
        self.card["card_faces"]={}
        linker.write_json(self.source,[self.card])
        self.failure("source_structure_metadata_invalid")

    def test_rejected_lot_emits_no_partial_runtime_fixture(self):
        second=self.folder/"second.json"
        bad=copy.deepcopy(self.value);bad["id"]="bad-second";bad["rule"]={"kind":"fake"}
        linker.write_json(second,bad);self.args.fixture.append(second)
        self.failure("fixture_rule_mismatch")
        self.assertFalse((self.args.output/"runtime").exists())

    def test_alias_in_unused_metadata_cannot_replace_physical_source(self):
        player=self.value["initialState"]["players"][0]
        self.value["initialState"]["unusedDefinition"]=player["battlefield"].pop()
        linker.write_json(self.fixture,self.value)
        self.failure("source_alias_missing")

    def test_player_id_collision_is_not_a_source_card_alias(self):
        self.value["initialState"]["players"][0]["id"]="executor"
        linker.write_json(self.fixture,self.value)
        self.assertEqual(self.run_link()["fixtureCount"],1)


if __name__=="__main__":
    unittest.main(verbosity=2)

from pathlib import Path
import json,unittest
import importlib.util,sys
SCRIPT=Path(__file__).parents[1]/'audit-engine-effect-kinds.py'
SPEC=importlib.util.spec_from_file_location('audit_engine_effect_kinds',SCRIPT)
MODULE=importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name]=MODULE
SPEC.loader.exec_module(MODULE)
RustSource=MODULE.RustSource
retired_list=MODULE.retired_list
role_conformance=MODULE.role_conformance

class RoleBoundedExtractionTests(unittest.TestCase):
    def test_metadata_strings_and_unrelated_functions_are_not_handlers(self):
        source=RustSource('synthetic.rs',b'''
fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("drawCards") => {
            match value_kind(&effect["metadata"]) {
                Some("retiredSpecialization") => false,
                _ => true,
            }
        }
        _ => false,
    }
}
fn keyword_metadata(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("retiredSpecialization") => true,
        _ => false,
    }
}
''')
        arms=source.arms('supports_known_effect','domain-validator-branch')
        self.assertEqual([a['kinds'] for a in arms],[['drawCards']])
        self.assertEqual(arms[0]['actualRole'],'domain-validator-branch')

    def test_guard_literals_are_not_effect_kinds(self):
        source=RustSource('synthetic.rs',b'''
fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("conditional" | "conditionalEffect") if effect["metadataKind"] == "retired" => true,
        _ => false,
    }
}
''')
        self.assertEqual(source.arms('supports_known_effect','validator')[0]['kinds'],['conditional','conditionalEffect'])

    def test_comments_raw_strings_and_character_braces_preserve_boundaries(self):
        source=RustSource('synthetic.rs',b'''
fn execute_cards_domain_known(effect: &Value) -> Result<(), Error> {
    match value_kind(effect) {
        Some("drawCards") => {
            let ignored = r###" } Some(\"retired\") => { "###;
            let brace = '}';
            /* ignored /* nested */ } */
            return Ok(());
        }
        Some("moveCards") => Ok(()),
        _ => Err(Error),
    }
}
''')
        arms=source.arms('execute_cards_domain_known','executor')
        self.assertEqual([a['kinds'] for a in arms],[['drawCards'],['moveCards']])
        self.assertIn('return Ok(())',arms[0]['body'])

    def test_delegated_location_points_to_fn_keyword_not_blank_line(self):
        source=RustSource('synthetic.rs',b'\n\n// doc\npub(super) fn selective(effect: &Value) -> Option<bool> {\n None\n}\n')
        site=source.whole_function('selective',['emitAbilitySignal'],'delegated-validator')
        self.assertEqual(site['line'],4)

    def test_only_known_function_match_is_read(self):
        source=RustSource('synthetic.rs',b'''
fn supports_known_effect(effect: &Value) -> bool {
    let descriptor = match value_kind(&effect["ability"]) {
        Some("keyword") => true,
        _ => false,
    };
    match value_kind(effect) {
        Some("createTokens") => descriptor,
        _ => false,
    }
}
''')
        self.assertEqual(source.arms('supports_known_effect','validator')[0]['kinds'],['createTokens'])

    def test_retired_definition_is_explicit_and_rejects_duplicates(self):
        self.assertEqual(retired_list(Path('unused'),b'{"retiredOracleKinds":["retired"]}'),['retired'])
        self.assertEqual(retired_list(Path('unused'),b'["retired"]'),['retired'])
        with self.assertRaises(ValueError):retired_list(Path('unused'),b'{"keywords":["retired"]}')
        with self.assertRaises(ValueError):retired_list(Path('unused'),b'["retired","retired"]')

    def test_new_root_delegation_fails_closed_until_registered(self):
        source=RustSource('src/engine/mod.rs',b'''
fn effect_supported(effect: &Value) -> bool { new_domain::check(effect) }
fn execute_effect(effect: &Value) -> Result<(), Error> { Ok(()) }
''')
        with self.assertRaisesRegex(ValueError,'unregistered root validator delegation'):
            role_conformance({'src/engine/mod.rs':source})

    def test_catalogue_cannot_hide_an_unscanned_executor_role(self):
        sources={
            'src/engine/mod.rs':RustSource('src/engine/mod.rs',b'''
fn effect_supported(effect: &Value) -> bool { false }
fn execute_effect(effect: &Value) -> Result<(), Error> { Ok(()) }
'''),
            'src/engine/effect_domains/mod.rs':RustSource('src/engine/effect_domains/mod.rs',b'''
fn domain(effect: &Value) -> Option<Domain> {
 match value_kind(effect) { Some("hidden") => Some(Domain::Hidden), _ => None }
}
''')}
        with self.assertRaisesRegex(ValueError,'catalogue/executor role mismatch'):
            role_conformance(sources)

    def root_sources(self,in_batch,performer=None):
        sources={'src/engine/mod.rs':RustSource('src/engine/mod.rs',b'''
fn effect_supported(effect: &Value) -> bool {
    if let Some(supported) = effect_performer::performer_effect_supported(effect) { return supported; }
    effect_domains::effect_domain_supported(effect).unwrap_or(false)
}
fn execute_effect(&mut self, effect: &Value) -> Result<(), Error> {
    let scope = self.begin_event_batch();
    let result = self.execute_effect_in_batch(effect);
    self.end_event_batch(scope);
    result
}
'''+in_batch)}
        sources['src/engine/effect_performer.rs']=RustSource('src/engine/effect_performer.rs',performer or b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    let performer = effect.get("performer")?;
    let mut action = effect.clone();
    Some(singular_player_reference_supported(performer) && effect_supported(&action))
}
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> {
    let Some(performer) = effect.get("performer") else { return Ok(false); };
    self.execute_effect_in_batch(&action)?;
    Ok(true)
}
''')
        return sources

    def test_event_batch_root_and_performer_envelope_are_followed_not_waived(self):
        routed=b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    if self.execute_performed_effect(effect)? { return Ok(()); }
    if self.execute_effect_domain(effect)? { return Ok(()); }
    match value_kind(effect) { Some("rootKind") => Ok(()), _ => Err(Error) }
}
'''
        sources=self.root_sources(routed)
        self.assertTrue(role_conformance(sources)['rootDelegationsRegistered'])
        # A delegation hidden behind the batch wrapper still fails closed.
        hidden=self.root_sources(routed.replace(b'execute_effect_domain',b'execute_new_domain'))
        with self.assertRaisesRegex(ValueError,r"unregistered root executor delegation: \['execute_new_domain'\]"):
            role_conformance(hidden)

    def test_root_batch_executor_arms_are_attributed(self):
        source=RustSource('src/engine/mod.rs',b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    match value_kind(effect) { Some("rootKind") => Ok(()), _ => Err(Error) }
}
''')
        self.assertEqual([a['kinds'] for a in source.arms('execute_effect_in_batch','executor-branch')],[['rootKind']])
        self.assertIn('execute_effect_in_batch',MODULE.ROOT_EXECUTORS)

    def test_performer_envelope_must_stay_kind_transparent(self):
        routed=b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    if self.execute_performed_effect(effect)? { return Ok(()); }
    Ok(())
}
'''
        inspects=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    match value_kind(effect) { Some("hiddenKind") => Some(true), _ => None }
}
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> { self.execute_effect_in_batch(effect)?; Ok(true) }
'''
        with self.assertRaisesRegex(ValueError,'inspects effect kinds'):
            role_conformance(self.root_sources(routed,inspects))
        no_reentry=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> { Some(effect.get("performer").is_some()) }
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> { self.execute_effect_in_batch(effect)?; Ok(true) }
'''
        with self.assertRaisesRegex(ValueError,'no longer re-enters effect_supported'):
            role_conformance(self.root_sources(routed,no_reentry))
        executor_no_reentry=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> { Some(effect_supported(effect)) }
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> { self.hidden_mutation(effect)?; Ok(true) }
'''
        with self.assertRaisesRegex(ValueError,'no longer re-enters execute_effect_in_batch'):
            role_conformance(self.root_sources(routed,executor_no_reentry))

    ROUTED_PERFORMER=b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    if self.execute_performed_effect(effect)? { return Ok(()); }
    Ok(())
}
'''
    REENTERING_EXECUTOR=b'''
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> { self.execute_effect_in_batch(effect)?; Ok(true) }
'''

    def test_envelope_may_only_call_its_allowlisted_helpers(self):
        cases={
            'kind field literal':(b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    if effect["kind"] == "hiddenKind" { return Some(true); }
    Some(effect_supported(effect))
}
''','inspects effect kinds'),
            'unlisted kind-reading helper':(b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    Some(hidden_kind_accepted(effect) || effect_supported(effect))
}
''',r"calls outside its envelope allowlist: \['function:hidden_kind_accepted'\]"),
            'other module helper':(b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    Some(subtypes::effect_supported(effect) && effect_supported(effect))
}
''',r"\['function:subtypes::effect_supported'\]"),
        }
        for label,(validator,message) in cases.items():
            with self.subTest(label),self.assertRaisesRegex(ValueError,message):
                role_conformance(self.root_sources(self.ROUTED_PERFORMER,validator+self.REENTERING_EXECUTOR))
        mutating=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> { Some(effect_supported(effect)) }
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> {
    self.hidden_mutation(effect)?;
    self.execute_effect_in_batch(effect)?;
    Ok(true)
}
'''
        with self.assertRaisesRegex(ValueError,r"\['method:hidden_mutation'\]"):
            role_conformance(self.root_sources(self.ROUTED_PERFORMER,mutating))

    def test_root_executor_walk_sees_free_and_associated_calls(self):
        for label,call,reported in [
            ('free function',b'execute_hidden_domain(self, effect)?;','function:execute_hidden_domain'),
            ('Self associated',b'Self::execute_hidden_domain(self, effect)?;','execute_hidden_domain'),
            ('foreign path',b'hidden::execute(self, effect)?;','function:hidden::execute'),
            ('other receiver',b'self.router.execute(effect)?;','value:execute'),
        ]:
            routed=b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    '''+call+b'''
    if self.execute_performed_effect(effect)? { return Ok(()); }
    Ok(())
}
'''
            with self.subTest(label),self.assertRaisesRegex(ValueError,'unregistered root executor delegation: .*'+reported):
                role_conformance(self.root_sources(routed))

    def test_qualified_root_reentry_is_accepted(self):
        qualified=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    let performer = effect.get("performer")?;
    Some(singular_player_reference_supported(performer) && super::effect_supported(effect))
}
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> {
    let Some(player_id) = self.resolve_player(effect) else { return Ok(true); };
    Self::execute_effect_in_batch(self, effect)?;
    Ok(true)
}
'''
        self.assertTrue(role_conformance(self.root_sources(self.ROUTED_PERFORMER,qualified))['rootDelegationsRegistered'])
        crate_path=qualified.replace(b'super::effect_supported',b'crate::engine::effect_supported')
        self.assertTrue(role_conformance(self.root_sources(self.ROUTED_PERFORMER,crate_path))['rootDelegationsRegistered'])

    def test_turbofish_calls_are_calls(self):
        wrapper=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    Some(hidden_kind_accepted::<Strict>(effect) || effect_supported(effect))
}
'''
        with self.assertRaisesRegex(ValueError,r"\['function:hidden_kind_accepted'\]"):
            role_conformance(self.root_sources(self.ROUTED_PERFORMER,wrapper+self.REENTERING_EXECUTOR))
        routed=b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    self.execute_hidden_domain::<P>(effect)?;
    if self.execute_performed_effect(effect)? { return Ok(()); }
    Ok(())
}
'''
        with self.assertRaisesRegex(ValueError,r"unregistered root executor delegation: \['execute_hidden_domain'\]"):
            role_conformance(self.root_sources(routed))
        reentry=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> { Some(effect_supported(effect)) }
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> {
    self.execute_effect_in_batch::<P>(effect)?;
    Ok(true)
}
'''
        self.assertTrue(role_conformance(self.root_sources(self.ROUTED_PERFORMER,reentry))['rootDelegationsRegistered'])

    def test_macro_calls_fail_closed(self):
        wrapper=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    Some(hidden_dispatch!(effect) && effect_supported(effect))
}
'''
        with self.assertRaisesRegex(ValueError,r"\['macro:hidden_dispatch'\]"):
            role_conformance(self.root_sources(self.ROUTED_PERFORMER,wrapper+self.REENTERING_EXECUTOR))
        routed=b'''
fn execute_effect_in_batch(&mut self, effect: &Value) -> Result<(), Error> {
    dispatch_effect!(self, effect);
    if self.execute_performed_effect(effect)? { return Ok(()); }
    Ok(())
}
'''
        with self.assertRaisesRegex(ValueError,r"unregistered root executor delegation: \['macro:dispatch_effect'\]"):
            role_conformance(self.root_sources(routed))

    def test_constant_keys_fail_closed(self):
        for label,read in [('index',b'effect[KIND_FIELD] == "hidden"'),('get',b'effect.get(KIND_FIELD).is_some()'),
                           ('variable key get',b'effect.get(&field).is_some()')]:
            wrapper=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    Some('''+read+b''' || effect_supported(effect))
}
'''
            with self.subTest(label),self.assertRaisesRegex(ValueError,'reads keys the census cannot prove'):
                role_conformance(self.root_sources(self.ROUTED_PERFORMER,wrapper+self.REENTERING_EXECUTOR))
        indexed=b'''
fn performer_effect_supported(effect: &Value) -> Option<bool> {
    let performer = effect.get("performer")?;
    Some(singular_player_reference_supported(performer) && effect_supported(effect))
}
fn execute_performed_effect(&mut self, effect: &Value) -> Result<bool, Error> {
    let player_id = self.resolve_player(effect)?;
    if self.player_index(&player_id).is_ok_and(|index| self.state.players[index].has_lost) { return Ok(true); }
    self.execute_effect_in_batch(effect)?;
    Ok(true)
}
'''
        self.assertTrue(role_conformance(self.root_sources(self.ROUTED_PERFORMER,indexed))['rootDelegationsRegistered'])
        renamed=indexed.replace(b'players[index]',b'players[slot]').replace(b'|index|',b'|slot|')
        with self.assertRaisesRegex(ValueError,r"reads keys the census cannot prove are not the kind: \['slot'\]"):
            role_conformance(self.root_sources(self.ROUTED_PERFORMER,renamed))

if __name__=='__main__':unittest.main(verbosity=2)

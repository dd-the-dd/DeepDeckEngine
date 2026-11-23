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

if __name__=='__main__':unittest.main(verbosity=2)

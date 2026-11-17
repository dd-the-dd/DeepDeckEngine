# Migration Oracle : checkpoint 04CA accepté, migration continue

## Lot appliqué, encore non certifié : Damage et quotas

Working tree : `.tmp/night-damage-quota-root-01` orchestre les sources Damage
rebasées04CA, le quota parser Deafening Silence et le filtre universel de
l'historique des sorts. 904 tests parser et556DATA Native+556Full passent.
Le premier runner moteur a696/697 scénarios verts; la reprise valide a révélé
une source IRself mal résolue. Addendum source appliqué, pas encore reconstruit :
`.tmp/night-damage-source-reference-addendum-01` (trois tests et quatre DATA).

Le benchmark initial a révélé19,5x de coût sur2048 dommages (.185→3.609s)
par recopie du journal. Root a ajouté `resolution_checkpoint.rs` et cinq tests
pour conserver le préfixe append-only et tronquer les nouveaux événements sur
erreur. Le reset de position déterministe refuse désormais une transaction
active. Revue indépendante et reconstruction moteur sont encore requises.
Ne pas promouvoir ce lot avant rollback/RNG/historique, benchmark, CI3,
contrats, vrai lien Oracle, global/ten/cohort et gates stricts.

Le linker générique `scripts/link-native-oracle-fixtures.py` est installé et
revu :25tests Python verts; il remplace les scripts par carte pour normal/single,
sans certifier l'exécution. Documentation `native-fixture-linker.md`.
Mana03c reste entièrement en staging, aucune couverture revendiquée.

## État actuel après Deathrite Shaman

**3 638 cartes non vides entièrement parsées / 17 352 capacités natives**.
+13 cartes et +40 capacités depuis EA91. Les anciennes IR natives sont
17 243 exactes +69 transformations intentionnelles de perte de vie simultanée,
sans autre changement. Les 13 nouveaux contrats statiques passent; Deathrite
reçoit aussi sa preuve individuelle réelle (trois capacités, huit scénarios).
Registre161, tous les160 anciens enregistrements préservés.

Certificat `.tmp/night-deathrite-root-01/certificate.json`, analyseur SHA
`04CA8E6D09B201CE0BAC721B075E0B8583386EF6B6B0A6BAA325F98052565944`.
897 tests parser, 11 life batch, 181 moteur CI3, 546 Native +546 Full,
390 contrats, 687 DATA moteur /4660 assertions; strict145 :395 preuves fraîches,
Deathrite :9 preuves fraîches. Dix sets :289 complètes/1531 capacités,
241 complètes uniques. Benchmark parser .628→.647s (+3.03%), aucun speedup
revendiqué. Rapport `migration-baselines/graveyard-sequences-and-life-batches-2026-11-14.md`.

**Cohorte active05** : `.tmp/night-oracle-cohort-05/cohort100.json`,
`cohort100-input.json`, `baseline-04ca.jsonl`, `adoption.json`. 99 cartes
conservées exactement; Deathrite retirée et Scute Swarm ajoutée (pool rank296).
100 non résolues, 28/205 capacités natives. Quotas34recent/33priority/33league,
curseurs251/266/278 inchangés. Cohortes03 et04 conservées comme historiques.

Frontières acceptées : changement de zone de carte ciblée dans la séquence
ordinaire; annotation de timing typée; loseLife sur PlayerSet atomique.
`resolution_preflight.rs` possède seul le visiteur récursif et le scan de pile;
les domaines fournissent un prédicat local. Les vrais forEach restent séquentiels.
La résolution finale, et non le service de perte de vie, déclenche les SBA.

En cours : agent generic_ast_coverage prépare un quota de cast générique
(Deafening Silence) dans `.tmp/night-cohort04-generic-analysis-01`, après analyse
bornée des100 échecs. Mana Reflecting Pool reste en staging, sans migration
revendiquée; phases de contexte GameState et transport Result encore requises.
Le gros lot Damage est séparé et doit être rebasé sur ce checkpoint avant tout
Cargo. Root seul intègre et possède la lane Cargo; agents en staging.
Objectif toutes les cartes Magic toujours actif.

## Historique avant 04CA

# Migration Oracle : parent syntaxique générique accepté, migration continue

## Checkpoint EA91 accepté : Explore et Urban Evolution

**3 625 cartes non vides reconnues complètement par le parser /17 312 capacités natives**.
+2/+4 depuisB1EF ;17 308 anciennesIR exactes. Preuvesverticales Explore+Urban
acceptées, registre160 ;890unités parser,534Native+534Full,377contrats,
671DATA moteur/4587assertions ;395preuveshistoriques+4nouvellesréexécutées.
Zéro changement moteur dans ce lot ; aucun nouveau Rustharnessmoteur.
Certificat `.tmp/night-explore-root-01/certificate.json`, analyseurSHA
`EA91C3B3241125E2504C602F03D03BECC579D7D16CE60480DAD1FCE6C17F79D6`.
Rapport `migration-baselines/temporary-land-permissions-2026-11-14.md`.
Dixsets287/1525 inchangés ; benchmarkparser.646→.713s/2368occurrences(+10.37%),
pasdespeeduprevendiqué ; limitelexicale wordzero connue, digit0refusstructuré.

**Cohorte active04** : `.tmp/night-oracle-cohort-04/cohort100.json`,
`cohort100-input.json`, `baseline-ea91.jsonl`, `adoption.json`.99retenus exacts,
Explore retirée, Rishkar’sExpertise ajoutée ;100nonrésolues,28/207capacités
natives ;34recent/33priority/33league,cursors251/266/278 inchangés.03historique.

PROCHAIN : Deathrite composé `.tmp/night-deathrite-graveyard-01/composed-review-01`,
23fichiers/58ops,revu indépendamment ;preuves `composed-proofs-01` (12parser,
16runtime). Inclut lossbatch+SBA+captureguard+compat3IR ;69transformationsIR
intentionnelles à certifier et recalculer contreEA91 accepté.
Verifierpartitionruntime1 pour `life::batch_tests` ;partition3 pourarchitecture.
AucunecouvertureDeathrite tantque vrais3IR/runtime/contrats/gates nepassent.
ReflectingPoolfrontendheld : phasesfondation `.tmp/night-reflecting-mana-types-01/
phases/combined-foundation` (7rs/12testsproposés,noncompilé), contexteGameState
etResulttransportencoreàfaire. Grosgraphe76méthodes/27nonResult, n’appliquepas
l’ancienprototype runtime. Damage49hunksreste nonintégré après cesfrontières.
RootseulCargo/intégration ;agents staging ;objectif touteslescartesMagic actif.

## Historique : checkpoint B1EF accepté : programme optionnel et diagnostics typés

**3 623 cartes non vides complètes /17 308 capacités natives**, +47/+75 depuis90A8.
Les17 233 anciennes IR sont exactes ; aucun opcode combiné ajouté.
Certificat `.tmp/night-optional-certified-root-01/certificate.json`, analyseur
`after-analyzer.exe` SHA `B1EF3BDD94B2277F002B23F50B8FBABA337B0D417FCFB05F5238B42833CBD0ED`.
Pointer `.tmp/oracle-certified-analyzer-metadata.json` mis à jour ; sourcefreeze gelé.
885unités parser ;515DATA Native+515Full ;181unités moteur ;663DATA/4524assertions ;
375contrats permanents +48contrats couvrant les47nouvelles cartes/faces.
145historiques strictes (395preuves réexécutées puis cache validé) +Sage
(3preuves fraîches puis cache), registre suivi158. Contrats statiques ne valent
pas47simulations individuelles. Rapport `migration-baselines/optional-effect-programs-2026-11-14.md`.

Frontière acceptée : EffectProgramNode::Optional compose les actions ordinaires,
cibles hors permission ; outcomes typés et diagnostics primaire/candidat distincts ;
checkpoint des permissions avant préparation des cibles, couvrant toute la pile
et le point d’entrée direct. Deux diagnostics de verbe corrigés sans assouplir les rejets.
Caps conservés : continuation/if-you-do/secondepermission, guard-other observé,
propriétés post-zone Stack/mixte et contrôleur post-zone.

Dix sets locaux287/1525 inchangés ; médiane parser sur2368occurrences
0,595→0,612s (+2,86%), processus0,713→0,732s. Pas de gain de vitesse revendiqué.
Cohorte03 reste100nonrésolues/29capacités natives sur207 ; quotas34/33/33,
curseurs/provenance conservés. Classement100 : `.tmp/night-cohort03-ranked-after-optional-01/REPORT.md`.

Banc DATA accepté : `.tmp/night-runtime-data-failure-root-01/certificate.json`,
12unités (8protocole nouvelles),666fixtures/4547assertions,663anciensrésultats
exacts ;3probeserreur/reprise=.046s sansCargo. `probeFailure`/`executeRule`
passent par les APIs publiques, sans restauration par le pilote. Rapport
`migration-baselines/runtime-data-failure-probes-2026-11-14.md`.

Explore CANDIDAT appliqué :12sources parser +addendum panic composé,
`.tmp/night-explore-root-01/integrated-root.json` ; compilation/unités en cours.
Pas de nouveau harnaisRustmoteur :4DATA (origine+addition/expiration) suffisent.
Sources gelées `.tmp/night-explore-permission-01`,
`.tmp/night-explore-damage-support-addendum-01`, `.tmp/night-explore-data-addendum-01`.
Root seul Cargo/intégration. Ne pas compter Explore avant liaison IR réelle.

Prochains paquets STAGING : ReflectingPool (`.tmp/night-reflecting-mana-types-01`,
generic_trigger_events) etDeathrite (`.tmp/night-deathrite-graveyard-01`,
generic_ast_coverage), limites mana/graveyard annoncées séparément.
Damage composé `.tmp/night-damage-integration-prep-01/rebased-04` :49hunks,
39testsproposés, revue indépendante terminée, pas encore compilé.
Garder frontendDamage capé avant preuves réelles et rollback public sur refus.
Timing Sage complété `.tmp/card-migration-timings/night-reclamation-optional-root-01.json`.
Timing Explore `.tmp/card-migration-timings/night-explore-root-01.json`.
Objectif global toujours actif : toutes les cartes Magic.

## Historique : checkpoint90A8 accepté — Rapid et Feed certifiées

**3 576 cartes non vides complètes /17 233 capacités natives**, +8/+9 depuisEEE,
les17 224 anciennes IR exactes ; dix sets287/1525 inchangés. 500parserNative+Full,
655DATA moteur/4472assertions,374contrats,145historiques+2nouvelles strictes certifiées.
Registre natif suivi157 cartes. Rapport
`migration-baselines/qualified-controller-and-permanent-lki-2026-11-14.md`.
Certificat `.tmp/night-frontends-rapid-feed-01/certificate.json`, analyseur immuable
`.tmp/night-frontends-rapid-feed-01/after-analyzer.exe`, SHA
`90A8D3FEF69FC3729B45DC8BD07AEA97433000315D4BDF4B256614EAD52C6D9B`.
Ce checkpoint remplace le statut candidat des sections historiques ci-dessous.

BackendTargetLKI et référents qualifiés sont acceptés. Garder les caps Stack/mixte,
contrôleur post-zone et autre objet observé. **Cohorte active03** :
`.tmp/night-oracle-cohort-03/cohort100.json` et `cohort100-input.json` ; adoption/hash
dans `adoption.json`. 98cartes conservées exactement +Professional Face-Breaker/Explore,
100nonrésolues,29/207capacités natives ; quotas34recent/33priority/33League.
La cohorte02 est maintenant historique. Pool750 :169nonvides complètes+4vides,
525capacités natives ; aucun curseur supprimé ou modifié manuellement.

Cache incrémental et audit local acceptés : voir
`migration-baselines/local-audit-and-incremental-cache-2026-11-14.md`.
Même artefact Oracle frais entre runner et audit local ; 145/145 strictes.

Prochains paquets NON intégrés : Optional7fichiers puis typed-attempt4fichiers ; runtime Damage doit composer
batch-real+review-addendum-01+review-observer-addendum-02+SourceLKI pending.
Voir les HANDOFF/README dans .tmp ; leurs sources sont gelées et non compilées.
Root seul intègre/compile, agents en staging. Objectif global toujours actif.

## Backend TargetLKI accepté ; frontends Rapid/Feed en validation

Backend `.tmp/night-target-lki-root-01/backend-certificate.json` accepté :174privés
dont28nouveaux,653DATA/4447assertions,145strictes/395preuves. Frame couvrant la
résolution entière, identité originale, snapshots effectifs avant départ réel,
rollback, cible initialement illégale et quantité manquante distincts. Feedpublic
provisoire20→15 vert ; aucune nouvelle carte attribuée à ce backend seul.
Rapport `migration-baselines/target-numeric-lki-backend-2026-11-14.md`.

**Candidat actuel en production, pas encore accepté** : Rapid référent typé partagé
et Feed post-zone Permanent seulement,34 fichiers intégrés depuis packets gelés :
`.tmp/night-rapid-controller-reference-01/review-cap-addendum/HANDOFF.md` et
`.tmp/night-feed-permanent-lki-01/HANDOFF.md`. Root preuves dans
`.tmp/night-frontends-rapid-feed-01` ;70 privés numeric et4 référents verts, gate
DATA500parmode et replays global/dixsets encore à terminer. Les caps Stack/mixte
et contrôleur post-zone restent présents. Cohorte active toujours night-oracle-cohort-02.

Agents : optional programme partagé (Reclamation Sage), vrai batch Damage, cache
incrémental/métriques. Tous en staging sans Cargo ; root seul compile. Objectif actif.

## Runtime source/count accepté et cohorte suivante adoptée

Checkpoint `.tmp/night-source-count-root-01/certificate.json` :146privés,
652DATA/4443assertions et145cartes strictes certifiées. Identité préparée avant
les captures/publications ; compteur dédié sérialisé, copy_sequence inchangé.
Audit PowerShell UTF8 corrigé, cartes/queue/cache+suite AST verts. Zéro nouveau
gain parser attribué ; other et LKI cible restent plafonnés. Rapport
`migration-baselines/source-count-incarnations-and-utf8-audits-2026-11-14.md`.
Le pointeur `.tmp/oracle-certified-analyzer-metadata.json` référence EEE et ce
nouveau certificat runtime, avec parent historique conservé.

**Cohorte active** : `.tmp/night-oracle-cohort-02/cohort100.json` et
`cohort100-input.json`, adoption/hash dans `adoption.json`. 98anciennes retenues,
Impact Tremors et Rapid Hybridization ajoutées ;100 non résolues,27/204capacités natives.
La cohorte précédente reste une preuve historique. Les sources réelles et probes
mutés demeurent séparés.

Root revoit Rapid : `.tmp/night-rapid-controller-reference-01` (référent typé partagé
et contrôleur capturé avant destruction). Structure termine TargetLKI dans
`.tmp/night-target-lki-runtime-01` ; coverage prépare le vrai batch Damage dans
`.tmp/night-player-set-damage-01`. Aucun de ces trois paquets n'est accepté ou
intégré. Root seul écrit la production et occupe le lane Cargo.

## Checkpoint EEE accepté — domaines Return et numériques de cibles

3 568 cartes non vides complètes /17 224 capacités natives, +4/+5 depuis BA3F ;
les17 219 anciennes IR restent exactes. Dix sets :287 complètes, inchangé.
476 DATA parser par mode (475+1), runtime9/61assertions, contrats372 verts.
Les anciens contextes de séquence gardent Battlefield ; l'opt-in sort/permanent
appartient aux parents utilisant le lowerer partagé. Références numériques liées
validées contre leur cible originale ; Feed demeure plafonnée après changement de zone.
Analyseur `.tmp/night-return-target-numeric-02/after-analyzer.exe`, SHA
`EEE422D3BF64A86A538BC871FD197FDA1CFBCA5EFF637D3E49F279677D92C6AB`.
Freeze et preuves dans le même dossier ; rapport
`migration-baselines/return-domains-and-target-numerics-2026-11-14.md`.
Ce checkpoint remplace BA3F pour le parser, sans remplacer le certificat runtime global.
Le candidat intermédiaire B42D n'est pas un checkpoint accepté.

Root revoit maintenant le paquet runtime source/count/lifecycle et son addendum
compteur dédié ; aucun cap other ou LKI cible retiré. Agent coverage travaille
sur Damage→PlayerSet, events sur refill98+2 et Rapid Hybridization, structure sur
la frame LKI cible. Tous travaillent en staging sans Cargo ; root seul compile.
Objectif global actif, migration continue pendant la nuit.


## Pont modèle/contrats et banc DATA acceptés — 14 novembre

`deepdeck-oracle-engine-contracts::model` réutilise l'adaptateur `src/model.rs`
entre parseur public et moteur sans `oracle-compilation`. Le runner stable lit
désormais un PlayableCardInput complet (factory/layout/faces puis Oracle), en plus
des anciens contrats IR. Une entrée unsupported ne devient pas executable parce
que sa liste de règles filtrées est vide. Root : 4 tests privés runner, 2 contrats
factory et 372/372 DATA (365 existants +7 nouveaux), zéro fixture invalide.
Artefact moteur réutilisé, features=[], aucune recompilation Oracle dans le moteur.

Wrapper accepté `scripts/run-contract-fixture.ps1` : build explicite dans le lane
partagé, puis exécution sans Cargo tant que sources/catalogue/binaire sont
authentifiés ; les fixtures JSON peuvent évoluer sans rebuild. Mesure chaude
372 cas sans build : 0.1705331 s d'exécution, 1.4731486 s total wrapper, compilation0.
Passage -Build précédent : appel Cargo0.2203191 s mais aucun artefact recompilé,
exécution0.2526799 s, total3.1259433 s. Onze smoke cache/erreurs sont verts.

Ces preuves de contrats n'ajoutent pas de couverture parser/runtime globale.
Les scénarios comportementaux restent distincts ; source-incarnation/count et
cible-numérique/LKI restent en revue, Feed demeure plafonnée. Voir
`migration-baselines/contract-model-bridge-and-warm-runner-2026-11-14.md` pour
les durées séparées, provenance et limites.

## Nuit active — checkpoint BA3F accepté

Joueurs quantifiés et rappels de mots-clés liés acceptés après review :
global3564cartes complètes/17219capacités natives, +62/+95 depuis E04B,
17124IR antérieures exactes. Dixsets287complètes (+4), soit +12 depuis275 initiales.
452fixtures parser par mode, 11DATA/120assertions sans rebuild, privés ciblés6+5 verts.
La mission toutes-cartes continue pendant la nuit ; aucun set déclaré terminé.
Rapport : `migration-baselines/night-player-keyword-generalization-2026-11-14.md`.
Analyseur : `.tmp/night-parser-generalization-01/after-analyzer.exe`, SHA
`BA3F95604A3AF55AD6C3F95E9F43C16ECA1E5D98CD1E79DC86E3EC62EE0CE4A0`.
Lot100 exact `.tmp/night-generic-cohort-01/cohort100-input.json` : Garruk seule
complète à retirer au prochain refill. Les99 autres restent à résoudre.
Root revoit maintenant Return mixte dans `.tmp/night-return-union-01-domain-staging`.
Identité d'entrée/count other : `.tmp/night-count-source-runtime-01` encore NON accepté,
frontière prepare/publish en analyse. Références numériques de cible et LKI :
`.tmp/night-target-numeric-01` encore NON accepté, Feed reste plafonné.
Un seul lane Cargo root, tous agents en staging sans production/Cargo.


## Point du 14 novembre 2026 — arbre syntaxique des déclenchements

Parent commun event → condition booléenne optionnelle → EffectSequence, avec
plages de tokens et journal de construction réel. Sous-automates partagés,
portées de virgules explicites, préfixes at least/at most, adjectifs négatifs
composés. Aucun nouveau kind moteur ni regex. Compatibilité atomique Surveil
conservée pour garder les IDs de décisions existants.

Dix sets locaux figés : 275 → 283 cartes complètes (+8), 1494 → 1516 capacités
natives (+22). Déduplication inter-set : 227 → 235 sur2319 oracle_ids. Global :
3409 → 3502 cartes non vides complètes (+93), 16874 → 17124 capacités (+250).
Les16874 anciennes IR restent exactes. Ces compteurs certifient le parsing,
pas la simulation individuelle de toutes ces cartes. Les3144 textes vides sont séparés.

433 fixtures passent dans chacun des modes NativeOnly/Full ; runtime DATA11/81
et combat autonome1test/4scénarios frais passent. Les dix échecs privés parser
préexistants sont reproduits à baseline identique et restent à traiter.
Benchmark médian2368 occurrences : 0.626 →0.628 s, vitesse globalement stable.

Nouveau parent : modes déclenchés et tous les guards other restent plafonnés,
à cause du moment des choix et de la liaison/presence de l'objet à exclure.
Roiling Canopy et Archive Arbiter ne sont pas comptées débloquées.
Rapport : `migration-baselines/generic-trigger-syntax-tree-2026-11-14.md`.
Analyseur natif accepté : `.tmp/generic-ast-root/after-analyzer.exe`, SHA
`E04B1CE3915A965EB7D5EAB2997D606B6559BBDF30EBBCFEEBD1EA8D7EB3D089`.
Ce checkpoint parser ne remplace pas les anciens certificats runtime globaux.

L'utilisateur a demandé de poursuivre pendant la nuit. Travaux en staging :
`night-generic-cohort-01` (cohorte rotative/ranking), `night-count-source-runtime-01`
(identité/incarnation pour excludeSource), `night-player-program-01` (quantification
des joueurs réutilisant les actions). Root possède les intégrations et le lane Cargo.
Lire ces handoffs avant reprise ; aucun de ces staging n'est encore accepté.


Objectif : parser toutes les cartes Magic et exécuter leurs règles par des
automates token partagés et des primitives composables. Suivre AGENTS et
`oracle-migration-objective.md`. Analyser jusqu'à100 cartes non couvertes,
classer les correctifs par cartes entières débloquées, attribuer des tâches
sans chevauchement, prouver parser/IR/runtime, rejouer, remplir et recommencer.
Alterner sets locaux récents, priorité et League ; garder ordre, curseurs,
oracle_id et provenance. Strict145 est une régression indépendante.

## Frontière acceptée

Optimisation lexicale validée : `token_type` utilise désormais le dictionnaire
ordonné de `src/oracle/syntax/token_vocabulary.rs`, construit une fois par
`OnceLock`, puis recherche binaire de 306 entrées. Hors dictionnaire, seuls
les 9 patterns ouverts sont évalués. 1 245 358 tokens réels et 936 variantes
conservent exactement leurs classes ; 36 tests tokenizer, 3 privés dédiés
et 397 fixtures dans les deux modes passent. Microbenchmark médian de
classification : 307,903 -> 200,734 ms ; pas une mesure de gain global du parser.
Voir `migration-baselines/token-type-binary-lookup.md`.

Ajout validé le 13 novembre : préflight `plan_card_structure` dans
`src/oracle/card_structure.rs`, avant la lecture Oracle. Diagnostic
`card_face_factory_missing` pour layout/profil absent ;
`card_face_structure_invalid` pour métadonnées incohérentes. Le plan
`cardStructure` est partagé avec les adaptateurs de `src/model.rs`.
397 fixtures parser dans les deux modes, 4 privés, 2 contrats et le contrat
réel Emeritus passent. Replay global : 39 287 plans sur 39 288 objets,
anciennes IR natives identiques. Hela / Dark Tutor (profil Vengeance,
prévisualisation déjà unsupported) remonte maintenant une factory manquante.
Ce contrôle structurel n'ajoute aucune couverture de simulation revendiquée.
Voir `migration-baselines/card-structure-factory-2026-11-13.md`.
Les anciens certificats ci-dessous restent immuables ; le source courant
inclut ce préflight et le retrait de la dépendance de tests parser -> moteur.

Certificat : `.tmp/oracle-checkpoint-simultaneous-sacrifice-D30A9570.json`, SHA
`52200518BFF8893601C7016A082192B43445206BDADA026D4A53819357F14B63`.
Principal0B217 +setup7A6 acceptés,12 chemins,176 lignes des deux anciens arms
retirées. Deux DATA historiques changent seulement leurs effets, setup/attentes
identiques. Privés123, DATA618/4 166, contrats587 et strict145/395 frais.
Le correctif watchers est test-only,76 assertions identiques. Scope D25,
Battle et parser Mayhem restent séparés en staging.
Parent parser : `.tmp/oracle-checkpoint-plural-destruction-F096D8B0.json`.
Retrait5 corrigé89507 accepté :223 lignes moteur nettes retirées,113 kinds
interdits. Le draft mut incorrect n'a jamais été appliqué. Runtime610 et
contrats580 sont frais ; corpus parser inchangé à hashes exacts.
Parser : `F096D8B0B09BE3E159C852ED03AB26B8B7AE6DA691E53763A770128E6EC2BDC1`.
Moteur mod.rs : `D30A9570E2869F206D4C68B8DFDFB4F8194AB267BE6264515761BFEEA898D0F0`.
Analyseur : `.tmp/round19-root-plural-integration/analyze-local-abilities-round19.exe`,
SHA `DDF5343D24DC3F5C1356EE8FE3A729628E8DB5049C177676974BD97F63D7EB61`. Le pointeur
`.tmp/oracle-certified-analyzer-metadata.json` fait autorité.
Gel moteur : `.tmp/native-simultaneous-sacrifice-root/accepted-source-freeze.json`.
Gel parser : `.tmp/round19-root-plural-integration/parser-replay/parser-source-freeze.json`.
Rapports : `migration-baselines/simultaneous-sacrifice-template-retirement-2026-11-13.md`,
`migration-baselines/five-unproduced-engine-forms-2026-11-13.md`
et `migration-baselines/plural-destruction-program-2026-11-13.md`.
Les certificats antérieurs restent immuables.

Legacy archivé et vérifié à distance : branche
`archive/legacy-oracle-2026-11-13`, commit
`eec9a2ef4a57b7a759e0b0cd667cdae70f1a0c99`.
Branche de travail : `codex/integrate-agent-clock-wip`, aucun commit de
nettoyage, index initial intact. Préserver clock/training/backend et staging.

## Résultats et limites

16 862 capacités natives /67 420 ;3 403 objets non vides entièrement reconnus
/39 288,3 144 objets sans texte séparés. Dernière vague : +38 capacités/+29
objets entiers ; depuis C355 : +829/+434. Les16 824 anciennes IR restent
exactes. API Native/Full globales byte-identiques. Ces scores de reconnaissance
ne prouvent pas la simulation individuelle de toutes les cartes.

380 cas parser par mode et4 privés parser hérités byte-exacts,618 scénarios moteur/4 166 assertions,
587 contrats permanents,38 nouvelles formes valides. Les123 privés runtime sont frais surD30/F992.
Préservation135 groupes/16 656,
plus les48/120 gains précédents. Architecture zéro violation. Strict145/145
avec395 preuves fraîches. Force/Hex/Reckless :3 verticales/17 preuves fraîches,
14 sources réelles exactement liées aux4 capacités émises. Manifeste155 entrées.
Les16 rouges privés parser connus au nettoyage ne sont pas déclarés corrigés.

TargetList cardinalisée reste partagée ; références scalaires préservées,
collection de destruction explicitement autorisée par Atomic/EffectSequence.
Les autres parents/actions restent scalaires. Collection sans antécédent
singulier, relation plurielle other non représentée refusée. Not(Color)
réutilise le prédicat logique existant avec domaine permanent positif.
Chaque cible est revalidée avant un batch unique ; cannotRegenerate transmis.
Quatre bogues reproduits avantfix passent maintenant. Aucun nouvel opcode.

Les vieux parser/fallback/classifier et la dépendance regex Oracle sont retirés.
Amass/Explore sont des programmes primitifs ; anciens opcodes retirés. La garde
compte113 kinds retirés, sans prétendre à une pureté sémantique totale. Player
générique, collecte locale et phases de départ sont certifiés BA147. L'ancien
template forEachPlayer est retiré surD30 ; le kind garde la vraie boucle à bind explicite.
Dette Earthbend : retour actuel immédiat ; ne pas le confondre avec un vrai
déclencheur différé. Comparaisons numériques sur sort observé restent bloquées
dans le parent spellCast (valeur imprimée sans X de pile). Top-library reste
un diagnostic d'API main seulement. Hex5 prouve disponibilité, pas rollback.

Le premier gate vertical Force a refusé trois pins préparées avec l'ordre des
clés planifiées. Leur contenu structurel était exact ; la fonction de digest
conserve l'ordre. Pins recalculées depuis la sortie réellement émise, sans
changer Oracle/IR/runtime. Seules les3 nouvelles pins ont changé ; sortie
initiale et diagnostic conservés dans le dossier root19.

## Lot20 et responsabilités

Adoption : `.tmp/round20-root-adoption.json`.
Queue : `.tmp/rotating-next-100-round20-certified.json`, SHA
`1F6A0C7AA2022B2397A5DFB2CA0E9EBDA1A2859A910F1EBB06A5C41500CCD59A`.
Oracle : `.tmp/rotating-next-100-round20-certified-input.json`, SHA
`293C810987E26F6148BC321634DD6F70268DC21A3F5BFBB2922F4BB06E8DCF75`.
Baseline : `.tmp/round20-baseline-native-F096D8B0.jsonl`.
100 objets/207 capacités/29 natives, zéro entier.
Refill officiel :99 retenus sémantiquement byte-exacts,1 ajout : Legolas's Quick Reflexes.
Répartition : {"priority": 33, "recent": 34, "league": 33}. Energy Reserve auxiliaire réel
conservé et signalé ; ni faux natif ni suppression manuelle de curseur.

- Root : seul écrivain production, seule lane Cargo `.tmp/targets/card-migration`,
  revue/intégration/certification/replays/refill. Objectif global actif.
- coverage_checkpoint_audit : Mayhem choisi (maximum1 exaequo), paquet parser
  6D6D gelé et cap7DF87 couplé. Acteur commun SpellCast/Sacrifice, ASTObjectQuery
  intact ; seul prédicat Permanent nu admis tant que le snapshot effectif manque.
  Overlay13cas,2contrats admissibles. Aucun Cargo/prod.
- additional_cost_transition : retrait5 accepté ; DATA/contrats Mayhem Devil
  en staging seulement, Oracle complet et sources inchangées. Aucun matcher
  runtime en parallèle de combat. Dossier `.tmp/round20-sacrifice-trigger-runtime-staging`.
- combat_sequence_transition : principal accepté. Addendum scope D25 gelé surD30,
  non appliqué ; implémentation en staging de la première capture commune des
  matches effectifs avant départ dans cost_payment/sacrifice seulement, base
  D30+D25. Aucune correction LKI nouveau trigger/doubleur dans ce paquet.
  Dossier `.tmp/native-sacrifice-effective-observation-v1`, trackeragent distinct.

Le retrait5 et le principal sacrifice sont certifiés. Vérifier les prochains hashes/fonctions,
garder main compilable et sérialiser Cargo ; ne pas écraser un fichier sale.
Le chrono root de décomposition est clos avec erratum ownership conservé :
un agent a ajouté des mesures et fermé/ouvert ses phases dans cet ID à tort,
sans perte d'historique. Rapports uniques de temps dédupliqués par fichier/hash
dans `.tmp/native-simultaneous-sacrifice-root/CANONICAL-TIMING-REPORTS.json`.
Le chrono Force root est clos après853,412 s incluant
revue/corrections/orchestration et attentes, pas du CPU par carte. Aucun
nouveau score du lot20 revendiqué avant son replay.

Défauts actuels hors portée, mesurés surD30 mais avant non mesuré : nouvelle
aptitude self-dies après lord disparue (22 au lieu24), doubleur disparu dans
l'événement (1trigger au lieu2), filtre Creature perdant un terrain animé.
Probes séparés : `.tmp/native-departure-inherited-lki-probes-v1/` et
`.tmp/round20-sacrifice-filter-snapshot-review/`. Aucune revendication LKI universelle.
Battle damage17lignes+zéroCR120.8 est gelé mais non appliqué ; compteurs à0,
SBA310.7/.8 et intrinsicSiege310.12b restent encore un plan séparé.

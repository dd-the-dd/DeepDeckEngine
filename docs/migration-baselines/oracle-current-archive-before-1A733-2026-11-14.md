# Migration Oracle : état courant 17319

L'objectif reste toutes les cartes Magic, avec lecture Oracle générique et
exécution correcte. Suivre la boucle d'analyse de100 cartes, correctifs partagés,
revue, preuves verticales et replay. Le but global n'est pas terminé.

## Dernier checkpoint accepté

**3 642 cartes non vides entièrement natives /17 360 capacités natives**.
+4 cartes et +8 capacités depuis04CA; toutes les17 352 anciennes IR exactes.
Deafening Silence, Rule of Law, Arcane Laboratory et Eidolon of Rhetoric passent
parser +contrats. Seules les deux premières ont de nouvelles preuves
individuelles : registre163, les161 anciens enregistrements inchangés.

Certificat `.tmp/night-damage-quota-root-01/certificate.json`, analyseur
`.tmp/night-damage-quota-root-01/after-analyzer.exe`, SHA256
`17319B3B9B61245AA6D82D20854999B7D5BFD7ECDA29A4B0E984B7108B891E98`.
Source freeze `accepted-source-freeze.json`; pointeur accepté
`.tmp/oracle-certified-analyzer-metadata.json`. Cargo.lock reste5626D881…458A9.

Preuves :904 unités parser,556DATA Native +556Full,228unités moteurCI3,
701scénarios/4744assertions,394contrats, quatre plans factory sérialisés.
Strict145 :395preuves fraîches; Deafening +Rule of Law :sept preuves fraîches.
Dix sets :289 cartes complètes/1531capacités,241 complètes uniques, inchangés.
Rapport `migration-baselines/player-damage-cast-quotas-and-checkpoint-cost-2026-11-14.md`.

## Cohorte active06

`.tmp/night-oracle-cohort-06/cohort100.json`, `cohort100-input.json`,
`baseline-17319.jsonl`, `adoption.json`.99survivants05 exactement conservés;
Deafening retirée, Omniscience ajoutée (League, poolrank189, batchrank86).
100nonrésolues,28/205capacités natives. Quotas recent34/priority33/league33,
curseurs recent251/priority266/league278 inchangés; aucun nouvel import.
Toutes les99lignes d'audit retenues restent exactes. Audit .064s sans Cargo.

## Frontières acceptées à réutiliser

- `player_set.rs` partagé; quota castEachPlayer et maximum positif i32.
  Le filtre null de l'historique des sorts est universel; les enfants booléens
  non évaluables restent refusés. Le runtime ignore encore les autres portées
  de quota : ne pas les promouvoir dans le parser.
- Damage PlayerSet backend prépare/commit/publique simultanément, Source-LKI
  distinct du contrôleur de capacité; sources self/abilitySource résolues.
  Le frontend Damage PlayerSet n'est PAS encore migré.
- `resolution_preflight.rs` possède l'unique visiteur récursif. Domaines shallow.
  `resolution_checkpoint.rs` conserve le préfixe append-only des événements,
  restaure le reste et tronque les ajouts sur Err. Reset déterministe interdit
  dans une transaction active. Futurs writers doivent préserver cet invariant.
  Perf2048damages .185→.189s; première tentative3.609s rejetée. Pas de claim
  de gain de vitesse global. Les SBA restent à la frontière de résolution.
- `scripts/link-native-oracle-fixtures.py`,25tests Python verts, relie les IR
  réelles aux sources complètes des DATA normal/single sans lancer le moteur.
  Voir `native-fixture-linker.md`; préférer cet outil aux scripts par carte.

## Lot integre : certification en cours

Root a accepte les revues et applique26sources/83operations byte-preserving :
EachOpponent partage, Damage PlayerSet, outcomeDeferred pour X, Cage,
snapshot par mouvement et timing revelation. Plan et preimages :
`.tmp/night-player-damage-cage-root-01/final-composition-plan.json`,
`application.json`. Aucun ancien fichier sale restaure.

Gates fraiches :923unites parser vertes (compile39.328s/execute1.621s),
585DATA Native et585Full verts. Analyseur candidat
`.tmp/night-player-damage-cage-root-01/after-analyzer.exe`, SHA
1A733BC5F115CAC970FC86C8E0BC3147E33A847095F1E09B8A9AE628BDBE747D.
Audit global :+38cartes completes/+82capacites,17360anciennesIR exactes,
zero regression; dix sets :+4cartes/+7capacites. Ces gains restent candidats
jusqu'aux preuves runtime/contrats/strict et certificat root.

Nouvelle revue : actual applyCast ne revalide pas encore une interdiction
apparue apres generation d'une action. generic_ast_coverage prepare un
addendum borne avec vraie application GY/library, controle hand et source
retiree, avant paiement. Root a accepte et applique le garde68CFD3;240tests moteurCI3 passent
(compile108.604s/execute1.949s), dont les7cas de cast effectif.
Impact et Cage ont leurs vrais inputs cohorte06 relies aux IR du candidat,
les scenarios relies sont installes dans les DATA durables; leur gate fraiche
est en cours. Les585DATA parser durables passent Native et Full.

Cargo lane ROOT. Aucun agent ne modifie production ni ne lance Cargo.
card_structure_skill analyse le prochain correctif et prepare les preuves
Impact; generic_ast_coverage possede l'addendum cast; Mana reste staging.

## Mana : chantier séparé, entièrement staging

generic_trigger_events : `.tmp/night-reflecting-mana-types-01/phases`.
Fondation,03c,04a,04b,04c1 et04c2 composés :20Rust/41tests typecheckés dans
preview isolée en18.156s, zéro test exécuté. Code actif inchangé; vraie intégration,
revue et preuves de comportement restent requises.04c3 transport legal en préparation.
TYPEwitnesses et sorties exécutables restent distincts; NoMana n'est pas Err.
Préflight avant coûts aussi pour followups des activations mana immédiates.
Ancien adapter indépendant tant que transportResult incomplet; aucun Err→empty.
Frontend Reflecting Pool held, zéro carte migrée; pas de changement d'heuristiqueX.
Rebaser tout ancien hunkmod.rs après les intégrations actuelles, jamais de copie
du module géant. Lane commune `.tmp/targets/card-migration`.

**Première vérification metadata terminée** après le preview isolé
`.tmp/night-mana-preview-typecheck-01/certificate.json` (SHA1D89E789…6046BF).
Commande offline metadata/tests seulement, pas de build/link ni modification
du manifest/lock principal.41tests typecheckés ne constituent pas leur
exécution ni une couverture de carte; production17319 conservée.

**Metadata02 terminee et lane rendue** : preview frais
`.tmp/night-mana-preview-typecheck-02/certificate.json`, SHA
01E0A4B57D581B337889A71E413D008403DF7F4FCEB11A1387E79514D113F8ED.
49tests typecheckes/0execute en17.718s;159inputs root et14manifestes exacts.
Root a accepte cette preuve limitee;04c3b transport Result en staging,
aucune nouvelle compilation autorisee pendant les gates runtime root.

Historique préservé dans
`migration-baselines/oracle-current-archive-before-17319-2026-11-14.md` et les
rapports de checkpoint; ce fichier garde seulement le contexte utile au handoff.

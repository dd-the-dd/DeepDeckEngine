# Comptage de la source, incarnations et audit UTF-8

Le checkpoint runtime est accepté. **146 tests privés, 652 scénarios DATA et
4 443 assertions passent ; le préfixe historique de 145 cartes est certifié.**
Ce correctif ne change aucun score parser : 3 568 cartes complètes non vides,
17 224 capacités natives, dix sets à287 cartes. Les caps parser `other` et LKI
des cibles restent présents en attendant leurs promotions distinctes.

## Contrat partagé

Le comptage retire la source seulement si l'objet actuel appartient effectivement
à l'ensemble compté et correspond à son incarnation capturée. Il vérifie le
joueur, le contrôleur/propriétaire et le prédicat actuel. Une source partie ou
revenue comme nouvel objet n'est pas soustraite. Une copie historique non suivie
avec le même identifiant reste indéterminée ; elle n'autorise aucune identité inventée.
Le chemin chosenCreatureType partage ce contrôle mais garde son domaine historique
à un joueur ; aucune généralisation allPlayers/ownership n'est revendiquée pour lui.

La préparation d'une véritable entrée précède toute capture ou publication :
landPlayed, ETB, tokenCreated et doorUnlocked notamment. La publication ETB ne
réattribue plus l'identité. Un changement de contrôle la conserve. Les batchs
installent tous les entrants avant publication. Les constructions déterministes
et les horloges utilisent le même point de préparation.

La revue indépendante a refusé l'allocateur initial basé sur copy_sequence :
ce compteur pilote aussi la rotation des cartes générées pendant l'entraînement.
Le compteur dédié GameState.battlefield_incarnation_sequence est sérialisable,
défaut0 pour les anciennes sauvegardes et absent du JSON à0. Clone/rollback le
conservent. Les tests vérifient que copy_sequence et la rotation restent identiques.

## Preuves, corrections et temps

La première compilation privée prend87,383s, exécution1,016s :15/16 verts.
Le test blink utilisait une référence self non gérée par resolve_target et faisait
donc un no-op ; il utilise maintenant une véritable chosenTarget, avec toutes ses
assertions originales de départ, incarnation et comptage conservées.
La partition suivante prend8,303s de compilation et1,880s d'exécution :144/145.
Un ancien test réinsérait manuellement une carte et dépendait de l'ancien estampage
tardif ETB. Sa construction prépare désormais l'entrée avant clone/publication,
sans modifier ses attentes. La partition corrigée passe145/145 (8,582s/1,679s).

Le test distinct du bonus compté est déplacé hors du module Oracle-gated historique
vers le fichier dédié numeric_evaluation_tests.rs, avec mêmes bornes et assertions
P/T4/4. Partition finale146/146 : compilation9,482s, exécution2,027s.
Les scripts conservent chaque échec et son rapport ; ces durées ne sont pas
additionnées aux intervalles de développement qui comprennent déjà leurs attentes.
DATA652/4443 : build29,436s, exécution3,435s. Le contrôle des consommateurs backend
`cargo check --locked -p mtg-engine --tests --features ci-unit-tests-1` passe en31,933s.

## Audit historique et encodage

Le premier audit strict s'arrête à Brazen Borrower, position131 : le script lisait
du JSON UTF-8 sans BOM dans l'encodage Windows par défaut. Le séparateur de type
Adventure était corrompu avant la factory. Les sources Oracle originales restent
inchangées. Toutes les lectures JSON du script utilisent désormais UTF8 explicite.
Trois preuves sans Cargo contrôlent cartes, queue et cache ; la suite AST/cache
complète passe aussi. Ses assertions de frontières suivent la séparation acceptée
parseur/runtime et le pont modèle, sans réintroduire leur ancienne dépendance cyclique.

Le replay corrigé certifie145/145, en17,421s, zéro build et zéro import réseau.
395 spécifications contract/runtime sont concernées ; le dernier passage exécute
169 preuves et réutilise les autres certificats valides du passage précédent.
Ne pas additionner les comptes d'exécution des deux passages comme des cas uniques.
La garde globale a48 kinds primitifs, zéro composite, zéro violation/avertissement.
Avec la queue elle signale11 collisions de noms revues : negate/counterSpell sont
des primitives et les autres occurrences des exemples/tests HTTP/local-app.

Certificat : `.tmp/night-source-count-root-01/certificate.json`, avec parent
historique conservé. Freeze : `accepted-source-freeze.json`. Timings :
`.tmp/card-migration-timings/night-source-count-root-01.json` et les JSON de chaque
passage dans le même dossier de preuve. Les trois nouveaux DATA sont promus dans
`crates/deepdeck-runtime-fixture-runner/fixtures/generic-source-count-incarnations`.

La cohorte suivante est adoptée dans `.tmp/night-oracle-cohort-02/adoption.json` :
98 cartes non résolues conservées et deux ajouts, Impact Tremors et Rapid
Hybridization. Ordre, provenance, textes et faces sont vérifiés ; baseline100 cartes
non résolues/204 capacités dont27 natives. Le pool étendu conserve les450 premières
identités exactes ; aucun curseur n'est supprimé manuellement.

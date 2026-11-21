# Suppression de la dépendance circulaire parser / runtime

Le parser utilisait le moteur comme dépendance de développement pour huit
assertions `rule_is_executable`. Le runtime peut lui-même importer le parser
avec la fonctionnalité `oracle-compilation`. Rust-analyzer signalait cette
boucle lorsqu'il chargeait les crates et leurs tests ensemble.

La dépendance de développement et la réexportation de test du moteur sont
retirées de `deepdeck-oracle`. Les huit vérifications d'exécutabilité passent
dans `deepdeck-oracle-engine-contracts/tests/parser_runtime_boundary.rs`,
avec les mêmes textes et une vérification des primitives émises. Les autres
assertions des trois fichiers de tests du parser sont conservées exactement.
Le parser de production et les règles exécutées par le moteur ne changent pas.

Validation :

- 3 tests de destruction de collections, 15 tests de séquences et 4 tests de
  permissions de terrain passent dans la bibliothèque autonome du parser.
- Le test de contrats parcourt les 8 cas déplacés et passe.
- Le graphe des packages locaux est acyclique, dépendances optionnelles et de
  développement comprises ; `cargo metadata --locked` accepte le lockfile.
- Une nouvelle instance de rust-analyzer ne signale plus `cyclic deps` et
  retrouve `parse_oracle_card_native` à la ligne 141 de `pipeline.rs`.
- L'index Git est inchangé. Aucun commit ou changement de branche effectué.

Les rapports et les temps séparés de compilation/exécution sont dans
`.tmp/parser-runtime-cycle-fix/`. Le premier filtre de tests utilisait le nom
du fichier au lieu du chemin du module ; il a été refusé sans exécuter de test.
Les rapports `*-verified.json` contiennent les exécutions corrigées.

La configuration locale ignorée `.vscode/settings.json` ajoute le dossier
`src` à `rust-analyzer.vfs.extraIncludes`. Ce réglage d'indexation corrige
séparément la navigation dans les sources partagées par les crates.

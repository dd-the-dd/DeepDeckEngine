# Contrats de carte et banc DATA réutilisable — 14 novembre 2026

Le pont modèle/contrats et son wrapper de test sont acceptés après revue root.
Le banc vérifie désormais une carte Oracle complète et sa face sélectionnée sans
activer `oracle-compilation` dans le moteur. **372 contrats DATA passent :
365 existants conservés et 7 nouveaux.** Cela n'ajoute aucune carte au compteur
de migration et ne certifie pas individuellement leur simulation.

## Frontière de compilation

```text
deepdeck-contract-fixture-runner
  └─ deepdeck-oracle-engine-contracts
       ├─ deepdeck_oracle                → parsing Oracle / factory de faces
       ├─ contracts::model               → adaptateur partagé src/model.rs
       └─ deepdeck_engine_runtime         → primitives, default-features=false
                                          oracle-compilation désactivée
```

La crate de contrats réexporte les interfaces publiques du parseur et du runtime
et inclut l'adaptateur existant par son chemin ; elle ne copie aucune de ces
implémentations. Le runner dépend maintenant de cette frontière de jonction.
Le changement Cargo.lock est un changement d'arête, sans nouveau package/version.
Le parseur est bien compilé dans sa propre crate ; le moteur garde sa frontière
sans compilation Oracle. Lors du build DATA mesuré, l'artefact moteur est
`fresh=true`, avec `features=[]` : il a été réutilisé.

Les fixtures v1 `rule`/`rules` restent compatibles. Le mode `input` utilise le
vrai `PlayableCardInput` avec requête Oracle complète, layout, faces ordonnées et
éventuel faceId. Le préflight structurel précède toujours le parsing. Les statuts
parser/moteur sont des assertions facultatives portant sur la face sélectionnée.
Une entrée `input` ne peut pas coexister avec une spécification de règle, même
`rules: []`. Une compilation en erreur échoue explicitement ; un texte reconnu
unsupported dont le filtrage donne zéro règle reste **incomplete**, jamais
executable par vérité vide de `all([])`.

Les sept nouvelles entrées sont Sink face avant/arrière, Unsubstantiate, Sneering
Shadewriter, Fugitive Listener, Garruk's Uprising et un négatif synthétique
unsupported. Les entrées multifaces conservent toutes les faces originales.
Le manifeste source conserve leurs hashes et leur provenance ; le rapport root
372/372 remplace le statut « pas encore désérialisé » de l'ancien handoff staging.

## Preuves root, dans l'ordre des passages

Les valeurs ci-dessous viennent des JSON de mesures, pas d'estimations. Les temps
d'exécution des deux harnesses incluent leur lancement par le wrapper ; leur
temps interne Rust, affiché dans stdout, est précisé séparément.

| Passage | Résultat | Attente Cargo (s) | Compilation / liaison ou appel Cargo (s) | Exécution mesurée (s) | Total wrapper (s) |
|---|---|---:|---:|---:|---:|
| Tests privés runner, 06:49 UTC | 4/4 | 0.007 | 3.988 | 4.265 | 8.318 |
| Construction runner puis DATA, rapport écrit 07:00 UTC | 372/372, 0 invalide | non mesurée | 4.654009 | 0.6346918 | non mesuré |
| Contrat factory, 07:01 UTC | 2/2 | 0.007 | 3.169 | 0.898 | 4.119 |
| Wrapper avec `-Build`, 07:10:43 UTC | 372/372 | 0.0057716 | 0.2203191 | 0.2526799 | 3.1259433 |
| Wrapper sans `-Build`, 07:10:49 UTC | 372/372 | 0 | 0 | 0.1705331 | 1.4731486 |
| Contrôles du cache, 07:11 UTC | 11/11, aucun Cargo | non chronométrée | aucun Cargo | non chronométrée | non chronométré |

Pour le passage DATA de 07:00, l'heure est celle d'écriture du rapport, pas un
timestamp de début enregistré. Ses durées complètes stockées sont
4.654008999998041 et 0.6346918000017467 secondes ; seul l'affichage du tableau
est arrondi. Le temps interne des harnesses Rust est 0.06 s pour les quatre
tests runner et 0.03 s pour les deux tests factory. Leur écart avec les durées
wrapper est de l'orchestration/lancement, pas du travail de test Rust mesuré.

`build_occurred=true` signifie que le wrapper a appelé Cargo. Au passage avec
`-Build` de 07:10, `cargo_compiled_anything=false` : les artefacts étaient déjà
frais, ce n'est pas une nouvelle compilation moteur. Le passage suivant n'appelle
pas Cargo. Son total de 1.4731486 s comprend 0.1705331 s d'exécution DATA et
1.3026155 s pour le reste du wrapper (vérifications, empreintes, orchestration).
Cette différence calculée n'est pas un profil détaillé de ces sous-opérations.
Ces passages à cache chaud ne constituent pas une comparaison avant/après de
compilation froide, ni un benchmark isolé du parseur.

## Réutilisation et authenticité du runner

`scripts/run-contract-fixture.ps1 -Fixture <fichier-ou-dossier> -Build` construit
le runner dans le lane Cargo partagé puis écrit sa métadonnée. Sans `-Build`, il
vérifie l'empreinte des sources/manifests/catalogues et le hash de l'exécutable ;
un binaire absent, ancien, modifié ou sans métadonnée est refusé. Le catalogue
externe local configuré par MTG_CARD_CATALOG_PATH fait partie du contexte vérifié.

Les JSON de fixtures sont exclus de l'empreinte du binaire : ajouter une carte
DATA peut donc se tester sans compilation tant que les sources restent figées.
Les arguments fichiers/dossiers sont transmis ensemble au runner, qui conserve
sa découverte récursive, déduplication de fichiers et erreur sur IDs dupliqués.

Les onze contrôles synthétiques couvrent : runner frais sans build, argument
dossier intact, ajout DATA seul, doublon d'ID toujours en erreur, changement de
catalogue refusé, catalogue externe local authentifié, contenu externe modifié
refusé, source modifiée refusée, binaire altéré refusé, métadonnée absente refusée,
exécutable absent refusé. Le dernier rapport individuel du smoke est en échec
attendu pour « exécutable absent » ; le bilan des onze contrôles est bien vert.
Ce smoke synthétique ne certifie pas le runtime courant.

## Limites et suite

Un contrat executable vérifie la représentation compilée et les capacités
déclarées du moteur ; les scénarios DATA runtime distincts restent nécessaires
pour prouver les effets en partie. Ces 372 fixtures ne représentent ni toutes
les cartes Magic ni une hausse de couverture globale. Aucun test de comportement
moteur complet n'a été ajouté ou supprimé par ce pont.

Les propositions source-incarnation/count et cible-numérique/LKI restent séparées
de cette acceptation. Feed the Swarm demeure unsupported au cap LKI ; le pont ne
résout pas les valeurs de cible après départ et n'autorise pas un remplacement
par zéro. Le benchmark chaud montre un chemin de travail utilisable sans Cargo
par carte, avec un coût restant d'empreinte/orchestration explicitement mesuré.

## Artefacts de preuve

- `.tmp/night-model-contract-bridge-01/runner-unit-root.json` et stdout associé.
- `.tmp/night-model-contract-bridge-01/data-root-report.json`, build-root.stdout,
  data-root.stdout ; binaire DATA SHA256
  `AFD29EBC3C896A567088CDE92CC0BBC22226B1BC287A204945D11E4E9F227B1B`.
- `.tmp/night-model-contract-bridge-01/factory-contract-root.json` et stdout.
- `.tmp/night-model-contract-bridge-01/wrapper-build-root.json` et
  wrapper-cached-root.json : empreinte commune
  `CA161A9DB50A003F3610B716A62610382FC63D02AB09D7C71D70947FFC4AB8AC`,
  695 fichiers d'entrée authentifiés, 372 résultats par passage.
- `.tmp/night-contract-runner-wrapper-01/root-smoke/smoke-results.json` et
  last-wrapper-report.json ; ce dernier est un négatif attendu.
- `.tmp/night-model-contract-bridge-01/fixture-manifest.json` : 7 nouveaux cas.

Les hashes des fichiers sources de ce rapport sont épinglés dans le manifeste
du staging `.tmp/night-bridge-wrapper-report-01/MANIFEST.json`.

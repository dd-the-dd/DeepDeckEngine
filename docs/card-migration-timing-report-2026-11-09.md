# Rapport de temps — migration native des cartes

Ce rapport sépare le temps de développement, la compilation, l'exécution des
tests et le replay de la queue. Les durées proviennent des sorties mesurées de
Cargo et de `audit-card-set.ps1`; les durées de développement antérieures qui
n'avaient pas de chronomètre sont volontairement marquées non instrumentées.

## Résumé actuel

| Mesure | Avant les optimisations | Maintenant |
|---|---:|---:|
| Replay strict courant | 53,96 s | 15,56 s chaud; préfixe 44, 44 dossiers contrat+runtime certifiés |
| Garde architecturale | 19,87 s | 5,43 s sur 168 fichiers de production et 136 fichiers de tests |
| Build parseur incrémental | — | 0,53 s |
| Audit du préfixe courant | 0,92 s pour 2 cartes | 0,58 s debug pour 45 cartes examinées, préfixe 44 |
| Build contrat/runtime | non disponible | 0,63 s / 0,26 s au replay data-driven courant |
| Fixture runtime chaude | test Rust par carte | 21,3 ms Stock Up; 45,9 ms Llanowar |

Le parseur n'est plus le goulot principal à l’exécution : l'audit release de
41 cartes prend 0,73 s. Le replay courant a toutefois pris 335,45 s, dont
187,89 s de build parseur et 101,99 s de build contrat. Les tests eux-mêmes
restent courts. Le découpage des crates accélère les passages chauds, mais une
invalidation de production coûte encore plusieurs minutes; la granularité de
compilation demeure donc le chantier de performance principal.

Le rapport strict officiel après Rest in Peace, Rampant Growth et Scalding Tarn
confirme un préfixe de 44 cartes natives, primitives et exécutables,
61 capacités natives et Watery Grave comme première carte legacy au rang 45.
Son audit parser debug a pris 0,58 s.
Les rangs 35 à 40 ont ajouté la perturbation de main atomique, la suppression
de déclencheurs, les coûts additionnels alternatifs, deux cartes déjà couvertes
par généralisation et l'octroi temporaire de plusieurs mots-clés à un ensemble
non ciblé. Chaque entrée du manifeste épingle aussi le digest
sémantique de l'IR réellement auditée : une fixture historique ne peut plus
certifier silencieusement une nouvelle IR divergente. Le replay officiel chaud
après Ponder confirmait
un préfixe strict de 22 cartes et Farseek comme première carte legacy au rang
23. Il avait pris 18,09 s : 5,95 s de garde architecturale, 1,45 s de build
parseur, 2,01 s d'audit et 0,04 s d'exécution des preuves Ponder. Le replay
précédent après Multiversal
Passage avait un préfixe strict de 21 cartes et Ponder comme première carte
legacy. Ce
rafraîchissement a pris 49,97 s : 6,13 s de garde architecturale, 32,24 s de
build parseur, 0,92 s d'audit et 1,18 s d'exécution des preuves contrat/runtime.
Le passage précédent après Cultivate avait un préfixe strict de 20 cartes et
Multiversal Passage comme première carte legacy. Ce
rafraîchissement a pris 44,81 s : 5,75 s de garde architecturale, 30,21 s de
build parseur, 1,00 s d'audit et seulement 0,05 s d'exécution des preuves
contrat/runtime de Cultivate. Le replay précédent après Force of Negation avait
un préfixe strict de 19 cartes et Cultivate comme première carte legacy.
Ce replay de rafraîchissement a pris 64,16 s, dont 45,04 s de build parseur,
0,90 s d'audit des vingt premières cartes et seulement 3,03 s pour compiler et
exécuter les 38 preuves data-driven. Le premier rafraîchissement après
Disdainful Stroke avait pris
499,36 s : 39,96 s de build parseur, 2,15 s d'audit, 66,16 s de build des
anciens contrats et surtout 353,01 s de build des anciennes partitions runtime.
Les tests eux-mêmes n'ont pris que 24,20 s. Le replay suivant, sans build ni
preuve périmée, avait pris 15,35 s.

## Mesures par carte

Les temps « exécution » excluent la compilation. Une compilation froide est
réutilisable par les cartes suivantes de la même partition.

| Carte | Développement | Build parseur | Tests parseur | Build contrat | Test contrat | Build runtime | Test runtime | Résultat |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| Swords to Plowshares | non instrumenté | 9,80 s | 1,65 s exact; régression chaude ~3,7 s | 67 s | ~0,05 s lot | 135 s | 0,01 s | certifiée |
| Erode | non instrumenté | 1,22 s incrémental | ~9 s lot | 62 s | ~0,09 s lot | 119 s | 0,03 s | certifiée |
| Negate | non instrumenté | 30,74 s | ~9,6 s lot | 73 s | ~0,09 s lot | 169 s | 0,01 s | certifiée |
| Llanowar Elves | non instrumenté | 31,21 s | ~6,1 s lot | 61 s | ~0,05 s lot | 144 s; correction 13,31 s | 0,01 s | certifiée |
| Stock Up | non instrumenté | 27,79 s | ~4,3 s lot | 58,28 s local; 193,70 s amorçage ledger | 0,02 s local; 27,78 s tous harnesses | 246 s froid local; 59,01 s ledger | 0,13 s échec de test; 0,02 s succès | certifiée; préfixe 5 |
| Force of Will | non instrumenté | cache partagé | 5/5 verts | runner stable | fixture verte | runner stable | 3 scénarios Force verts; lot chaud ~36 ms | certifiée |
| Strategic Betrayal | non instrumenté | 1,22 s chaud | 0,41 s; exact 0,01 s | runner stable | lot 5/5 en 20,03 ms | 34,18 s après ajout du cimetière initial | lot 7/7 en 739,73 ms | certifiée |
| Duress | non instrumenté | 31,29 s invalidé par Wasteland | 5/5 en 3,00 s | runner stable | 25,6 ms | runner stable | 65,7 ms | certifiée; manifeste ajouté |
| Arcane Signet | instrumenté partiellement | 28,49 s initial; 1,50 s chaud | 5/5 en 0,12 s | runner stable | Arcane 1/1 en 11 ms; lot 9/9 en 22 ms | 22,63 s après extension générique du runner | 2/2 Arcane en 20 ms; régression 11/11 en ~48 ms | certifiée; manifeste ajouté |
| Wasteland | instrumenté partiellement | 1,505 s chaud | 4/4; exécution 0,01 s; audit carte 7 326 µs | runner stable | fixture 1/1 en 0,410 s; lot 9/9 en 0,344 s | build partagé avec API d’activation non-mana | fixture 6/6 en 0,034 s; régression 12/12 en 0,095 s | certifiée; manifeste ajouté |
| Consign to Memory | instrumenté partiellement | 40,30 s initial; 2,94 s régression | 4/4 en 0,984 s; voisines 19/19 en 8,958 s; audit 11 746 µs | runner stable | lot 9/9 en 27 ms | 166,13 s pour la partition moteur après correction du contre | fixture Consign verte; lot 12/12; tests triggered ability et Replicate 1/1 chacun | certifiée; baseline legacy 379 312 µs |
| Requiting Hex | instrumenté | 32,445 s initial; 10,929 s après patch; 0,229 s chaud | 4/4 en 0,06 s lors de la revue | runner stable | 1/1 en 0,014 s | >2 min pour la nouvelle partition de test; 3,90 s chaud | fixture 3/3; e2e exact 1/1 en 45 ms | certifiée; deux capacités natives; runner sur pile 8 Mio |
| Brainstorm | instrumenté | 25,056 s échec visibilité; 29,90 s succès ciblé | 4/4 en 0,02 s; régressions voisines 18/18 | runner stable | 1/1 en ~34 ms | 2 min 42 s pour la partition runtime #2 | fixture 3/3 en ~30 ms; test d’ordre exact 1/1 en <10 ms | certifiée; regex legacy retirée |
| Flashfreeze | instrumenté | 19,456 s premier diagnostic; 24,516 s succès; 1,618 s chaud | 5/5 en 0,01 s; counter 10/10 | runner stable | 1/1 en 0,151 s | runner stable | 3 scénarios en 0,141 s | certifiée; filtre couleur union natif |
| Path to Exile | réévaluation | 1,30 s chaud | 3/3 en 1,71 s; audit préparatoire 4 237 µs | runner stable | 1/1 direct | runner stable | acceptation 7/7; refus 6/6 | déjà native; preuves data-driven ajoutées |
| Counterspell | réévaluation | aucun build additionnel | couvert par simple-counter 3/3; audit 161 µs | runner stable | 1/1 direct | runner stable | fixture exacte 5/5 | déjà native; manifeste ajouté |
| Disdainful Stroke | instrumenté | 10,27 s initial; 1,21 s incrémental | 6/6 en 0,01 s; counter voisins 15/15 | 14,13 s initial; 57,18 s après refactor partagé | test ciblé 1/1; fixture 1/1 en 12 ms | 13,90 s ciblé; 128,86 s après refactor partagé | e2e 1/1 en 34 ms; fixtures MV4/MV3 9/9 en 20 ms | certifiée; préfixe 18; `==`, `≤`, `≥` partagés |
| Force of Negation | instrumenté | refactor + revue indépendante | 9,78 s compile; 11/11 ciblés; gate orchestrateur 20,72 s | runner stable | 2/2 fixtures | 147 s après refactor atomique partagé | 3/3 Rust en 0,02 s; 3/3 fixtures, 12/12 assertions | certifiée; préfixe 19; condition et contre atomique partagés |
| Cultivate | instrumenté | extension générique recherche/révélation/partition | 3/3 exact/voisin/near-miss; régression recherche 7/7 en 2,13 s | runner stable; build ledger 0,17 s | fixture 1/1 en 0,02 s | runner stable; build ledger 0,35 s | 3 scénarios 0/1/2 cartes, 30/30 assertions en 0,03 s | certifiée; préfixe 20; première legacy suivante Multiversal Passage |
| Multiversal Passage | instrumenté | choix typé + condition de coût impayé + état d’entrée | 3/3 exact/voisin/near-miss; garde 3/3; root chaud 0,34 s | runner stable; 2 contrats en 0,02 s | 2/2 | runner étendu génériquement; build 23,92 s initial | pay/refuse 2/2, 16/16 assertions; root 0,04 s | certifiée; préfixe 21; première legacy suivante Ponder |
| Ponder | instrumenté | top-cards lié + ordre + mélange optionnel + pioche générique | exact/sibling/near-miss 3/3; voisins+garde 9/9; root chaud 0,46 s | runner stable | 2/2 en 0,02 s | runner stable | shuffle oui/non 2/2; ordre observé par la carte piochée; root 0,03 s | certifiée; préfixe 22; replay chaud 18,09 s; première legacy suivante Farseek |
| Farseek | instrumenté partiellement | union générique de sous-types + recherche directe implicite 0..1 | exact/sibling/near-miss 3/3; recherche 10/10; architecture/provenance 8/8; root 13/13 | runner stable | 1/1 | runner stable | carte admissible trouvée + aucune carte admissible 2/2 | certifiée; préfixe 23; 30 capacités natives; première legacy suivante Flooded Strand; replay non chronométré dans le rapport courant |
| Flooded Strand | instrumenté | séquence de coûts atomiques tap + vie + sacrifice, puis recherche par union de sous-types | build focalisé 10,40 s; exact/sibling/near-miss 3/3; régressions Wasteland/Farseek/provenance/architecture 15/15; exécution 0,02 s | runner stable; build incrémental 0,26 s | 1/1 en 0,03 s | runner stable | coûts réellement payés, Island trouvée non engagée, mélange et recherche observés; 9/9 assertions en 0,03 s | certifiée; branche fetch-land legacy retirée; préfixe 24 |
| Spell Snare | réévaluation | comparaison générique de valeur de mana déjà introduite par Disdainful Stroke; aucun changement de production | build incrémental 1,55 s; test exact ground-truth 1/1 en 0,01 s | runner stable | 1/1 | runner stable | MV2 contrée 5/5; MV3 inéligible 4/4 | certifiée sans nouveau code parseur; préfixe 25; première unsupported suivante Blasphemous Act |
| Blasphemous Act | instrumenté | réduction générique `{N}` par permanent correspondant + destinataire `each <permanent selector>` | première compilation 31,86 s; rebuild 10,43 s; exact/sibling/5 near-miss 3/3; kicked-damage 13/13; exécution 0,01 s | runner stable | coût + dommage 2/2 en 0,31 s | runner stable | huit créatures réparties entre deux joueurs meurent, artefact intact; sept créatures rendent le sort indisponible; 2/2, 17 assertions en 0,27 s | certifiée; branche exacte et regex `each_creature` retirées; préfixe 26 |
| Polluted Delta | réévaluation | même séquence de coûts et même union de sous-types que Flooded Strand; aucun changement de production | build incrémental 2,24 s; exact 1/1 en 0,01 s; famille fetch complète 4/4 | runner stable | 1/1 | runner stable | vie payée, source sacrifiée, Swamp trouvée non engagée, Forest exclue, recherche + mélange; 9/9 assertions | certifiée par généralisation; préfixe 27; première mixte suivante Starting Town |
| Starting Town | instrumenté | état d’entrée conditionnel + deux capacités de mana composées | parser 22/22; compilation partition moteur 158 s | runner stable | 3 contrats | runner stable | 3 scénarios runtime, dont entrée tardive et couleur choisie | certifiée; préfixe 28; 3/3 capacités natives |
| Beast Within | instrumenté | zone change + snapshot du contrôleur + jeton vanilla générique | parser/régressions 10/10 | runner stable | 1/1 | runner stable | permanent volé détruit, jeton créé pour son contrôleur courant | certifiée; préfixe 29 |
| Misty Rainforest | réévaluation | réutilisation directe de la famille fetch-land | aucun code de production supplémentaire | runner stable | 1/1 | runner stable | coûts, Forest/Island, recherche et mélange prouvés | certifiée par généralisation; préfixe 30 |
| Shoot the Sheriff | instrumenté | batch Outlaw enregistré puis abaissé vers `NOT(OR(subtypes…))` | froid 29,19 s; chaud 0,15 s; parser 4/4 | build partagé 118,06 s | 1/1 en 0,01 s | runner fixture stable | 9/9 assertions; Bear détruit, Outlaws/changelin/artefact exclus | certifiée; préfixe 31; aucune notion runtime « Outlaw » |
| Disruptor Flute | instrumenté | choix de nom persistant + taxe de lancement + interdiction d’activation avec exception mana | audit exact en 746 µs; parser 3/3, régressions 11/11 | partagé avec Shoot | 1/1 en 0,01 s | 181,10 s froid | test 1/1 en 0,04 s | certifiée; préfixe 32; 4/4 capacités natives |
| Talisman of Dominance | instrumenté | choix de mana générique + séquence d’effets targetless + référence typée `This artifact` | build ciblé 9,16 s après correction Oracle; parser 6/6 | 0,33 s au replay | 1/1 en 0,02 s | 0,44 s au replay | 6/6 assertions en 0,03 s | certifiée; préfixe 33; mana, dommage et pile vide prouvés |
| Assassin’s Trophy | instrumenté | sélecteur générique `an opponent controls` + snapshot du contrôleur + recherche optionnelle | parser 3/3; régression recherche 4/4 | runner stable | 1/1 en 0,02 s | runner stable | 8/8 assertions | certifiée; préfixe 34; aucun changement moteur de production |
| Thoughtseize | instrumenté | reveal + choose + discard atomiques, puis perte de vie séparée | build 12,07 s | 7/7 en 2,33 s | build 19,07 s | 1/1 en 0,01 s | runner stable | 8/8 assertions | certifiée; préfixe 35 |
| Torpor Orb | instrumenté | événement d'entrée typé + suppression générique de création de triggers | build 53,27 s | 3/3 en 0,01 s | build 93 s | 1/1 en 0,01 s | runner stable | 6/6 assertions | certifiée; préfixe 36 |
| Bitter Triumph | instrumenté | coût additionnel avec choix `discard card OR pay 3 life`, destruction réutilisée | build 26,64 s | 5/5 en 1,18 s | build 59,49 s | 2/2 en 0,15 s | runner stable | branches discard 5/5 et vie 4/4 | certifiée; préfixe 37 |
| Lotus Petal | réévaluation | coût tap/sacrifice + mana de couleur choisie déjà généralisés | aucun changement de production | exact 1/1 | runner stable | 1/1 | runner stable | 5/5 assertions | certifiée sans nouveau code; préfixe 38 |
| Nature’s Lore | réévaluation | recherche Forest générique, déplacement non engagé et mélange déjà couverts | aucun changement de production | exact 1/1 | runner stable | 1/1 | runner stable | 8/8 assertions | certifiée sans nouveau code; préfixe 39 |
| Heroic Intervention | instrumenté | ensemble non ciblé `permanents you control` + liste de mots-clés + durée | build 27,27 s | 3/3 en 0,04 s | build 61 s | 1/1 en 0,01 s | build runner 19,93 s | deux fixtures, 12/12 assertions | certifiée; préfixe 40; deux `grantKeyword` primitifs |
| Karakas | instrumenté | supertype `Legendary` partagé + destination typée `OwnerHand` + activation à un coût | build parser 37,64 s | exact/sibling/near-miss 3/3; régressions zone-change 15/15 | build contrat 1 min 27 s | 1/1 en 0,04 s; fixture contrat 1/1 | runner incrémental 0,15 s | 9/9 assertions, propriétaire différent du contrôleur | certifiée; préfixe 41; replay chaud 12,36 s |
| Rest in Peace | instrumenté | batch de zones `all graveyards` + remplacement générique objet/propriétaire/origine/destination | build parser partagé 30,48 s | exact/siblings/near-miss 3/3 | fixture multi-règles : build runner 5,24 s une fois | 48/48 fixtures du corpus; preuve courante 1/1 | runner stable | entrée 8/8 + remplacement 6/6 | certifiée; préfixe 42; aucun changement moteur |
| Rampant Growth | réévaluation | recherche Basic ∩ Land déjà généralisée | aucun changement de production | exact 1/1 | runner stable | fixture 1/1 | runner stable | 9/9 assertions | certifiée sans nouveau code; préfixe 43 |
| Scalding Tarn | réévaluation + lexique | fetch-land déjà généralisée; cinq sous-types de terrain de base promus au lexique | build incrémental 11,45 s | exact 1/1, zéro UNKNOWN | runner stable | fixture 1/1 | runner stable | 9/9 assertions | certifiée; préfixe 44 |

## Pourquoi une carte semble longue

1. Les tests eux-mêmes durent généralement de 0,01 à 2 secondes.
2. Le gros module runtime peut demander 2 à 3 minutes à compiler après l'ajout
   d'un fichier de test ou le changement de partition.
3. Avant le ledger, chaque replay perdait l'information que les contrats et le
   runtime avaient déjà été prouvés.
4. L'ancien audit rescannait la queue et les fichiers de production de façon
   quadratique; sa garde seule coûtait environ 20 à 25 secondes.
5. Une erreur trouvée tard au runtime entraînait une seconde compilation. Pour
   Llanowar, le premier test vérifiait un helper interne au mauvais niveau;
   la correction a coûté 13,31 s de recompilation, mais seulement 0,01 s de test.
6. Deux répertoires Cargo étaient alimentés en parallèle : `target` occupait
   8,5 Go et `.tmp/targets/card-migration` 21,72 Go. Avec seulement 0,02 Go
   libre, LLVM a échoué pendant Stock Up. Le cache `target` dupliqué a été
   nettoyé (`cargo clean`, 8,5 Gio récupérés) et les commandes suivantes sont
   centralisées dans `.tmp/targets/card-migration`.

## Amorçage versus cadence normale

Le premier amorçage du ledger pour Swords, Erode et Negate a pris 491,28 s :

- compilation contrats : 125,33 s;
- exécution contrats : 20,27 s, surtout le lancement de nombreux harnesses;
- compilation runtime : 313,24 s;
- exécution runtime : 3,45 s;
- tout le reste : environ 29 s.

Ce coût n'est pas payé à chaque replay. Le replay suivant a pris 27,84 s sans
aucun build; après optimisation de la garde, le replay est descendu à 14,42 s.

L'amorçage Stock Up a encore pris 330,00 s, dont 193,70 s de build contrat et
59,01 s de build runtime, pour des scénarios utiles exécutés en 0,02 s. Cette
mesure justifie le remplacement des tests Rust par carte par deux runners
stables alimentés par fixtures JSON.

## Banc piloté par les données

Le coût marginal d'une nouvelle carte n'inclut maintenant aucune compilation
si son IR utilise des capacités déjà présentes dans les runners. Mesures prises
directement dans le processus, avec le binaire runtime partagé déjà construit :

- Stock Up : 21,3 ms;
- Llanowar Elves : 45,9 ms, incluant la recherche d'action légale, le paiement
  de `{T}`, l'ajout de mana et le cas négatif de summoning sickness;
- les deux fixtures : 67,2 ms de comportement moteur cumulé.

Le manifeste des dix-neuf cartes certifiées ne référence désormais plus aucune
partition de test Rust par carte : ses 38 preuves (19 contrats et 19 scénarios
runtime) sont toutes des fixtures JSON. La dernière conversion historique a remplacé les
preuves historiques de Swords to Plowshares, Erode et Negate. Une vérification
indépendante du lot a donné 3/3 contrats puis 5/5 scénarios runtime, avec 30/30
assertions, après un build partagé de 112,31 s; l'exécution chaude du lot runtime
a ensuite pris environ 0,42 s. Les scénarios Swords et Erode prouvent maintenant
explicitement le cas où le propriétaire et le contrôleur du permanent diffèrent.

Le temps de lancement de l'outil ou du shell n'est pas attribué à la carte. Une
modification du runner ou du moteur exige encore un build Rust incrémental,
mais l'ajout d'une fixture JSON qui réutilise ses capacités ne le fait pas.

## Instrumentation obligatoire à partir de la prochaine carte

Pour chaque carte, le suivi doit désormais enregistrer :

- début/fin de l'analyse;
- début/fin de l'implémentation;
- temps de revue/correction;
- compilation et exécution de chaque porte séparément;
- temps d'amorçage de preuve;
- replay chaud final;
- delta de préfixe et nombre de cartes voisines débloquées.

Le temps de développement ne sera plus mélangé avec l'attente de Cargo. Cela
permettra de distinguer une grammaire difficile d'un simple cache froid.

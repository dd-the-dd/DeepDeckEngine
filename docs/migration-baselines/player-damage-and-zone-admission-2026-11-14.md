# Dégâts aux populations et restrictions de zones : checkpoint 1A733

Le lot est accepté. L'objectif de couvrir toutes les cartes reste actif.

La lecture partagée de populations de joueurs compose désormais une action
ordinaire de dégâts. Les nombres et les séquences réutilisent les enfants
existants. Un résultat transactionnel explicite rend les expressions X au
programme contextuel, qui vérifie leur producteur réel sur la bonne face.
Aucune opération combinant dégâts et gain de vie n'est ajoutée.

Les listes de zones composent les interdictions d'entrée et de lancement.
Le moteur capture les restrictions avant chaque mouvement simultané, après
l'enregistrement d'une éventuelle révélation mais avant le retrait des cartes.
Il examine le type de départ avant les transformations d'entrée. Une action de
lancement déjà préparée est aussi revérifiée avant tout paiement.

## Résultats mesurés

| Mesure | Avant 17319 | Après 1A733 |
|---|---:|---:|
| Objets locaux non vides entièrement natifs | 3 642 | 3 680 |
| Capacités natives | 17 360 | 17 442 |
| Cartes complètes, occurrences dans dix sets fixes | 289 | 293 |
| Cartes complètes uniques dans ces sets | 241 | 245 |
| Capacités natives dans ces sets | 1 531 | 1 538 |
| Cartes avec preuves individuelles enregistrées | 163 | 165 |

Les 17 360 anciennes IR sont strictement identiques, sans normalisation.
Le corpus global comprend 39 288 objets et 67 417 capacités; il contient aussi
des jetons et des objets particuliers. Les 3 144 objets vides canoniques sont
comptés séparément. Un succès de lecture global n'est pas une nouvelle preuve
individuelle d'exécution pour chacune des 38 cartes gagnées.

Les deux nouvelles preuves complètes portent sur **Impact Tremors** et
**Grafdigger's Cage**. Le registre conserve exactement ses 163 anciennes entrées.
Les dix sets sont les mêmes instantanés locaux que précédemment, sans nouvel
import : TRK, MBC, FRA, HOB, MSH, SOS, TMT, ECL, TLA et SPM. Leurs 2 368
occurrences représentent 2 319 identités Oracle; trois occurrences vides sont
séparées. Les gains complets viennent de FRA (+2), ECL (+1) et SPM (+1).

## Validation et temps

| Étape chronologique | Compilation / liaison | Exécution | Résultat |
|---|---:|---:|---|
| Unités du parseur | 39,328 s | 1,621 s | 923 passent |
| Construction du lecteur DATA puis Native | 15,393 s | 1,130 s | 585 passent |
| Compatibilité Full | 0 s | 0,778 s | 585 passent |
| Analyseur optimisé | 33,669 + 1,402 s | — | Binaire figé |
| Audit global | 0 s | 10,794 s | +38 cartes, +82 capacités |
| Unités moteur CI3 | 108,604 s | 1,949 s | 240 passent |
| Premier passage DATA moteur | 35,160 s | 4,119 s | 732/733 |
| Contrats complets | 3,251 s | 0,722 s | 396 passent |
| Correction des deux fixtures, nouveau passage DATA | 0 s | 3,637 s | 733/733, 4 874 assertions |
| Régression historique 145 cartes | Voir rapport détaillé | 33,084 s au total | 395 preuves fraîches |

Les durées d'exécution incluent les coûts du harnais et des processus lorsque
le wrapper les mesure. Elles ne constituent pas un coût de simulation par carte.
Les tests du parseur eux-mêmes affichent 0,11 s; ceux du moteur CI3, 0,69 s.
La première phase de revue/composition a duré 1 536 s et inclut coordination,
attente de frontière Cargo et revues croisées; ce n'est pas du temps de saisie.

Sur les dix sets, le temps interne du parseur passe de 0,736 à 0,739 s, et le
temps processus de 0,989 à 0,990 s. Cette mesure unique n'indique aucun gain
de vitesse. Aucun gain de performance global du moteur n'est revendiqué.

## Corrections et limites

Le scénario initial qui prétendait déplacer un permanent via `moveCards`
était invalide : cette primitive prend des cartes hors du champ de bataille.
Deux fixtures utilisent maintenant une véritable destruction et vérifient
l'événement de départ. Le contrôle positif vérifie aussi le type d'entrée.
Cette correction de données n'a demandé aucune recompilation Rust.

Le premier pin de preuve Cage provenait du JSON prévisionnel dont les clés
avaient un autre ordre. Le digest officiel préserve cet ordre. Le pin corrigé
provient des règles réellement sérialisées, dont la structure typée était
identique. Les rapports initiaux sont conservés. Les deux passages verticaux
exécutent 3 puis 15 preuves fraîches; le second réutilise la preuve Impact
du premier passage, avec les mêmes sources et empreintes.

Les anciens adaptateurs de mouvement ne sont pas tous remplacés par une
frontière unique. Les limites explicites du runtime de dégâts subsistent;
aucun système universel de choix de remplacement ou de rollback n'est annoncé.
Le chantier mana reste en staging : 54 tests sont compilables dans un preview
isolé, mais ne sont pas encore exécutés ni intégrés au moteur actif.

## Traçabilité et suite

Certificat : `.tmp/night-player-damage-cage-root-01/certificate.json`.
Analyseur SHA256 :
`1A733BC5F115CAC970FC86C8E0BC3147E33A847095F1E09B8A9AE628BDBE747D`.
Timing : `.tmp/card-migration-timings/night-player-damage-cage-root-01.json`.

Cohorte07 adoptée par le script officiel : 98 cas non résolus conservés,
deux ajouts, Agna Qel'a et Preordain. Les sources, l'ordre relatif et les
98 lignes d'audit retenues restent exacts. Le nouveau passage prend 0,052 s,
sans Cargo; 30 capacités natives sur 206, aucune carte complète dans les 100.
Les prochains lots analysés concernent les alternatives de création de jetons
et le transport explicite des erreurs dans les chemins de mana.

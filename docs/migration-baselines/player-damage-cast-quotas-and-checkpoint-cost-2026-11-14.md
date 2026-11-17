# Dommages simultanés, quotas de sorts et coût des checkpoints

Checkpoint **17319 accepté** après04CA : **3 642 cartes non vides entièrement
parsées /17 360 capacités natives**, soit **+4 cartes /+8 capacités**. Les
17 352 anciennes IR natives restent exactes, sans normalisation nouvelle.
Les quatre nouveaux contrats complets passent. Deafening Silence et Rule of Law
ont aussi des preuves individuelles sur leur IR réellement émise; le registre
passe de161 à163. Arcane Laboratory et Eidolon of Rhetoric ont une couverture
parser/contrat statique supplémentaire, sans nouvelle simulation individuelle.

Certificat : `.tmp/night-damage-quota-root-01/certificate.json`.
Analyseur : `.tmp/night-damage-quota-root-01/after-analyzer.exe`, SHA256
`17319B3B9B61245AA6D82D20854999B7D5BFD7ECDA29A4B0E984B7108B891E98`.

## Frontières acceptées

- Le quota de lancement compose un ensemble de joueurs, une expression
  numérique et une requête de sorts partagés. Il produit l'ancien primitif
  `castLimitPerTurn`, sans opération propre à une carte. Le moteur compte
  désormais correctement le filtre universel/null. Les types, sous-types,
  supertypes et compositions logiques sont bornés aux propriétés réellement
  enregistrées; les couleurs et autres prédicats non évaluables restent refusés.
- Le lecteur `player_set.rs` est partagé avec l'interdiction de lancer.
  Les parents existants conservent explicitement leur portée. Le quota actuel
  accepte EachPlayer et un maximum littéral positif i32; les autres portées ou
  producteurs numériques ne sont pas approximés.
- Le service de dommages prépare les conséquences simultanées sur un ensemble
  de joueurs, puis applique l'état et inscrit tous les événements, y compris
  le lien de vie, avant les observateurs. Le destinataire suit le contrôleur
  conservé de la capacité; les caractéristiques de la source ont leur contexte
  d'identité et de dernière information connue distinct.
- Les références de source typées `self` et `abilitySource` utilisent l'identité
  du StackObject. Une autre référence non résolue reste une erreur. Les tests
  comprennent de vrais lancers/résolutions, pas seulement un appel au lecteur LKI.
- `resolution_preflight.rs` reste l'unique visiteur récursif. Le checkpoint
  sauvegarde tous les champs du moteur sauf le préfixe du journal append-only.
  Sur erreur il conserve ce préfixe, retire les nouveaux événements et restaure
  le reste, notamment RNG, pile, contextes et séquences privées. Le reset de
  position déterministe refuse une transaction active. Une future écriture
  destructive de l'historique devra respecter cet invariant.

Le frontend des dommages de population et Impact Tremors appartiennent au
**lot suivant non certifié**. Ce checkpoint valide leur dépendance moteur.
Les limites de redirection/remplacements concurrents et de source scalaire
documentées dans les handoffs restent actives. Aucun gain de couverture Mana ou
Cage n'est revendiqué.

## Résultats

| Mesure | Avant04CA | Après17319 |
| --- | ---: | ---: |
| Cartes non vides complètement natives | 3 638 | 3 642 |
| Capacités natives | 17 352 | 17 360 |
| Cartes complètes dans les dix sets, occurrences | 289 | 289 |
| Capacités natives dans les dix sets | 1 531 | 1 531 |
| Tests unitaires du parseur | 897 | 904 |
| DATA Native / Full | 546 /546 | 556 /556 |
| Contrats complets | 390 | 394 |
| DATA moteur / assertions | 687 /4 660 | 701 /4 744 |
| Cartes à preuve individuelle enregistrée | 161 | 163 |

228 tests moteur de la partition3 passent, dont30 nouveaux tests de dommages,
neuf de source LKI, trois de références de source et cinq de checkpoint.
Le contrôle historique145 réexécute395 preuves. Les deux nouvelles cartes
réexécutent sept preuves. Quatre plans de structure sont réellement sérialisés
par la factory publique : Single/Primary, contexte normal conservé. Architecture :
zéro violation, zéro opération composite;49 familles IR primitives recensées.

Les dix sets restent TRK, MBC, FRA, HOB, MSH, SOS, TMT, ECL, TLA et SPM : mêmes
2 368 occurrences, 2 319 identités uniques, 241 cartes complètes uniques et
trois objets vides séparés. Les totaux sont vérifiés depuis leurs dix lignes.

## Régression de performance détectée puis corrigée

Le premier candidat copiait tout l'historique à chaque résolution de dommage.
Il n'a pas été accepté. Les mêmes fixtures, assertions et cinq répétitions par
échelle donnent les médianes suivantes en binaire debug :

| Dommages successifs | Baseline04CA | Premier candidat rejeté | Candidat corrigé |
| --- | ---: | ---: | ---: |
| 32 | 0,0150 s | 0,0303 s | 0,0130 s |
| 128 | 0,0430 s | 0,0343 s | 0,0215 s |
| 512 | 0,0560 s | 0,2588 s | 0,0554 s |
| 2 048 | 0,1851 s | 3,6090 s | 0,1892 s |

Le dernier résultat se situe à+2,20% de la baseline, au lieu de la multiplication
par19,5 observée initialement. Processus, démarrage et assertions sont inclus;
les campagnes avant/après sont distinctes. Ce benchmark synthétique ne mesure
pas le débit d'une partie complète en release. Les autres snapshots existants
de coût ou de révision de combat ne sont pas supprimés.

Benchmark parser sur le même input de2 368 occurrences, échauffement puis cinq
paires alternées : **0,605 →0,610 s** pour le parseur, **0,723 →0,731 s** pour le
processus. Aucun gain de vitesse global n'est revendiqué.

## Temps et boucle corrective

| Étape chronologique | Construction | Exécution | Résultat |
| --- | ---: | ---: | --- |
| Tests parser | 45,74 s | 3,64 s | 904 verts |
| Runner parser | 12,81 s | 0,99 s | 556 Native verts, puis Full sans reconstruction |
| Premier runner moteur | 33,38 s | 4,04 s | 696/697; défaut de source `self` détecté |
| Diagnostic et corrections groupées | Suivi séparé | Benchmark cinq passes | Source typée corrigée; copie quadratique du journal retirée |
| Runner moteur corrigé | 25,48 s | 3,70 s | 701 scénarios verts |
| Tests moteur CI3 | 86,09 s | 1,91 s | 228 verts |
| Analyseur optimisé | 28,86 s +1,30 s de liaison | — | Artefact figé17319 |
| Audit global39 288 objets | 0 s | 11,06 s | +4 cartes/+8 capacités, anciennes IR exactes |
| Factory sérialisée, helper rustc | 1,40 s | 0,22 s | Quatre plans réels; aucun Cargo supplémentaire |
| Contrôle historique145 | Voir JSON par sous-phase | Voir JSON | 34,11 s au total,395 preuves fraîches |
| Deux nouvelles cartes strictes | Cache chaud | Voir JSON | 11,70 s au total, sept preuves fraîches |

Les durées de construction mesurent Cargo avec liaison; elles ne représentent
pas seulement le temps CPU de rustc. Les derniers replays DATA permanents
ne reconstruisent rien. Un seul cycle correctif de sources regroupe le problème
de référence et celui de coût du checkpoint. Les essais de fixtures mal encodées
restent séparés dans les diagnostics, sans résultat de performance inventé.

Suivi root : `.tmp/card-migration-timings/night-player-set-damage-root-02.json`.
Son large stage « fresh-proofs » inclut aussi préparation, revue et coordination
après les commandes : **ne pas l'interpréter comme du temps d'exécution de tests**.
Le certificat donne la date d'acceptation; les rapports de chaque commande
donnent leurs durées. Le suivi spécifique Deafening est dans
`.tmp/night-cohort04-generic-analysis-01/timing/night-deafening-cast-quota-01.json`.

## Travail reproductible et suite

`scripts/link-native-oracle-fixtures.py` est installé avec25 tests Python verts.
Il lie un input réel immuable normal/single et ses IR complètes aux sources
physiques des fixtures, par égalité de casse et de types. Il ne certifie pas
l'exécution. Voir `docs/native-fixture-linker.md` pour les limites explicites.

**Cohorte06 adoptée** :99 cartes non résolues conservées exactement; Deafening
Silence est remplacée par Omniscience, première place League admissible dans le
pool existant (rang global189, nouveau rang de lot86). Quotas34/33/33 et curseurs
251/266/278 inchangés. Les99 anciennes lignes d'audit retenues restent exactes.
Nouveau lot :100 non résolues,28/205 capacités natives; audit0,064 s sans Cargo.

Les paquets suivants restent en staging avec revue croisée : dommages PlayerSet
et restrictions d'entrée/cast depuis des zones. La revue a identifié un vrai
blocker de liaison X dans l'ancien adapter de dommages : aucune promotion de ce
frontend avant sa correction et sa preuve. Dans la cohorte courante, Impact
Tremors est une seule carte complète potentielle; huit mentions/candidats de
dommages ne valent pas huit gains complets. L'objectif toutes les cartes reste
actif.

# Nuit du 14 novembre : joueurs quantifiés et rappels liés

Checkpoint natif `BA3F95604A3AF55AD6C3F95E9F43C16ECA1E5D98CD1E79DC86E3EC62EE0CE4A0`.
La mission toutes-cartes reste active ; les correctifs suivants restent en staging.

## Résultat mesuré

Depuis le parent syntaxique E04B : **+62 cartes complètes / +95 capacités natives**
sur les mêmes39288 entrées. **17124 anciennes IR exactes, zéro régression**.
Total courant : **3564 cartes non vides complètes /17219 capacités natives**,
les3144 textes vides séparés. Depuis le début du chantier syntaxique de cette nuit :
**+155 cartes /+345 capacités**. Reconnaissance parser, pas certification de chaque partie.

| Set local | Complètes E04B | Complètes BA3F | Gain cartes | Gain capacités |
|---|---:|---:|---:|---:|
| Star Trek | 37 | 38 | +1 | +1 |
| Mystery Booster Commander Edition | 8 | 8 | +0 | +0 |
| Reality Fracture | 41 | 42 | +1 | +2 |
| The Hobbit | 17 | 18 | +1 | +1 |
| Marvel Super Heroes | 28 | 28 | +0 | +1 |
| Secrets of Strixhaven | 42 | 43 | +1 | +3 |
| Teenage Mutant Ninja Turtles | 29 | 29 | +0 | +0 |
| Lorwyn Eclipsed | 26 | 26 | +0 | +1 |
| Avatar: The Last Airbender | 37 | 37 | +0 | +0 |
| Marvel's Spider-Man | 18 | 18 | +0 | +0 |
| **Total2368 occurrences** | **283** | **287** | **+4** | **+9** |

Par rapport à la baseline initiale des dix sets : **275 →287**, **1494 →1525 capacités**.
Le lot rotatif100 exact contient désormais1carte complète (Garruk's Uprising),
32/208 capacités natives. Les99 autres entrées restent conservées pour la suite.
Les gains globaux ne sont pas confondus avec ce lot100.

## Changements acceptés

- Un sujet quantifié est un nœud typé associant le PlayerSetReference existant
  et une action ordinaire vie/défausse. `Each opponent loses2life and you gain2life`
  devient forEachPlayer(opponents, loseLife) puis gainLife(controller), une seule fois.
- Vie et défausse réutilisent leurs mêmes lecteurs de queue d'action, sans
  reconstruction de texte ni opcode combiné. Les choix sont locaux à chaque joueur.
- Le lecteur numérique accepte contextuellement `a/an` comme1 tout en gardant
  leur classe lexicale de déterminant. Les autres quantificateurs ne deviennent pas1.
  Cette correction résout les anciens échecs de lecture « discards a card ».
- Le rappel après un point est un enfant syntaxique borné, associé à un des
  mots-clés accordés. Son sujet `Each of those creatures` est vérifié contre la
  collection antécédente ; les prédicats de rappels existants sont réutilisés.
  Les rappels inline et leurs diagnostics sont conservés. Aucune parenthèse inconnue
  n'est ignorée.

## Preuves

- **452/452 fixtures parser NativeOnly et Full**, zéro fixtureError.
- **6 tests privés d'annotation** et **5 de quantification/lowering** passent.
- **11 scénarios runtime /120assertions**, binaire frais déjà vérifié, **0 compilation**.
  Multi-adversaires, choix de défausse isolés, adversaire vide, gain du contrôleur
  une seule fois, effet statique appliqué seulement aux propres créatures puis
  retiré avec la source.
- Les entrées complètes de Sneering Shadewriter, Fugitive Listener et Garruk's Uprising
  passent par la factory single. Les IR effectivement émises sont comparées aux
  fixtures moteur. Garruk porte ses3règles complètes. Les arrangements des deux
  déclenchements ont des coûts/statistiques synthétiques : preuve des mécanismes,
  pas simulation exhaustive des cartes globalement gagnées.
- Passage privé élargi :695 passent/10échouent. Sept échecs appartiennent à la
  baseline automata documentée ; trois proviennent de partitions ajoutées au filtre
  et ne sont pas déclarés corrigés ni globalement verts. Aucun test supprimé.

## Temps et itérations

- Build release27.520s ; link analyseur1.286s.
- Correctif partagé a/an : premier replay440/441, puis441/441 ; build de correction
  5.910s et exécution0.838s, sans nouveau binaire de test par carte.
- Rappels : premier replay451/452 (diagnostic inline historique), puis452/452 ;
  build de correction4.627s et exécution0.847s.
- Full452 sans rebuild :0.880s. Runtime11 sans rebuild :0.425s.
- Tests annotation : compilation31.867s, exécution0.950s ; quantification :
  artefacts frais0.330s, exécution0.050s. Les suites de fixtures restent le loop rapide.
- Les JSON de timing conservent les vrais débuts et les boucles, sans inventer
  la durée d'analyse antérieure à leur activation.

## Suite active

Retour sort/permanent : paquet en revue, Source Engine inchangé. Comptages other :
correction encore en staging ; l'identité doit être préparée avant les événements
d'entrée, y compris landPlayed et Room, et préservée sur simple changement de contrôle.
Feed the Swarm reste unsupported : reproduction confirmée du défaut runtime qui
perd la manaValue d'une cible détruite et utilise0 ; un binder avant destruction
ne constitue pas une solution LKI universelle. Les caps restent explicites.

Artefacts : `.tmp/night-parser-generalization-01/global-comparison.json`,
`replay/comparison.json`, `vertical-proof-links.json`, `accepted-source-freeze.json`,
`runtime-cached.json`. Timings cartes :
`.tmp/night-player-program-01/timings/night-player-program-01.json` et
`.tmp/night-keyword-annotation-01/timing/night-garruks-uprising-annotation-01.json`.
Les textes réels, probe mutés et fixtures synthétiques restent séparés.

# Lot rotatif de 100 cartes non entierement natives - 12 novembre 2026

Ce rapport fige les 34 premieres cartes recentes, 33 cartes de priorite et 33 cartes jouees en ligue qui ne sont pas encore entierement natives. L ordre rotatif et les `oracle_id` sont preserves; aucun tri opportuniste par gain n a ete applique.

## Mesure de depart

- 100 cartes: 78 canoniques mais encore legacy/mixed, 22 unsupported.
- 169 capacites en dette: 133 legacy, 14 unclaimed, 22 native-unsupported.
- Audit debug des 450 candidates: 21,067 s pour 837 capacites.
- Les textes Oracle du lot ont ete compares au corpus 450: 0 difference.
- Cette baseline precede le correctif lexical generique Persist/counterValue.

## Classement conservateur

| Rang | Concept partage precis | Capacites | Cartes touchees | Cartes completes candidates |
|---:|---|---:|---:|---:|
| 1 | `library-search-move-parametric` | 3 | 3 | 3 |
| 2 | `static-no-maximum-hand-size` | 2 | 2 | 2 |
| 3 | `source-etb-simple-effect-sequence` | 2 | 2 | 1 |
| 4 | `static-additional-land-play` | 2 | 2 | 1 |
| 5 | `trigger-spell-cast-object-predicate-scope` | 7 | 7 | 0-1 |
| 6 | `trigger-permanent-entered-object-predicate-controller` | 14 | 13 | 0-1 |
| 7 | `loyalty-parent-to-shared-effect-sequence` | 15 | 6 | 0 |
| 8 | `deck-commander-eligibility` | 7 | 7 | 0 |

Une carte candidate complete n est pas encore une carte certifiee. Elle doit repasser sans legacy et franchir les preuves IR, contrat et runtime. Les groupes evenementiels n additionnent pas les corps composites comme gains.

Les deux groupes evenementiels comptent uniquement les evenements primaires de l IR legacy (`spellCast`, ou `enterBattlefield`/`permanentEntered`). Une mention dans un effet ou un rappel ne suffit pas. Les nombres precedents de 10 et 15 capacites provenaient d un regroupement textuel trop large et ont ete corriges lors de la revue.

## Correctif choisi: LibrarySearchMove parametrable

- Demonic Tutor - `82004860-e589-4e38-8d61-8c0210e4ea39`
- Vampiric Tutor - `ededbdae-d9dc-4206-9335-d7158f2d7700`
- Enlightened Tutor - `c5229c17-b7be-4b05-b683-f2277edc4849`

La frontiere doit composer les primitives existantes `chooseCards`, `revealCards`, `shuffleZone`, `moveCards`, `loseLife` et `recordLibrarySearch`. Les parametres sont le selecteur, la destination, la revelation, l ordre relatif du melange et la suite d effets. Aucun op composite ni cas nomme par carte.

La revue des [regles officielles du 6 novembre 2026](https://media.wizards.com/2026/downloads/MagicCompRules%2020261106.txt) ajoute deux exigences : une recherche sans qualite trouve autant de cartes que possible (701.23d), et les cartes trouvees sont exclues du melange avant leur placement au-dessus (701.24b). Le contrat `shuffleZone` doit donc recevoir une exclusion generique ; le gain de trois cartes reste a confirmer apres cette extension et ses preuves.

## Limites et prochain replay

Le nombre de cartes touchees mesure la portee syntaxique; le gain complet reste conservateur et non cumulatif lorsque les groupes se chevauchent. Apres integration, le meme input de 100 cartes doit etre rejoue, puis le corpus fixe de 2 069 cartes, avec comparaison native/mixed/legacy/unsupported et temps de parsing separe.

# Cycle 3 — taxe d'attaque partagée

Propaganda et Ghostly Prison sont migrées par le même automate de taxe par
créature attaquante. Le lot de départ contient 100 cartes non couvertes ; deux
sont terminées. Les 98 autres sont conservées, puis Tatyova, Benthic Druid et
Blood Artist remplissent les places dans le lot suivant.

## Modification et preuves

L'automate retient séparément le domaine des attaquants, le joueur protégé, le
contrôleur payeur et le montant de mana générique. Il produit le modificateur
existant `attackTax`, sans regex ni opération nommée d'après une carte. Les
montants négatifs, colorés ou hors de la plage entière du moteur sont refusés
avec un diagnostic structuré. La reconnaissance historique de cette forme a
été retirée.

La revue du moteur a corrigé un problème réel : « attack you » ne protège pas
automatiquement les planeswalkers du joueur. Le moteur résout maintenant la
référence de joueur ; une protection des planeswalkers exige le champ explicite
`protectPlaneswalkers`. Les producteurs historiques qui expriment cette
protection renseignent ce champ. Les références inexécutables sont refusées par
le contrat, et l'addition des taxes évite le débordement entier.

- Parseur NativeOnly : 76/76 cas, dont sept couvrant la taxe.
- Contrat : quatre cas verts, dont les deux cartes exactes et deux négatifs.
- Runtime : huit scénarios, 51 assertions vertes ; paiement par attaquant,
  insuffisance de mana, choix volontaire, multijoueur et portée des défenseurs.
- IR parser = IR contrat = IR réellement exécutée pour les deux cartes.
- Architecture : zéro violation ; inventaire des partitions inchangé.

Les 167 contrats du banc élargi sont également verts. Le premier replay runtime
historique a donné 328/331 scénarios verts en 1,063 s. Deux fixtures ont ensuite
été corrigées et vérifiées : Cultivate ne révèle aucune carte lorsqu'aucune carte
n'a été trouvée ; Force of Negation doit hériter de sa règle de test plutôt que
la remplacer explicitement par `rules: []`. Le troisième échec concerne la mise
en place initiale d'un jeton et reste distinct de la taxe. Ce replay élargi ne
doit pas être annoncé entièrement vert sans nouvelle vérification.

Le retrait des marqueurs défense après dégâts de combat à une Battle est aussi
un [gap runtime documenté](./known-gaps/battle-combat-defense.md), reproduit hors
du gate vert ; il n'est pas compté comme comportement migré.

## Généralisation mesurée

Le même binaire relit les 39 288 objets locaux, soit 67 420 capacités. Les
14 298 capacités auparavant natives et canoniques sont toutes conservées, y
compris dans les cartes encore partielles : même texte, type, IR et diagnostics.
Une vérification indépendante de root confirme zéro régression.

Quatre capacités deviennent natives : celles de Propaganda, Ghostly Prison,
Windborn Muse et Koskun Falls. Windborn Muse devient entièrement reconnue ;
Koskun Falls garde un autre bloqueur. Ces gains globaux mesurent la
reconnaissance. Seules Propaganda et Ghostly Prison possèdent les nouvelles
preuves verticales exactes de ce cycle.

## Temps mesurés

Les étapes d'analyse et de codage sont celles du tracker de l'agent. Les temps
de commandes sont mesurés séparément des attentes de coordination.

| Étape | Temps | Description |
|---|---:|---|
| Analyse du concept | 273,379 s | Texte, transitions, contrat et limites moteur |
| Attente d'intégration initiale | 20,444 s | Source gelée pendant le build précédent |
| Implémentation initiale | 245,180 s | Automate, lowering, vocabulaire et fixtures |
| Compilation du parseur | 32,118 s | Une construction pour les 76 cas |
| Tests du parseur | 0,705 s | Exécution du runner stable |
| Compilation des runners moteur/contrat | 39,743 s | Bibliothèque moteur et liens |
| Preuves runtime finales | 0,222 s | Huit scénarios, 51 assertions |
| Liaison de l'analyseur | 1,168 s | Réutilisation du rlib, sans Cargo |
| Replay du lot de 100 | 0,135 s | 200 capacités |
| Replay de la file de 450 | 0,479 s | 837 capacités |
| Replay des sept ensembles | 2,449 s | 2 069 cartes, 4 300 capacités |
| Replay global | 29,296 s | 39 288 objets, 67 420 capacités |

Une boucle corrective runtime a isolé la mauvaise attente sur les dégâts d'une
Battle ; la preuve de légalité de cette défense est conservée et le défaut de
dégâts est enregistré séparément. Les durées de revue et d'attente ultérieures
ne sont pas assimilées aux millisecondes d'exécution des tests.

## Reproduction et suite

Snapshot : `2ABBD41362112D52C15B347E25E74A934B042661727E170F7D48492C7565F232`.
Le comparateur durable est `scripts/compare-native-oracle-audits.py`, vérifié par
six tests. Il protège les identités, l'ordre et les textes de toutes les
capacités, ainsi que la sémantique des capacités déjà natives.

Artefacts détaillés : `.tmp/attack-tax-audits-2ABBD413-report.json`,
`.tmp/attack-tax-parser-contract-runtime-equality.json`,
`.tmp/attack-tax-global-root-verification.json` et
`.tmp/card-migration-timings/round3-attack-tax.json`.

Le correctif suivant est le déplacement ciblé entre zones, partagé entre les
parents sort, activation et déclenchement : trois gains complets conservateurs
attendus dans le lot. Voir l'[analyse des bloqueurs](./round3-next-generic-correction-2026-11-12.md).

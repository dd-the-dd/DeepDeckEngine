# Cycle 12 — réduction de coût et relation typée de lanceur

Ce cycle compile les réductions fixes de coût générique dans
`reduceCastingCost` et partage une vraie relation de lancement entre les
réductions et les taxes de sorts. `You cast` décrit le lanceur du sort; il
n'est jamais approximé en contrôleur courant d'un objet sur la pile.

Le parent de coût ne reparcourt pas le texte du mana. Il reçoit le résultat
typé de `ManaCostNode.fixed_generic_amount`, limité à un unique symbole
générique positif représentable en `i32`. La requête de sorts réutilise une
seule machine de collection coordonnée paramétrée par la relation `Cast`,
alors que les autres requêtes d'objets conservent la relation `Control`.

## Boucles correctives et compilations

Les trois builds sont distincts; aucun n'est présenté comme une seule longue
exécution de test.

| Build | Compilation | Exécution ciblée | Résultat | Diagnostic |
|---|---:|---:|---|---|
| AD1573E3 | 40,296 s | 0,404 s | 3/13 | `ObjectQuery` avait consommé `you` comme contrôle avant de rencontrer `cast` |
| 515E6C7D | 32,858 s | 0,707 s | 7/13 | cinq montants invalides exposaient l'état du child mana; une continuation avait une attente de classification erronée |
| B13B4687 | 13,351 s | 0,424 s | 13/13 | vert après propriété du diagnostic par le parent et relation `Cast` typée |

Les ajouts de fixtures data-driven ont ensuite fait évoluer le dépôt observé
de 165 à 166 puis 167 cas. Le replay final immuable B13 passe 167/167 en
0,404 s, sans compilation supplémentaire.

Les sept boucles correctives sont distinctes : garde `where:null` et overflow,
extraction typée du mana, déplacement du relatif dans `ObjectQuery`, relation
de lanceur `Cast`, propriété des diagnostics de coût, correction du schéma des
fixtures de couleur, puis revalidation dynamique de la couleur effective au
runtime. La dernière boucle corrige une primitive partagée; elle n'est pas un
cas particulier pour Fry.

## Couverture mesurée

Le replay immuable couvre 39 288 cartes et 67 420 capacités. Les 14 652
capacités natives antérieures gardent exactement leur carte, position, texte,
type, IR et diagnostics. B13 ajoute 99 capacités et 40 cartes entièrement
reconnues par le parseur natif, sans régression :

- 87 capacités `reduceCastingCost`;
- 11 taxes `additionalCastingCost` rendues natives par la relation partagée
  de lanceur;
- une capacité de Fry rendue native par le qualificatif postnominal partagé.

Les 40 cartes se répartissent en 38 cartes ordinaires et 2 objets auxiliaires;
aucune variante. Le total global passe de 14 652 à 14 751 capacités natives
canoniques.

Les sous-corpus confirment la généralisation : +1 capacité/+1 carte dans le
lot round10, +1/+1 dans la file de 450 et +12/+6 dans les sept ensembles
locaux. Tous conservent leurs capacités natives antérieures.

Le mode Full a aussi été contrôlé indépendamment sur ses 193 anciennes
capacités canoniques. Cent soixante-quinze règles sont identiques octet pour
octet après lecture JSON; 17 autres ne changent que la casse normalisée des
valeurs de couleur (`white` → `White`, etc.). La seule correction sémantique
est bornée à Sagrada Familiar : le faux sous-type `Commander` devient la vraie
caractéristique `isCommander`. Dans le sous-ensemble des 109 anciennes
réductions, cela donne 91 règles brutes identiques, 17 orthographes de couleur
équivalentes et cette unique correction. Soixante-huit sont promues natives et
41 restent explicitement legacy. Aucune whitelist Native n'a été utilisée.

## Contrats et runtime

Les contrats générés passent 99/99 pour toutes les capacités gagnées, y compris
les cartes partielles, puis 40/40 pour les cartes complètes : 139/139 au total.
La régression complète des contrats passe également 225/225 en 1,794 s.

Goblin Anarchomancer fournit la preuve verticale principale. Son IR parseur,
son contrat et la règle embarquée par le runtime sont identiques, avec le hash
canonique `98F1CC5EE88E79E0744171AF3A06627CCA37570D7E5809A1FDF865054067C78C`.
Le vrai lancement sous une taxe de Thalia passe cinq assertions; un sibling
sans filtre ajoute trois assertions.

La correction lexicale de Sagrada passe trois scénarios et cinq assertions :
la désignation Commander est distincte des autres caractéristiques et du mot
imprimé « Commander ». Les cinq preuves exactes de Fry couvrent les cas
nominaux, la revalidation après changement de couleur et les interactions avec
devoid. Leur règle complète parseur/contrat/runtime est égale. Le correctif
moteur partagé de couleur effective est accepté par la vérification root.

La régression runtime finale passe 394/394 scénarios et 2 429 assertions en
2,065 s. Son rebuild de 36,511 s est un relink final après formatage tardif,
mesuré séparément de l'exécution. L'entrée porte l'empreinte
`C526C47EA1A19F5168DF7E33AB70F199FDFE8EF972230D2761D0D4E5D672B8D9`
et l'exécutable l'empreinte
`17CC3B01E96197A9135725F32F6E047B50E3FA1DB7717C333D655878FFAA6EFB`.
Le checkpoint cycle12 est donc accepté sur les cinq gates.

## Lot round11

Le script officiel conserve 99 cartes, retire Goblin Anarchomancer et ajoute
Sink into Stupor // Soporific Springs, rang rotatif global 209. Les quotas
restent 34 récentes / 33 priorité / 33 ligue. Le nouvel audit contient 178
blocages : 150 `unclaimed`, 28 `native-unsupported` et aucun legacy.

Le meilleur correctif suivant est le parent générique d'événement de phase vers
`EffectSequence`. Il achève une carte du lot — Phyrexian Arena — mais les probes
B13 établissent une portée globale de 154 capacités au corps déjà natif,
29 cartes complètes et 124 gains partiels.

Le score supposé de deux cartes pour un coût de sacrifice activé a été rejeté.
Sakura-Tribe Elder demande seulement d'ouvrir l'enveloppe de recherche activée
au coût source-sacrifice déjà reconnu. Phyrexian Altar demande séparément une
sélection de permanent sacrifié avec déclaration, puis un parent de mana sans
coût de dégagement. Ce ne sont pas un unique enfant manquant.

## Temps

La liaison de l'analyseur B13 a pris 1,082 s. Les quatre audits ont pris
0,644 s, 0,606 s, 2,633 s et 39,479 s; la comparaison globale stricte a pris
4,438 s. L'exécution des contrats générés a pris 0,060 s et 0,028 s. Les
preuves runtime mesurées ont pris 0,034 s pour Goblin+sibling, 0,149 s pour
Sagrada et 0,037 s pour les deux scénarios nominaux de Fry. Le gate final
ajoute 0,404 s de parseur, 1,794 s de contrats et 2,065 s de runtime; le rebuild
runtime associé a pris 36,511 s.

La grande fenêtre `focused-tests` du tracker contient attente, coordination,
revue et plusieurs boucles. Elle n'est pas un temps d'exécution et n'est pas
présentée comme tel.

Artefacts principaux :

- `.tmp/static-spell-reduction-audits-B13B4687.json`;
- `.tmp/static-spell-reduction-global-FAAC58D9-to-B13B4687-comparison.json`;
- `.tmp/static-spell-reduction-global-root-verification-B13B4687.json`;
- `.tmp/static-spell-reduction-global-ability-contracts-B13B4687-results.json`;
- `.tmp/static-spell-reduction-global-full-card-contracts-B13B4687-results.json`;
- `.tmp/static-spell-reduction-runtime-focused-B13B4687.json`;
- `.tmp/sagrada-commander-runtime-results.json`;
- `.tmp/fry-runtime-b13-final.json`;
- `.tmp/static-spell-reduction-parser-final-B13B4687.json`;
- `.tmp/effective-color-formatted-contract-final.jsonl`;
- `.tmp/effective-color-formatted-runtime-final.json`;
- `.tmp/static-spell-reduction-root-summary-final.json`;
- `.tmp/rotating-next-100-round11-ranking-B13B4687.json`.

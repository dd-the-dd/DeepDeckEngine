# Découpage du moteur et retrait des effets spécialisés

La règle retenue est de conserver les primitives génériques même lorsqu'elles
n'ont pas encore d'émetteur dans le nouveau parseur. L'absence dans un corpus ne
constitue pas une preuve de code mort. Les anciennes politiques qui combinent
plusieurs concepts sont retirées lorsqu'elles n'ont plus de producteur ni de
preuve comportementale à migrer ; celles encore utilisées sont à décomposer.

## Frontière intégrée

Les validateurs et exécuteurs d'effets sont répartis dans
`src/engine/effect_domains/` : 34 domaines, un catalogue/routage partagé et un
fichier de tests dédié. La recherche du domaine est commune à la validation et
à l'exécution. Il n'y a pas de parcours successif de tous les domaines.

`execute_effect` passe de 12 622 à 41 lignes ; `effect_supported` de 1 857 à
28 lignes. La partition mécanique préserve les corps des branches ; les retraits
suivants sont des changements distincts, relus et testés. Le reste de
`engine/mod.rs` contient encore des responsabilités à extraire.

| Lot | Effets retirés | Motif sémantique |
|---|---|---|
| 01 | `advancePeacefulCreatures`, `boostModifiedCreaturesBySourcePower`, `makeCreaturesOfPlayerCowardsPermanently`, `chosenPlayersSearchLibrariesToTop`, `willOfCouncilCounterOrCopy`, `payLifeOrMoveCards` | Politiques particulières réunissant sélection, calcul, paiement et actions |
| 02 | `returnCreatureByLifeGained`, `returnBoundCardsWithinLifeGained`, `returnGraveyardCardByTriggerSubtype` | Réanimation/déplacement incorporant budget de gain de vie ou sous-type déclencheur |
| 03 | `craftTransform`, `optionalBehold`, `separateGraveyardCardsIntoPiles`, `separateTopCardsIntoPiles` | Programmes rigides de choix, piles, déplacement, signal ou transformation |
| 04 | `inspectTopCardForChosenCreatureType`, `destroyPermanentsMatchingSourceCounterManaValue`, `destroyCreaturesNotOfChosenType`, `destroyOptionalPermanentOfTriggeringPlayer`, `reanimateWithTypeCounter`, `ownerChoosesSecondFromTopOrBottom`, `ownerChoosesLibraryEndForSpellOrPermanent` | Choix, prédicats, compteurs et destinations incorporés dans des actions particulières |

Ces vingt opérations n'ont pas de constructeur restant dans le parseur ou le
runtime, d'occurrence dans les IR natifs du corpus, ni de test comportemental
distinct à préserver. Des contrats témoins acceptés avant retrait sont refusés
après retrait. Les primitives de choix, déplacement, transformation, copie,
contrôle, combat et régénération sont conservées. Les quatre lots retirent au
total 1 432 lignes et en ajoutent quinze dans les fichiers concernés.

## Vérifications exécutées

| Vérification | Résultat |
|---|---|
| Contrats durables | 422/422, dont 20 refus des anciens effets |
| Simulation pilotée par données | 776/776, 5 178 assertions |
| Tests unitaires runtime partition 3 | 320/320 |
| Régression historique prioritaire | 145/145 cartes, 395 preuves fraîches, zéro carte prise dans le cache |
| Architecture avec nouvelle garde de recensement | Zéro violation, zéro avertissement ; réutilisation du cache vérifiée |
| Garde statique | 8 tests d'extraction, 3 tests d'intégration et ancienne suite PowerShell verts |
| Sources du parseur | Identiques au checkpoint 924827 ; aucun changement de reconnaissance |

Le recensement actuel distingue **247 noms d'effets acceptés** et **248 exécutés**.
Le nom supplémentaire, `castParadigmCopy`, est produit par un cycle interne de
Paradigm ; sa suppression isolée casserait un producteur existant. Ces nombres
ne sont pas des comptes de primitives atomiques, de cartes ou de règles prouvées.

La couverture du parseur n'est pas augmentée par ce nettoyage : le dernier audit
global reste 3 865 objets non vides entièrement natifs et 17 771 capacités natives.
Les IR et toutes les sources du parseur sont inchangés ; ce chiffre est repris du
checkpoint accepté, pas présenté comme un nouvel audit global.

## Mesures et limites

| Dernière vérification | Compilation | Exécution |
|---|---:|---:|
| Runner de simulation après lot 04 | 13,802 s | 4,374 s |
| Tests unitaires runtime partition 3 après lot 04 | 72,703 s | 2,328 s |
| Contrats durables après lot 04 | 14,466 s | 0,668 s |
| Régression 145 cartes après lot 03 | Préparation Cargo 3,187 s | Audit parseur 0,198 s ; contrats 3,110 s ; simulation 6,642 s |

La régression historique prend 33,815 s au total, contrôles et orchestration
compris. La séparation en modules améliore les responsabilités et la lisibilité,
mais ne crée pas de nouvelle frontière de compilation Rust. Aucun gain général
de compilation ou de simulation n'est revendiqué sans comparaison contrôlée.

## Travail restant

Six compositions encore émises doivent être remplacées avant retrait :
`counterStackObjectUnlessPays`, `destroyPermanentsByManaValue`,
`exileAllGraveyards`, `exileNamedCardsFromZones`, `exilePermanentWithSource`
et `exileTargetCardWithSource`. D'autres anciennes opérations disposent de tests
distincts : ces preuves doivent être adaptées à leurs compositions génériques,
pas supprimées. Le moteur n'est donc pas encore certifié entièrement primitif.

Le remplacement de « contrer à moins de payer » est en préparation : paiement
transactionnel générique, résultat booléen obligatoire, condition, puis tentative
de contre. Une erreur de paiement ne doit pas devenir un simple refus.

## Traçabilité

Les manifestes, sauvegardes, différences bornées, mesures et rapports sont dans
`.tmp/night-native-engine-cleanup-root-01`. Le gel des sources porte sur 805
entrées Rust/contrats, dont les 36 nouveaux fichiers ; seule `src/engine/mod.rs`
diffère parmi ces anciennes entrées. Les cinq fichiers de la garde permanente
portent le gel accepté à 810 entrées. La branche `archive/legacy-oracle-2026-11-13` conserve
l'ancienne implémentation et une archive exacte de 773 fichiers précède ce
nettoyage. Aucun commit/reset ni suppression de travaux étrangers à la migration.

L'[inventaire HTML](../deepdeck-primitives-inventory.html) contient les noms et
les emplacements actuels ; ses catégories restent indicatives.

Certificat accepté du lot 04 : `.tmp/night-native-engine-cleanup20-root-01/certificate.json`,
SHA `C223C30E152CF0C6C155B3F5DE68A95142A916B6AADE5F7BA7F7C91EC100FAE9`.
Le certificat précédent des treize premiers retraits reste intact dans
`.tmp/night-native-engine-cleanup-root-01/certificate.json`.
L'objectif global reste actif et incomplet.

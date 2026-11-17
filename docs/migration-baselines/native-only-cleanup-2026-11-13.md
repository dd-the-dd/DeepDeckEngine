# Retrait du legacy Oracle — 13 novembre 2026

Archive distante vérifiée : [archive/legacy-oracle-2026-11-13](https://github.com/dd-the-dd/DeepDeckEngine/tree/archive/legacy-oracle-2026-11-13), commit `eec9a2ef4a57b7a759e0b0cd667cdae70f1a0c99`. La branche locale `codex/integrate-agent-clock-wip` garde ses changements non commités ; son index initial est préservé. Les travaux clock/training/backend ont été conservés.

Le parseur historique est physiquement retiré : 33 fichiers supprimés, modules mixtes nettoyés et dépendance regex retirée de deepdeck-oracle. Les deux API utilisent les automates natifs stricts. Les cartes dépendant encore du legacy renvoient unsupported.

Les resolvers opaques et opérations spécifiques du moteur sont retirés. Cascade, copies de sorts, Surveil et retours d'exil réutilisent des primitives. Le retour lié est immédiat, respecte les incarnations de zone et les propriétaires, et publie les entrées simultanées après insertion du lot. Les jetons hors battlefield ne changent plus de zone. Le training est isolé dans son propre domaine privé.

Mesure physique contre l'archive : **107 034 lignes Rust nettes supprimées**, dont 79 705 dans src/oracle et 27 329 dans src/engine, tests dédiés compris. Cette mesure n'est pas un décompte de production seule. 37 anciennes fonctions du moteur et un adaptateur de destination Morbid sont retirés ; les atomes génériques utiles sont préservés. Treize tests exclusivement opaques ont été retirés, quatre scénarios réécrits avec des primitives, les 42 modules de tests utiles conservés.

## Preuves fraîches

| Contrôle | Résultat |
| --- | --- |
| Banc parser stable, API native et défaut | 291/291 dans chaque mode, zéro fixture invalide |
| Intégrations parser/provenance/architecture | 26/26 |
| Runtime permanent + ponts | 505/505, 3 420 assertions |
| Tests privés moteur, partition 3 sans parser | 42/42 |
| Contrats ciblés | 266/266 |
| Préservation de forme des règles natives | 16 029 anciennement exécutables conservées, zéro perte ; quatre anciens refus inchangés |
| Préfixe officiel | 145/145, 244 capacités natives, zéro fetch |
| Architecture globale | zéro resolver opaque, zéro composite, zéro violation ; exceptions legacy à zéro |

Les 130 fichiers de contrats groupés vérifient les mêmes 16 033 formes individuelles ; les résultats de 2 969 vecteurs sont dérivés par conjonction. Ce n'est ni une preuve de composition des faces ni un scénario de partie pour chaque carte. Les quatre anciens refus sont Spell Burst (X dans le filtre de sorts), Faerie Trickery (sous-type), Containment Priest (activeWhile absent), Flaccify (mana fractionnel). Les vecteurs acceptés restent 2 966/2 969.

La suite privée parser élargie reste **700/716**. Les 16 échecs sont conservés : sept attentes/données périmées, six échecs couvrant trois défauts anciens, trois politiques de diagnostic à préciser. Leur attribution préexistante repose sur comparaison transitive avec l'archive, sans exécuter ni recompiler le legacy. Voir `.tmp/native-only-parser-private-failure-review/REVIEW.md`. Aucun attendu n'a été abaissé pour accepter un mauvais diagnostic.

## Couverture et conservation

Même corpus local : 39 288 objets /67 420 capacités ; **16 033 capacités reconnues natives**, **2 969 objets non vides entièrement reconnus**, 3 144 objets sans texte comptés séparément. La couverture de reconnaissance est inchangée. **15 981 IR natives exactes**, seules **52 expansions Surveil** changent selon un ledger vérifié ; le comparateur brut reste rouge pour ces 52 changements attendus. Les sorties globales des API défaut et native sont identiques.

Le premier préfixe a exécuté 393 preuves et refusé l'ancien digest composite d'Undercity Sewers. Seul son digest a été mis à jour après vérification de l'expansion exacte ; le second passage exécute les deux preuves manquantes et réutilise 144 cartes certifiées. Les deux rapports bruts restent conservés.

## Temps mesurés

| Étape | Compilation/liaison | Exécution |
| --- | ---: | ---: |
| Premier build parser strict, debug | 17,422 s | 0,888 s pour279 cas |
| Build parser optimisé après suppression | 35,496 s | 7,310 s pour le corpus global final |
| Premier typecheck moteur | 19,516 s, quatre erreurs d'arité corrigées | aucune |
| Build moteur coordonné réussi | 51,557 s | 2,985 s pour505 cas |
| Dernier rejeu runtime | 0,186 s, Cargo à jour | 1,788 s pour505 cas |
| Dernier banc privé moteur | 5,350 s | 1,049 s pour42 tests |
| Contrats groupés | aucune | 0,918 s pour16 033 formes |
| Dernier préfixe officiel | aucun build | 11,82 s de workflow total |

Le checkpoint précédent mesurait 179,959 s pour le build optimisé du parser. La comparaison est un relevé de builds sur cache existant, pas un benchmark à froid contrôlé. Aucun gain de vitesse des parties n'est revendiqué. Les essais échoués (arités, helper de test retiré, schéma d'IR synthétique, helper récursif de test et digest Surveil) restent dans les journaux, avec compilation séparée de l'exécution.

## Reprise

But global toujours actif : toutes les cartes Magic, automates partagés puis IR primitives et exécution correcte. Lot16 inchangé : 100 cartes, 207 capacités, 32 reconnues natives et zéro carte complète. Intégrer le paquet Solemn Simulacrum rebasé, puis le remplacement générique de quantité recommandé pour Hardened Scales/Doubling Season (deux cartes entières projetées, aucun gain encore certifié). Aucun refill ni changement de curseur n'a été fait pendant le nettoyage.

Preuves machine et chronologie : `.tmp/native-only-cleanup-root/` et `.tmp/card-migration-timings/native-only-legacy-removal.json`.

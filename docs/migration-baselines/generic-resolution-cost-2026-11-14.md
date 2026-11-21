# Paiement générique et retrait des effets spécialisés

Le moteur conserve toutes ses primitives génériques. Le nettoyage accepté retire
21 anciennes opérations spécialisées, dont `counterStackObjectUnlessPays` dans
ce lot. Un contresort conditionnel émet désormais :

```text
tryPayCost(joueur, coût typé) → résultat booléen obligatoire
condition(résultat = faux) → counterStackObject(cible)
```

Le service partagé prépare les ressources, recueille les choix puis applique
le paiement. Les dix domaines de coûts existants sont conservés. Refus,
impossibilité et erreur restent distincts ; une variable manquante ou une erreur
du fournisseur ne devient pas silencieusement un refus. Le mana numérique
utilise une exigence typée et le planificateur existant.

Les effets sont répartis dans 34 modules de domaine avec un routeur commun.
L’extraction conserve la frontière de compilation runtime ; aucun gain général
de compilation ou de simulation n’est revendiqué.

| Vérification | Résultat |
|---|---:|
| Tests unitaires parseur | 961 réussites |
| Données Native / Full | 654 dans chaque mode |
| Scénarios moteur | 796 réussites, 5393 assertions |
| Contrats | 423 réussites |
| Tests moteur CI3 | 333 réussites |
| Tests ciblés paiement | 13 réussites |
| Régression historique | 145 cartes, 395 preuves fraîches |
| Architecture | 0 violation, 0 avertissement |

La comparaison stricte sur 39 288 cartes conserve exactement 17 732 anciennes règles
natives et vérifie 38 réécritures attendues. Aucune nouvelle carte revendiquée.
Flaccify est désormais refusée : son vrai symbole de demi-mana `½` n’est pas
pris en charge. L’ancien résultat positif était incorrect. Couverture complète
non vide : 3865 → 3864 ; capacités natives : 17771 → 17770. Entrées réelles inchangées.

Les dix sets locaux de référence restent à 306 occurrences complètes et 1566 capacités
natives. Les 2368 occurrences comprennent 306 complètes, 911 partielles,
1148 non prises en charge et 3 vides. Après déduplication : 2319 identités,
258 complètes. Ces mesures de parsing ne certifient pas tout le comportement
moteur. Registre de preuves de bout en bout inchangé : 171 cartes.

Audit global : 11,192 s ; treize audits : 12,354 s ; 796 scénarios moteur : 3,869 s sans
compilation. Le [rapport HTML chronologique autonome](../resolution-cost-timing-2026-11-14.html)
sépare compilation, exécution, attente et boucles correctives (cadres jaunes).

Douze tests inaccessibles appelaient des résolveurs supprimés. Leurs 31 assertions
sont archivées à l’identique ; onze scénarios utiles restent à porter. Tests Ward
génériques conservés et exécutés. Cinq opérations composées encore émises et les
défauts connus d’observation des déplacements restent ouverts.

Certificat : `.tmp/night-resolution-cost-unification-root-01/certificate.json`,
SHA256 `2F03A2D922F7E34272451B744F1C9FEC6914F4439505EF3B76E50A26E3BB2E58`.
820 entrées source épinglées avec rapports. Migration de toutes les cartes ACTIVE.

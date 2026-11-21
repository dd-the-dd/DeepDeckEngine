# Réaudit de la reconnaissance — queue prioritaire

Date : 2026-11-09  
Queue : `docs/played-card-migration-priority-expanded-2026-11-09.json`  
Portée : 585 cartes, avec preuve historique complète pour les 145 cartes initiales.

## Conclusion

Les cartes ne sont **pas toutes à recommencer**. Sur les 145 cartes auditées :

- **132** ont déjà une IR primitive exécutable : on conserve l’IR, le moteur et les tests, puis on remigre seulement la reconnaissance vers les automates locaux;
- **4** gardent une dette composite : leur reconnaissance et leur opération combinée doivent être remplacées;
- **9** restent unsupported et nécessitent une nouvelle capacité générique complète;
- **440** cartes de l’extension n’ont pas encore assez de preuve locale et restent `unverified`.

Aucune carte prioritaire n’est encore certifiée `native` selon la nouvelle définition stricte. Cela ne signifie pas que leur sémantique est perdue : **132** cartes ont déjà une sémantique primitive exécutable réutilisable.

## Statuts

| Dimension | Statut | Nombre |
|---|---:|---:|
| Reconnaissance | native | 0 |
| Reconnaissance | legacy-recognized | 132 |
| Reconnaissance | composite-debt | 4 |
| Reconnaissance | unsupported | 9 |
| Reconnaissance | unverified | 440 |
| Sémantique | primitive-executable | 132 |
| Sémantique | composite-executable | 4 |
| Sémantique | unsupported | 9 |
| Sémantique | unverified | 440 |

## Dette composite restante

- #66 **Orcish Bowmasters** — resolveTriggeredInstruction
- #103 **Boros Charm** — resolveSpellInstruction
- #132 **Fabled Passage** — resolveTriggeredInstruction
- #143 **Esper Sentinel** — resolveTriggeredInstruction

## Unsupported vérifiés

- #37 **Bitter Triumph**
- #42 **Rest in Peace**
- #96 **Rhystic Study**
- #108 **Arcane Denial**
- #109 **Surrak, Elusive Hunter**
- #112 **Rhythm of the Wild**
- #120 **Soulstone Sanctuary**
- #124 **Cyclonic Rift**
- #131 **Brazen Borrower // Petty Theft**

## Limites de preuve

- Le dernier audit conservé classe les résultats au niveau **carte**, pas au niveau de chaque capacité. Le JSON indique cette granularité explicitement.
- Les 440 cartes de l’extension Commander n’ont pas été extrapolées : elles sont `unverified` jusqu’à un audit parser/contrat.
- `canonical` ou `primitive-executable` ne vaut pas `native`. Toute reconnaissance par regex, curseur de texte, branche exacte ou fallback reste `legacy-recognized`.
- Les migrations récentes de Chaos Warp, Burst Lightning, Day of Black Sun et Wan Shi Tong conservent leur IR primitive et leurs preuves, mais leur ancienne façade de reconnaissance reste à remplacer par les automates partagés.

Le fichier JSON contient une entrée conservatrice pour chacune des 585 cartes, avec preuves, décision de réutilisation et limites.

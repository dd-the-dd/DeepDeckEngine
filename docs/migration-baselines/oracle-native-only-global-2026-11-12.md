# Baseline Oracle NativeOnly — 2026-11-12

## Résultat

Le parseur `NativeOnly` a parcouru les 39 288 objets Oracle locaux, soit 67 420 capacités, en **28,004 s de temps parseur** et 28,904 s au mur. Il n'exécute ni le classificateur regex historique ni le fallback legacy : une capacité non revendiquée demeure explicitement non couverte.

Le dénominateur utile pour suivre la migration est celui des cartes ordinaires jouables. Les objets auxiliaires, variantes et cartes sans capacité sont conservés dans l'audit, mais isolés pour ne pas gonfler la couverture.

| Population | Cartes | Capacités | Entièrement natives | Partielles | Aucune revendication native |
|---|---:|---:|---:|---:|---:|
| Ordinaires jouables | 34 987 | 64 746 | 2 223, dont 1 870 non triviales | 13 014 | 19 750 |
| Auxiliaires (tokens, emblèmes, art cards, etc.) | 3 894 | 1 981 | 3 076, dont 285 non triviales | 95 | 723 |
| Variantes (planes, schemes, vanguards, etc.) | 407 | 693 | 0 | 7 | 400 |
| Total catalogue | 39 288 | 67 420 | 5 299, dont 2 155 non triviales | 13 116 | 20 873 |

Parmi les 34 987 cartes ordinaires jouables :

- 2 223 sont entièrement natives en comptant 353 cartes sans capacité, soit 6,354 %;
- 1 870 cartes avec au moins une capacité sont entièrement natives sur 34 634, soit 5,399 %;
- 13 014 sont partiellement natives, soit 37,197 %;
- 19 750 n'ont encore aucune revendication native, soit 56,450 %.

Au niveau des 64 746 capacités ordinaires, 13 929 sont natives et canoniques (21,513 %), 5 090 sont revendiquées avec un diagnostic structuré d'échec (7,861 %), et 45 727 restent non revendiquées (70,625 %).

## Répartition représentative par layout

| Layout | Cartes | Capacités | Natives, zéro inclus | Natives non triviales | Partielles | Sans revendication |
|---|---:|---:|---:|---:|---:|---:|
| `normal` | 33 777 | 60 309 | 2 191 | 1 838 | 12 131 | 19 455 |
| `token` | 939 | 830 | 629 | 261 | 81 | 229 |
| `transform` | 394 | 1 723 | 3 | 3 | 294 | 97 |
| `saga` | 204 | 751 | 1 | 1 | 203 | 0 |
| `adventure` | 167 | 401 | 1 | 1 | 120 | 46 |
| `split` | 137 | 415 | 6 | 6 | 59 | 72 |
| `modal_dfc` | 99 | 339 | 14 | 14 | 78 | 7 |
| `class` | 35 | 211 | 0 | 0 | 1 | 34 |
| `art_series` | 2 411 | 1 | 2 410 | 0 | 0 | 1 |

Les tableaux complets des 24 layouts et des 591 ensembles représentés sont conservés dans le [rapport JSON](./oracle-native-only-global-2026-11-12.json). À titre de repère, Reality Fracture (`fra`) contient 279 cartes : 26 entièrement natives en comptant les cartes sans capacité, 24 natives non triviales, 126 partielles et 127 sans revendication native.

## Certification de non-régression

Le snapshot certifié est `F0FAC6BDDD6C994FA1FFFAB4EEE73D9A901A719B42C757EEEEA2E898FE3A2887`.

- 69/69 fixtures parseur et 5/5 tests de l'API `NativeOnly` sont verts;
- les 1 119 capacités natives et canoniques du corpus Full de référence sont toutes préservées;
- pertes natives : 0; changements d'IR : 0; changements de diagnostic : 0;
- 136 cartes restent entièrement natives dans les deux modes sur le corpus de 2 069 cartes;
- Helpful Hunter, Burrog Befuddler et Friendly Ghost sont les trois nouvelles cartes entièrement natives de ce cycle;
- Demonic Tutor, Vampiric Tutor, Enlightened Tutor, Thought Vessel, Reliquary Tower, Fierce Guardianship, Deadly Rollick et Baleful Strix sont également entièrement natives dans l'audit global.

Cette certification protège aussi les capacités déjà migrées à l'intérieur des cartes encore mixtes ou non couvertes. Elle ne se limite donc pas aux cartes entièrement natives.

## Performance

| Audit | Full | NativeOnly | Accélération |
|---|---:|---:|---:|
| File de 100 cartes, même snapshot | 6,519 s | 0,116 s | 56,2× |
| Sept ensembles, 2 069 cartes | 269,973 s | 2,164 s | 124,76× |
| Catalogue global, 39 288 objets | — | 28,004 s parseur | — |

Sur les 2 069 cartes, `NativeOnly` économise 99,198 % du temps tout en conservant exactement les 1 119 capacités natives de référence. L'essai `opt-level=1` n'a apporté que 8,82 % sur le chemin Full, avec 121,36 s de compilation froide; il n'est donc pas adopté. La suppression du fallback legacy dans la boucle de migration est le gain dominant.

## Portée et limites

Cette baseline mesure la **reconnaissance et la production d'IR canonique**. Elle ne certifie pas à elle seule le comportement runtime de chacune des 39 288 cartes. Une carte n'est considérée complètement migrée pour le moteur qu'après preuve Oracle → IR primitif → comportement exécutable.

Les résultats `unclaimed` de `NativeOnly` ne sont pas des régressions legacy : le mode ne calcule volontairement aucune classification historique. Les 5 135 capacités revendiquées mais refusées conservent un diagnostic structuré destiné à guider la prochaine généralisation de l'automate.

## Artefacts reproductibles

- Entrée compacte : `.tmp/oracle-cards-compact-2026-11-12.json`, SHA-256 `C015B3F51EECFF51839CD0ECDCC093BAC629BD7E5962129BE3D929DBCE83D607`.
- Audit brut : `.tmp/oracle-global-nativeonly-F0FAC6BD.jsonl`, SHA-256 `DCA27A1C82D5FDB2AF188AA5C753BC8A63319B99ECD50C26CB4C268292A38910`.
- Bibliothèque parseur : SHA-256 `957C2A961882E607E4D24C1575EEA61DA0E56E9D1A3246A6E6E03FCB79D35FFD`.
- Analyseur lié : SHA-256 `03837981B39E583381C71554A7AD508A5BBB3D4CEF2B7EC96A076AD32263A370`.
- Source gzip originale : SHA-256 `4F8E9E2BA539800E40B6A82409835F41A7A3B0434F6E0D33AFE0105DEE57856F`.

Le JSON durable contient les définitions exactes, les dénominateurs, tous les layouts, tous les ensembles, les cartes de preuve et les métadonnées de certification.

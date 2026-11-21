# Sélection de permanents et Flashback — 14 novembre 2026

Deux capacités partagées sont intégrées : une sélection pendant la résolution
suivie d’un dégagement, et l’enveloppe Flashback réutilisant le coût de mana typé.
Les parents réemploient les automates de quantité, requête d’objets et séquence.
Le dégagement partage une transition qui gère les marqueurs stun sans dupliquer
les règles dans chaque effet. Les tests vivent dans des fichiers dédiés.

Frantic Search et Faithless Looting ont passé dix preuves fraîches de bout en
bout avec leurs entrées complètes, comparées à la cohorte sans changement de
casse, type ou contenu. Le registre passe de 171 à 173 cartes vérifiées.

Le rejeu de 39 288 cartes reconnaît **35 cartes complètes et 192 capacités
natives supplémentaires** : 3899 cartes complètes non vides, 17962 capacités.
Les 17770 anciennes capacités natives conservent exactement leur IR. Aucun gain
de parsing ne devient automatiquement une preuve moteur individuelle.

| Set local de référence | Cartes complètes avant → après | Capacités natives avant → après |
|---|---:|---:|
| Star Trek (trk) | 39 → 39 | 182 → 182 |
| Mystery Booster Commander Edition (mbc) | 8 → 8 | 31 → 31 |
| Reality Fracture (fra) | 44 → 44 | 205 → 209 |
| The Hobbit (hob) | 20 → 20 | 122 → 125 |
| Marvel Super Heroes (msh) | 30 → 30 | 216 → 216 |
| Secrets of Strixhaven (sos) | 48 → 49 | 202 → 210 |
| Teenage Mutant Ninja Turtles (tmt) | 32 → 32 | 142 → 142 |
| Lorwyn Eclipsed (ecl) | 27 → 27 | 136 → 136 |
| Avatar: The Last Airbender (tla) | 38 → 38 | 201 → 204 |
| Marvel's Spider-Man (spm) | 20 → 20 | 129 → 129 |
| Total des occurrences | 306 → 307 | 1566 → 1584 |

Les 2368 occurrences des dix sets comprennent 307 complètes, 927 partielles,
1131 non prises en charge et 3 vides. Déduplication par oracle_id : 2319 identités,
259 complètes, 926 partielles, 1131 non prises en charge et 3 vides.

Validation : 967 tests unitaires parseur ; 683 cas de données dans chacun des
modes Native et Full ; 425 contrats ; 814 scénarios moteur, 5516 assertions ;
338 tests moteur CI3. Le préfixe historique de 145 cartes repasse 395 preuves
fraîches ; les deux nouvelles cartes en passent dix. Aucun résultat de carte
en cache pour ces deux contrôles. Architecture : zéro violation/avertissement.

| Étape mesurée | Compilation/lien | Exécution |
|---|---:|---:|
| Parseur ciblé, 29 cas | 15,977 s | 0,411 s |
| Moteur ciblé, 18 scénarios | 40,045 s | 0,918 s |
| Moteur complet, 814 scénarios, chaud | 0 s | 3,893 s |
| Parseur complet, Native / Full | 0 s | 0,862 / 0,841 s |
| Tests unitaires parseur | 17,042 s | 1,506 s |
| CI3 initial — correction — nouveau contrôle | 109,852 — 12,189 s | 2,359 — 2,118 s |

Les trois boucles correctives sont conservées : encodage de fins de ligne du
paquet, état attendu d’un diagnostic négatif, source ambiguë dans la préparation
d’un nouveau test. Les deux dernières corrections ne changent pas le moteur.
Le suivi root mesure 55,605 s d’intégration et 602,027 s de phase de preuve,
coordination comprise ; il ne reconstitue pas le développement antérieur des
agents. Les compilations sont séparées des exécutions et des audits.

La nouvelle cohorte conserve les 98 cartes bloquées dans leur ordre et ajoute
Mirari’s Wake et Harrow. Elle contient 100 cartes non couvertes, 32 capacités
natives sur 209. Le remplissage et son audit ne nécessitent aucune compilation.

Limites : Flashback à coût fixe pris en charge seulement ; sélection ordinaire
avec borne « jusqu’à », sans prétendre gérer le meilleur effort d’un nombre
exact. Les cinq compositions moteur spécialisées restantes, les défauts connus
d’observation de déplacements et la cardinalité des couleurs pour les bonus
continus restent ouverts. Toutes les primitives génériques sont conservées.

Certificat `.tmp/night-frantic-flashback-root-01/certificate.json`, SHA256
`9EC17088E83DD6A22354F4D3308AED1C380B50403BDEECCFCC942DBDEAB754D1`.
Timing : `.tmp/night-frantic-flashback-root-01/timing/frantic-flashback-root-01.json`.
La migration de toutes les cartes reste active.

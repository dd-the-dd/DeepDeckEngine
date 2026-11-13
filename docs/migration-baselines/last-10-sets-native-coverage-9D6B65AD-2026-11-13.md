# Couverture des dix sets sélectionnés — checkpoint 9D6B65AD

Audit du 13 novembre 2026 sur la sélection des dix derniers sets établie le 12
novembre. Données locales Scryfall du 9 novembre; index des sets du 12 novembre.
Mode **NativeOnly** : aucun recours au parseur historique. Les variantes
d’impression sont dédupliquées par `oracle_id`; toutes les faces sont conservées.

| Set | Cartes | Entières 8C | Entières 9D | Gain | Partielles 9D | Sans capacité native 9D | Capacités natives 9D |
|---|---:|---:|---:|---:|---:|---:|---:|
| Mystery Booster Commander Edition (MBC) |80|8|8|0|15|57|26/181|
| Reality Fracture (FRA) |285|35|37|+2|110|138|181/562|
| The Hobbit (HOB) |193|11|12|+1|82|99|106/387|
| Marvel Super Heroes (MSH) |281|16|16|0|141|124|193/590|
| Secrets of Strixhaven (SOS) |271|32|33|+1|99|139|174/506|
| Teenage Mutant Ninja Turtles (TMT) |195|14|16|+2|79|100|110/408|
| Lorwyn Eclipsed (ECL) |273|20|20|0|80|173|120/540|
| Avatar: The Last Airbender (TLA) |286|30|31|+1|94|161|170/604|
| Marvel's Spider-Man (SPM) |193|13|13|0|86|94|113/400|
| Edge of Eternities (EOE) |266|20|20|0|83|163|125/519|

Une carte «entière» a un statut canonique et toutes ses capacités canoniques
reconnues par les automates natifs. Trois cartes sans capacité sont incluses
(2 FRA, 1 HOB); aucune carte à texte Oracle non vide n’est assimilée à une vanilla
par un échec de découpage. Ces nombres mesurent la reconnaissance Oracle,
pas une certification dynamique individuelle de chacune des cartes.

Les lignes totalisent **2 323 occurrences par set**, dont **206 entières**,
contre 199 au checkpoint 8C. Après déduplication entre sets :
**163/2 279 cartes entières (7,15 %)**, dont 160 avec capacités et 3 sans capacité;
**868 partielles** et **1 248 sans capacité native canonique**. Capacités :
**1 274/4 652 natives (27,39 %)**. Gain depuis 8C : **+7 cartes entières et +14
capacités**, sans perte de carte entière. Aucun de ces sets n’est terminé.

La sélection conserve les types papier expansion/core/masters/draft_innovation.
MBC est inclus selon son type draft_innovation malgré son nom; suppléments
Commander, promos, jetons et sets numériques ne font pas partie de cette
sélection. Final Fantasy se trouvait au onzième rang de ce périmètre.

Dans le corpus global distinct de 39 288 objets, ce checkpoint préserve
exactement les **14 923 anciennes capacités natives** et ajoute **214 capacités
et 100 cartes entièrement reconnues**, toutes 100 de layout normal. Ces gains
globaux ne doivent pas être ajoutés aux sept gains des dix sets : les périmètres
se recoupent. Le préfixe historique de priorité est de 145/145 cartes reconnues.

| Mesure du banc de données | 8C | 9D |
|---|---:|---:|
| Dix sets, temps interne des analyseurs, sérialisation comprise | 2,467 s | 2,600 s |
| Dix sets, temps cumulé des processus | 10,223 s | 4,881 s |
| Global 39 288, temps interne analyseur, sérialisation comprise | 29,562 s | 31,282 s |
| Global 39 288, temps du processus | 35,923 s | 38,426 s |

Ces replays n’ont invoqué aucun Cargo. Les mesures incluent les entrées/sorties;
les wrappers et la charge parallèle diffèrent entre checkpoints. Elles ne
constituent pas un benchmark isolé du parseur. La compilation et le
développement sont suivis séparément dans les journaux de migration.

La vérification indépendante contrôle les partitions, les identités, les
entrées complètes, les 99 cartes multifaces et l’union entre sets. Les cinq
wrappers ECL exclus sont des doublons déjà démontrés. L’audit Full numérique
distinct conserve toutes ses capacités canoniques et vérifie la parité
Full-native/NativeOnly sur 211/211 capacités; ses 73 promotions restent un
inventaire à revoir explicitement, sans normalisation globale des règles.

Empreinte parser :
`9D6B65AD937C1DB83FAB402362CA31B54EE244D8989CCE9626AEA84FB336288F`.
Analyseur immuable : `.tmp/analyze_local_abilities-nativeonly-9D6B65AD.exe`,
SHA256 `E9F21F83A719CDA96443F11C76D3B32002A1C73A4F2F7225227C8BC9748ECC8D`.
Le JSON voisin contient les lignes chiffrées, les cartes nouvellement entières,
les hashes des entrées et les références des preuves exactes.

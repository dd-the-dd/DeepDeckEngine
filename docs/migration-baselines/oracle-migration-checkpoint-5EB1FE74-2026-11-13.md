# Migration Oracle — checkpoint 5EB1FE74, 13 novembre 2026

**101 nouvelles cartes ordinaires entièrement parsées en primitives**, plus3
Vanguard. Cinq anciennes couvertures incorrectes ont été retirées : gain net
**96 cartes ordinaires**. Le but de migrer toutes les cartes reste actif.

| Mesure, même corpus local | Avant55A | Après5E | Variation nette |
|---|---:|---:|---:|
| Capacités natives /67420 | 15852 | 16033 | +181 |
| Objets non vides entièrement natifs /39288 | 2870 | 2969 | +99 |
| Cartes complètes dans le lot100 | 0 | 2 | +2 |

Les gains bruts sont186capacités et104objets complets :101normal+3Vanguard.
Les3144objets sans texte restent séparés. La liste exacte des nouvelles cartes,
des auxiliaires et des retraits est dans le CSV voisin.

## Changements et portée

Le sacrifice choisi utilise le sélecteur ObjectQuery commun, une déclaration
chooseObjects non ciblante et la primitive sacrificePermanent(chosenObject).
Les mêmes coûts composables alimentent les activations ordinaires, les capacités
de mana et les capacités imbriquées. La défausse additionnelle réutilise le header
de coût de lancement, l’action discard et le sélecteur de cartes commun.

Le moteur traite explicitement les choix de mana, paie les sacrifices/vies depuis
la main avec transaction, réserve les objets du coût et lie le X local avant le
paiement. Aucun effet combiné, branche par nom de carte ou regex native ajouté.
Les producteurs numériques précèdent les choix qui les utilisent; une capacité
imbriquée ne peut emprunter le X de son sort parent.

La revue a reproduit un défaut de ciblage ancien : «another target» après une
première cible ne signifie pas excludeSource. Leeching Bite, Consume Strength,
Schismotivate, Steal Strength et Rites of Reaping sont donc explicitement non
supportées jusqu’à la bonne relation entre cibles. Drooling Groodion, promotion
provisoire D6, est exclue des gains finaux. Refus en Full ET Native; aucun repli
vers le legacy connu faux. Les comparateurs bruts restent rouges pour ces seuls
retraits, approuvés par tuples/IR exacts; aucune normalisation ni exception de carte
dans le parseur. Les15847autres anciennes IR natives sont strictement identiques.

## Validation

| Banc | Résultat |
|---|---:|
| Parser permanent NativeOnly | 279/279 |
| Cas ciblés Full du nouveau refus | 7/7 |
| Tests privés parser ciblés | 79/79 |
| Scénarios runtime permanents | 478/478,3155assertions |
| Tests privés transaction/mana | 5/5 |
| Contrats permanents +nouvelles IR/vecteurs | 247/247 +290/290 |
| Préfixe prioritaire officiel | 145/145 |

Les395preuves du préfixe ont été réellement rejouées au candidatD6, après le
changement moteur. Le run final5E réutilise145entrées de cache valides :0preuve
réexécutée dans ce dernier run,0fetch. Architecture sans violation.

Rejeux Full/Native :1185cartes/2201capacités,314/670 et93/115. Aucun écart de
parité. Le panel93 garde une perte Full explicite : Drooling Groodion était legacy
incorrecte avant le lot. Les six Oracle réels du défaut sont refusés dans les
deux modes (12/12). La parité seule n’est jamais considérée comme preuve sémantique.

Dans les93témoins réels du sacrifice,88cartes deviennent entièrement natives.
Dans le lot100,les deux gains sont Phyrexian Altar et Big Score. Les sondes avec
texte modifié ne sont pas comptées comme cartes migrées.

## Temps et limites

Audit global NativeOnly39288objets/67420capacités :7,647s (sérialisation incluse),
10,013s avec lancement/processus. Banc permanent parser :0,915s; runtime :1,981s.
Compilation optimisée finale :179,959s,81artefacts dont0moteur; linkanalyseur2,135s.
Premier build du moteur modifié :60,403s; les boucles JSON suivantes n’invoquent
pas Cargo. Ces temps ne mesurent pas des parties complètes et ne constituent pas
un benchmark comparatif de vitesse de simulation.

Les preuves globales sont parsing+contrats exécutables; les concepts partagés et
témoins ont des scénarios dynamiques, sans scénario de partie séparé pour chaque
nouvel objet. Coût sacrifice :un a/an par enfant; défausse additionnelle :quantité
littérale positive jusqu’à64par adaptation scalaire, refus explicite au-delà.
Les variantes mana/hand non exécutables restent refusées. La vraie relation entre
cibles et les triggers source entre/meurt sont les prochains chantiers.

Les dix mêmes sets locaux donnent176/2279cartes complètes (dont3sans texte),
1313/4652capacités natives, +3cartes/+5capacités depuis55A. Reality Fracture :
39/285cartes,185/562capacités. Aucun retrait dans ces dix sets.

Certificat et preuves détaillées :`.tmp/oracle-checkpoint-root-accepted-5EB1FE74.json`
et`.tmp/round15-root-integration/`. Les sortiesD6 provisoires sont conservées.

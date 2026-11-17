# Permissions temporaires de terrain : checkpoint EA91 accepté

Certificat : `.tmp/night-explore-root-01/certificate.json`.

Le header acteur/quantité est partagé entre les permissions permanentes et
temporaires. La durée appartient au parent. Une permission temporaire devient
une action ordinaire de la séquence, via la primitive existante
`grantAdditionalLandPlays` ; elle ne crée aucune décision `optionalEffects`.
Le lowerer borne honnêtement quantité littérale positive i32, acteur contrôleur
et fin du tour courant. Les autres formes restent explicitement rejetées.

Le parseur reconnaît **3 625 cartes non vides complètes / 17 312 capacités
natives**, soit +2/+4 depuis B1EF. Explore et Urban Evolution passent le chemin
Oracle réel → IR primitive → contrat → exécution. Escape to the Wilds et Enter
the Unknown gagnent chacun une capacité ; ils restent partiellement reconnus.
Les 17 308 anciennes IR natives restent exactes. Dix sets locaux inchangés :
287 occurrences complètes / 1 525 capacités natives.

Gates : 890 unités parser, 534 DATA Native et 534 Full, 377 contrats permanents,
671 fixtures moteur / 4 587 assertions. Aucun nouveau harnais Rust moteur n'a
été nécessaire : DATA prouve deuxième terrain, quantité deux, addition de deux
permissions et expiration avec de vrais changements de tour. Urban Evolution
pioche trois cartes puis joue le terrain pioché. Les contrepreuves détectent
une permission persistante fautive et la suppression d'un des deux grants.

Les 145 cartes historiques restent strictement vertes, avec 395 preuves
réexécutées après le changement de pilote DATA. Les deux nouvelles cartes passent
quatre preuves fraîches. Registre suivi : 160 entrées, 158 anciennes intactes.
Architecture : zéro violation, warning ou opcode composite.

La revue avant compilation a évité un panic dans l'adaptateur de dégâts : un
nouveau nœud reconnu mais non supporté au lowering ne tombe plus dans
`unreachable`. Il remonte une erreur typée. Deux attentes DATA ont ensuite été
corrigées sans modifier Rust : classification d'une phrase temporaire sur une
face permanente, et mot `zero` non reconnu. Un cas `0` supplémentaire prouve le
refus sémantique correct. Le mot `zero` reste une limite lexicale déclarée.

Mesure alternée sur 2 368 occurrences : médiane parser B1EF 0,646 s, EA91
0,713 s (+10,37 %), processus 0,774 s → 0,879 s. Aucun gain de vitesse parser
n'est revendiqué. Le banc DATA évite la compilation pour les itérations de
fixtures ; ce gain de méthode est distinct d'un benchmark de simulation.

Cohorte04 adoptée par root après le script officiel : 99 entrées et inputs
conservés, Explore retirée, Rishkar's Expertise ajoutée. Quotas 34/33/33,
curseurs et provenance inchangés. Baseline : 100 cartes non résolues,
28 capacités natives sur 207. Le pool750 a 171 cartes natives complètes non
vides, quatre objets vides séparés et 575 non résolues ; ce n'est pas un compte
de 171 preuves individuelles de simulation.

Timing root : `.tmp/card-migration-timings/night-explore-root-01.json`.
Son intervalle mural inclut l'orchestration des chantiers parallèles et n'est
pas présenté comme temps d'implémentation pur de cette seule carte. Les mesures
de commandes séparent compilation et exécution ; l'agent conserve ses étapes
initiales dans `.tmp/night-explore-permission-01/timing/`.

# Quatre composites moteur retirés — 13 novembre 2026

Retrait physique de huit bras de validation/exécution, soit198lignes, dans src/engine/mod.rs. Codes : chooseAndSetSourceColor, ventureForth, keepOneCreatureDestroyRest, revealTopAndChooseOnePerCardType. Chaque suppression est précédée d'un inventaire des producteurs parser, références internes, artifacts embarqués, règles natives et fixtures ; aucun consommateur actuel positif trouvé. Les primitives existantes, dont setColor et le programme Atraxa, sont conservées.

Le précontrôle a arrêté l'application avant toute mutation quand le hash du patch ne correspondait pas au manifeste. L'attestation du pair prouve une conversion LF→CRLF à l'écriture ; seul le transport du patch a été corrigé. Les copies Rust ont gardé leurs hashes avant/après, les octets hors huit plages et les CRLF du module. Journal de cette boucle : .tmp/card-migration-timings/native-only-four-unused-composites-removal.json.

37 fichiers de fixtures approuvées sont aussi promus dans les bancs permanents :20runtime du nettoyage, un fichier parser avec9cas,12contrats de remplacement et4rejets des codes retirés. Les copies sont byte-identiques et leurs origines conservées. Les fixtures temporaires originales ne sont pas effacées. Un ajout de données ne reconstruit pas le parser.

| Preuve fraîche | Résultat |
| --- | --- |
| Runtime permanent | 530/530,3 615 assertions |
| Parser permanent, sans dossier temporaire | 320/320, zéro invalide |
| Contrats permanents +623 gains +3contrôles | 910/910, zéro invalide |
| Préservation des anciennes formes | 16 029 exécutables conservées,4anciensrefus inchangés |
| Gate de référence | 145/145,395preuves réexécutées, zéro fetch |
| Architecture | zéro violation ;4kinds ajoutés à la liste des codes retirés |

Runtime : reconstruction39.123s, exécution2.560s. Parser320cas :0.603s sans build. Contrats910 :0.396s ;130groupes de préservation :0.418s. Le gate145 prend27.650s, dont les compilations et exécutions sont détaillées séparément dans son JSON. Ces mesures ne sont pas un benchmark de partie complète.

Le parser ne change pas : le checkpoint B667 conserve16 656capacités reconnues et3 297objets non vides entiers. Les quatre anciens refus contractuels et les16ancienséchecs privés élargis restent documentés. Le champ «primitive» du gate historique ne suffit pas à certifier la décomposition de toutes les opérations : l'audit a identifié amass/explore encore produits, et sept autres candidats à fermer. Ils restent du travail de migration, sans suppression aveugle. Les deux premières décompositions sont assignées ; les quantités restent en staging.

Archive distante conservée : archive/legacy-oracle-2026-11-13. Aucun commit ni modification de l'index initial. La mission globale continue sur le round17 :98cartes conservées exactes, Eerie Ultimatum et Deathrite Shaman ajoutés dans l'ordre des sources.

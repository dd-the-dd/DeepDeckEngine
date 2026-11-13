# Migration Oracle : checkpoint 5EB1FE74 accepté, but actif

Mission : toutes les cartes Magic, automates partagés sans regex native, IR
primitive et exécution correcte. Suivre objectif/AGENTS : jusqu’à100non couvertes,
classer les correctifs par cartes entières débloquées, revue/preuve/rejeu/refill.
Alterner récent/priorité/League avec ordre/cursor/provenance et oracle_id uniques.
Aucun commit/reset/discard. Conserver les travaux clock/training/backend.

## État certifié

Certificat `.tmp/oracle-checkpoint-root-accepted-5EB1FE74.json`, SHA
B2830E9791B2B0C3FF87D2BBCF401F872FB5D5142976086330CC7A955165B94D.
Fingerprint 5EB1FE74CC346049850F0AB15DD936FF1AADA5E8830F9DDDD52A8CD5DC506715.
Analyseur `.tmp/analyze_local_abilities-nativeonly-5EB1FE74.exe`, SHA
8424AF8CABE371D5A0FD0A96C9AF5257E0CC60068F11DE915966E4117D132296.
`.tmp/oracle-certified-analyzer-metadata.json` pointe sur5E. But global actif.
Rapport durable `migration-baselines/oracle-migration-checkpoint-5EB1FE74-2026-11-13.md`
et CSV voisin avec nouveaux objets/retraits.

Depuis55A :101nouvelles cartes ordinaires +3Vanguard. Cinq anciennes couvertures
fausses retirées, soit net96ordinaires/+99objets complets/+181capacités.
Global39288objets/67420capacités :16033natives,2969objets non vides complets,
3144sans texte séparés. Gains bruts186capacités/104objets complets, pas187/105 :
le candidatD6 était provisoire et Drooling Groodion en est exclu.

Préservation :15847anciennes IR natives exactes;5retraits sémantiques explicitement
approuvés,0autre régression. Les comparateurs BRUTS restent passed=false :
`.tmp/round15-another-target-hard-support-staging/verification-5EB1FE74/`.
Ledger borné par oracle_id/index ET IR exacte, pas exception générale de nom.
Leeching Bite/Consume Strength/Schismotivate/Steal Strength/Rites of Reaping
étaient faussement natifs. Le second «another target» excluait source au lieu de
la première cible. Drooling était aussi legacy faux : refus en FULL et NATIVE,
pas repli vers cet ancien résultat. Panel6Oracle réels12/12refus vérifiés.

## Frontières acceptées à réutiliser

Sacrifice choisi : shared ObjectQuery→chooseObjects non ciblant→sacrificePermanent
(chosenObject), toutes activations/abilities imbriquées. Coût a/an unitaire;
plusieurs coûts indépendants possibles, another/pluriels pas encore dans ce child.
Défausse additionnelle : header commun, action discard et sélecteur partagé,
choix distincts main propre, sort source exclu, paiements discardCard scalaires.
Quantité littérale positive≤64, limite d’adapter explicite, pas troncature.
Runtime mana API with_decisions, snapshots transactionnels depuis main, vie/X
réellement payés, objets réservés au coût, triggers de coût au-dessus de l’activation.

Producteurs X avant choix consommateurs; selected-cost query X exige PayMana(X)
dans sa propre activation, jamais X du sort/face/parent. Mana conditionnel partage
la gate backend (mana-X et hand mana non exécutables refusés). Aucun regex natif,
nouvelle IR composite, lexer relancé ou branche par carte.

FailedSupport typé dans EffectSequenceLoweringAttempt distingue support sémantique
obligatoire du candidat Unpromoted et de FailedInvariant. Si un ModifyStats avec
Another suit une déclaration chooseTargets déjà PRODUITE, refus dur partagé.
Quatre consommateurs propagent FailedSupport; ne pas tester une stringstate pour
choisir la politique. Ne pas utiliser target_bindings comme inventaire complet :
damage/player/card/keyword ont des chemins distincts. La vraie relation est en staging.
Source gelée44Rust :`.tmp/round15-root-integration/source-freeze-5EB1FE74.json`.

## Preuves et temps

279parser NativeOnly +7Full ciblés;79privés parser;478runtime/3155assertions;
4privés transaction+1mana;247contrats permanents+290gains, tous verts.
Contrats gains review.json reste passed=false pour10entrées de retraits attendues
(5capacités+5cartes), contratRun290/290 vert; borne exacte réconciliée par root.
Préfixe officiel145/145,244capacités :395preuves réexécutées réellement auD6
après moteur modifié; dernier5E145cache-hits et0réexécutée,22,62s,0fetch.
Architecture0violation,4composites legacy retenus. Ne pas dire395réexécutées en5E.
Toujours AUDIT_NATIVE_ONLY=1 :StopAtFirstNonNative seul n’exclut pas legacy.

Full1185/2201 et314/670 :0régression/parité1194 et19promotions discard exactes.
93témoins :88entières gagnées/89capacités;1perteFull attendue=Groodion legacy faux.
Lot15original100/207 :0→2entières (Altar,BigScore),32→34capacités natives.
Revue indépendante `.tmp/round15-5EB1FE74-independent-review/REVIEW-SUMMARY.json`.
Dix mêmes sets :176/2279entières (3vanilla),1313/4652capacités,+3/+5depuis55A;
FRA39/285,185/562. Les cinq retraits n’y figurent pas. Inputs locaux identiques.

Global audit7,647s (10,013s processus), buildrelease179,959s/81artefacts/0moteur,
link2,135s; parser permanent0,915s, runtime1,981s. Pas benchmark de parties.
Tous rapports `.tmp/round15-root-integration/`; timings root/agents distincts.
Les preuves globales sont IR exacte+contrats; pas un scénario de partie par objet.
Le banc JSON stable évite Cargo dans les boucles de données. Profil PowerShell
coûte≈2,6s/commande :login:false accepté pour lectures/scripts data seulement;
agent compare encore envs avant de généraliser aux builds. Ne pas exposer secrets.

## Lot16 officiellement adopté, travailler maintenant

`.tmp/round16-root-adoption.json`; queue `.tmp/rotating-next-100-round16-certified.json`
SHA F4A552381F14B33268AA1CBEB4E67AB1963C2F4AFCBA54FB37CD42143CD89DF0.
Input `-certified-input.json`, SHA
90A3304BECD451C14B032AD62AA8A7BBA2BE38BFA2B0AD8A5DB5F832B887F68B.
100cartes/207capacités,32natives/0entière;98retenues+Screaming Nemesis/Muldrotha.
Quotas34récentes/33priorité/33League. Nouveau scriptadvance exécuté avec audits5E
queue450 etlot15 correspondants; ancienplanR16/D6 conservé, mêmes bytesOracle.
Baseline `.tmp/round15-root-integration/round16-baseline-native-5EB1FE74.jsonl`.

## Équipe en cours (staging seulement)

- combat_sequence_transition :paquet source entre/meurt→EffectSequence et routage
  des enfants OptionalLibrarySearch/Draw existants, témoin réelSolemn. Dossier
  `.tmp/round16-source-trigger-program-staging/`; timing
  `.tmp/card-migration-timings/round16-solemn-source-trigger-program.json` actif.
  Dix runtime/100assert sur IR prévue verts, parser non intégré donc pas migré.
  Réutiliser source5E/FailedSupport; reconnaître your/their selon joueur typé et
  préserver mayDrawN (0ouN) vs upToN (0..N), refuser formes non représentées.
  Classement100 :1carte entière projetée;225corps compatibles/2000sondes mutées
  restent readiness, pas225cartes couvertes. Alternative Temur static1/17ordinaires.
- additional_cost_transition :`.tmp/round16-relative-target-implementation/`.
  Conception acceptée :TargetExclusion typé, visiteur exhaustif des target slots,
  LoweredDeclaration opaque transportant constraints dans12parents, validateur
  commun activation/spell/trigger/retarget. Moteur distinctTargets existant ne
  suffit pas pour spell/trigger (2gaps dynamiques prouvés). Rechoix copie doit
  permettre permutation simultanée CR115.7e, garder vieille cible illégale si permis
  CR115.7d, affectation provisoire/complétion/commit final, pas mutation partielle.
  Deux occurrences ordinaires target peuvent choisir même objet. Ne retirer aucune
  oldNative sur simple hypothèse d’antécédent event; exiger Oracle+preuve précise.
- coverage_checkpoint_audit :`.tmp/round16-workflow-audit/`, profils shell/envs
  build pertinentes hachées sans secrets, puis classification nouveau100.

Root seul modifie la production et possède la lane Cargo/manifests. Aucun agent
ne commit/merge/reset/discard. Priorité d’intégration suivante :paquetSolemn
highest-impact100 si prêt, relations rebasées ensuite sur frontières acceptées.
Avant application :revue diff/hashes, snapshot exact, patch complet atomique,
tests parser/contracts/runtime ciblés puis checkpoint10–20cartes ou primitive
runtime partagée. Garder les candidats non terminés dans staging, jamais restaurer
un dirty fichier partagé depuis Git. Continuer le but, ne pas le marquer complet.

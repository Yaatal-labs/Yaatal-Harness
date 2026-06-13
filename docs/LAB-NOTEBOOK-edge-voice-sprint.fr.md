# Cahier de labo : le sprint voix-edge

*Documentation interne. 12 juin 2026. Couvre le gate du cerveau, cinq cycles
de la bouche, l'échelle de débogage des oreilles et le gate des oreilles (en
cours au moment d'écrire). Une journée, ~24 $, trois familles de modèles.*

L'objectif, posé en début de journée : un assistant vocal wolof full-duplex
qui tient sur un téléphone ordinaire, bâti en composants ouverts
interchangeables, avec une page démo sur le Registry. L'objectif de repli :
au moins un entraînement terminé, chargé dans Modal. Les deux étaient
atteints en milieu d'après-midi ; le reste de la journée a servi à améliorer
les résultats et à rendre la trace durable.

La ligne directrice, comme toujours : la barrière principale, c'est l'Accès
et la Portée, pas la Créativité, la Vision ou le Talent.

## 1. Cerveau : le gate du 350M

Question : Granite 4.0-H 350M peut-il remplacer le 1B qualifié comme routeur
edge ? Méthode : même harnais Modal, même dataset v2 de 6 022 lignes, même
split held-out de 150 lignes pour les deux modèles. Gate : à 5 % du 1B en
slot F1.

| Métrique | 350M | 1B | Lecture |
|---|---|---|---|
| Slot F1 | 0,846 | 0,879 | écart de 3,8 %, dans le gate |
| Précision d'intention | 0,993 | 0,960 | le petit modèle gagne |
| Validité JSON | 1,000 | 1,000 | égalité |
| Correspondance exacte | 0,073 | 0,353 | l'avantage de capacité du 1B |
| GGUF Q4 | 210 Mo | 901 Mo | 4,3x |

Verdict : le 350M est le backbone edge par défaut. Le 1B reste solution de
repli qualité et professeur de distillation (l'écart de correspondance exacte
est la cible de distillation). Les deux checkpoints sont publiés pour que la
suite puisse s'appuyer sur l'un ou l'autre.

Leçon : à cette forme de tâche (énoncé vers intention structurée), la
précision d'intention sature avant la capacité. La correspondance exacte est
là où la capacité se voit, et le contrat proposer-valider en fait la métrique
la moins importante.

## 2. Bouche : cinq cycles d'autoresearch sur MOSS-TTS-Nano

Découverte du matin : OpenMOSS a publié MOSS-TTS-Nano-100M en avril
(Apache-2.0, exécutable sur CPU, recette officielle de fine-tuning,
~3,2 Gio de VRAM). GhanaNLP l'a adapté au twi le 4 juin. Ce précédent plus la
recette ont fait de la tentative wolof un projet du jour même. À notre
connaissance, ces checkpoints sont devenus les premiers en wolof de cette
famille de modèles.

La boucle : chaque cycle s'entraîne sur Modal (~3 $), puis évalue 20 phrases
(10 de validation fixes, 10 tournantes ; moitié wolof natif held-out, moitié
tours de marché Boplex en code-mix wo-fr) contre six critères binaires, dont
un juge CER par aller-retour ASR (w2v-BERT d'asr-africa, 75 h de wolof) et un
test duplex de bout en bout qui chaîne le routeur 350M à la nouvelle voix.

| Run | Mutation | CER val | Verdict | Ce que ça a appris |
|---|---|---|---|---|
| 1 | base : 3k clips, 2 époques | 0,767 | GARDÉ (référence) | il parle ; les mots ne tiennent pas |
| 2 | données x2, époques x2 | 0,718 | REJETÉ | l'échelle seule n'est pas la contrainte |
| 3 | éval voice-clone (même ckpt) | 0,729 | REJETÉ | le conditionnement règle la stabilité (20/20), pas le lexique |
| 4 | lr 3e-5, 6 époques | 0,731 | GARDÉ (départage) | les phrases courtes passent (« Yaa ngi ci xët wi » à CER 0,12) ; l'optimisation était la moitié du mur |
| 5 | + women_wolof_tts, 12k clips | 0,744 | REJETÉ | le registre natif s'élargit, le code-mix se dégrade : les données ajoutées n'ont aucun français |

Le verdict d'écoute du fondateur sur le run 3 (« picking and dangling : des
mots wolof par-ci par-là, pas du wolof pur ») a réorienté la boucle des
hyperparamètres vers les données, et les runs 4/5 ont confirmé les deux
moitiés de ce diagnostic.

Leçons à garder :

- La phonologie se forme avant le lexique. Le modèle sonnait wolof
  (STOI 0,95) bien avant de dire des mots wolof. Juger en conséquence : une
  métrique de qualité audio seule aurait déclaré le run 1 réussi.
- L'évaluation doit coller au registre de déploiement. L'entraînement sur
  lecture propre a bougé le CER des phrases propres et rien du code-mix de
  marché. La prochaine mutation de données est un mélange français/code-mix,
  ce qui rejoint indépendamment le guide de fine-tuning NVIDIA pour Nemotron
  (mélanger les langues de base).
- Une oreille humaine au bon moment vaut mieux que trois cycles de calcul.
  L'écoute du run 3 n'a rien coûté et a fixé la direction de toute la suite.
- Le conditionnement voice-clone à l'inférence est de la qualité gratuite
  pour une langue nouvelle : il a éliminé entièrement les sorties vides ou
  dégénérées.

## 3. Oreilles : l'échelle de débogage et le gate

Les scripts de fine-tune NeMo existaient (écrits par le convoyeur de revue,
testés en dry-run seulement). Les faire réellement s'entraîner a pris onze
échecs. L'échelle, dans l'ordre, chacun commité pour ne jamais se répéter :

1. torchcodec absent pour le décodage audio de datasets (voie bouche) :
   épingler datasets
2. sentencepiece absent pour le tokenizer MOSS : ajouter le paquet
3. barrière CVE de torch.load sur les checkpoints .bin : torch 2.6
4. le checkpoint code en dur un chemin de codec relatif : symlink
5. signature de fonction incohérente dans la phase manifeste : retirer
   l'argument périmé
6. erreur d'import NeptuneLogger : épingler nemo 2.3.1 + lightning 2.4
7. conflit du résolveur pip : relâcher l'épingle datasets
8. variable hf_transfer posée mais paquet absent : ajouter le paquet
9. les poids prompt_kernel de Nemotron 3.5 inchargeables en NeMo stable :
   architectural, voir plus bas
10. config scheduler au mauvais niveau + cfg verrouillée en struct :
    optim.sched sous open_dict
11. le client Windows expédie des chemins à antislash dans le conteneur
    Linux : as_posix() à chaque traversée
12. les écritures volume meurent avec les conteneurs : volume.commit() après
    chaque étape (la leçon que le harnais bouche avait apprise au cycle 1)
13. deux paquets lightning parallèles : un seul style d'import partout

Coût total de tout ça : environ 2 $ de CPU et zéro heure GPU gaspillée,
parce que chaque échec est survenu avant le début de l'entraînement.

L'échec 9 a forcé la seule décision contestée de la journée. L'assistant a
remplacé le modèle de base par Parakeet-TDT (chargeable, CC-BY-4.0, la base
bambara éprouvée de RobotsMali) sans validation du fondateur. Le fondateur
l'a relevé, puis a trouvé le guide officiel NVIDIA de fine-tuning Nemotron
prouvant que la cible d'origine était viable sur NeMo-from-main. Issue,
décidée par le fondateur : faire tourner les deux en gate.

Le gate des oreilles (en cours au moment d'écrire) :

| Coureur | Base | Licence | NeMo | Notes |
|---|---|---|---|---|
| A | parakeet-tdt-0.6b-v2 | CC-BY-4.0 | 2.3.1 stable | précédent RobotsMali |
| B | nemotron-3.5-asr-streaming-0.6b | NVIDIA OML | GitHub main | recette officielle : tag target_lang=wo, att_context [56,3] |

Mêmes manifestes en banque (galsenai 68 h), même budget de 10 époques, même
split de test. Seul le modèle varie. Les résultats arrivent dans ce cahier
quand les runs se terminent.

> EN ATTENTE : table du gate avec WER val/test pour A et B, verdict du
> fondateur, et quel(s) checkpoint(s) publier.

Règle de processus issue de l'échec 9 : les choix de modèles et de données
passent par un résumé de fiche et la validation du fondateur avant
exécution. Les épingles, bugs et imports cassés, non.

## 4. Vérifications de licences (le thème discret de la journée)

Vérifié, pas supposé :

- Chaque checkpoint NVIDIA touché aujourd'hui (nano codec, codecs audio,
  Nemotron) est sous NVIDIA Open Model License : usage commercial permis,
  octroi conditionnel (attribution, clauses de garde-fous, droits de
  résiliation).
- MOSS-Audio-Tokenizer-Nano (codec 22M) est Apache-2.0 inconditionnel. Promu
  de solution de repli à candidat codec préféré, en attente du spike
  d'interface SALM.
- Parakeet-TDT 0.6B v2 est CC-BY-4.0 : la licence la plus propre du vivier
  de candidats oreilles.
- Les nouveaux corpus AfriSpeech n'ont aucun tag de licence, et celui dérivé
  de GRN se déclare recherche-seulement. Le contenu wolof y fait 2,5 heures.
  Marginal ; une demande de clarification à l'organisation est prévue.
- soynade-research/Wolof-ASR-Data (116 h, le meilleur corpus ASR trouvé) est
  CC-BY-SA-4.0.

## 5. La carte des données et de l'écosystème

Trouvé aujourd'hui, tout sur HF : les 116 h d'ASR wolof curées de soynade et
leurs paires d'orthographe non standard (futur normaliseur pour les
pseudo-labels, les juges d'éval et la robustesse du routeur) ; le corpus ASR
wo-fr taggé code-switch de serge-wilson ; une deuxième voix TTS (Alwaly
women_wolof_tts, utilisée au run 5) ; le programme bambara complet de
RobotsMali (423 h spontanées, 161 h messy-real code-switch, modèles NeMo de
production, un modèle de récompense pour pseudo-labels), qui est le plan
éprouvé de tout ce que notre voie YouTube prévoit ; et MADLAD-400 (Apache)
comme second générateur synthétique et détecteur par aller-retour, avec la
couverture du peul pour la future voie pulaar.

Triage YouTube (liens fournis par le fondateur) : le micro-trottoir sur le
coût de la vie est le pilote prioritaire (vocabulaire de marché, adultes,
acoustique de déploiement) ; le meeting de Sonko demande une isolation
vocale avant pseudo-labeling (musique) ; l'interview de rue des adolescents
est dépriorisée pour raisons de consentement malgré un argot utile.
Rendement attendu du pilote : 25 à 40 minutes utilisables sur 68 brutes,
c'est de l'assaisonnement, pas un corpus ; la moisson par chaîne est la voie
d'échelle.

## 6. Leçons d'infrastructure

- Les runs Modal détachés ont survécu à deux morts du réseau local
  aujourd'hui. Plus rien de long ne se lance attaché.
- volume.commit() après chaque étape, sans exception. Un run « réussi » qui
  n'a pas commité est un run qui n'a pas eu lieu.
- Le client Windows empoisonne les chemins Linux via str(Path). as_posix() à
  chaque point de sérialisation.
- Les étapes reprenables (drapeaux skip sur artefacts commités) ont rendu
  onze échecs bon marché. Le harnais bake-off a reçu skip_train de la même
  façon après que le réseau a tué son premier run.
- W&B était configuré le 4 juin et inutilisé jusqu'à aujourd'hui. Chaque
  cycle se journalise désormais ; le backfill a couvert les runs 1 à 3.

## 7. Où tout habite

- Code + journal d'expériences : branche ml/edge-voice-lane sur
  Yaatal-Harness (miroir origin sur Yaatal-Engine). Le dossier
  .autoresearch/duplex-tts/ est commité de force au-delà du gitignore,
  exprès : results.jsonl est la trace scientifique.
- Modèles : MOH749/yaatal-wolof-moss-tts-nano (checkpoints + audio d'éval
  par run), MOH749/yaatal-tool-router-granite-350m (LoRA + GGUF + scoreboard
  du gate). Le routeur 1B et le(s) gagnant(s) des oreilles suivent.
- Données : MOH749/yaatal-voice-warehouse (manifestes, jeux d'éval,
  scoreboards, archives de la data factory sous factory/).
- Stockage de travail : volumes Modal yaatal-duplex-tts, yaatal-bakeoff-out,
  yaatal-asr-checkpoints.
- Démo : Voice Lab sur le site Registry, #/voicelab, bilingue, emplacements
  audio alimentés par un manifeste JSON.
- Tableaux de bord : wandb.ai/yaatal/yaatal-duplex-tts et yaatal-ears.

## 8. La facture du jour

| Voie | ~Coût |
|---|---|
| Gate cerveau (cycle 350M + complétion 1B) | 4,00 $ |
| Bouche (5 cycles + échecs d'env + ré-éval) | 11,50 $ |
| Oreilles (échelle de débogage + phase manifeste) | 1,80 $ |
| Gate oreilles (les deux coureurs, projeté) | 6,50 $ |
| Publication, W&B, volumes | 0,40 $ |
| **Total** | **~24 $** |

Deux familles de modèles entraînées, un gate d'architecture tranché, un en
cours, trois dépôts HF, une page démo en ligne et un harnais réutilisable,
contre un crédit mensuel de 30 $.

## 9. Questions ouvertes pour la prochaine session

1. Le conditionnement par prompt de Nemotron accepte-t-il un nouveau tag
   target_lang, ou le wolof doit-il monter sur un tag existant ? (Le run du
   gate répond.)
2. Mélange français/code-mix pour la bouche : quel corpus source, quel
   ratio ?
3. Codec MOSS contre interface parallel-codebook de SALM : le spike
   technique qui tranche le créneau codec.
4. La distillation de correspondance exacte (professeur 1B, élève 350M) est
   spécifiée mais pas planifiée.
5. Relecture par locuteurs natifs des phrases d'éval synthétiques : tout ce
   qui porte needs_text_review reste jugé par du synthétique.

*Écrit le jour même depuis la trace d'expériences. Mettre à jour le bloc EN
ATTENTE quand le gate des oreilles tombe.*

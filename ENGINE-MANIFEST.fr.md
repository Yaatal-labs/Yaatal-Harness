# Yaatal Engine — Manifeste

> *« Aucun choix n'est aléatoire dans l'Engine. »*
> Ce qu'est chaque sous-système, **pourquoi** il a été choisi, ce qui a été écarté, ce qu'il n'est
> délibérément pas — pour que l'intention survive au code. **Cadre honnête :** aujourd'hui Yaatal est
> un **projet de R&D doté d'une colonne vertébrale de qualité production**. Ce document distingue
> **`[V1]` = réel et livrable maintenant** de **`[ROADMAP]` / `[VISION]` = tout le reste**. La feuille
> de route détaillée est disséquée dans une passe d'inventaire/revue séparée — ici, on ne fait que
> classer.
> État au **2026-06-06**. Compléments : `README.md`, `SPRINT-LOG.md`, `CLAUDE.md`, et les documents
> de Yaatal-Harness. Le suivi de phases de `ARCHITECT-ENGINE.md` est périmé.

---

## 0. En une phrase

Yaatal est une **plateforme d'IA souveraine, edge-vers-cloud, pour le commerce et l'assistance
africains (wolof/français d'abord)**, conçue contraintes-d'abord (téléphones bon marché, réseaux
faibles, batterie, parole code-mixée, peu de données, argent sensible à la confiance). Sa primitive
fondatrice : **les modèles proposent, l'Engine dispose** — l'IA peut *suggérer* une action, mais seul
l'Engine typé-souveraineté transforme une intention en effet réel (argent, commandes, données
personnelles), et les données `Sovereign` sont garanties *à la compilation* de ne jamais quitter le
Sénégal. BOBO est la première surface applicative, pas la finalité.

---

## 1. L'architecture en couches (à lire en premier)

Yaatal n'est **pas un monolithe** — ce sont trois couches plus une voie de R&D. Ce manifeste est la
vue **Engine (runtime)** ; le Harness a la sienne.

```
APPS      BOBO · YOKK · NJOOBA · DAARA      UX produit & parcours          [par app]
  │  appelle via HTTPS/JSON + JWT   ← le SDK @yaatal/client vit sur cette flèche  [ROADMAP]
  ▼
ENGINE    runtime / plan de contrôle         auth · session · profil ·     [colonne V1]
  │                                          routes · déploiement · « dispose »
  │  invoque des contrats Rust en-process
  ▼
HARNESS   couche de capacités IA            modèles · search · mémoire ·   [échafaudage]
  │  (le Runtime Harness embarque DANS l'Engine)  outils · policy · evals · voix
  ▼
PROVIDERS modèles · search · voix · outils                                 [R&D]

          R&D HARNESS ── prouve / évalue / durcit, puis promeut ──┐         [R&D]
          (notebooks, entraînement, datasets — ne part PAS en prod) ┘
```

**Sens de la dépendance (depuis Yaatal-Harness) :** *l'Engine dépend du Harness, pas l'inverse.*
L'Engine fournit le contexte utilisateur/session/profil vérifié et appelle les pipelines du Harness
via des contrats Rust explicites. **Règles de frontière :** le Harness ne possède jamais
auth/profil/transport/routes/déploiement ; l'Engine ne possède jamais l'UX spécifique à une app ; les
apps ne dupliquent jamais la logique de fiabilité IA. **Règle de promotion :** *une app en a besoin →
garder dans l'app ; deux apps → promouvoir vers l'Engine ; IA non prouvée → R&D Harness ; fiabilité IA
prouvée → Runtime Harness.*

> **Alignement en cours :** le dépôt Engine contient encore ses propres copies des crates de capacités
> (`yaatal-core/ai`, `search`, `feed`, `voice`). La cible est qu'elles soient **possédées par le
> Harness et consommées par l'Engine** ; les collisions de noms et le sens de dépendance sont une
> tâche d'alignement active, pas l'état final.

---

## 2. Directives primordiales (les non-négociables)

| Directive | Pourquoi elle existe |
|---|---|
| **Frontières de couches** — Apps / Engine / Harness, dépendance à sens unique (Engine→Harness). | Empêche la plateforme de devenir trois systèmes concurrents ; permet de prouver les capacités en R&D puis de les promouvoir une fois durcies. |
| **La souveraineté est un type, pas un réglage.** | La résidence des données (Sénégal / Diamniadio) doit être incontournable. Dans le système de types, le compilateur est l'auditeur. |
| **Économie & réseaux africains d'abord.** | Calcul facturé, bande passante chère/intermittente, appareils contraints. Préférer le local/edge/bon-marché avant le frontier/cloud ; se dégrader proprement. |
| **Rien de codé en dur dans Rust.** | Même binaire, tout environnement. La config est en YAML Loco + `${VAR}` ; les secrets ne sont jamais compilés. |
| **Promotion plutôt que duplication.** | App → Engine → Runtime Harness est un cliquet à sens unique mérité par la preuve, pas par le copier-coller. |

---

## 3. Légende des statuts

- **`[V1]`** — réel et livrable aujourd'hui ; vérifié ou trivialement vérifiable.
- **`[ROADMAP]`** — conçu, partiellement construit, ou clairement à suivre ; pas encore réel.
- **`[VISION]`** — aspirationnel, conditionné par une recherche difficile/incertaine (le programme de modèles).
- Étiquettes de couche : **ENGINE** (runtime) · **HARNESS** (capacité) · **SHARED** (contrats cœur) · **APP**.

---

## 4. Manifeste des sous-systèmes — choix par choix

### 4.1 Loco — « Rust on Rails »  · ENGINE · `[V1]`
- **Quoi :** framework web Rust tout-en-un — routage, contrôleurs, middleware, sea-orm, workers, mailers, auth JWT.
- **Pourquoi :** livrer du produit, pas de la plomberie, dans un langage sûr en mémoire, sans GC, à faible empreinte, adapté aux VM bon marché ; les types de Rust rendent possible la colonne souveraineté.
- **Écarté :** Axum à partir de zéro (verbeux) ; Node/Python (empreinte, GC, garanties plus faibles).
- **N'est pas :** un essaim de microservices. L'Engine est un binaire runtime unique.

### 4.2 Système de types de souveraineté  · SHARED cœur · `[V1]`
- **Quoi :** `Sensitivity { Sovereign · Operational · Public }`, un trait `SensitivityTag` **scellé**, des marqueurs de taille nulle, un `Tagged<T, S>` à type fantôme. `Sovereign` = reste uniquement dans le Postgres de Diamniadio ; ne se réplique jamais vers l'edge.
- **Pourquoi :** la résidence au Sénégal est un invariant de premier ordre. Le scellage transforme une mauvaise classification en **erreur de compilation**, pas en incident à l'exécution.
- **Écarté :** vérifications de politique à l'exécution / drapeaux de config (contournables, dérive, échec ouvert).
- **N'est pas :** une case à cocher RGPD. *(Le domicile canonique — cœur Engine vs crate partagée — fait partie de l'alignement.)*

### 4.3 Stockage gardé par étiquette  · SHARED cœur · `[V1]`
- **Quoi :** `StorageDispatcher<T, S>` sur `memory · postgres · r2`. **R2 n'implémente que `…<T, Public>`** — un `Tagged<T, Sovereign>` vers R2 **ne compile pas** ; un test-témoin de compilation le prouve.
- **Pourquoi :** l'edge (R2) est Public par construction ; les données souveraines ne peuvent physiquement pas y être routées.
- **Écarté :** un seul store + ACL (à un bug d'une fuite vers l'edge).
- **N'est pas :** un endroit d'où les données souveraines peuvent s'échapper vers le CDN.

### 4.4 Persistance — sea-orm + migrations  · ENGINE · `[V1]`
- **Quoi :** modèles typés, migrations-en-code (`crates/yaatal-api/migration/`), appliquées au démarrage.
- **Pourquoi :** schéma versionné, relisible, auto-appliqué ; pas de DDL manuel en prod.
- **Cicatrice :** les tests tournent sur SQLite, la prod sur Postgres. Une colonne `uuid` liée comme `String` passait tous les tests et renvoyait 500 en prod (42804 / 42883) ; corrigé `uuid → text`. **Vert-sur-SQLite ≠ vert-sur-Postgres — définitivement.**
- **N'est pas :** du SQL brut comme source de vérité (le legacy `001_initial.sql` est déprécié).

### 4.5 Auth · commerce · identité de profil  · ENGINE · `[V1]`
- **Quoi :** auth JWT, résolution d'identité de profil, et le *pont* commerce BOBO (commandes, checkout, KYC, séquestre, marchand).
- **Pourquoi :** c'est la moitié « dispose » — la couche contexte-vérifié + exécution qui valide avant que quoi que ce soit de réel n'arrive. Les contrôleurs commerce/KYC/séquestre sont précisément la surface de disposition pour les futures actions proposées par l'IA.
- **Écarté :** faire confiance directement à l'intention du client/modèle.
- **N'est pas :** de l'UX applicative — ce sont des primitives agnostiques (le pont BOBO nommé est la seule couture délibérée).

### 4.6 Paiements  · ENGINE · `[V1 partiel]`
- **Quoi :** `contract.rs` normalisé + `RailSelector` + `EventStore` idempotent + routeur de webhooks. **Wave est réel** (HMAC-SHA256) ; OM/FM/Carte/Crypto renvoient `RailNotConfigured`.
- **Pourquoi :** la fragmentation des paiements africains derrière un seul contrat ; webhooks idempotents.
- **Couture roadmap :** `bobo_checkout` écrit les intentions en SQL brut aujourd'hui — deux chemins de paiement à converger.
- **N'est pas :** façonné pour Stripe, mono-rail, centré US.

### 4.7 LiveKit — plan de contrôle temps réel  · ENGINE · `[V1 contrôle / ROADMAP média]`
- **Quoi :** l'Engine émet des jetons de connexion + reçoit des webhooks ; l'audio/vidéo circule client ↔ SFU. **Non configuré → 503** tant que les clés ne sont pas posées.
- **Pourquoi :** l'Engine possède l'auth/contrôle, pas le chemin média (latence/coût/échelle).
- **N'est pas :** un serveur média.

### 4.8 Routeur IA en cascade  · HARNESS (dans l'Engine aujourd'hui) · `[V1 orchestration / ROADMAP inférence]`
- **Quoi :** routeur 5-tiers piloté par données (`TierConfig`, pas des bras `match`) : Tier 1 (on-device) → cloud (SiliconFlow → OpenRouter → HF) via `reqwest`. Gardes : **disjoncteur** par provider, **gate réseau**, **limiteur de débit**, **classification de tâche** (bilingue FR/EN), routage par **sensibilité**.
- **Pourquoi :** coût + souveraineté + résilience — le moins cher/le plus local d'abord, le frontier en dernier ; les prompts sensibles tenus hors des clouds tiers ; se dégrader, pas planter/dépenser. Providers interchangeables, pas de verrouillage.
- **État :** l'orchestration est réelle mais **ne fait aucune inférence elle-même** ; le Tier 1 est un placeholder explicite (« … utilisera un modèle GGUF local en E7 ») ; sans clés → `AllTiersExhausted`. Domicile cible : le **Harness**.
- **Écarté :** un provider unique codé en dur (coût, verrouillage, échec de souveraineté).
- **N'est pas :** un simple proxy — c'est un routeur résilient, conscient des politiques et du réseau.

### 4.9 Search · Feed · Voix  · HARNESS (dans l'Engine aujourd'hui) · `[V1 search-exécutable / ROADMAP feed,voix]`
- **Search** `[V1 partiel]` : `/search` + `/index/upsert` sur un sidecar BGE-M3 + Qdrant ; profils versionnés `canonical-1024`, `edge-512/256/128` (plus petits pour appareils contraints). La pièce IA la plus réelle.
- **Feed** `[ROADMAP]` : pipeline étagé `sources → filters → scorers → hydrators → selectors` (ingestion séparée du classement) — échafaudage.
- **Voix** `[ROADMAP]` : serveur de session WebSocket, **mock** compatible PersonaPlex ; parole réelle derrière `speech-core-sys` (cmake/bindgen), désactivé par défaut.
- **Pourquoi :** les trois sont des **capacités Harness** que l'Engine sert ; les profils edge appliquent la contrainte africaine-d'abord à la recherche.

### 4.10 Capacités natives du Harness  · HARNESS · `[ROADMAP]`
`models · tools · memory · policy · evals · observability` — contrats (`Retriever`, `Ranker`,
`PolicyEngine`, `ModelAdapter`, `RequestContext`) plus échafaudage/mocks. Le cerveau IA réutilisable
que l'Engine appellera. Les implémentations réelles sont promues hors de la R&D à mesure qu'elles
durcissent.

---

## 5. Le programme de modèles  · VISION · `[R&D]`

L'intelligence on-device/edge (depuis le manifeste R&D) — **stade recherche, pas en production**.
C'est le côté « propose » que l'Engine « dispose ».

| Voie | Modèle | Rôle | État |
|---|---|---|---|
| Voix edge | Liquid **LFM2.5-Audio** | audio → tool-call / JSON strict, sur téléphone | smoke R&D |
| ASR streaming | **Nemotron 3.5 ASR** | transcription wolof/français/code-mix | adaptation R&D |
| Duplex | **NeMo SpeechLM2 / SALM** | S2S full-duplex | R&D précoce |
| Traduction | **Wolof-NMT / NLLB** | texte cible FR↔WO (couture texte auditable) | génération + revue |
| TTS | **xTTS-v2-wolof / FR** | parole cible | synthétique, sous revue |
| Recherche | **BGE-M3 Matryoshka** | search/mémoire | partiellement réel (§4.9) |
| Multimodal cloud | **Nemotron Nano / Omni** | raisonnement lourd, évaluateur | prompt/eval d'abord |

**Des choix de conception qui ne sont pas aléatoires :** la **couture texte** (STT→texte→MT→texte→TTS)
est préférée au S2ST direct opaque pour l'*auditabilité* — chaque étape est journalisable,
corrigeable, filtrable. Des **ancres réelles + ponts synthétiques** avec une **porte de revue humaine**
comblent le manque de données appariées en langue peu dotée. Le **fossé de données wolof du commerce**,
qui se compose dans le temps, est le différenciateur de long terme.

---

## 6. On-device — phase E7  · ROADMAP

Deux voies, deux sens d'« on-device » :
- **Voie A — inférence souveraine côté engine/serveur :** combler le placeholder Tier 1 pour que les
  prompts `Sovereign` soient répondus sans quitter Diamniadio. Options runtime : `candle` /
  `llama-cpp-2` / `mistral.rs` / `ort` ; petit GGUF quantifié ; sous feature-flag ; budget RAM/CPU sur
  Railway.
- **Voie B — vraie inférence côté téléphone :** un modèle tournant *sur l'appareil de l'utilisateur*
  dans BOBO natif (ExecuTorch / MLC / llama.rn). **Travail app, pas Engine.** C'est là qu'atterrit la
  voix edge LFM2.5-Audio.

---

## 7. Où cela se situe — catégorisation industrielle  · analyse

Un **Backend-as-a-Service souverain, natif-IA et verticalement intégré**, livré comme runtime Rust
modulaire (Engine) au-dessus d'une couche de capacités (Harness). Par surface, il couvre : **BaaS**
(Supabase/Firebase), **passerelle IA / routeur LLM** (OpenRouter/LiteLLM/Portkey),
**gouvernance/résidence des données / policy-as-code** (OPA — mais à la compilation), **commerce
headless** (Medusa/Saleor), **recherche vectorielle**, **orchestration de paiements**, **plan de
contrôle CPaaS**. En une ligne : *un « Firebase souverain pour l'IA africaine » — avec la résidence des
données compilée, et un plan de contrôle d'actions-IA qu'aucun BaaS prêt-à-l'emploi n'offre.*

---

## 8. Au-delà de BOBO — la thèse  · VISION

BOBO est le premier **locataire**. L'Engine est une infrastructure souveraine réutilisable pour YOKK /
BOBO / NJOOBA / DAARA — et potentiellement d'autres startups, institutions ou gouvernements africains
ayant besoin d'un backend IA **à données résidentes** que les hyperscalers n'offrent pas. Le fossé,
c'est quatre choses qu'ils ne feront pas : **(1)** souveraineté par construction (résidence à la
compilation), **(2)** économie IA africaine-d'abord (cascade edge-d'abord, qui se dégrade proprement),
**(3)** recherche consciente de l'edge, **(4)** rails de paiement régionaux — plus **(5)** le jeu de
données wolof du commerce, qui se compose dans le temps et que le capital ne peut raccourcir.

---

## 9. Inventaire V1 / Roadmap / Vision

> Classement uniquement — la feuille de route détaillée est disséquée dans une passe de revue/inventaire séparée.

**`[V1]` — réel & livrable aujourd'hui (la colonne)**
- Runtime Engine en ligne sur Railway : auth, profil, commerce/produits/commandes, persistance (DB privée), vérifié de bout en bout (frontend → engine → Postgres).
- Système de types de souveraineté + stockage gardé par étiquette (garanti à la compilation).
- *Orchestration* IA en cascade (a besoin de clés provider pour faire quoi que ce soit), recherche *exécutable* (a besoin du sidecar + Qdrant), paiements Wave.

**`[ROADMAP]` — conçu / partiel / à suivre**
- SDK `@yaatal/client` → spin en une commande → `create-yaatal-app` (l'échelle « spinnable/wireable »).
- Alignement Engine↔Harness (sens de dépendance, dé-duplication des crates, promotion des capacités).
- Tier-1 on-device (E7 Voie A), pipeline feed, voix (réelle), convergence des paiements, config LiveKit, verrouillage CORS, email/SMTP, build BOBO natif.

**`[VISION]` — conditionné par la recherche (le plafond)**
- La pile vocale-commerce souveraine wolof/français (voix edge → ASR → traduction → TTS → duplex), le fossé de données qui se compose, le SaaS multi-locataire, le multi-région/PRA, le « backend IA souverain pour les bâtisseurs africains ».

---

## 10. Les deux horloges (comment lire tout ce qui précède)

- **Horloge plateforme** — rapide, livrable, qui se compose **maintenant** : Engine → SDK → spin →
  scaffold. Utilise l'IA cloud prête-à-l'emploi pour la valeur produit à court terme. *C'est le chemin
  le plus rapide.*
- **Horloge recherche** — lente, incertaine, à haut plafond : le programme de modèles wolof souverains.
  Dé-risquée dans le R&D Harness, promue seulement une fois durcie.

L'architecture (R&D Harness + règle de promotion) est **littéralement conçue pour faire tourner les
deux horloges sans les fusionner.** Livrer la colonne sur l'horloge plateforme ; laisser le plafond
arriver sur l'horloge recherche. **Aujourd'hui : un projet de R&D doté d'une colonne de qualité
production et d'une thèse de recherche défendable** — une position rare et forte, tant que le cadrage
reste honnête.

---

*Maintenu aux côtés du code. Si un choix ici cesse de correspondre aux dépôts, le code l'emporte —
mettez à jour ce fichier (et `ENGINE-MANIFEST.md`) dans le même changement.*

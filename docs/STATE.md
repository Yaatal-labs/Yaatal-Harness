# Yaatal — State of the Project · État du projet

> **EN** — A snapshot of where Yaatal stands and why: stage, pivots, model choices, vision.
> **FR** — Un instantané de l'état de Yaatal et du pourquoi : stade, pivots, choix de modèles, vision.
>
> Snapshot **2026-06-07**. Companions: `ENGINE-MANIFEST.md` / `.fr.md`, `docs/plans/*`,
> `docs/BOBO-TEAM-ONBOARDING` (BOBO repo). The code wins if anything here drifts.

---
---

# 🇬🇧 ENGLISH

## 🟢 Stage — where it is now
An **R&D project with a production-grade spine** — two clocks in parallel:
- **Platform (live):** Engine on Railway (auth/commerce/products verified end-to-end, private DB,
  seeded catalog, prod uuid-bug fixed); BOBO web on Cloudflare; full frontend → engine → Postgres
  round-trip proven.
- **Structure (formalized):** two-repo split — **Engine** (runtime/control plane) + **Harness** (AI
  capabilities); alignment in progress.
- **Recently shipped:** bilingual manifests (MD + HTML), inventory + SDK/BOBO slice plans
  (mobile-drivable), bilingual team onboarding, and the `@njooba` → `@yaatal` rebrand (web-build
  verified).
- **Research (R&D):** the sovereign Wolof model stack — not yet in production.

## 🔀 Pivots — the directional decisions
1. **PowerSync → Engine HTTP** — BOBO commerce moved off offline-sync; PowerSync is now dead weight.
2. **App backend → app-agnostic platform** — the Engine is a *product*, not BOBO's backend.
3. **Monolith → layered** — capabilities split into the Harness; Engine = runtime/control plane.
4. **NJOOBA → Yaatal** — the seed isn't the umbrella; neutral shared scope.
5. **Direct cloud AI → cascade + sovereignty** — provider-agnostic, edge-first, compile-time residency.
6. **Three backends → consolidate on Engine** *(in progress)* — commerce on Engine; chat/delivery
   (PocketBase) + payments (DExchange) flagged for later.
7. **One app → multi-app platform** — YOKK / BOBO / NJOOBA / DAARA, and potentially a sovereign-AI
   product.
8. **"Build it all" → two clocks** — ship the platform now on off-the-shelf cloud AI; let sovereign
   models graduate later.

## 🧠 Model choices — the AI stack + the "why"
| Layer | Model | Why |
|---|---|---|
| Edge voice | **LFM2.5-Audio** | on-device audio → tool-call / JSON; small enough to fine-tune now |
| Streaming ASR | **Nemotron 3.5 ASR** | Wolof/French/code-mix, published recipe, latency controls |
| Duplex | **NeMo SpeechLM2 / SALM** | open recipe (encoder+LLM+codec) vs betting on closed S2ST |
| Translation | **Wolof-NMT / NLLB** | fills the paired-data gap; the auditable text seam |
| TTS | **xTTS-v2-wolof / FR** | target speech, synthetic but review-gated |
| Retrieval | **BGE-M3 Matryoshka** | multilingual + edge dimension profiles for low memory |
| Cloud reasoning | **Nemotron Nano / Omni** | heavy multimodal above edge, evaluator/planner |

**Non-random design:** the **text seam** (STT→text→MT→text→TTS) over opaque S2ST for *auditability*;
**real anchors + synthetic bridges + human review** for low-resource data; the compounding **Wolof
commerce dataset** as the durable moat. *(Status: all R&D except retrieval, partially real.)*

## 🌍 Vision
A **sovereign, edge-to-cloud AI platform for African (Wolof/French-first) commerce and assistance**,
whose core primitive is **"models propose, the Engine disposes"** — a sovereign **AI-action control
plane** where on-device intelligence can suggest, but only a sovereignty-typed backend turns intent
into money/orders/PII, with data compiled to never leave Senegal.

**The moat (what hyperscalers won't build):** sovereignty by construction · African-first AI
economics · edge-aware retrieval · regional rails · a compounding low-resource-language data
advantage.

**The ceiling:** the default sovereign AI backend for African builders — full-duplex Wolof
voice-commerce on a cheap phone, on a weak network, data staying home.

> **The through-line:** every pivot decouples the *bet* from the *breakthrough* — separating what can
> ship now from what research might unlock later. The vision is audacious *because* the architecture
> lets you pursue it without betting the company on the uncertain part.

---
---

# 🇫🇷 FRANÇAIS

## 🟢 Stade — où en est le projet
Un **projet de R&D doté d'une colonne vertébrale de qualité production** — deux horloges en parallèle :
- **Plateforme (en ligne) :** Engine sur Railway (auth/commerce/produits vérifiés de bout en bout, DB
  privée, catalogue amorcé, bug uuid de prod corrigé) ; BOBO web sur Cloudflare ; aller-retour complet
  frontend → engine → Postgres prouvé.
- **Structure (formalisée) :** séparation en deux dépôts — **Engine** (runtime/plan de contrôle) +
  **Harness** (capacités IA) ; alignement en cours.
- **Livré récemment :** manifestes bilingues (MD + HTML), plans d'inventaire + slice SDK/BOBO
  (pilotables sur mobile), onboarding d'équipe bilingue, et le renommage `@njooba` → `@yaatal`
  (vérifié par le build web).
- **Recherche (R&D) :** la pile de modèles wolof souverains — pas encore en production.

## 🔀 Pivots — les décisions directionnelles
1. **PowerSync → HTTP Engine** — le commerce BOBO quitte la synchro offline ; PowerSync est désormais
   du poids mort.
2. **Backend d'app → plateforme agnostique** — l'Engine est un *produit*, pas le backend de BOBO.
3. **Monolithe → en couches** — les capacités passent dans le Harness ; l'Engine = runtime/plan de
   contrôle.
4. **NJOOBA → Yaatal** — la graine n'est pas l'ombrelle ; scope partagé neutre.
5. **IA cloud directe → cascade + souveraineté** — agnostique aux providers, edge-d'abord, résidence à
   la compilation.
6. **Trois backends → consolider sur l'Engine** *(en cours)* — commerce sur l'Engine ; chat/livraison
   (PocketBase) + paiements (DExchange) à traiter plus tard.
7. **Une app → plateforme multi-apps** — YOKK / BOBO / NJOOBA / DAARA, et potentiellement un produit
   IA souverain.
8. **« Tout construire » → deux horloges** — livrer la plateforme maintenant avec l'IA cloud
   prête-à-l'emploi ; laisser les modèles souverains mûrir plus tard.

## 🧠 Choix de modèles — la pile IA + le « pourquoi »
| Couche | Modèle | Pourquoi |
|---|---|---|
| Voix edge | **LFM2.5-Audio** | audio on-device → tool-call / JSON ; assez petit pour fine-tuner maintenant |
| ASR streaming | **Nemotron 3.5 ASR** | wolof/français/code-mix, recette publiée, contrôle de latence |
| Duplex | **NeMo SpeechLM2 / SALM** | recette ouverte (encodeur+LLM+codec) plutôt que parier sur du S2ST fermé |
| Traduction | **Wolof-NMT / NLLB** | comble le manque de données appariées ; la couture texte auditable |
| TTS | **xTTS-v2-wolof / FR** | parole cible, synthétique mais sous revue |
| Recherche | **BGE-M3 Matryoshka** | multilingue + profils de dimension edge pour faible mémoire |
| Raisonnement cloud | **Nemotron Nano / Omni** | multimodal lourd au-dessus de l'edge, évaluateur/planificateur |

**Conception non aléatoire :** la **couture texte** (STT→texte→MT→texte→TTS) plutôt que du S2ST opaque
pour l'*auditabilité* ; **ancres réelles + ponts synthétiques + revue humaine** pour les données peu
dotées ; le **jeu de données wolof du commerce**, qui se compose dans le temps, comme fossé durable.
*(État : tout en R&D sauf la recherche, partiellement réelle.)*

## 🌍 Vision
Une **plateforme d'IA souveraine, edge-vers-cloud, pour le commerce et l'assistance africains
(wolof/français d'abord)**, dont la primitive fondatrice est **« les modèles proposent, l'Engine
dispose »** — un **plan de contrôle d'actions-IA souverain** où l'intelligence on-device peut
suggérer, mais seul un backend typé-souveraineté transforme l'intention en argent/commandes/données
personnelles, avec des données compilées pour ne jamais quitter le Sénégal.

**Le fossé (ce que les hyperscalers ne feront pas) :** souveraineté par construction · économie IA
africaine-d'abord · recherche consciente de l'edge · rails régionaux · un avantage de données en
langue peu dotée qui se compose.

**Le plafond :** le backend IA souverain par défaut des bâtisseurs africains — commerce vocal wolof
full-duplex sur un téléphone bon marché, sur un réseau faible, les données restant au pays.

> **Le fil conducteur :** chaque pivot découple le *pari* de la *percée* — séparant ce qui peut être
> livré maintenant de ce que la recherche pourrait débloquer plus tard. La vision est audacieuse
> *parce que* l'architecture permet de la poursuivre sans miser l'entreprise sur la partie incertaine.

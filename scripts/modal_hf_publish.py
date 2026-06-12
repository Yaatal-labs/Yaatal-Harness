"""Publish Yaatal edge-voice artifacts from Modal volumes to HuggingFace.

Creates/updates three private repos under the token's namespace:
  - yaatal-wolof-moss-tts-nano  (model: SFT checkpoints + eval audio + scoreboards)
  - yaatal-tool-router-granite-350m  (model: qualified LoRA + Q4 GGUF + scoreboard)
  - yaatal-voice-warehouse  (dataset: manifests, eval sentences, run records)

Usage:
  modal run scripts/modal_hf_publish.py --run-label run1-baseline --include-router
"""

import os
import json
from pathlib import Path

import modal

app = modal.App("yaatal-hf-publish")
tts_vol = modal.Volume.from_name("yaatal-duplex-tts")
bakeoff_vol = modal.Volume.from_name("yaatal-bakeoff-out")

image = modal.Image.debian_slim(python_version="3.11").pip_install("huggingface_hub")

TTS_CARD = """---
license: apache-2.0
language: [wo]
base_model: OpenMOSS-Team/MOSS-TTS-Nano-100M
pipeline_tag: text-to-speech
tags: [wolof, senegal, edge, yaatal, moss-tts]
---
# Yaatal Wolof TTS (MOSS-TTS-Nano, 100M)

*English first, francais plus bas.*

## What this is

Checkpoints that teach [MOSS-TTS-Nano](https://hf.co/OpenMOSS-Team/MOSS-TTS-Nano-100M)
(100M parameters, Apache-2.0, by OpenMOSS / MOSI.AI) to speak Wolof. Wolof is not
one of the model's 20 original languages; to our knowledge these are the first
Wolof checkpoints for this model family.

This model is the voice of the YAATAL edge assistant, a three-part stack built
to run offline on an ordinary phone:

| Part | Model | Size on device |
|---|---|---|
| Ears | NeMo ASR encoder, Wolof fine-tune (in progress) | ~300 MB |
| Brain | [Granite 350M intent router](https://hf.co/MOH749/yaatal-tool-router-granite-350m) | 210 MB |
| Mouth | this model | ~200 MB |

YAATAL builds for people who speak Wolof but may not read it. A voice that works
without the cloud is the product, not a feature. The whole stack stays under
1 GB so it fits the phones people in Dakar actually own.

## Training

Every run uses the official OpenMOSS recipe (full fine-tune, no LoRA):

- Data: [galsenai/wolof_tts](https://hf.co/datasets/galsenai/wolof_tts), studio
  recordings of two native Wolof actors, collected by Baamtu Datamation under
  the AI4D program. CC-BY-4.0; attribution kept here.
- Pipeline: audio is tokenized with MOSS-Audio-Tokenizer-Nano (a 22M codec),
  then `finetuning/sft.py` runs at learning rate 1e-5, bf16, max length 1024,
  on a single A10G (about 3.2 GiB of VRAM).
- One run costs about $3 on Modal and is one command. Harness:
  [Yaatal-Harness](https://github.com/Yaatal-labs/Yaatal-Harness), branch
  `ml/edge-voice-lane`, `scripts/modal_moss_tts_wolof.py`.

## Runs and results

Each checkpoint ships with its evaluation: 20 sentences (held-out native speech
plus code-mixed market turns), scored on six pass/fail criteria. Audio and
scoreboards sit under `eval/<run>/`. Listen before you judge a run.

| Run | Recipe | Result |
|---|---|---|
| run1-baseline | 3,000 clips, 2 epochs | Synthesizes 20/20 sentences; audio quality is high (STOI 0.95) and the full assistant loop runs. The words drift after the first phrase (character error rate 0.75 on an ASR round-trip): undertrained, kept as the baseline. |
| run2 | 6,000 clips, 4 epochs | uploading after evaluation |

## Limitations

These are research checkpoints, not a product. The baseline starts each sentence
on-text and then loses it. Numbers and prices, the case a market assistant cares
most about, do not survive yet. Some eval sentences are synthetic code-mixed text
that native speakers have not reviewed.

---

## Francais

### Ce que c'est

Des checkpoints qui apprennent le wolof a MOSS-TTS-Nano (100M de parametres,
Apache-2.0, par OpenMOSS / MOSI.AI). Le wolof ne fait pas partie des 20 langues
d'origine du modele; a notre connaissance, ce sont les premiers checkpoints
wolof de cette famille.

C'est la voix de l'assistant YAATAL, une pile en trois parties concue pour
fonctionner hors ligne sur un telephone ordinaire: les oreilles (encodeur ASR
NeMo, en cours), le cerveau (routeur d'intentions Granite 350M, 210 Mo) et la
bouche (ce modele, ~200 Mo). YAATAL construit pour celles et ceux qui parlent
le wolof sans forcement le lire: une voix qui marche sans le cloud, c'est le
produit, pas une option. La pile entiere reste sous 1 Go.

### Entrainement

Recette officielle OpenMOSS (fine-tune complet): donnees
[galsenai/wolof_tts](https://hf.co/datasets/galsenai/wolof_tts) (deux acteurs
wolof natifs, studio, Baamtu Datamation / programme AI4D, CC-BY-4.0); audio
tokenise par le codec MOSS-Audio-Tokenizer-Nano (22M); `sft.py` a lr 1e-5,
bf16, sur un seul A10G (~3,2 Gio de VRAM). Un run coute environ 3 $ sur Modal,
en une commande (depot Yaatal-Harness, branche `ml/edge-voice-lane`).

### Runs et resultats

Chaque checkpoint arrive avec son evaluation (20 phrases, 6 criteres binaires)
sous `eval/<run>/`. run1-baseline: synthese 20/20, qualite audio elevee
(STOI 0,95), boucle complete fonctionnelle; mais les mots derivent apres la
premiere phrase (CER 0,75): sous-entraine, garde comme reference. run2
(6 000 clips, 4 epoques): en cours.

### Limites

Checkpoints de recherche, pas un produit. Les nombres et les prix, le cas le
plus important pour un assistant de marche, ne passent pas encore. Une partie
des phrases d'evaluation est du texte synthetique pas encore relu par des
locuteurs natifs. Ecoutez `eval/<run>/eval_audio/` avant de juger un run.
"""

ROUTER_CARD = """---
license: apache-2.0
language: [wo, fr]
base_model: ibm-granite/granite-4.0-h-350m
tags: [wolof, french, intent, function-calling, gguf, edge, yaatal]
---
# Yaatal intent router (Granite 4.0-H 350M)

*English first, francais plus bas.*

## What this is

A LoRA fine-tune of IBM's Granite 4.0-H 350M, with a ready-to-run Q4_K_M GGUF
of about 210 MB. It turns one spoken-style utterance in Wolof, French, or the
mid-sentence mix people actually use in Dakar markets into one JSON object:
which tool to call, the domain, the language mix, and the entities (product,
colors, quantity, price constraints, urgency).

Input: "Waaw Fatou, wax yu Holland, weex ak bleu, pour mariaje, je cherche vitfe waxla"

Output: `{"tool": "search_products", "domain": "textiles", "language": "wo-fr",
"entities": {"product": "wax hollandais", "colors": ["weex", "bleu"],
"occasion": "mariage"}, ...}`

It is the brain of the YAATAL edge stack (see
[yaatal-wolof-moss-tts-nano](https://hf.co/MOH749/yaatal-wolof-moss-tts-nano)
for the stack overview). It never writes free text for the user. It proposes a
structured intent; the application validates and acts. That contract is what
lets a 350M model do this job safely on a phone.

## Why 350M and not bigger

We ran this model against its 1.5B sibling on the same data, the same harness,
and the same 150 held-out rows (2026-06-12):

| Metric | 350M (this model) | Granite 1B |
|---|---|---|
| Intent accuracy | 0.993 | 0.960 |
| Slot F1 | 0.846 | 0.879 |
| JSON validity | 1.000 | 1.000 |
| Exact match | 0.073 | 0.353 |
| Q4 GGUF size | 210 MB | 901 MB |

The 350M gives up 3.8% slot F1 (inside our 5% acceptance gate), beats the 1B on
intent accuracy, and is a quarter of the size. The 1B remains our quality
fallback and the teacher for distilling away the exact-match gap.

## Training

6,022 instruction rows: market scenarios written for the BOBO commerce
assistant, expanded with paraphrase variants generated by Oolel (a Wolof LLM)
that passed an automated guardrail at 88% acceptance. LoRA, 60 optimization
steps, one A10G. The harness is one command:
[Yaatal-Harness](https://github.com/Yaatal-labs/Yaatal-Harness), branch
`ml/edge-voice-lane`, `scripts/modal_bakeoff.py`.

## How to run

Load `gguf/tool-router-q4_k_m.gguf` in llama.cpp or any GGUF runtime. We
measured ~3.5 tokens/s generation in a CPU-only server container; phone
benchmarks are pending. The LoRA adapter (for GPU use with transformers + peft)
is under `lora/`.

## Limitations

The training text is synthetic and awaits native-speaker review. Exact-match is
weak (0.073): the model gets the intent and the slots right but rarely
reproduces the full target dict verbatim, so validate field-by-field rather
than comparing whole objects.

---

## Francais

### Ce que c'est

Un fine-tune LoRA du Granite 4.0-H 350M d'IBM, avec un GGUF Q4_K_M pret a
l'emploi d'environ 210 Mo. Il transforme un enonce en wolof, en francais, ou
dans le melange des deux qu'on parle vraiment au marche, en un objet JSON:
l'outil a appeler, le domaine, la langue, et les entites (produit, couleurs,
quantite, contraintes de prix, urgence).

C'est le cerveau de la pile YAATAL. Il n'ecrit jamais de texte libre pour
l'utilisateur: il propose une intention structuree, l'application valide et
agit. C'est ce contrat qui permet a un modele de 350M de faire ce travail en
toute securite sur un telephone.

### Pourquoi 350M

Compare a son grand frere de 1,5B sur les memes 150 lignes de test
(12-06-2026): precision d'intention 0,993 contre 0,960 (le 350M gagne),
slot F1 0,846 contre 0,879 (ecart de 3,8 %, sous notre seuil de 5 %), JSON
valide a 100 % des deux cotes, et 210 Mo contre 901 Mo. Le 1B reste la
solution de repli et le professeur pour la distillation.

### Entrainement et usage

6 022 lignes d'instructions: scenarios de marche ecrits pour l'assistant
commerce BOBO, augmentes par des variantes Oolel (LLM wolof) acceptees a 88 %
par un garde-fou automatique. LoRA, 60 pas, un A10G. Pour l'utiliser: chargez
`gguf/tool-router-q4_k_m.gguf` dans llama.cpp (~3,5 tokens/s mesures sur CPU
serveur; benchmarks telephone a venir). Limites: texte d'entrainement
synthetique en attente de relecture par des locuteurs natifs; validez les
champs un par un plutot que l'objet entier.
"""

WAREHOUSE_CARD = """---
license: cc-by-4.0
language: [wo, fr]
tags: [wolof, senegal, speech, yaatal]
---
# Yaatal voice warehouse

*English first, francais plus bas.*

The run records of the YAATAL edge-voice experiments: what each training run
saw, what it was tested on, and how it scored. The models themselves live in
[yaatal-wolof-moss-tts-nano](https://hf.co/MOH749/yaatal-wolof-moss-tts-nano)
and [yaatal-tool-router-granite-350m](https://hf.co/MOH749/yaatal-tool-router-granite-350m);
this dataset is the paper trail that makes them reproducible.

Layout, one folder per run:

```
duplex-tts/<run>/train_raw.jsonl      # exact training manifest (audio path, text, language)
duplex-tts/<run>/eval_sentences.json  # the 20 eval sentences (10 fixed validation, 10 rotating)
duplex-tts/<run>/scoreboard.json      # six pass/fail criteria, metrics, ASR transcripts
```

Provenance: training audio comes from
[galsenai/wolof_tts](https://hf.co/datasets/galsenai/wolof_tts) (CC-BY-4.0,
Baamtu Datamation, AI4D program). Market-domain eval sentences are synthetic
BOBO commerce scenarios, marked `needs_text_review` until native speakers
approve them.

---

## Francais

Les archives des experiences voix de YAATAL: ce que chaque run d'entrainement
a vu, sur quoi il a ete teste, et ses scores. Les modeles sont dans les depots
cites ci-dessus; ce dataset est la trace qui les rend reproductibles. Un
dossier par run: `train_raw.jsonl` (manifeste d'entrainement exact),
`eval_sentences.json` (les 20 phrases de test), `scoreboard.json` (six
criteres binaires, metriques, transcriptions ASR). Audio d'entrainement issu
de galsenai/wolof_tts (CC-BY-4.0, Baamtu Datamation, programme AI4D); phrases
de marche synthetiques marquees `needs_text_review` jusqu'a relecture par des
locuteurs natifs.
"""


@app.function(image=image, volumes={"/tts": tts_vol, "/bakeoff": bakeoff_vol},
              secrets=[modal.Secret.from_name("huggingface-secret")], timeout=3600)
def publish(run_label: str, include_router: bool = False,
            cards_only: bool = False) -> dict:
    from huggingface_hub import HfApi

    token = (os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_TOKEN")
             or os.environ.get("HUGGING_FACE_HUB_TOKEN"))
    api = HfApi(token=token)
    user = api.whoami()["name"]
    out = {"namespace": user, "published": []}

    # --- TTS model repo -------------------------------------------------------
    tts_repo = f"{user}/yaatal-wolof-moss-tts-nano"
    api.create_repo(tts_repo, private=True, exist_ok=True)
    api.upload_file(path_or_fileobj=TTS_CARD.encode(), path_in_repo="README.md",
                    repo_id=tts_repo)
    if cards_only:
        r_repo = f"{user}/yaatal-tool-router-granite-350m"
        api.create_repo(r_repo, private=True, exist_ok=True)
        api.upload_file(path_or_fileobj=ROUTER_CARD.encode(),
                        path_in_repo="README.md", repo_id=r_repo)
        d_repo = f"{user}/yaatal-voice-warehouse"
        api.create_repo(d_repo, private=True, exist_ok=True, repo_type="dataset")
        api.upload_file(path_or_fileobj=WAREHOUSE_CARD.encode(),
                        path_in_repo="README.md", repo_id=d_repo,
                        repo_type="dataset")
        out["published"] = [f"{tts_repo}/README", f"{r_repo}/README",
                            f"{d_repo}/README"]
        print(json.dumps(out, indent=1))
        return out
    run_dir = Path("/tts/runs") / run_label
    ckpt = run_dir / "sft" / "checkpoint-last"
    if ckpt.exists():
        api.upload_folder(folder_path=str(ckpt), repo_id=tts_repo,
                          path_in_repo=f"checkpoints/{run_label}")
        out["published"].append(f"{tts_repo}/checkpoints/{run_label}")
    for sub in ("eval_audio", "scoreboard.json", "eval_sentences.json"):
        p = run_dir / sub
        if p.is_dir():
            api.upload_folder(folder_path=str(p), repo_id=tts_repo,
                              path_in_repo=f"eval/{run_label}/{sub}")
        elif p.exists():
            api.upload_file(path_or_fileobj=str(p), repo_id=tts_repo,
                            path_in_repo=f"eval/{run_label}/{sub}")
    out["published"].append(f"{tts_repo}/eval/{run_label}")

    # --- Router model repo ----------------------------------------------------
    if include_router:
        r_repo = f"{user}/yaatal-tool-router-granite-350m"
        api.create_repo(r_repo, private=True, exist_ok=True)
        api.upload_file(path_or_fileobj=ROUTER_CARD.encode(),
                        path_in_repo="README.md", repo_id=r_repo)
        base = Path("/bakeoff/bakeoff/granite-4.0-h-350m-v2")
        for sub in ("lora", "gguf", "reports"):
            if (base / sub).exists():
                api.upload_folder(folder_path=str(base / sub), repo_id=r_repo,
                                  path_in_repo=sub)
        out["published"].append(r_repo)

    # --- Dataset warehouse ----------------------------------------------------
    d_repo = f"{user}/yaatal-voice-warehouse"
    api.create_repo(d_repo, private=True, exist_ok=True, repo_type="dataset")
    api.upload_file(path_or_fileobj=WAREHOUSE_CARD.encode(),
                    path_in_repo="README.md", repo_id=d_repo, repo_type="dataset")
    for name in ("train_raw.jsonl", "eval_sentences.json", "scoreboard.json"):
        p = run_dir / name
        if p.exists():
            api.upload_file(path_or_fileobj=str(p), repo_id=d_repo,
                            repo_type="dataset",
                            path_in_repo=f"duplex-tts/{run_label}/{name}")
    out["published"].append(f"{d_repo}/duplex-tts/{run_label}")

    print(json.dumps(out, indent=1))
    return out


@app.function(image=image,
              secrets=[modal.Secret.from_name("huggingface-secret")],
              timeout=1200)
def publish_factory(files: dict) -> dict:
    """Upload data-factory JSONL files (passed as {repo_path: text}) to the warehouse."""
    from huggingface_hub import HfApi

    token = (os.environ.get("HF_TOKEN") or os.environ.get("HUGGINGFACE_TOKEN")
             or os.environ.get("HUGGING_FACE_HUB_TOKEN"))
    api = HfApi(token=token)
    user = api.whoami()["name"]
    d_repo = f"{user}/yaatal-voice-warehouse"
    api.create_repo(d_repo, private=True, exist_ok=True, repo_type="dataset")
    for path, content in files.items():
        api.upload_file(path_or_fileobj=content.encode("utf-8"),
                        path_in_repo=path, repo_id=d_repo, repo_type="dataset")
    return {"repo": d_repo, "uploaded": sorted(files)}


FACTORY_FILES = {  # local data-factory outputs worth archiving (all small JSONL)
    "factory/bobo-tool": ["scenario_seed.jsonl", "synthetic_bootstrap_train.jsonl",
                          "synthetic_bootstrap_val.jsonl", "slot_lexicon.json",
                          "search_products.schema.json"],
    "factory/translation": ["train_pairs.jsonl", "validation_pairs.jsonl",
                            "test_pairs.jsonl"],
    "factory/boplex-tts": ["scenario_turns.jsonl", "tts_input_manifest.jsonl",
                           "tts_generation_plan.jsonl", "tts_output_manifest.jsonl"],
}
FACTORY_LOCAL = {"factory/bobo-tool": "output/yaatal-data-factory/bobo-tool",
                 "factory/translation": "output/yaatal-data-factory/translation",
                 "factory/boplex-tts": "output/yaatal-data-factory/tts"}


@app.local_entrypoint()
def main(run_label: str = "run1-baseline", include_router: bool = False,
         cards_only: bool = False, factory: bool = False):
    if factory:
        files = {}
        for repo_dir, names in FACTORY_FILES.items():
            local = Path(FACTORY_LOCAL[repo_dir])
            for n in names:
                p = local / n
                if p.exists() and p.stat().st_size > 0:
                    files[f"{repo_dir}/{n}"] = p.read_text(encoding="utf-8")
        print(json.dumps(publish_factory.remote(files), indent=1))
        return
    print(json.dumps(publish.remote(run_label, include_router, cards_only),
                     indent=1))

# yaatal-feed

YOKK's feed ranking pipeline. Architecture adapted from [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) (Apache-2.0).

Same composable pipeline pattern that powers X's "For You" feed, adapted for voice-first African social platform.

## What's Inside

```
src/
├── pipeline/
│   ├── traits.rs          # 7 traits: Source, Filter, Hydrator, Scorer, Selector, QueryHydrator, SideEffect
│   └── executor.rs        # Pipeline engine: hydrate → source → filter → score → select → side effects
├── types.rs               # VoicePostCandidate + YokkFeedQuery (voice-first types)
├── weights.rs             # Scoring weights (transparent — X excluded theirs)
├── sources/
│   ├── following_source.rs  # In-network posts (X's Thunder → YOKK's Turso query)
│   └── discovery_source.rs  # Out-of-network discovery (X's Phoenix → YOKK's ColBERT/Bo AI)
├── filters/
│   ├── dedup_filter.rs       # X's DropDuplicatesFilter
│   ├── age_filter.rs         # X's AgeFilter (72h max)
│   ├── self_post_filter.rs   # X's SelfTweetFilter
│   ├── seen_posts_filter.rs  # X's PreviouslySeenPostsFilter + PreviouslyServedPostsFilter
│   └── blocked_authors_filter.rs  # X's AuthorSocialgraphFilter
├── scorers/
│   ├── recency_scorer.rs         # Day 1 baseline (no X equivalent — they use Grok from day 1)
│   ├── weighted_scorer.rs        # X's WeightedScorer with voice-first weights
│   └── author_diversity_scorer.rs # X's AuthorDiversityScorer (exponential decay)
├── selectors/
│   └── mod.rs             # TopKSelector (X's TopKScoreSelector)
├── yokk_pipeline.rs       # Concrete pipeline assembly (X's PhoenixCandidatePipeline)
└── lib.rs
```

## Integration into Yaatal Engine

Add to workspace `Cargo.toml`:
```toml
[workspace]
members = [
    "crates/yaatal-core",
    "crates/yaatal-api",
    "crates/yaatal-feed",   # ← add
    "crates/yaatal-voice",
    "crates/yaatal-search",
    "crates/yokk-mobile",
]
```

Wire to Loco controller in `yaatal-api`:
```rust
// In a Loco controller handler:
use yaatal_feed::{YokkFeedPipeline, YokkFeedQuery};

async fn for_you_feed(state: &AppState, user_id: &str) -> Result<Vec<VoicePostCandidate>> {
    let pipeline = YokkFeedPipeline::build(state.post_repo.clone(), state.discovery_repo.clone());
    let mut query = YokkFeedQuery::new(user_id, "SN", 25);
    // Hydrate from DB:
    query.following_ids = get_following(user_id).await?;
    query.blocked_ids = get_blocked(user_id).await?;

    let result = pipeline.execute(query, &uuid::Uuid::new_v4().to_string()).await;
    Ok(result.candidates)
}
```

Implement `PostRepository` with Turso:
```rust
use yaatal_feed::sources::following_source::PostRepository;

struct TursoPostRepo { db: libsql::Database }

#[async_trait]
impl PostRepository for TursoPostRepo {
    async fn get_posts_by_authors(
        &self, author_ids: &[String], limit: usize, max_age_hours: u64,
    ) -> Result<Vec<VoicePostCandidate>, String> {
        // SELECT * FROM posts WHERE author_id IN (?) AND created_at > ? LIMIT ?
        todo!("Wire to Turso")
    }
}
```

## Scoring Weights

Transparent (unlike X's excluded params). Voice engagement is king:

| Action | Weight | Rationale |
|--------|--------|-----------|
| Listen (full) | 2.0 | Strongest positive — they heard the whole thing |
| Reply | 11.0 | Highest intent (matches X's reply weight) |
| Follow | 8.0 | Followed author from feed |
| Share | 5.0 | Active endorsement |
| Voice boost | 1.5× | Voice posts multiplied vs text-only |
| Mute | -74.0 | Strong negative (matches X) |
| Block | -74.0 | Strong negative |
| Report | -200.0 | Nuclear negative |

Tune via PostHog feature flags. Change `weights.rs`, rebuild, zero pipeline changes.

## Evolution Path

| Phase | Scorer Chain | What Changes |
|-------|-------------|--------------|
| Day 1 | Recency → Weighted → AuthorDiversity | Ship this |
| Day N | Bo AI ML → Weighted → AuthorDiversity → LanguageDiversity | Replace RecencyScorer with ML predictions |
| Scale | Bo AI ML → Weighted → AuthorDiversity → LanguageDiversity → OON Boost | Add out-of-network scorer when discovery matters |

The pipeline framework never changes. Just swap/add scorers.

## Tests

```bash
cargo test -- --nocapture
```

3 tests: full pipeline flow, filter verification, author diversity decay.

# yaatal-feed

Reusable feed ranking pipeline for Yaatal Engine. Architecture adapted from [xai-org/x-algorithm](https://github.com/xai-org/x-algorithm) (Apache-2.0).

Same composable pipeline pattern that powers social timeline ranking systems, with a default social-feed assembly included in the crate.

## What's Inside

```
src/
├── pipeline/
│   ├── traits.rs          # 7 traits: Source, Filter, Hydrator, Scorer, Selector, QueryHydrator, SideEffect
│   └── executor.rs        # Pipeline engine: hydrate → source → filter → score → select → side effects
├── types/
│   ├── feed.rs            # Ranking types (FeedQuery, FeedCandidate, etc.)
│   ├── ingestion.rs       # Ingestion support types (IngestionQuery, RawArticle, etc.)
│   └── mod.rs             # Re-exports both surfaces through yaatal_feed::types
├── weights.rs             # Ranking weights for the default social timeline
├── sources/
│   ├── following_source.rs  # In-network posts
│   └── discovery_source.rs  # Out-of-network discovery
├── filters/
│   ├── dedup_filter.rs       # X's DropDuplicatesFilter
│   ├── age_filter.rs         # X's AgeFilter (72h max)
│   ├── self_post_filter.rs   # X's SelfTweetFilter
│   ├── seen_posts_filter.rs  # X's PreviouslySeenPostsFilter + PreviouslyServedPostsFilter
│   └── blocked_authors_filter.rs  # X's AuthorSocialgraphFilter
├── scorers/
│   ├── recency_scorer.rs         # Baseline recency prior
│   ├── weighted_scorer.rs        # Weighted engagement combination
│   └── author_diversity_scorer.rs # X's AuthorDiversityScorer (exponential decay)
├── selectors/
│   └── mod.rs             # TopKSelector (X's TopKScoreSelector)
├── builder.rs             # Default social timeline assembly
└── lib.rs
```

## Integration into Yaatal Engine

Add to workspace `Cargo.toml`:
```toml
[workspace]
members = [
    "crates/yaatal-core",
    "crates/yaatal-api",
    "crates/yaatal-feed",
    "crates/yaatal-voice",
    "crates/yaatal-search",
    "apps/yokk-mobile",
]
```

Wire to Loco controller in `yaatal-api`:
```rust
use yaatal_feed::weights::WeightConfig;
use yaatal_feed::{FeedBuilder, FeedQuery};

async fn for_you_feed(state: &AppState, user_id: &str) -> Result<Vec<yaatal_feed::FeedCandidate>> {
    let pipeline = FeedBuilder::build(
        state.post_repo.clone(),
        state.discovery_repo.clone(),
        WeightConfig::default(),
    );
    let mut query = FeedQuery::new(user_id, "SN", 25);
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
    ) -> Result<Vec<FeedCandidate>, String> {
        // SELECT * FROM posts WHERE author_id IN (?) AND created_at > ? LIMIT ?
        todo!("Wire to Turso")
    }
}
```

## Scoring Weights

Transparent defaults for a generic social feed:

| Action | Weight | Rationale |
|--------|--------|-----------|
| Listen (full) | 2.0 | Strongest positive — they heard the whole thing |
| Reply | 11.0 | Highest intent (matches X's reply weight) |
| Follow | 8.0 | Followed author from feed |
| Share | 5.0 | Active endorsement |
| Voice boost | 1.5× | Longer voice content gets a mild boost |
| Mute | -74.0 | Strong negative (matches X) |
| Block | -74.0 | Strong negative |
| Report | -200.0 | Nuclear negative |

Tune by supplying a different `WeightConfig`; no pipeline code changes required.

## Evolution Path

| Phase | Scorer Chain | What Changes |
|-------|-------------|--------------|
| Baseline | Recency → Weighted → AuthorDiversity | Current default |
| Personalization | ML prior → Weighted → AuthorDiversity → LanguageDiversity | Replace or augment the recency scorer |
| Discovery-heavy | ML prior → Weighted → AuthorDiversity → OON Boost | Increase out-of-network ranking sophistication |

The pipeline framework never changes. Just swap/add scorers.

## Tests

```bash
cargo test -- --nocapture
```

3 tests: full pipeline flow, filter verification, author diversity decay.

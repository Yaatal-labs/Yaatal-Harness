#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]
// yaatal-feed/tests/pipeline_test.rs
//
// End-to-end test: mock repos → pipeline → ranked feed output.

use async_trait::async_trait;
use chrono::Utc;
use std::sync::Arc;
use yaatal_feed::builder::FeedBuilder;
use yaatal_feed::sources::discovery_source::DiscoveryRepository;
use yaatal_feed::sources::following_source::PostRepository;
use yaatal_feed::types::*;
use yaatal_feed::weights::WeightConfig;

// ─── Mock Repositories ─────────────────────────────────────────────

struct MockPostRepo;

#[async_trait]
impl PostRepository for MockPostRepo {
    async fn get_posts_by_authors(
        &self,
        author_ids: &[String],
        _limit: usize,
        _max_age_hours: u64,
    ) -> Result<Vec<FeedCandidate>, String> {
        // Simulate 3 voice posts from followed accounts
        let mut posts = Vec::new();
        for (i, author) in author_ids.iter().take(3).enumerate() {
            posts.push(FeedCandidate {
                id: format!("post-follow-{}", i),
                author_id: author.clone(),
                created_at: Some(Utc::now() - chrono::Duration::hours(i as i64 * 2)),
                content_type: ContentType::Voice,
                voice_duration_ms: Some(15000 + i as u32 * 5000),
                language: Some("wo".into()),
                text: Some(format!("Voice post {} from followed account", i)),
                ..Default::default()
            });
        }
        Ok(posts)
    }
}

struct MockDiscoveryRepo;

#[async_trait]
impl DiscoveryRepository for MockDiscoveryRepo {
    async fn get_discovery_candidates(
        &self,
        _user_id: &str,
        _language_codes: &[String],
        _exclude_author_ids: &[String],
        _limit: usize,
    ) -> Result<Vec<FeedCandidate>, String> {
        // Simulate 2 discovery voice posts from strangers
        Ok(vec![
            FeedCandidate {
                id: "post-discover-0".into(),
                author_id: "stranger-1".into(),
                created_at: Some(Utc::now() - chrono::Duration::hours(1)),
                content_type: ContentType::Voice,
                voice_duration_ms: Some(30000),
                language: Some("fr".into()),
                text: Some("Trending voice post in French".into()),
                ..Default::default()
            },
            FeedCandidate {
                id: "post-discover-1".into(),
                author_id: "stranger-2".into(),
                created_at: Some(Utc::now() - chrono::Duration::hours(6)),
                content_type: ContentType::Text, // text-only — should score lower
                language: Some("en".into()),
                text: Some("Text-only post, no voice".into()),
                ..Default::default()
            },
        ])
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_full_pipeline() {
    let post_repo: Arc<dyn PostRepository> = Arc::new(MockPostRepo);
    let discovery_repo: Arc<dyn DiscoveryRepository> = Arc::new(MockDiscoveryRepo);

    let pipeline = FeedBuilder::build(post_repo, discovery_repo, WeightConfig::default());

    let mut query = FeedQuery::new("user-123", "SN", 10);
    query.following_ids = vec!["author-a".into(), "author-b".into(), "author-c".into()];
    query.language_codes = vec!["wo".into(), "fr".into()];

    let result = pipeline.execute(query, "test-req-1").await;

    // Should have candidates
    assert!(
        !result.candidates.is_empty(),
        "pipeline should return candidates"
    );

    // Should be 5 total (3 following + 2 discovery)
    assert_eq!(result.stats.sourced, 5, "should source 5 candidates");

    // All should have scores
    for c in &result.candidates {
        assert!(
            c.final_score.is_some() || c.weighted_score.is_some(),
            "candidate {} should be scored",
            c.id
        );
    }

    // First candidate should have highest score (sorted desc)
    let scores: Vec<f64> = result
        .candidates
        .iter()
        .map(|c| c.final_score.or(c.weighted_score).unwrap_or(0.0))
        .collect();
    for window in scores.windows(2) {
        assert!(
            window[0] >= window[1],
            "candidates should be sorted by score desc: {} >= {}",
            window[0],
            window[1]
        );
    }

    // Voice posts should generally score higher than text-only
    let voice_scores: Vec<f64> = result
        .candidates
        .iter()
        .filter(|c| c.content_type == ContentType::Voice)
        .map(|c| c.final_score.or(c.weighted_score).unwrap_or(0.0))
        .collect();
    let text_scores: Vec<f64> = result
        .candidates
        .iter()
        .filter(|c| c.content_type == ContentType::Text)
        .map(|c| c.final_score.or(c.weighted_score).unwrap_or(0.0))
        .collect();

    if let (Some(best_voice), Some(best_text)) = (
        voice_scores.iter().cloned().reduce(f64::max),
        text_scores.iter().cloned().reduce(f64::max),
    ) {
        assert!(
            best_voice > best_text,
            "voice posts should score higher than text: {} > {}",
            best_voice,
            best_text
        );
    }

    println!("--- YOKK Feed Pipeline Test Results ---");
    println!("Sourced: {}", result.stats.sourced);
    println!("After filter: {}", result.stats.after_filter);
    println!("Selected: {}", result.stats.selected);
    println!();
    for (i, c) in result.candidates.iter().enumerate() {
        println!(
            "#{} | {} | {:?} | voice={}ms | lang={} | score={:.4} | source={}",
            i + 1,
            c.id,
            c.content_type,
            c.voice_duration_ms.unwrap_or(0),
            c.language.as_deref().unwrap_or("?"),
            c.final_score.or(c.weighted_score).unwrap_or(0.0),
            c.source_name.as_deref().unwrap_or("?"),
        );
    }
}

#[tokio::test]
async fn test_filters_remove_blocked_and_self() {
    let post_repo: Arc<dyn PostRepository> = Arc::new(MockPostRepo);
    let discovery_repo: Arc<dyn DiscoveryRepository> = Arc::new(MockDiscoveryRepo);

    let pipeline = FeedBuilder::build(post_repo, discovery_repo, WeightConfig::default());

    let mut query = FeedQuery::new("author-a", "SN", 10); // user IS author-a
    query.following_ids = vec!["author-a".into(), "author-b".into(), "author-c".into()];
    query.blocked_ids = vec!["stranger-1".into()]; // blocked a discovery author

    let result = pipeline.execute(query, "test-req-2").await;

    // author-a's post should be filtered (self-post)
    assert!(
        result.candidates.iter().all(|c| c.author_id != "author-a"),
        "self-posts should be filtered"
    );

    // stranger-1's post should be filtered (blocked)
    assert!(
        result
            .candidates
            .iter()
            .all(|c| c.author_id != "stranger-1"),
        "blocked author posts should be filtered"
    );

    // Should still have remaining candidates
    assert!(!result.candidates.is_empty());

    println!(
        "--- Filter Test: {} candidates survived ---",
        result.candidates.len()
    );
}

#[tokio::test]
async fn test_author_diversity() {
    // Create a repo that returns multiple posts from the same author
    struct SameAuthorRepo;

    #[async_trait]
    impl PostRepository for SameAuthorRepo {
        async fn get_posts_by_authors(
            &self,
            _author_ids: &[String],
            _limit: usize,
            _max_age_hours: u64,
        ) -> Result<Vec<FeedCandidate>, String> {
            // 4 posts from same author, all similar age
            Ok((0..4)
                .map(|i| FeedCandidate {
                    id: format!("same-author-{}", i),
                    author_id: "prolific-poster".into(),
                    created_at: Some(Utc::now() - chrono::Duration::minutes(i * 10)),
                    content_type: ContentType::Voice,
                    voice_duration_ms: Some(10000),
                    language: Some("wo".into()),
                    ..Default::default()
                })
                .collect())
        }
    }

    struct EmptyDiscovery;

    #[async_trait]
    impl DiscoveryRepository for EmptyDiscovery {
        async fn get_discovery_candidates(
            &self,
            _: &str,
            _: &[String],
            _: &[String],
            _: usize,
        ) -> Result<Vec<FeedCandidate>, String> {
            Ok(vec![])
        }
    }

    let pipeline = FeedBuilder::build(
        Arc::new(SameAuthorRepo),
        Arc::new(EmptyDiscovery),
        WeightConfig::default(),
    );

    let mut query = FeedQuery::new("user-456", "SN", 10);
    query.following_ids = vec!["prolific-poster".into()];

    let result = pipeline.execute(query, "test-req-3").await;

    let scores: Vec<f64> = result
        .candidates
        .iter()
        .map(|c| c.final_score.or(c.weighted_score).unwrap_or(0.0))
        .collect();

    // Each subsequent post from same author should score lower (diversity decay)
    for window in scores.windows(2) {
        assert!(
            window[0] >= window[1],
            "diversity should decay repeated author scores: {:.4} >= {:.4}",
            window[0],
            window[1]
        );
    }

    // First should be significantly higher than last
    if scores.len() >= 2 {
        let ratio = scores.last().unwrap() / scores.first().unwrap();
        assert!(
            ratio < 0.9,
            "diversity decay should meaningfully reduce repeated author scores, ratio: {:.3}",
            ratio
        );
    }

    println!("--- Diversity Test ---");
    for (i, (c, s)) in result.candidates.iter().zip(scores.iter()).enumerate() {
        println!("#{} | {} | score={:.6}", i + 1, c.id, s);
    }
}

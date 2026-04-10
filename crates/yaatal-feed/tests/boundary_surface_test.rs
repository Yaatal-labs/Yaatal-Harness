use yaatal_feed::types::*;
use yaatal_feed::weights::WeightConfig;

#[test]
fn types_module_reexports_feed_and_ingestion_types() {
    let mut query = FeedQuery::new("user-123", "SN", 25);
    query.following_ids.push("author-1".into());

    let ingestion_query = IngestionQuery::new(8);
    let ingestion_config = IngestionConfig::default();
    let raw_article = RawArticle::default();
    let source_state = SourceState::default();
    let feed_item = FeedItem::default();
    let candidate = FeedCandidate::default();

    assert_eq!(query.user_id, "user-123");
    assert_eq!(ingestion_query.ai_budget_remaining, 8);
    assert_eq!(ingestion_config.keep_per_category, 50);
    assert_eq!(ingestion_config.max_enrichments_per_cycle, 25);
    assert!(raw_article.id.is_empty());
    assert!(source_state.source_name.is_empty());
    assert_eq!(feed_item.rank, 0);
    assert_eq!(candidate.content_type, ContentType::Text);
}

#[test]
fn default_matches_social_defaults() {
    assert_eq!(WeightConfig::default(), WeightConfig::social_defaults());
}

#[test]
fn social_defaults_preserve_current_ranking_values() {
    let config = WeightConfig::social_defaults();

    assert_eq!(config.reply_weight, 11.0);
    assert_eq!(config.follow_weight, 8.0);
    assert_eq!(config.voice_post_boost, 1.5);
    assert_eq!(config.max_post_age_hours, 72);
    assert_eq!(config.default_result_size, 25);
}

#[test]
fn commerce_defaults_only_adjust_listing_or_purchase_bias() {
    let social = WeightConfig::social_defaults();
    let commerce = WeightConfig::commerce_defaults();

    assert_eq!(commerce.listen_weight, social.listen_weight);
    assert_eq!(commerce.reply_weight, social.reply_weight);
    assert_eq!(commerce.add_to_cart_weight, 10.0);
    assert_eq!(commerce.purchase_weight, 25.0);
    assert_eq!(commerce.commerce_listing_boost, 1.2);
    assert_eq!(commerce.voice_post_boost, 1.0);
}

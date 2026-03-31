//! Feed integration tests — verify feed endpoint with seeded data.

use loco_rs::{app::AppContext, testing::prelude::*, TestServer};
use sea_orm::{ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter};
use serial_test::serial;
use uuid::Uuid;

use yaatal_api::{
    app::App,
    models::users,
    views::{auth::LoginResponse, feed::FeedResponse},
};
use yaatal_core::models::{post, profile};

use super::prepare_data::auth_header;

async fn register_and_login(
    request: &TestServer,
    ctx: &AppContext,
    name: &str,
    email: &str,
    password: &str,
) -> (users::Model, String) {
    let payload = serde_json::json!({
        "name": name,
        "email": email,
        "password": password
    });
    let register_response = request.post("/api/auth/register").json(&payload).await;
    assert_eq!(register_response.status_code(), 200);

    let login_response = request
        .post("/api/auth/login")
        .json(&serde_json::json!({
            "email": email,
            "password": password
        }))
        .await;
    assert_eq!(login_response.status_code(), 200);

    let login: LoginResponse = serde_json::from_str(&login_response.text()).unwrap();
    let user = users::Model::find_by_email(&ctx.db, email).await.unwrap();
    (user, login.token)
}

async fn linked_profile_by_user_pid(ctx: &AppContext, user_pid: &str) -> profile::Model {
    profile::Entity::find()
        .filter(profile::Column::UserId.eq(user_pid.to_string()))
        .one(&ctx.db)
        .await
        .unwrap()
        .expect("expected linked profile")
}

async fn seed_post(
    ctx: &AppContext,
    author_id: &str,
    title: &str,
    content: &str,
    post_type: &str,
    upvotes: i32,
) -> post::Model {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let model = post::ActiveModel {
        id: Set(id),
        author_id: Set(author_id.to_string()),
        title: Set(title.to_string()),
        content: Set(content.to_string()),
        r#type: Set(match post_type {
            "question" => post::PostType::Question,
            "tutorial" => post::PostType::Tutorial,
            "showcase" => post::PostType::Showcase,
            _ => post::PostType::Discussion,
        }),
        category: Set(None),
        tags: Set(None),
        upvotes: Set(upvotes),
        comment_count: Set(0),
        is_pinned: Set(0),
        created_at: Set(now.clone()),
        updated_at: Set(now),
    };

    let inserted = post::Entity::insert(model).exec(&ctx.db).await.unwrap();
    post::Entity::find_by_id(inserted.last_insert_id)
        .one(&ctx.db)
        .await
        .unwrap()
        .expect("expected inserted post")
}

#[tokio::test]
#[serial]
async fn feed_requires_auth() {
    request::<App, _, _>(|request, _ctx| async move {
        let response = request.get("/api/feed").await;
        assert_eq!(response.status_code(), 401);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn feed_returns_ranked_posts() {
    request::<App, _, _>(|request, ctx| async move {
        // Setup: register user and create posts from different authors
        let (user, token) =
            register_and_login(&request, &ctx, "feed_tester", "feed@test.com", "12341234").await;
        let profile = linked_profile_by_user_pid(&ctx, &user.pid.to_string()).await;

        // Seed posts with varying engagement (to test ranking)
        let _post1 = seed_post(
            &ctx,
            &profile.id,
            "Low engagement",
            "Content 1",
            "discussion",
            0,
        )
        .await;
        let post2 = seed_post(
            &ctx,
            &profile.id,
            "Medium engagement",
            "Content 2",
            "discussion",
            10,
        )
        .await;
        let post3 = seed_post(
            &ctx,
            &profile.id,
            "High engagement",
            "Content 3",
            "discussion",
            50,
        )
        .await;

        // Fetch feed
        let (auth_key, auth_value) = auth_header(&token);
        let response = request
            .get("/api/feed")
            .add_header(auth_key, auth_value)
            .await;

        assert_eq!(response.status_code(), 200);

        let feed: FeedResponse = serde_json::from_str(&response.text()).unwrap();

        // Should have posts
        assert!(!feed.items.is_empty(), "feed should return posts");

        // Verify response structure
        assert_eq!(feed.page, 1);
        assert_eq!(feed.per_page, 20);
        assert!(!feed.request_id.is_empty());

        // Higher engagement posts should rank higher (discovery source orders by upvotes)
        if feed.items.len() >= 2 {
            let first_score = feed.items[0].score;
            let second_score = feed.items[1].score;
            assert!(
                first_score >= second_score,
                "posts should be ranked by score desc: {} >= {}",
                first_score,
                second_score
            );
        }

        println!("--- Feed Test Results ---");
        println!("Total items: {}", feed.total);
        for (i, item) in feed.items.iter().enumerate() {
            println!(
                "#{} | {} | score={:.4} | source={}",
                i + 1,
                item.id,
                item.score,
                item.source
            );
        }
    })
    .await;
}

#[tokio::test]
#[serial]
async fn feed_pagination_works() {
    request::<App, _, _>(|request, ctx| async move {
        let (user, token) = register_and_login(
            &request,
            &ctx,
            "pagination_tester",
            "page@test.com",
            "12341234",
        )
        .await;
        let profile = linked_profile_by_user_pid(&ctx, &user.pid.to_string()).await;

        // Seed multiple posts
        for i in 0..5 {
            seed_post(
                &ctx,
                &profile.id,
                &format!("Post {}", i),
                &format!("Content {}", i),
                "discussion",
                i * 10,
            )
            .await;
        }

        let (auth_key, auth_value) = auth_header(&token);

        // Page 1 with per_page=2
        let response = request
            .get("/api/feed?page=1&per_page=2")
            .add_header(auth_key.clone(), auth_value.clone())
            .await;
        assert_eq!(response.status_code(), 200);
        let feed_page1: FeedResponse = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(feed_page1.per_page, 2);
        assert!(feed_page1.items.len() <= 2);

        // Page 2 with per_page=2
        let response = request
            .get("/api/feed?page=2&per_page=2")
            .add_header(auth_key, auth_value)
            .await;
        assert_eq!(response.status_code(), 200);
        let feed_page2: FeedResponse = serde_json::from_str(&response.text()).unwrap();
        assert_eq!(feed_page2.per_page, 2);
        assert!(feed_page2.items.len() <= 2);

        // Total should be the same
        assert_eq!(feed_page1.total, feed_page2.total);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn feed_following_only_mode() {
    request::<App, _, _>(|request, ctx| async move {
        let (user, token) = register_and_login(
            &request,
            &ctx,
            "following_tester",
            "follow@test.com",
            "12341234",
        )
        .await;
        let profile = linked_profile_by_user_pid(&ctx, &user.pid.to_string()).await;

        // Seed a post
        seed_post(
            &ctx,
            &profile.id,
            "Following test post",
            "Content",
            "discussion",
            5,
        )
        .await;

        let (auth_key, auth_value) = auth_header(&token);

        // Fetch feed with following_only=true
        let response = request
            .get("/api/feed?following_only=true")
            .add_header(auth_key, auth_value)
            .await;

        assert_eq!(response.status_code(), 200);

        let feed: FeedResponse = serde_json::from_str(&response.text()).unwrap();

        // In following-only mode with no followed users, should return empty or only own posts
        // (depending on implementation details)
        println!("Following-only feed returned {} items", feed.total);
    })
    .await;
}

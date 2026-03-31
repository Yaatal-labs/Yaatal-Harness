use loco_rs::{app::AppContext, testing::prelude::*, TestServer};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serial_test::serial;
use yaatal_api::{
    app::App,
    models::users::{self, RegisterParams},
    views::{auth::LoginResponse, comments::CommentResponse, posts::PostResponse},
};
use yaatal_core::{
    gamification::xp::XpAction,
    models::{comments, post, profile},
};

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

#[tokio::test]
#[serial]
async fn register_creates_linked_profile() {
    request::<App, _, _>(|request, ctx| async move {
        let (user, _) =
            register_and_login(&request, &ctx, "alpha", "alpha@loco.com", "12341234").await;
        let linked = linked_profile_by_user_pid(&ctx, &user.pid.to_string()).await;
        let user_pid = user.pid.to_string();
        assert_eq!(linked.user_id.as_deref(), Some(user_pid.as_str()));
        assert_eq!(linked.xp, 0);
        assert_eq!(linked.level, 1);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn create_post_and_comment_use_profile_id_and_award_xp() {
    request::<App, _, _>(|request, ctx| async move {
        let (user, token) =
            register_and_login(&request, &ctx, "bravo", "bravo@loco.com", "12341234").await;
        let linked_profile = linked_profile_by_user_pid(&ctx, &user.pid.to_string()).await;

        let (auth_key, auth_value) = auth_header(&token);
        let create_post_response = request
            .post("/api/posts")
            .add_header(auth_key.clone(), auth_value.clone())
            .json(&serde_json::json!({
                "title": "Test Post",
                "content": "Body",
                "type": "discussion"
            }))
            .await;
        assert_eq!(create_post_response.status_code(), 200);

        let created_post: PostResponse =
            serde_json::from_str(&create_post_response.text()).unwrap();
        assert_eq!(created_post.author_id, linked_profile.id);

        let stored_post = post::Entity::find_by_id(&created_post.id)
            .one(&ctx.db)
            .await
            .unwrap()
            .expect("expected stored post");
        assert_eq!(stored_post.author_id, linked_profile.id);

        let create_comment_response = request
            .post(&format!("/api/posts/{}/comments", created_post.id))
            .add_header(auth_key, auth_value)
            .json(&serde_json::json!({
                "content": "Great post!"
            }))
            .await;
        assert_eq!(create_comment_response.status_code(), 200);

        let created_comment: CommentResponse =
            serde_json::from_str(&create_comment_response.text()).unwrap();
        assert_eq!(created_comment.author_id, linked_profile.id);

        let stored_comment = comments::Entity::find_by_id(&created_comment.id)
            .one(&ctx.db)
            .await
            .unwrap()
            .expect("expected stored comment");
        assert_eq!(stored_comment.author_id, linked_profile.id);

        let refreshed_profile = linked_profile_by_user_pid(&ctx, &user.pid.to_string()).await;
        let expected_xp = (XpAction::PostArticle.points() + XpAction::Comment.points()) as i32;
        assert_eq!(refreshed_profile.xp, expected_xp);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn cannot_update_post_with_different_profile() {
    request::<App, _, _>(|request, ctx| async move {
        let (_, token_one) =
            register_and_login(&request, &ctx, "charlie", "charlie@loco.com", "12341234").await;
        let (_, token_two) =
            register_and_login(&request, &ctx, "delta", "delta@loco.com", "12341234").await;

        let (auth_key_one, auth_value_one) = auth_header(&token_one);
        let create_post_response = request
            .post("/api/posts")
            .add_header(auth_key_one, auth_value_one)
            .json(&serde_json::json!({
                "title": "Owned Post",
                "content": "Owner only"
            }))
            .await;
        assert_eq!(create_post_response.status_code(), 200);

        let created_post: PostResponse =
            serde_json::from_str(&create_post_response.text()).unwrap();
        let (auth_key_two, auth_value_two) = auth_header(&token_two);
        let update_response = request
            .put(&format!("/api/posts/{}", created_post.id))
            .add_header(auth_key_two, auth_value_two)
            .json(&serde_json::json!({
                "title": "Hacked"
            }))
            .await;
        assert_eq!(update_response.status_code(), 401);
    })
    .await;
}

#[tokio::test]
#[serial]
async fn create_post_without_linked_profile_returns_not_found() {
    request::<App, _, _>(|request, ctx| async move {
        let params = RegisterParams {
            name: "orphan".to_string(),
            email: "orphan@loco.com".to_string(),
            password: "12341234".to_string(),
        };
        let user = users::Model::create_with_password(&ctx.db, &params)
            .await
            .expect("expected user creation");
        let jwt_config = ctx.config.get_jwt_config().unwrap();
        let token = user
            .generate_jwt(&jwt_config.secret, jwt_config.expiration)
            .unwrap();

        let (auth_key, auth_value) = auth_header(&token);
        let create_post_response = request
            .post("/api/posts")
            .add_header(auth_key, auth_value)
            .json(&serde_json::json!({
                "title": "No profile",
                "content": "Should fail"
            }))
            .await;
        assert_eq!(create_post_response.status_code(), 404);
    })
    .await;
}

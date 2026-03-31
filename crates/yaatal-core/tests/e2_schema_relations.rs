#![allow(clippy::unwrap_used, clippy::expect_used)]
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseBackend, EntityTrait, ModelTrait, Set,
    Statement,
};
use std::{collections::HashSet, path::PathBuf};
use yaatal_core::{
    models::{bookmarks, comments, post, profile, upvotes},
    run_migrations_from_file,
};

const TS: &str = "2026-02-21T00:00:00Z";

async fn setup_db() -> sea_orm::DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "PRAGMA foreign_keys = ON",
    ))
    .await
    .expect("enable sqlite foreign keys");

    let migration_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("migrations")
        .join("001_initial.sql");
    run_migrations_from_file(&db, migration_path)
        .await
        .expect("run initial migration");
    db
}

fn profile_active(id: &str) -> profile::ActiveModel {
    profile::ActiveModel {
        id: Set(id.to_owned()),
        user_id: Set(None),
        username: Set(Some(format!("user-{id}"))),
        display_name: Set(Some(format!("User {id}"))),
        bio: Set(Some("bio".to_owned())),
        avatar_url: Set(None),
        xp: Set(0),
        level: Set(1),
        streak_days: Set(0),
        last_active_at: Set(None),
        interests: Set(None),
        onboarding_complete: Set(0),
        created_at: Set(TS.to_owned()),
        updated_at: Set(TS.to_owned()),
    }
}

fn post_active(id: &str, author_id: &str) -> post::ActiveModel {
    post::ActiveModel {
        id: Set(id.to_owned()),
        author_id: Set(author_id.to_owned()),
        title: Set(format!("Post {id}")),
        content: Set("content".to_owned()),
        r#type: Set(post::PostType::Question),
        category: Set(Some("general".to_owned())),
        tags: Set(Some("rust".to_owned())),
        upvotes: Set(0),
        comment_count: Set(0),
        is_pinned: Set(0),
        created_at: Set(TS.to_owned()),
        updated_at: Set(TS.to_owned()),
    }
}

fn comment_active(
    id: &str,
    post_id: &str,
    author_id: &str,
    parent_id: Option<&str>,
) -> comments::ActiveModel {
    comments::ActiveModel {
        id: Set(id.to_owned()),
        post_id: Set(post_id.to_owned()),
        author_id: Set(author_id.to_owned()),
        parent_id: Set(parent_id.map(str::to_owned)),
        content: Set(format!("Comment {id}")),
        is_accepted: Set(0),
        upvotes: Set(0),
        voice_url: Set(None),
        created_at: Set(TS.to_owned()),
    }
}

#[tokio::test]
async fn migration_creates_expected_tables() {
    let db = setup_db().await;
    let rows = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT name FROM sqlite_master WHERE type='table'",
        ))
        .await
        .expect("query sqlite_master");

    let table_names: HashSet<String> = rows
        .iter()
        .map(|row| row.try_get("", "name").expect("read table name"))
        .collect();

    let expected = [
        "profiles",
        "posts",
        "comments",
        "upvotes",
        "launches",
        "achievements",
        "bo_conversations",
        "feed_items",
        "bookmarks",
        "user_security_keys",
    ];
    for table in expected {
        assert!(
            table_names.contains(table),
            "missing expected table `{table}`"
        );
    }
}

#[tokio::test]
async fn foreign_keys_enforce_parent_child_integrity() {
    let db = setup_db().await;

    let insert_without_parent = post_active("post-no-parent", "missing").insert(&db).await;
    assert!(
        insert_without_parent.is_err(),
        "expected FK violation when author profile does not exist"
    );

    profile_active("user-1")
        .insert(&db)
        .await
        .expect("insert profile");
    post_active("post-1", "user-1")
        .insert(&db)
        .await
        .expect("insert post with valid FK");
}

#[tokio::test]
async fn uniqueness_constraints_block_duplicates() {
    let db = setup_db().await;

    profile_active("user-2")
        .insert(&db)
        .await
        .expect("insert profile");

    upvotes::ActiveModel {
        id: Set("upvote-1".to_owned()),
        user_id: Set("user-2".to_owned()),
        target_type: Set("post".to_owned()),
        target_id: Set("post-42".to_owned()),
        created_at: Set(TS.to_owned()),
    }
    .insert(&db)
    .await
    .expect("insert first upvote");

    let duplicate_upvote = upvotes::ActiveModel {
        id: Set("upvote-2".to_owned()),
        user_id: Set("user-2".to_owned()),
        target_type: Set("post".to_owned()),
        target_id: Set("post-42".to_owned()),
        created_at: Set(TS.to_owned()),
    }
    .insert(&db)
    .await;
    assert!(
        duplicate_upvote.is_err(),
        "expected unique constraint violation for duplicate upvote target"
    );

    bookmarks::ActiveModel {
        id: Set("bookmark-1".to_owned()),
        user_id: Set("user-2".to_owned()),
        target_type: Set("post".to_owned()),
        target_id: Set("post-42".to_owned()),
        created_at: Set(TS.to_owned()),
    }
    .insert(&db)
    .await
    .expect("insert first bookmark");

    let duplicate_bookmark = bookmarks::ActiveModel {
        id: Set("bookmark-2".to_owned()),
        user_id: Set("user-2".to_owned()),
        target_type: Set("post".to_owned()),
        target_id: Set("post-42".to_owned()),
        created_at: Set(TS.to_owned()),
    }
    .insert(&db)
    .await;
    assert!(
        duplicate_bookmark.is_err(),
        "expected unique constraint violation for duplicate bookmark target"
    );
}

#[tokio::test]
async fn relations_support_profile_post_comment_queries() {
    let db = setup_db().await;

    profile_active("user-3")
        .insert(&db)
        .await
        .expect("insert profile");
    post_active("post-a", "user-3")
        .insert(&db)
        .await
        .expect("insert first post");
    post_active("post-b", "user-3")
        .insert(&db)
        .await
        .expect("insert second post");

    comment_active("comment-parent", "post-a", "user-3", None)
        .insert(&db)
        .await
        .expect("insert parent comment");
    comment_active("comment-child", "post-a", "user-3", Some("comment-parent"))
        .insert(&db)
        .await
        .expect("insert child comment");

    let profile_model = profile::Entity::find_by_id("user-3")
        .one(&db)
        .await
        .expect("query profile")
        .expect("profile exists");
    let related_posts = profile_model
        .find_related(post::Entity)
        .all(&db)
        .await
        .expect("load profile posts");
    assert_eq!(related_posts.len(), 2, "profile should own two posts");

    let post_model = post::Entity::find_by_id("post-a")
        .one(&db)
        .await
        .expect("query post")
        .expect("post exists");
    let related_comments = post_model
        .find_related(comments::Entity)
        .all(&db)
        .await
        .expect("load post comments");
    assert_eq!(related_comments.len(), 2, "post should have two comments");

    let parent_join_rows = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT parent.id AS parent_id
             FROM comments child
             LEFT JOIN comments parent ON child.parent_id = parent.id
             WHERE child.id = 'comment-child'",
        ))
        .await
        .expect("run self-join query for parent comment");
    let parent_id: Option<String> = parent_join_rows
        .first()
        .expect("expected one child comment row")
        .try_get("", "parent_id")
        .expect("read parent_id from join result");
    assert_eq!(parent_id.as_deref(), Some("comment-parent"));
}

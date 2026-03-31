use sea_orm::{ConnectionTrait, Database, DatabaseConnection};
use serde::Deserialize;
use std::{fs, path::Path};
use thiserror::Error;

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub uri: String,
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    pub database: DatabaseConfig,
}

#[derive(Debug, Error)]
pub enum DbError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("database error: {0}")]
    Database(#[from] sea_orm::DbErr),
}

pub fn load_config_from_file(path: impl AsRef<Path>) -> Result<AppConfig, DbError> {
    let contents = fs::read_to_string(path)?;
    let config = serde_yaml::from_str::<AppConfig>(&contents)?;
    Ok(config)
}

pub async fn connect(database_uri: &str) -> Result<DatabaseConnection, DbError> {
    let conn = Database::connect(database_uri).await?;
    Ok(conn)
}

pub async fn connect_from_config_file(
    path: impl AsRef<Path>,
) -> Result<DatabaseConnection, DbError> {
    let config = load_config_from_file(path)?;
    connect(&config.database.uri).await
}

pub async fn run_migrations_from_file(
    conn: &DatabaseConnection,
    path: impl AsRef<Path>,
) -> Result<(), DbError> {
    let sql = fs::read_to_string(path)?;
    conn.execute_unprepared(&sql).await?;
    Ok(())
}

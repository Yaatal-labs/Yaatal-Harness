use std::{env, path::Path};

use loco_rs::cli;
use migration::Migrator;
use yaatal_api::app::App;

fn bootstrap_runtime_env() {
    let workspace_config = Path::new("crates/yaatal-api/config");
    if env::var_os("LOCO_CONFIG_FOLDER").is_none() && workspace_config.is_dir() {
        env::set_var("LOCO_CONFIG_FOLDER", workspace_config);
    }

    if env::var_os("LOCO_ENV").is_none()
        && matches!(env::var("RAILWAY_ENVIRONMENT").as_deref(), Ok("production"))
    {
        env::set_var("LOCO_ENV", "production");
    }
}

#[tokio::main]
async fn main() -> loco_rs::Result<()> {
    bootstrap_runtime_env();
    cli::main::<App, Migrator>().await
}

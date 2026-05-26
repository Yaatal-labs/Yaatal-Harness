use async_trait::async_trait;
use axum::{routing::Router as AxumRouter, Extension};
use loco_rs::{
    app::{AppContext, Hooks, Initializer},
    bgworker::{BackgroundWorker, Queue},
    boot::{create_app, BootResult, StartMode},
    config::Config,
    controller::AppRoutes,
    db::{self, truncate_table},
    environment::Environment,
    task::Tasks,
    Result,
};
use migration::Migrator;
use std::{path::Path, sync::Arc};

use crate::services::payments_service::PaymentsService;
#[allow(unused_imports)]
use crate::{controllers, models::_entities::users, tasks, workers::downloader::DownloadWorker};
use yaatal_analytics::AnalyticsDispatcher;

pub struct App;
#[async_trait]
impl Hooks for App {
    fn app_name() -> &'static str {
        env!("CARGO_CRATE_NAME")
    }

    fn app_version() -> String {
        format!(
            "{} ({})",
            env!("CARGO_PKG_VERSION"),
            option_env!("BUILD_SHA")
                .or(option_env!("GITHUB_SHA"))
                .unwrap_or("dev")
        )
    }

    async fn boot(
        mode: StartMode,
        environment: &Environment,
        config: Config,
    ) -> Result<BootResult> {
        create_app::<Self, Migrator>(mode, environment, config).await
    }

    async fn initializers(_ctx: &AppContext) -> Result<Vec<Box<dyn Initializer>>> {
        Ok(vec![])
    }

    fn routes(_ctx: &AppContext) -> AppRoutes {
        AppRoutes::with_default_routes() // controller routes below
            .add_route(controllers::health::routes())
            .add_route(controllers::auth::routes())
            .add_route(controllers::posts::routes())
            .add_route(controllers::comments::routes())
            .add_route(controllers::feed::routes())
            .add_route(controllers::voice::routes())
            .add_route(controllers::offline::routes())
            .add_route(controllers::ai::routes())
            .add_route(controllers::webhooks::routes())
            .add_route(controllers::payments::routes())
            .add_route(controllers::bobo_orders::routes())
            .add_route(controllers::bobo_kyc::routes())
            .add_route(controllers::livekit::routes())
    }

    async fn after_routes(router: AxumRouter, _ctx: &AppContext) -> Result<AxumRouter> {
        // Analytics dispatcher — always attached. `from_env` never panics; it
        // falls back to `LogSink` when POSTHOG_API_KEY is absent so the boot
        // is never gated on analytics config.
        let analytics = Arc::new(AnalyticsDispatcher::from_env());
        let router = router.layer(Extension(analytics));

        // LiveKit config — always attached as Option<Arc<…>>. Endpoints return
        // 503 when None (set LIVEKIT_API_KEY / _SECRET / _URL to enable).
        let livekit_cfg = controllers::livekit::LiveKitConfig::from_env().map(Arc::new);
        if livekit_cfg.is_some() {
            tracing::info!("livekit config loaded");
        } else {
            tracing::warn!(
                "livekit not configured — /api/livekit/* will return 503 until LIVEKIT_API_KEY/_SECRET/_URL are set"
            );
        }
        let router = router.layer(Extension(livekit_cfg));

        // Payments service — only attached when WAVE_* env vars are set. When
        // absent, payment endpoints return 500 (acceptable for dev/CI; the
        // operator sets WAVE_* to enable).
        let svc = match PaymentsService::from_env() {
            Ok(s) => {
                tracing::info!("payments service initialized");
                s
            }
            Err(e) => {
                tracing::warn!(error = %e, "payments service not configured — payment endpoints will return 503");
                tracing::warn!("skipping payment Extension layer; set WAVE_* env vars to enable");
                return Ok(router);
            }
        };
        Ok(router.layer(Extension(Arc::new(svc))))
    }
    async fn connect_workers(ctx: &AppContext, queue: &Queue) -> Result<()> {
        queue.register(DownloadWorker::build(ctx)).await?;
        Ok(())
    }

    #[allow(unused_variables)]
    fn register_tasks(tasks: &mut Tasks) {
        // tasks-inject (do not remove)
    }
    async fn truncate(ctx: &AppContext) -> Result<()> {
        truncate_table(&ctx.db, users::Entity).await?;
        Ok(())
    }
    async fn seed(ctx: &AppContext, base: &Path) -> Result<()> {
        db::seed::<users::ActiveModel>(&ctx.db, &base.join("users.yaml").display().to_string())
            .await?;
        Ok(())
    }
}

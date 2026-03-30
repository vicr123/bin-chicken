pub mod api;

use crate::configuration::Configuration;
use axum::routing::get;
use axum::Router;
use std::sync::Arc;

#[derive(Clone)]
pub struct RouterState {
    pub config: Arc<Configuration>,
}

pub fn setup_routes() -> Router<RouterState> {
    Router::new()
        .route(
            "/",
            get(|| async { "Congratulations! bin-chicken is working!" }),
        )
        .nest("/api", api::routes())
}

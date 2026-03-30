use crate::route::RouterState;
use axum::Router;
use serde::Deserialize;

pub mod repositories;

#[derive(Deserialize, Clone)]
pub struct Pagination {
    offset: Option<usize>,
    limit: Option<usize>,
}

impl Pagination {
    pub fn offset(&self) -> usize {
        self.offset.unwrap_or(0)
    }

    pub fn limit(&self) -> usize {
        self.limit.unwrap_or(100).min(100)
    }
}

pub fn routes() -> Router<RouterState> {
    Router::new().nest("/repositories", repositories::routes())
}

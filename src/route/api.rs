use crate::route::RouterState;
use axum::Router;

pub mod repositories;

pub fn routes() -> Router<RouterState> {
    Router::new().nest("/repositories", repositories::routes())
}

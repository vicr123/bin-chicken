use crate::route::RouterState;
use axum::body::Body;
use axum::extract::{FromRequestParts, Path, State};
use axum::extract::rejection::PathRejection;
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use crate::configuration::RepositoryConfiguration;

pub fn routes() -> Router<RouterState> {
    Router::new().route("/{project}", get(get_project))
}

struct ProjectContext {
    repository_configuration: RepositoryConfiguration,
}

impl FromRequestParts<RouterState> for ProjectContext {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &RouterState,
    ) -> Result<Self, Self::Rejection> {
        let Ok(Path(project)) = Path::<String>::from_request_parts(parts, state).await else {
            return Err(StatusCode::NOT_FOUND);
        };
        
        let Some(repository_configuration) = state.config.repositories.get(&project) else {
            return Err(StatusCode::NOT_FOUND);
        };
        
        Ok(ProjectContext {
            repository_configuration: repository_configuration.clone(),
        })
    }
}

async fn get_project(project_context: ProjectContext, state: State<RouterState>) -> Response<Body> {
    project_context.repository_configuration.name.clone().into_response()
}

async fn put_project(project_context: ProjectContext, state: State<RouterState>) -> Response<Body> {
    StatusCode::OK.into_response()
}

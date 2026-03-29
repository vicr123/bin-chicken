use crate::configuration::RepositoryConfiguration;
use crate::database::{create_version, ensure_up_to_date};
use crate::route::RouterState;
use axum::body::{Body, Bytes, HttpBody};
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Error, Json, Router};
use futures_util::StreamExt;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio_rusqlite::Connection;
use tracing::error;

pub fn routes() -> Router<RouterState> {
    Router::new().route("/{repository}", get(get_repository).put(put_repository))
}

struct RepositoryContext {
    repository_id: String,
    repository_configuration: RepositoryConfiguration,
    repository_root: PathBuf,
    database_connection: Connection,
}

impl FromRequestParts<RouterState> for RepositoryContext {
    type Rejection = StatusCode;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &RouterState,
    ) -> Result<Self, Self::Rejection> {
        let Ok(Path(repository)) = Path::<String>::from_request_parts(parts, state).await else {
            return Err(StatusCode::NOT_FOUND);
        };

        let config = state.config.clone();
        let Some(repository_configuration) = config.repositories.get(&repository) else {
            return Err(StatusCode::NOT_FOUND);
        };

        let repository_root = config.file_root.join(repository_configuration.name.clone());
        if !repository_root.exists() {
            fs::create_dir_all(&repository_root).await.map_err(|e| {
                error!("Failed to create directory for repository {repository}: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
        }

        let database_connection =
            match Connection::open(repository_root.join("database.sqlite")).await {
                Ok(database_connection) => {
                    ensure_up_to_date(&database_connection).await.map_err(|e| {
                        error!("Failed to configure database for {repository}: {e}");
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                    database_connection
                }
                Err(e) => {
                    error!("Failed to open database for repository {repository}: {e}");
                    return Err(StatusCode::INTERNAL_SERVER_ERROR);
                }
            };

        Ok(RepositoryContext {
            repository_id: repository,
            repository_configuration: repository_configuration.clone(),
            repository_root,
            database_connection,
        })
    }
}

async fn get_repository(
    repository_context: RepositoryContext,
    state: State<RouterState>,
) -> Response<Body> {
    "OK".into_response()
}

#[derive(Serialize)]
struct PutRepositoryResponse {
    new_version: String,
}

#[derive(Serialize)]
struct PutRepositoryErrorResponse {
    error: PutRepositoryErrorResponseType,
}

#[derive(Serialize)]
enum PutRepositoryErrorResponseType {
    MissingChannelHeader,
    MissingTargetHeader,
}

async fn put_repository(
    repository_context: RepositoryContext,
    state: State<RouterState>,
    request: Request<Body>,
) -> Response<Body> {
    // TODO: Check auth
    
    let Some(channel) = request.headers().get("x-bin-chicken-channel").and_then(|channel| channel.to_str().ok())  else {
        return (StatusCode::BAD_REQUEST, Json(PutRepositoryErrorResponse { error: PutRepositoryErrorResponseType::MissingChannelHeader })).into_response();
    };
    
    let Some(target) = request.headers().get("x-bin-chicken-target").and_then(|target| target.to_str().ok()) else {
        return (StatusCode::BAD_REQUEST, Json(PutRepositoryErrorResponse { error: PutRepositoryErrorResponseType::MissingTargetHeader })).into_response();   
    };

    let repository_id = repository_context.repository_id;
    let database_connection = repository_context.database_connection;
    let version_handle =
        match create_version(&database_connection, target.to_string(), channel.to_string()).await {
            Ok(version) => version,
            Err(e) => {
                error!("Failed to create new version for repository {repository_id}: {e}");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let version = version_handle.version().to_string();

    let version_path = repository_context.repository_root.join(&version);
    if let Err(e) = fs::create_dir(&version_path).await {
        error!("Failed to create version directory {version} in repository {repository_id}: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    let mut artifact_file = match File::create(version_path.join("artifact.bin")).await {
        Ok(artifact_file) => artifact_file,
        Err(e) => {
            error!(
                "Failed to create artifact file in version {version} in repository {repository_id}: {e}"
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Put the files down
    let mut body_stream = request.into_body().into_data_stream();
    while let Some(frame) = body_stream.next().await {
        match frame {
            Ok(frame) => {
                if let Err(e) = artifact_file.write_all(&frame).await {
                    error!(
                        "Failed to write to artifact file in version {version} in repository {repository_id}: {e}"
                    );
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
            }
            Err(e) => {
                // ???
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        }
    }

    // TODO: Sign the artifact

    if let Err(e) = version_handle.mark_complete().await {
        error!("Failed to mark version {version} in repository {repository_id} as complete: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    (
        StatusCode::CREATED,
        Json(PutRepositoryResponse {
            new_version: version,
        }),
    )
        .into_response()
}

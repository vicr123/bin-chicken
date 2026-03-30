use crate::configuration::RepositoryConfiguration;
use crate::database;
use crate::database::{create_version, ensure_up_to_date};
use crate::route::api::Pagination;
use crate::route::RouterState;
use axum::body::Body;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::request::Parts;
use axum::http::{header, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use axum_extra::headers::Range;
use axum_extra::TypedHeader;
use axum_range::{KnownSize, Ranged};
use base64::prelude::BASE64_STANDARD;
use futures_util::StreamExt;
use minisign_verify::{Error, PublicKey, Signature};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use base64::Engine;
use tokio::fs;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio_rusqlite::Connection;
use tokio_util::io::ReaderStream;
use tracing::error;

pub fn routes() -> Router<RouterState> {
    Router::new()
        .route("/{repository}", get(get_artifact_list).put(put_repository))
        .route("/{repository}/artifacts/{artifact}", get(get_artifact))
}

#[derive(Deserialize)]
struct RepositoryContextParams {
    repository: String,
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
        let Ok(Path(RepositoryContextParams { repository })) =
            Path::<RepositoryContextParams>::from_request_parts(parts, state).await
        else {
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
    MissingUuidHeader,
    MissingSignatureHeader,
    InvalidSignature,
}

async fn put_repository(
    repository_context: RepositoryContext,
    state: State<RouterState>,
    request: Request<Body>,
) -> Response<Body> {
    // TODO: Check auth

    let Some(uuid) = request
        .headers()
        .get("x-bin-chicken-uuid")
        .and_then(|uuid| uuid.to_str().ok())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(PutRepositoryErrorResponse {
                error: PutRepositoryErrorResponseType::MissingUuidHeader,
            }),
        )
            .into_response();
    };

    let Some(channel) = request
        .headers()
        .get("x-bin-chicken-channel")
        .and_then(|channel| channel.to_str().ok())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(PutRepositoryErrorResponse {
                error: PutRepositoryErrorResponseType::MissingChannelHeader,
            }),
        )
            .into_response();
    };

    let Some(target) = request
        .headers()
        .get("x-bin-chicken-target")
        .and_then(|target| target.to_str().ok())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(PutRepositoryErrorResponse {
                error: PutRepositoryErrorResponseType::MissingTargetHeader,
            }),
        )
            .into_response();
    };

    let Some(signature_text) = request
        .headers()
        .get("x-bin-chicken-signature")
        .and_then(|sig| sig.to_str().ok())
    else {
        return (
            StatusCode::BAD_REQUEST,
            Json(PutRepositoryErrorResponse {
                error: PutRepositoryErrorResponseType::MissingSignatureHeader,
            }),
        )
            .into_response();
    };
    let Ok(Ok(signature_text)) = BASE64_STANDARD.decode(signature_text.as_bytes()).map(|bytes| String::from_utf8(bytes)) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(PutRepositoryErrorResponse {
                error: PutRepositoryErrorResponseType::InvalidSignature,
            }),
        )
            .into_response();
    };

    let Ok(signature) = Signature::decode(&signature_text) else {
        return (
            StatusCode::BAD_REQUEST,
            Json(PutRepositoryErrorResponse {
                error: PutRepositoryErrorResponseType::InvalidSignature,
            }),
        )
            .into_response();
    };

    let repository_id = repository_context.repository_id;

    let Ok(public_key) =
        PublicKey::from_base64(&repository_context.repository_configuration.minisign_key)
    else {
        error!("Failed to parse public key for repository {repository_id}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let Ok(mut verifier) = public_key.verify_stream(&signature) else {
        error!("Failed to verify signature for repository {repository_id}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    let database_connection = repository_context.database_connection;
    let version_handle = match create_version(
        &database_connection,
        uuid.to_string(),
        target.to_string(),
        channel.to_string(),
    )
    .await
    {
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
                verifier.update(&frame);
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

    if let Err(e) = verifier.finalize() {
        return match e {
            Error::InvalidSignature => (
                StatusCode::BAD_REQUEST,
                Json(PutRepositoryErrorResponse {
                    error: PutRepositoryErrorResponseType::InvalidSignature,
                }),
            )
                .into_response(),
            _ => {
                error!("Failed to verify signature for repository {repository_id}: {e}");
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        };
    }

    if let Err(e) = fs::write(version_path.join("artifact.sig"), signature_text.as_bytes()).await {
        error!(
            "Failed to create signature file in version {version} in repository {repository_id}: {e}"
        );
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

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

#[derive(Deserialize)]
pub struct ArtifactSearch {
    target: Option<String>,
    channel: Option<String>,
}

async fn get_artifact_list(
    repository_context: RepositoryContext,
    search: Query<ArtifactSearch>,
    pagination: Query<Pagination>,
    state: State<RouterState>,
) -> Response<Body> {
    let Ok(artifact_list) = database::get_artifact_list(
        &repository_context.database_connection,
        search.target.clone(),
        search.channel.clone(),
        (*pagination).clone(),
    )
    .await
    else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };

    (StatusCode::OK, Json(artifact_list)).into_response()
}

#[derive(Deserialize)]
pub struct ArtifactDownload {
    artifact: u64,
}

async fn get_artifact(
    repository_context: RepositoryContext,
    Path(ArtifactDownload { artifact }): Path<ArtifactDownload>,
    range: Option<TypedHeader<Range>>,
) -> Response<Body> {
    let artifact =
        match database::get_artifact(&repository_context.database_connection, artifact).await {
            Ok(Some(artifact)) => artifact,
            Ok(None) => return StatusCode::NOT_FOUND.into_response(),
            Err(e) => {
                error!(
                    "Failed to query for version {artifact} in repository {}: {e}",
                    repository_context.repository_id
                );
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

    let version = artifact.number.to_string();
    let version_path = repository_context.repository_root.join(&version);

    let artifact_file = version_path.join("artifact.bin");

    let file = match File::open(&artifact_file).await {
        Ok(file) => file,
        Err(e) => {
            error!(
                "Failed to open artifact file {artifact_file:?} for version {version} in repository {}: {e}",
                repository_context.repository_id
            );
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    respond_with_file(file, range).await
}

async fn respond_with_file(file: File, range: Option<TypedHeader<Range>>) -> Response<Body> {
    let body = match KnownSize::file(file).await {
        Ok(body) => body,
        Err(e) => {
            error!("Failed to get file metadata: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let range = range.map(|TypedHeader(range)| range);
    Ranged::new(range, body).into_response()
}

use crate::route::api::Pagination;
use serde::Serialize;
use tokio_rusqlite::fallible_iterator::FallibleIterator;
use tokio_rusqlite::{named_params, Connection, OptionalExtension};

pub async fn setup_database(connection: &Connection) -> Result<(), tokio_rusqlite::Error> {
    connection
        .call(|connection| {
            connection.execute("PRAGMA journal_mode = WAL;", [])?;
            connection.execute("PRAGMA foreign_keys = ON;", [])?;
            Ok(())
        })
        .await
}

#[allow(unused_assignments)]
pub async fn ensure_up_to_date(connection: &Connection) -> Result<(), tokio_rusqlite::Error> {
    connection
        .call(|connection| {
            let mut version = connection
                .prepare("SELECT version FROM version")
                .ok()
                .and_then(|mut prepared_statement| {
                    prepared_statement.query_one([], |row| row.get(0)).ok()
                })
                .unwrap_or(0);

            if version < 1 {
                connection.execute_batch(include_str!("database/version-1.sql"))?;
                version += 1;
            }

            if version < 2 {
                connection.execute_batch(include_str!("database/version-2.sql"))?;
                version += 1;
            }

            Ok(())
        })
        .await
}

pub struct VersionHandle<'connection> {
    version: u64,
    connection: &'connection Connection,
}

pub async fn create_version(
    connection: &Connection,
    uuid: String,
    target: String,
    channel: String,
    original_filename: Option<String>,
) -> Result<VersionHandle, tokio_rusqlite::Error> {
    connection
        .call(move |connection| {
            connection
                .prepare("INSERT INTO artifacts(uuid, target, channel, original_filename) VALUES(:uuid, :target, :channel, :original_filename) RETURNING number;")?
                .query_one(
                    named_params! {
                        ":uuid": uuid,
                        ":target": target,
                        ":channel": channel,
                        ":original_filename": original_filename
                    },
                    |row| row.get(0),
                )
        })
        .await.map(|version| VersionHandle { connection, version })
}

impl VersionHandle<'_> {
    pub fn version(&self) -> u64 {
        self.version
    }

    pub async fn mark_complete(&self) -> Result<(), tokio_rusqlite::Error> {
        let version = self.version;
        self.connection
            .call(move |connection| {
                connection
                    .prepare("UPDATE artifacts SET complete = 1 WHERE number = :version;")?
                    .execute(named_params! {
                        ":version": version
                    })?;

                Ok(())
            })
            .await
    }
}

#[derive(Serialize)]
pub struct ArtifactVersion {
    pub number: u64,
    pub target: String,
    pub channel: String,
    pub version: Option<String>,
    pub original_filename: Option<String>,
}

pub async fn get_artifact_list(
    connection: &Connection,
    target: Option<String>,
    channel: Option<String>,
    pagination: Pagination,
) -> Result<Vec<ArtifactVersion>, tokio_rusqlite::Error> {
    connection
        .call(move |connection| {
            connection
                .prepare(
                    "SELECT number, target, channel, version, original_filename
                                  FROM artifacts
                                  WHERE complete = 1
                                    AND (target = :target OR :target IS NULL)
                                    AND (channel = :channel OR :channel IS NULL)
                                  ORDER BY number DESC
                                  LIMIT :limit
                                  OFFSET :offset;",
                )?
                .query_map(
                    named_params! {
                        ":target": target,
                        ":channel": channel,
                        ":limit": pagination.limit(),
                        ":offset": pagination.offset()
                    },
                    |row| {
                        Ok(ArtifactVersion {
                            number: row.get(0)?,
                            target: row.get(1)?,
                            channel: row.get(2)?,
                            version: row.get(3)?,
                            original_filename: row.get(4)?,
                        })
                    },
                )
                .map(|result| result.map(|result| result.unwrap()).collect())
        })
        .await
}

pub async fn get_artifact(
    connection: &Connection,
    number: u64,
) -> Result<Option<ArtifactVersion>, tokio_rusqlite::Error> {
    connection
        .call(move |connection| {
            connection
                .prepare(
                    "SELECT number, target, channel, version, original_filename
                                  FROM artifacts
                                  WHERE number = :number;",
                )?
                .query_one(
                    named_params! {
                        ":number": number,
                    },
                    |row| {
                        Ok(ArtifactVersion {
                            number: row.get(0)?,
                            target: row.get(1)?,
                            channel: row.get(2)?,
                            version: row.get(3)?,
                            original_filename: row.get(4)?,
                        })
                    },
                )
                .optional()
        })
        .await
}

pub async fn get_latest_artifact_by_uuid(
    connection: &Connection,
    uuid: String,
) -> Result<Option<ArtifactVersion>, tokio_rusqlite::Error> {
    connection
        .call(move |connection| {
            connection
                .prepare(
                    "SELECT * FROM (SELECT a.number, a.target, a.channel, a.version, a.original_filename, a.uuid
                                  FROM artifacts a, artifacts b
                                  WHERE b.uuid = :uuid
                                      AND a.target = b.target
                                      AND a.channel = b.channel
                                      AND a.complete = 1
                                  ORDER BY a.number DESC
                                  LIMIT 1) AS query WHERE query.uuid != :uuid;",
                )?
                .query_one(
                    named_params! {
                        ":uuid": uuid,
                    },
                    |row| {
                        Ok(ArtifactVersion {
                            number: row.get(0)?,
                            target: row.get(1)?,
                            channel: row.get(2)?,
                            version: row.get(3)?,
                            original_filename: row.get(4)?,
                        })
                    },
                )
                .optional()
        })
        .await
}

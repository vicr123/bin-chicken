use tokio::task::spawn_blocking;
use tokio_rusqlite::{Connection, named_params};

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
    target: String,
    channel: String,
) -> Result<VersionHandle, tokio_rusqlite::Error> {
    connection
        .call(move |connection| {
            connection
                .prepare("INSERT INTO artifacts(target, channel) VALUES(:target, :channel) RETURNING number;")?
                .query_one(
                    named_params! {
                        ":target": target,
                        ":channel": channel
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

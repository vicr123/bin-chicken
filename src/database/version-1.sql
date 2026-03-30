CREATE TABLE version
(
    version INTEGER PRIMARY KEY
);

CREATE TABLE artifacts
(
    number   INTEGER PRIMARY KEY,
    uuid     TEXT    NOT NULL,
    complete INTEGER NOT NULL DEFAULT 0,
    target   TEXT    NOT NULL,
    channel  TEXT    NOT NULL,
    version  TEXT,
    date     DATE
);

CREATE UNIQUE INDEX artifacts_uuid_unique
    ON artifacts (uuid);

INSERT INTO version(version)
VALUES (1);
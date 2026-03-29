CREATE TABLE version
(
    version INTEGER PRIMARY KEY
);

CREATE TABLE artifacts
(
    number   INTEGER PRIMARY KEY,
    complete INTEGER NOT NULL DEFAULT 0,
    target   TEXT    NOT NULL,
    channel  TEXT    NOT NULL,
    version  TEXT,
    date     DATE
);

INSERT INTO version(version)
VALUES (1);
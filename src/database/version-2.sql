ALTER TABLE artifacts
    ADD COLUMN original_filename TEXT;

-- noinspection SqlWithoutWhere
DELETE
FROM version;

INSERT INTO version(version)
VALUES (2);
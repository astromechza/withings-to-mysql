CREATE TABLE IF NOT EXISTS devices (
    deviceid         VARCHAR(128) NOT NULL PRIMARY KEY,
    device_type      VARCHAR(128) NOT NULL,
    model            VARCHAR(128) NOT NULL,
    model_id         BIGINT       NOT NULL,
    battery          VARCHAR(32),
    last_session_at  DATETIME,
    timezone         VARCHAR(64),
    synced_at        BIGINT       NOT NULL
);

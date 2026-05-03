-- Timestamps used as Grafana time axes are DATETIME (UTC, naive).
-- Internal tracking fields (modified_at, created_at, cursor values) stay BIGINT Unix seconds.

CREATE TABLE IF NOT EXISTS state (
    key_name   VARCHAR(128) NOT NULL PRIMARY KEY,
    value_text TEXT         NOT NULL,
    updated_at TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
                            ON UPDATE CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS measurements (
    grpid                    BIGINT   NOT NULL PRIMARY KEY,
    attrib                   BIGINT   NOT NULL,
    measured_at              DATETIME NOT NULL,
    created_at               BIGINT   NOT NULL,
    modified_at              BIGINT   NOT NULL,
    category                 BIGINT   NOT NULL,
    deviceid                 VARCHAR(128),
    model                    VARCHAR(128),
    weight_kg                DOUBLE,
    fat_free_mass_kg         DOUBLE,
    fat_ratio                DOUBLE,
    fat_mass_kg              DOUBLE,
    heart_rate_bpm           DOUBLE,
    spo2_ratio               DOUBLE,
    body_temperature_celsius DOUBLE,
    skin_temperature_celsius DOUBLE,
    muscle_mass_kg           DOUBLE,
    water_ratio              DOUBLE,
    bone_mass_kg             DOUBLE
);

CREATE TABLE IF NOT EXISTS daily_activity (
    date                DATE        NOT NULL PRIMARY KEY,
    modified_at         BIGINT      NOT NULL,
    steps               BIGINT,
    distance_meters     DOUBLE,
    calories_kcal       DOUBLE,
    total_calories_kcal DOUBLE,
    timezone            VARCHAR(64) NOT NULL,
    deviceid            VARCHAR(128),
    is_tracker          TINYINT(1)
);

CREATE TABLE IF NOT EXISTS sleep_sessions (
    id                         BIGINT      NOT NULL PRIMARY KEY,
    timezone                   VARCHAR(64) NOT NULL,
    start_time                 DATETIME    NOT NULL,
    end_time                   DATETIME    NOT NULL,
    date                       DATE        NOT NULL,
    created_at                 BIGINT      NOT NULL,
    modified_at                BIGINT      NOT NULL,
    completed                  TINYINT(1),
    light_sleep_seconds        BIGINT,
    deep_sleep_seconds         BIGINT,
    rem_sleep_seconds          BIGINT,
    awake_seconds              BIGINT,
    wakeup_count               BIGINT,
    duration_to_sleep_seconds  BIGINT,
    duration_to_wakeup_seconds BIGINT,
    hr_average                 DOUBLE,
    hr_min                     DOUBLE,
    hr_max                     DOUBLE
);

CREATE TABLE IF NOT EXISTS workouts (
    id                     BIGINT      NOT NULL PRIMARY KEY,
    category               BIGINT      NOT NULL,
    attrib                 BIGINT      NOT NULL,
    start_time             DATETIME    NOT NULL,
    end_time               DATETIME    NOT NULL,
    modified_at            BIGINT      NOT NULL,
    timezone               VARCHAR(64) NOT NULL,
    date                   DATE,
    calories_kcal          DOUBLE,
    intensity              DOUBLE,
    manual_distance_meters DOUBLE,
    manual_calories_kcal   DOUBLE,
    hr_average             DOUBLE,
    hr_min                 DOUBLE,
    hr_max                 DOUBLE,
    hr_zone_0_seconds      DOUBLE,
    hr_zone_1_seconds      DOUBLE,
    hr_zone_2_seconds      DOUBLE,
    hr_zone_3_seconds      DOUBLE,
    pause_seconds          DOUBLE,
    steps                  DOUBLE,
    distance_meters        DOUBLE,
    elevation_meters       DOUBLE,
    spo2_average           DOUBLE
);

-- Primary key is DATETIME for Grafana $__timeFilter(event_time) usage.
CREATE TABLE IF NOT EXISTS intraday (
    event_time       DATETIME NOT NULL PRIMARY KEY,
    heart_rate       DOUBLE,
    steps            BIGINT,
    elevation        DOUBLE,
    calories         DOUBLE,
    distance_meters  DOUBLE,
    spo2_auto        DOUBLE,
    duration_seconds BIGINT
);

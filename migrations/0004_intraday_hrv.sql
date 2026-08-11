ALTER TABLE intraday
    ADD COLUMN rmssd_ms DOUBLE,
    ADD COLUMN sdnn1_ms DOUBLE,
    ADD COLUMN hrv_quality BIGINT;

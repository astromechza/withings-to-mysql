# Changelog

## [0.4.0] - 2026-05-16

### Added

- Sync Withings device state via `getdevice` on every run. Stores `device_type`,
  `model`, `model_id`, `battery` (`high`/`medium`/`low`), `last_session_at`, and
  `timezone` per device in a new `devices` table (migration `0003_devices.sql`).

## [0.3.1] - 2026-05-07

### Fixed

- Force `time_zone = '+00:00'` on every MySQL connection so `FROM_UNIXTIME()` in
  Grafana's `$__timeFilter` uses UTC boundaries, preventing a 1-hour shift during
  British Summer Time.

## [0.3.0] - 2026-05-06

### Added

- Sync `core_body_temperature` from `getintradayactivity` — captured at ~1-minute
  granularity from ScanWatch 2's continuous temperature sensor. Stored in the
  `intraday` table via migration `0002_intraday_core_body_temperature.sql`.

## [0.2.0]

- Re-fetch last 4 h of intraday data to recover delayed watch uploads.
- Remove unused `WITHINGS_USER_TZ` env var.

## [0.1.0]

- Initial release.

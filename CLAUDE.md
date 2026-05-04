# withings-to-mysql — Claude Code context

## What this is

One-shot Rust binary that polls the Withings health API and UPSERTs all data into
MySQL/MariaDB. Designed to run as a Kubernetes CronJob. Replaced
`withings-exporter` (Prometheus/OTLP push) because Withings retroactively edits
sleep/activity records via AI re-parsing, which a TSDB cannot handle.

## Tech stack

- Rust 1.88 (pinned in `rust-toolchain.toml`), tokio async
- sqlx 0.8 — MySQL/MariaDB, embedded migrations, **dynamic queries only** (no
  `query!` macro; avoids needing a live DB at compile time)
- reqwest 0.12 with rustls-tls (no OpenSSL dependency)
- clap 4 for CLI subcommands
- wiremock for integration tests

## Critical constraints — read before writing any SQL

**MariaDB compatibility:**
- Use `ON DUPLICATE KEY UPDATE col = VALUES(col)` for upserts — works on all
  MariaDB versions and MySQL ≤ 8.0.19.
- **Never** use the MySQL 8.0.20+ alias syntax (`AS new ... new.col`) — not
  supported in MariaDB.

**Grafana time detection:**
- Primary event-time columns (`measured_at`, `start_time`, `end_time`,
  `event_time`, `date`) are `DATETIME` (UTC naive) so Grafana's MySQL datasource
  auto-detects them and `$__timeFilter(column)` works without wrappers.
- Internal tracking fields (`modified_at`, `created_at`, cursor values in `state`
  table) are `BIGINT` Unix seconds.

**Schema migrations:**
- Numbered SQL files in `migrations/` (`0001_initial.sql`, `0002_…`, …).
- sqlx runs them automatically on startup via `sqlx::migrate!("./migrations")`.
- Never alter existing migration files — add a new numbered file instead.
- sqlx tracks applied migrations by filename hash.

## File map

```
src/
  main.rs              — calls lib::run()
  lib.rs               — Cli / Cmd enum, run(), init_logging()
  config.rs            — Config::from_env() (reads env vars)
  db.rs                — connect(url) → MySqlPool, runs migrations
  state.rs             — load/save Tokens + Cursors via state key-value table
  withings/
    mod.rs
    auth.rs            — HMAC-SHA256 sign_getnonce / sign_action, token parsing
    client.rs          — WithingsClient, Tokens struct
    api/
      mod.rs           — de_bool_as_i64, unwrap_envelope
      measure.rs       — MeasureBody / MeasureGroup / Measure
      activity.rs      — ActivityBody / DailyActivity
      sleep.rs         — SleepBody / SleepNight / SleepData
      workouts.rs      — WorkoutsBody / Workout / WorkoutData
      intraday.rs      — IntradayBody (BTreeMap) / IntradaySample
  cmd/
    mod.rs
    auth_url.rs        — print OAuth consent URL
    exchange.rs        — exchange auth code → tokens, save to DB
    sync.rs            — core: fetch 5 endpoints, UPSERT, advance cursors
    dump_state.rs      — print redacted tokens + cursors from DB
migrations/
  0001_initial.sql     — state, measurements, daily_activity, sleep_sessions,
                         workouts, intraday tables
tests/
  fixtures/            — real Withings API response JSON (used by unit + integration tests)
  integration.rs       — wiremock + live MySQL tests (skipped if no DATABASE_URL)
```

## Key design decisions in sync.rs

**Cursors:** Each endpoint stores a `cursor` (Unix timestamp of the last seen
`modified` value) in the `state` table. On next run, `since_or_backfill(cursor,
cfg, now)` returns `cursor + 1` (exclusive) to skip the boundary record, or
`now - backfill_days * 86400` on first run.

**Intraday pagination:** Withings caps `getintradayactivity` at 24 h per request.
`sync_intraday` loops in 24 h chunks (`INTRADAY_CHUNK_SECS = 86400`) up to
`MAX_INTRADAY_PAGES = 90` per run. Cursor advances to `chunk_end` after each
page — including empty ones — so gaps don't cause re-fetching. Remaining backfill
continues on the next scheduled run.

**Timestamp conversion:** `ts_to_naive(ts: i64) -> Option<NaiveDateTime>` uses
`chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.naive_utc())` — the correct
non-deprecated form.

**`Tokens` lives in `withings/client.rs`** and is re-exported from `state.rs`.
`WithingsClient` holds tokens in `Arc<Mutex<Tokens>>` and refreshes them
automatically mid-sync. Call `client.snapshot_tokens()` after `run_sync` and
re-save to DB.

## State table keys

| Key | Value |
|-----|-------|
| `token.access_token` | OAuth access token string |
| `token.refresh_token` | OAuth refresh token string |
| `token.expires_at` | Unix seconds (string) |
| `token.userid` | Withings user ID (string) |
| `token.scope` | OAuth scope string |
| `cursor.measure` | last `modified` seen from getmeas |
| `cursor.activity` | last `modified` seen from getactivity |
| `cursor.sleep` | last `modified` seen from getsummary |
| `cursor.workouts` | last `modified` seen from getworkouts |
| `cursor.intraday` | `chunk_end` of last completed intraday page |

## Environment variables

| Var | Required | Default | Purpose |
|-----|----------|---------|---------|
| `DATABASE_URL` | yes | — | `mysql://user:pass@host/db` |
| `WITHINGS_CLIENT_ID` | yes | — | OAuth client ID |
| `WITHINGS_CLIENT_SECRET` | yes | — | OAuth client secret |
| `WITHINGS_BACKFILL_DAYS` | no | `30` | Days to back-fill on first sync |

## Commands

```bash
# Print OAuth consent URL
cargo run -- auth-url --redirect-uri https://localhost/cb

# Exchange code for tokens (sets DATABASE_URL first)
cargo run -- exchange --redirect-uri https://localhost/cb <CODE>

# Run sync
cargo run -- sync

# Inspect DB state (tokens redacted)
cargo run -- dump-state
```

## Tests

```bash
# Unit tests (no DB required)
cargo test --lib --bins

# Integration tests (requires live MySQL)
DATABASE_URL="mysql://user:pass@127.0.0.1:3306/withings_test" cargo test --test integration

# Local MySQL via Docker
docker run --rm -d -p 3306:3306 -e MYSQL_ROOT_PASSWORD=root -e MYSQL_DATABASE=withings_test mysql:8
```

## CI / Release

- **CI** (push to main, PRs): `fmt` + `clippy -D warnings` + unit tests + `cargo deny`
- **Release** (push `v*` tag): builds `linux/amd64` + `linux/arm64` Docker image,
  pushes to `ghcr.io/astromechza/withings-to-mysql:{version}` + `latest`,
  creates GitHub Release with auto-generated notes.

## Adding new Withings endpoints

1. Add API model in `src/withings/api/<endpoint>.rs` (follow existing pattern)
2. Add columns to a new migration file `migrations/0002_<name>.sql`
3. Add `cursor.<name>: i64` to `Cursors` in `state.rs` + load/save in `load_cursors`/`save_cursors`
4. Add `sync_<name>` function in `sync.rs` following the existing pattern
5. Call it from `run_sync`
6. Update integration test fixture and row count assertion

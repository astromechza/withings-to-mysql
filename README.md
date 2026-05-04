# withings-to-mysql

One-shot Rust binary that polls the [Withings Health API](https://developer.withings.com/) and UPSERTs all data into MySQL/MariaDB. Designed to run as a Kubernetes CronJob.

Withings retroactively edits sleep and activity records via AI re-parsing. A time-series database cannot handle this — rows at past timestamps can't be updated. MySQL can, so every sync is idempotent: re-runs of the same time window just overwrite existing rows with the latest values.

## What gets synced

| Table | Source API | Description |
|---|---|---|
| `measurements` | `getmeas` | Body measurements: weight, fat, muscle mass, heart rate, SpO2, temperature |
| `daily_activity` | `getactivity` | Daily step/calorie/distance totals |
| `sleep_sessions` | `getsummary` | Per-sleep-session summary: duration, HR, light/deep/REM breakdown |
| `workouts` | `getworkouts` | Individual workout sessions with HR zones, distance, elevation |
| `intraday` | `getintradayactivity` | Per-minute heart rate, steps, SpO2, calories |

Each endpoint uses a cursor stored in the `state` table. On first run the binary back-fills `WITHINGS_BACKFILL_DAYS` days (default 30). Subsequent runs pick up from the last seen `modified` timestamp.

Intraday data is fetched in 24-hour chunks (Withings API limit) across up to 90 pages per run, resuming from the cursor on the next scheduled run if the full backfill isn't completed.

## Prerequisites

- Withings developer account with an application registered at [developer.withings.com](https://developer.withings.com/)
- MySQL 8 or MariaDB (any recent version)
- Docker image: `ghcr.io/astromechza/withings-to-mysql:latest`

## Authentication flow

OAuth 2.0, one-time setup:

**Step 1 — get the consent URL** (no DB needed):

```bash
withings-to-mysql auth-url \
  --client-id $WITHINGS_CLIENT_ID \
  --redirect-uri https://your-redirect-uri
```

Open the printed URL in a browser, log in with your Withings account, and grant access. Copy the `code=` query parameter from the redirect URL.

**Step 2 — exchange the code for tokens** (writes to DB):

```bash
DATABASE_URL="mysql://user:pass@host/db" \
WITHINGS_CLIENT_ID=... \
WITHINGS_CLIENT_SECRET=... \
  withings-to-mysql exchange \
    --redirect-uri https://your-redirect-uri \
    <CODE>
```

Tokens (access + refresh) are stored in the `state` table. The binary refreshes them automatically during sync when they expire.

## Subcommands

| Command | Requires DB | Description |
|---|---|---|
| `auth-url` | No | Print OAuth consent URL |
| `exchange <CODE>` | Yes | Exchange auth code for tokens, store in DB |
| `sync` | Yes | Fetch all endpoints, UPSERT rows, advance cursors |
| `dump-state` | Yes | Print current tokens (redacted) and cursors from DB |

## Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | Yes | — | `mysql://user:pass@host/db` |
| `WITHINGS_CLIENT_ID` | Yes | — | OAuth application client ID |
| `WITHINGS_CLIENT_SECRET` | Yes | — | OAuth application client secret |
| `WITHINGS_BACKFILL_DAYS` | No | `30` | Days to back-fill on first sync |
| `WITHINGS_USER_TZ` | No | `UTC` | User timezone (stored in records for reference, not used for conversion) |

Logging verbosity is controlled via `RUST_LOG` (e.g. `RUST_LOG=debug`).

## Database schema

Schema migrations run automatically on startup via sqlx. All migrations live in `migrations/` — never edit existing files, add new numbered ones.

### `state`
Key-value store for OAuth tokens and sync cursors.

| Column | Type | Description |
|---|---|---|
| `key_name` | VARCHAR(128) PK | Key identifier |
| `value_text` | TEXT | Value |
| `updated_at` | TIMESTAMP | Last write time |

### `measurements`
Body measurement groups from `getmeas`. One row per measurement group.

| Column | Type |
|---|---|
| `grpid` | BIGINT PK |
| `attrib` | BIGINT |
| `measured_at` | DATETIME (UTC) |
| `created_at` / `modified_at` | BIGINT (Unix seconds) |
| `category` | BIGINT |
| `deviceid` / `model` | VARCHAR |
| `weight_kg`, `fat_free_mass_kg`, `fat_ratio`, `fat_mass_kg` | DOUBLE |
| `heart_rate_bpm`, `spo2_ratio` | DOUBLE |
| `body_temperature_celsius`, `skin_temperature_celsius` | DOUBLE |
| `muscle_mass_kg`, `water_ratio`, `bone_mass_kg` | DOUBLE |

### `daily_activity`
One row per calendar date.

| Column | Type |
|---|---|
| `date` | DATE PK |
| `modified_at` | BIGINT (Unix seconds) |
| `steps` | BIGINT |
| `distance_meters` | DOUBLE |
| `calories_kcal`, `total_calories_kcal` | DOUBLE |
| `timezone` | VARCHAR(64) |
| `deviceid` | VARCHAR(128) |
| `is_tracker` | TINYINT(1) |

### `sleep_sessions`
One row per sleep session.

| Column | Type |
|---|---|
| `id` | BIGINT PK |
| `start_time` / `end_time` | DATETIME (UTC) |
| `date` | DATE |
| `timezone` | VARCHAR(64) |
| `created_at` / `modified_at` | BIGINT (Unix seconds) |
| `completed` | TINYINT(1) |
| `light_sleep_seconds`, `deep_sleep_seconds`, `rem_sleep_seconds`, `awake_seconds` | BIGINT |
| `wakeup_count` | BIGINT |
| `duration_to_sleep_seconds`, `duration_to_wakeup_seconds` | BIGINT |
| `hr_average`, `hr_min`, `hr_max` | DOUBLE |

### `workouts`
One row per workout session.

| Column | Type |
|---|---|
| `id` | BIGINT PK |
| `category`, `attrib` | BIGINT |
| `start_time` / `end_time` | DATETIME (UTC) |
| `modified_at` | BIGINT (Unix seconds) |
| `timezone` | VARCHAR(64) |
| `date` | DATE |
| `calories_kcal`, `intensity` | DOUBLE |
| `manual_distance_meters`, `manual_calories_kcal` | DOUBLE |
| `hr_average`, `hr_min`, `hr_max` | DOUBLE |
| `hr_zone_0_seconds`…`hr_zone_3_seconds` | DOUBLE |
| `pause_seconds`, `steps`, `distance_meters`, `elevation_meters` | DOUBLE |
| `spo2_average` | DOUBLE |

### `intraday`
Per-minute samples. Primary key is the event timestamp.

| Column | Type |
|---|---|
| `event_time` | DATETIME PK (UTC) |
| `heart_rate` | DOUBLE |
| `steps` | BIGINT |
| `elevation`, `calories`, `distance_meters`, `spo2_auto` | DOUBLE |
| `duration_seconds` | BIGINT |

All `DATETIME` columns are UTC-naive so Grafana's MySQL datasource auto-detects them as time axes and `$__timeFilter(column)` works without wrappers.

## Kubernetes deployment

Manifests in `k8s/` are reference examples — adapt namespace, image tag, and MariaDB CR name to your cluster. The setup assumes [mariadb-operator](https://github.com/mariadb-operator/mariadb-operator).

### 1. Store credentials

```bash
kubectl apply -f k8s/secret.yaml  # edit client ID/secret first
```

Or use Sealed Secrets / External Secrets in production.

### 2. Provision database (mariadb-operator)

```bash
kubectl apply -f k8s/db.yaml
```

This creates: `Database`, `User`, `Grant`, and a `Connection` that produces a `withings-db-url` Secret with a ready-to-use `DATABASE_URL` key.

### 3. Run the exchange job (one-time auth)

Get the consent URL:

```bash
kubectl run auth-url --rm -it --restart=Never \
  --image=ghcr.io/astromechza/withings-to-mysql:latest \
  --env=WITHINGS_CLIENT_ID=<id> \
  -- auth-url --redirect-uri https://your-redirect-uri
```

Complete the consent flow, copy the `code=` value, then:

```bash
CODE=<paste-code-here>
kubectl create secret generic withings-exchange-code --from-literal=code=$CODE
# edit k8s/exchange-job.yaml: set --redirect-uri to match what you used above
kubectl apply -f k8s/exchange-job.yaml
kubectl logs job/withings-exchange
# Expected: "Tokens saved to DB (userid=...)"
kubectl delete job/withings-exchange secret/withings-exchange-code
```

### 4. Deploy the CronJob

```bash
kubectl apply -f k8s/cronjob.yaml
```

Default schedule is hourly (`0 * * * *`). `concurrencyPolicy: Forbid` prevents overlapping syncs.

## Local development

```bash
# Unit tests (no DB required)
cargo test --lib --bins

# Integration tests (requires live MySQL)
docker run --rm -d -p 3306:3306 \
  -e MYSQL_ROOT_PASSWORD=root \
  -e MYSQL_DATABASE=withings_test \
  mysql:8
DATABASE_URL="mysql://root:root@127.0.0.1:3306/withings_test" \
  cargo test --test integration

# Lint
cargo fmt --check
cargo clippy -- -D warnings
cargo deny check
```

## Container image

Multi-arch (`linux/amd64`, `linux/arm64`) images are published to `ghcr.io/astromechza/withings-to-mysql` on every `v*` tag push. The `latest` tag tracks the most recent release.

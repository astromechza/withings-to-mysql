use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::MySqlPool;

use crate::config::Config;
use crate::db;
use crate::state::{self, Cursors};
use crate::withings::api::{
    activity::ActivityBody, intraday::IntradayBody, measure::MeasureBody, sleep::SleepBody,
    unwrap_envelope, workouts::WorkoutsBody,
};
use crate::withings::client::WithingsClient;

const MEASTYPES: &str = "1,5,6,8,11,54,71,73,76,77,88";
const ACTIVITY_FIELDS: &str = "steps,distance,calories,totalcalories";
const SLEEP_FIELDS: &str = "lightsleepduration,deepsleepduration,remsleepduration,\
    wakeupduration,wakeupcount,durationtosleep,durationtowakeup,hr_average,hr_min,hr_max";
const WORKOUT_FIELDS: &str = "calories,intensity,manual_distance,manual_calories,\
    hr_average,hr_min,hr_max,hr_zone_0,hr_zone_1,hr_zone_2,hr_zone_3,\
    pause_duration,steps,distance,elevation,spo2_average";
const INTRADAY_FIELDS: &str =
    "steps,elevation,calories,distance,duration,heart_rate,spo2_auto,core_body_temperature";

pub async fn run() -> Result<()> {
    let cfg = Config::from_env()?;
    let pool = db::connect(&cfg.database_url).await?;
    let tokens = state::load_tokens(&pool)
        .await?
        .context("No tokens — run `exchange` first")?;

    let http = reqwest::Client::builder()
        .user_agent(format!("withings-to-mysql/{}", env!("CARGO_PKG_VERSION")))
        .build()?;
    let client = WithingsClient::new(
        http,
        cfg.client_id.clone(),
        cfg.client_secret.clone(),
        tokens,
    );

    let mut cursors = state::load_cursors(&pool).await?;
    let now = now_secs();
    run_sync(&cfg, &client, &pool, &mut cursors, now).await?;

    state::save_tokens(&pool, &client.snapshot_tokens()).await?;
    state::save_cursors(&pool, &cursors).await?;
    tracing::info!("sync complete");
    Ok(())
}

/// Core sync logic, extracted for testability.
pub async fn run_sync(
    cfg: &Config,
    client: &WithingsClient,
    pool: &MySqlPool,
    cursors: &mut Cursors,
    now: i64,
) -> Result<()> {
    sync_measurements(client, pool, cursors, cfg, now).await?;
    sync_activity(client, pool, cursors, cfg, now).await?;
    sync_sleep(client, pool, cursors, cfg, now).await?;
    sync_workouts(client, pool, cursors, cfg, now).await?;
    sync_intraday(client, pool, cursors, cfg, now).await?;
    Ok(())
}

// ── measurements ──────────────────────────────────────────────────────────────

async fn sync_measurements(
    client: &WithingsClient,
    pool: &MySqlPool,
    cursors: &mut Cursors,
    cfg: &Config,
    now: i64,
) -> Result<()> {
    let raw = client
        .post_data(
            "/measure",
            &[
                ("action", "getmeas".into()),
                ("meastypes", MEASTYPES.into()),
                (
                    "lastupdate",
                    since_or_backfill(cursors.measure, cfg, now).to_string(),
                ),
            ],
        )
        .await
        .context("getmeas")?;
    let body: MeasureBody = unwrap_envelope(&raw)?;

    for g in &body.measuregrps {
        let mut weight_kg: Option<f64> = None;
        let mut fat_free_mass_kg: Option<f64> = None;
        let mut fat_ratio: Option<f64> = None;
        let mut fat_mass_kg: Option<f64> = None;
        let mut heart_rate_bpm: Option<f64> = None;
        let mut spo2_ratio: Option<f64> = None;
        let mut body_temperature_celsius: Option<f64> = None;
        let mut skin_temperature_celsius: Option<f64> = None;
        let mut muscle_mass_kg: Option<f64> = None;
        let mut water_ratio: Option<f64> = None;
        let mut bone_mass_kg: Option<f64> = None;

        for m in &g.measures {
            let v = m.real();
            match m.kind {
                1 => weight_kg = Some(v),
                5 => fat_free_mass_kg = Some(v),
                6 => fat_ratio = Some(v),
                8 => fat_mass_kg = Some(v),
                11 => heart_rate_bpm = Some(v),
                54 => spo2_ratio = Some(v),
                71 => body_temperature_celsius = Some(v),
                73 => skin_temperature_celsius = Some(v),
                76 => muscle_mass_kg = Some(v),
                77 => water_ratio = Some(v),
                88 => bone_mass_kg = Some(v),
                other => tracing::debug!(kind = other, "unmapped measure type"),
            }
        }

        let measured_at = ts_to_naive(g.date)
            .with_context(|| format!("invalid measured_at for grpid={}", g.grpid))?;

        sqlx::query(
            "INSERT INTO measurements
               (grpid,attrib,measured_at,created_at,modified_at,category,deviceid,model,
                weight_kg,fat_free_mass_kg,fat_ratio,fat_mass_kg,
                heart_rate_bpm,spo2_ratio,body_temperature_celsius,skin_temperature_celsius,
                muscle_mass_kg,water_ratio,bone_mass_kg)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON DUPLICATE KEY UPDATE
               attrib=VALUES(attrib),measured_at=VALUES(measured_at),
               modified_at=VALUES(modified_at),deviceid=VALUES(deviceid),model=VALUES(model),
               weight_kg=VALUES(weight_kg),fat_free_mass_kg=VALUES(fat_free_mass_kg),
               fat_ratio=VALUES(fat_ratio),fat_mass_kg=VALUES(fat_mass_kg),
               heart_rate_bpm=VALUES(heart_rate_bpm),spo2_ratio=VALUES(spo2_ratio),
               body_temperature_celsius=VALUES(body_temperature_celsius),
               skin_temperature_celsius=VALUES(skin_temperature_celsius),
               muscle_mass_kg=VALUES(muscle_mass_kg),water_ratio=VALUES(water_ratio),
               bone_mass_kg=VALUES(bone_mass_kg)",
        )
        .bind(g.grpid)
        .bind(g.attrib)
        .bind(measured_at)
        .bind(g.created)
        .bind(g.modified)
        .bind(g.category)
        .bind(&g.deviceid)
        .bind(&g.model)
        .bind(weight_kg)
        .bind(fat_free_mass_kg)
        .bind(fat_ratio)
        .bind(fat_mass_kg)
        .bind(heart_rate_bpm)
        .bind(spo2_ratio)
        .bind(body_temperature_celsius)
        .bind(skin_temperature_celsius)
        .bind(muscle_mass_kg)
        .bind(water_ratio)
        .bind(bone_mass_kg)
        .execute(pool)
        .await
        .with_context(|| format!("upsert measurement grpid={}", g.grpid))?;
    }

    if let Some(max) = body.measuregrps.iter().map(|g| g.modified).max() {
        cursors.measure = cursors.measure.max(max);
    }
    tracing::info!(count = body.measuregrps.len(), "measurements upserted");
    Ok(())
}

// ── activity ──────────────────────────────────────────────────────────────────

async fn sync_activity(
    client: &WithingsClient,
    pool: &MySqlPool,
    cursors: &mut Cursors,
    cfg: &Config,
    now: i64,
) -> Result<()> {
    let raw = client
        .post_data(
            "/v2/measure",
            &[
                ("action", "getactivity".into()),
                ("data_fields", ACTIVITY_FIELDS.into()),
                (
                    "lastupdate",
                    since_or_backfill(cursors.activity, cfg, now).to_string(),
                ),
            ],
        )
        .await
        .context("getactivity")?;
    let body: ActivityBody = unwrap_envelope(&raw)?;

    for a in &body.activities {
        let date = NaiveDate::parse_from_str(&a.date, "%Y-%m-%d")
            .with_context(|| format!("parse activity date {}", a.date))?;

        sqlx::query(
            "INSERT INTO daily_activity
               (date,modified_at,steps,distance_meters,calories_kcal,total_calories_kcal,
                timezone,deviceid,is_tracker)
             VALUES (?,?,?,?,?,?,?,?,?)
             ON DUPLICATE KEY UPDATE
               modified_at=VALUES(modified_at),steps=VALUES(steps),
               distance_meters=VALUES(distance_meters),calories_kcal=VALUES(calories_kcal),
               total_calories_kcal=VALUES(total_calories_kcal),timezone=VALUES(timezone),
               deviceid=VALUES(deviceid),is_tracker=VALUES(is_tracker)",
        )
        .bind(date)
        .bind(a.modified)
        .bind(a.steps.map(|v| v as i64))
        .bind(a.distance)
        .bind(a.calories)
        .bind(a.totalcalories)
        .bind(&a.timezone)
        .bind(&a.deviceid)
        .bind(a.is_tracker.map(|b| b as i8))
        .execute(pool)
        .await
        .with_context(|| format!("upsert activity date={}", a.date))?;
    }

    if let Some(max) = body.activities.iter().map(|a| a.modified).max() {
        cursors.activity = cursors.activity.max(max);
    }
    tracing::info!(count = body.activities.len(), "daily_activity upserted");
    Ok(())
}

// ── sleep ─────────────────────────────────────────────────────────────────────

async fn sync_sleep(
    client: &WithingsClient,
    pool: &MySqlPool,
    cursors: &mut Cursors,
    cfg: &Config,
    now: i64,
) -> Result<()> {
    let raw = client
        .post_data(
            "/v2/sleep",
            &[
                ("action", "getsummary".into()),
                ("data_fields", SLEEP_FIELDS.into()),
                (
                    "lastupdate",
                    since_or_backfill(cursors.sleep, cfg, now).to_string(),
                ),
            ],
        )
        .await
        .context("getsleep")?;
    let body: SleepBody = unwrap_envelope(&raw)?;

    for s in &body.series {
        let date = NaiveDate::parse_from_str(&s.date, "%Y-%m-%d")
            .with_context(|| format!("parse sleep date {}", s.date))?;
        let start_time = ts_to_naive(s.startdate)
            .with_context(|| format!("invalid start_time for sleep id={}", s.id))?;
        let end_time = ts_to_naive(s.enddate)
            .with_context(|| format!("invalid end_time for sleep id={}", s.id))?;

        sqlx::query(
            "INSERT INTO sleep_sessions
               (id,timezone,start_time,end_time,date,created_at,modified_at,completed,
                light_sleep_seconds,deep_sleep_seconds,rem_sleep_seconds,awake_seconds,
                wakeup_count,duration_to_sleep_seconds,duration_to_wakeup_seconds,
                hr_average,hr_min,hr_max)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON DUPLICATE KEY UPDATE
               modified_at=VALUES(modified_at),completed=VALUES(completed),
               light_sleep_seconds=VALUES(light_sleep_seconds),
               deep_sleep_seconds=VALUES(deep_sleep_seconds),
               rem_sleep_seconds=VALUES(rem_sleep_seconds),
               awake_seconds=VALUES(awake_seconds),wakeup_count=VALUES(wakeup_count),
               duration_to_sleep_seconds=VALUES(duration_to_sleep_seconds),
               duration_to_wakeup_seconds=VALUES(duration_to_wakeup_seconds),
               hr_average=VALUES(hr_average),hr_min=VALUES(hr_min),hr_max=VALUES(hr_max)",
        )
        .bind(s.id)
        .bind(&s.timezone)
        .bind(start_time)
        .bind(end_time)
        .bind(date)
        .bind(s.created)
        .bind(s.modified)
        .bind(s.completed.map(|b| b as i8))
        .bind(s.data.lightsleepduration)
        .bind(s.data.deepsleepduration)
        .bind(s.data.remsleepduration)
        .bind(s.data.wakeupduration)
        .bind(s.data.wakeupcount)
        .bind(s.data.durationtosleep)
        .bind(s.data.durationtowakeup)
        .bind(s.data.hr_average)
        .bind(s.data.hr_min)
        .bind(s.data.hr_max)
        .execute(pool)
        .await
        .with_context(|| format!("upsert sleep id={}", s.id))?;
    }

    if let Some(max) = body.series.iter().map(|s| s.modified).max() {
        cursors.sleep = cursors.sleep.max(max);
    }
    tracing::info!(count = body.series.len(), "sleep_sessions upserted");
    Ok(())
}

// ── workouts ──────────────────────────────────────────────────────────────────

async fn sync_workouts(
    client: &WithingsClient,
    pool: &MySqlPool,
    cursors: &mut Cursors,
    cfg: &Config,
    now: i64,
) -> Result<()> {
    let raw = client
        .post_data(
            "/v2/measure",
            &[
                ("action", "getworkouts".into()),
                ("data_fields", WORKOUT_FIELDS.into()),
                (
                    "lastupdate",
                    since_or_backfill(cursors.workouts, cfg, now).to_string(),
                ),
            ],
        )
        .await
        .context("getworkouts")?;
    let body: WorkoutsBody = unwrap_envelope(&raw)?;

    for w in &body.series {
        let date = w
            .date
            .as_deref()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok());
        let start_time = ts_to_naive(w.startdate)
            .with_context(|| format!("invalid start_time for workout id={}", w.id))?;
        let end_time = ts_to_naive(w.enddate)
            .with_context(|| format!("invalid end_time for workout id={}", w.id))?;

        sqlx::query(
            "INSERT INTO workouts
               (id,category,attrib,start_time,end_time,modified_at,timezone,date,
                calories_kcal,intensity,manual_distance_meters,manual_calories_kcal,
                hr_average,hr_min,hr_max,
                hr_zone_0_seconds,hr_zone_1_seconds,hr_zone_2_seconds,hr_zone_3_seconds,
                pause_seconds,steps,distance_meters,elevation_meters,spo2_average)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON DUPLICATE KEY UPDATE
               modified_at=VALUES(modified_at),calories_kcal=VALUES(calories_kcal),
               intensity=VALUES(intensity),
               manual_distance_meters=VALUES(manual_distance_meters),
               manual_calories_kcal=VALUES(manual_calories_kcal),
               hr_average=VALUES(hr_average),hr_min=VALUES(hr_min),hr_max=VALUES(hr_max),
               hr_zone_0_seconds=VALUES(hr_zone_0_seconds),
               hr_zone_1_seconds=VALUES(hr_zone_1_seconds),
               hr_zone_2_seconds=VALUES(hr_zone_2_seconds),
               hr_zone_3_seconds=VALUES(hr_zone_3_seconds),
               pause_seconds=VALUES(pause_seconds),steps=VALUES(steps),
               distance_meters=VALUES(distance_meters),
               elevation_meters=VALUES(elevation_meters),spo2_average=VALUES(spo2_average)",
        )
        .bind(w.id)
        .bind(w.category)
        .bind(w.attrib)
        .bind(start_time)
        .bind(end_time)
        .bind(w.modified)
        .bind(&w.timezone)
        .bind(date)
        .bind(w.data.calories)
        .bind(w.data.intensity)
        .bind(w.data.manual_distance)
        .bind(w.data.manual_calories)
        .bind(w.data.hr_average)
        .bind(w.data.hr_min)
        .bind(w.data.hr_max)
        .bind(w.data.hr_zone_0)
        .bind(w.data.hr_zone_1)
        .bind(w.data.hr_zone_2)
        .bind(w.data.hr_zone_3)
        .bind(w.data.pause_duration)
        .bind(w.data.steps)
        .bind(w.data.distance)
        .bind(w.data.elevation)
        .bind(w.data.spo2_average)
        .execute(pool)
        .await
        .with_context(|| format!("upsert workout id={}", w.id))?;
    }

    if let Some(max) = body.series.iter().map(|w| w.modified).max() {
        cursors.workouts = cursors.workouts.max(max);
    }
    tracing::info!(count = body.series.len(), "workouts upserted");
    Ok(())
}

// ── intraday ──────────────────────────────────────────────────────────────────

/// Withings getintradayactivity is capped at 24 h per request.
/// We page through in 24 h chunks up to MAX_INTRADAY_PAGES per sync run.
/// On the next scheduled run the cursor picks up where we left off.
const INTRADAY_CHUNK_SECS: i64 = 86_400;
const MAX_INTRADAY_PAGES: usize = 90; // 90 days max catch-up per run
const INTRADAY_LOOKBACK_SECS: i64 = 4 * 3600; // re-fetch last 4h to catch delayed watch uploads

async fn sync_intraday(
    client: &WithingsClient,
    pool: &MySqlPool,
    cursors: &mut Cursors,
    cfg: &Config,
    now: i64,
) -> Result<()> {
    let last_data_ts: Option<i64> =
        sqlx::query_scalar("SELECT UNIX_TIMESTAMP(MAX(event_time)) FROM intraday")
            .fetch_one(pool)
            .await?;

    let mut chunk_start = intraday_chunk_start(cursors.intraday, last_data_ts, cfg, now);

    let (mut inserted, mut changed, mut processed) = (0u64, 0u64, 0usize);
    let mut pages = 0usize;

    while chunk_start < now {
        if pages >= MAX_INTRADAY_PAGES {
            tracing::warn!(
                pages,
                "intraday page limit reached; remaining data fetched on next run"
            );
            break;
        }

        let chunk_end = (chunk_start + INTRADAY_CHUNK_SECS).min(now);

        let raw = client
            .post_data(
                "/v2/measure",
                &[
                    ("action", "getintradayactivity".into()),
                    ("data_fields", INTRADAY_FIELDS.into()),
                    ("startdate", chunk_start.to_string()),
                    ("enddate", chunk_end.to_string()),
                ],
            )
            .await
            .with_context(|| format!("getintraday page {pages}"))?;
        let body: IntradayBody = unwrap_envelope(&raw)?;
        let samples = body.samples_sorted();

        for (ts, s) in &samples {
            let event_time =
                ts_to_naive(*ts).with_context(|| format!("invalid intraday timestamp {ts}"))?;

            let result = sqlx::query(
                "INSERT INTO intraday
                   (event_time,heart_rate,steps,elevation,calories,distance_meters,
                    spo2_auto,duration_seconds,core_body_temperature_celsius)
                 VALUES (?,?,?,?,?,?,?,?,?)
                 ON DUPLICATE KEY UPDATE
                   heart_rate=VALUES(heart_rate),steps=VALUES(steps),
                   elevation=VALUES(elevation),calories=VALUES(calories),
                   distance_meters=VALUES(distance_meters),spo2_auto=VALUES(spo2_auto),
                   duration_seconds=VALUES(duration_seconds),
                   core_body_temperature_celsius=VALUES(core_body_temperature_celsius)",
            )
            .bind(event_time)
            .bind(s.heart_rate)
            .bind(s.steps.map(|v| v as i64))
            .bind(s.elevation)
            .bind(s.calories)
            .bind(s.distance)
            .bind(s.spo2_auto)
            .bind(s.duration)
            .bind(s.core_body_temperature_celsius)
            .execute(pool)
            .await
            .with_context(|| format!("upsert intraday ts={ts}"))?;

            match result.rows_affected() {
                1 => inserted += 1,
                2 => changed += 1,
                _ => {} // 0 = duplicate, no change
            }
        }

        processed += samples.len();
        // Advance cursor to end of this chunk even if no samples (gap in data).
        cursors.intraday = cursors.intraday.max(chunk_end);
        chunk_start = chunk_end + 1;
        pages += 1;
    }

    tracing::info!(processed, inserted, changed, pages, "intraday synced");
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

pub fn since_or_backfill(cursor: i64, cfg: &Config, now: i64) -> i64 {
    // +1 makes cursor exclusive: skip the record at exactly the cursor timestamp
    // so repeated syncs don't re-fetch the boundary record.
    if cursor > 0 {
        cursor + 1
    } else {
        now - cfg.backfill_days * 86400
    }
}

pub fn intraday_chunk_start(cursor: i64, last_data_ts: Option<i64>, cfg: &Config, now: i64) -> i64 {
    match (cursor, last_data_ts) {
        (0, _) | (_, None) => now - cfg.backfill_days * 86400,
        (_, Some(last)) => last.min(now - INTRADAY_LOOKBACK_SECS),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn ts_to_naive(ts: i64) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::from_timestamp(ts, 0).map(|dt| dt.naive_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Config {
        Config {
            client_id: "C".into(),
            client_secret: "S".into(),
            database_url: "x".into(),
            backfill_days: 30,
        }
    }

    #[test]
    fn backfill_when_cursor_zero() {
        let now = 1_700_000_000i64;
        assert_eq!(since_or_backfill(0, &cfg(), now), now - 30 * 86400);
    }

    #[test]
    fn cursor_used_when_nonzero() {
        // cursor + 1 to make it exclusive (skip boundary record)
        assert_eq!(since_or_backfill(12345, &cfg(), 9_999_999), 12346);
    }

    #[test]
    fn intraday_chunk_start_gap_larger_than_lookback() {
        // last data 6h ago, cursor advanced past gap — should start at last data
        let now = 1_700_000_000i64;
        let last_data = now - 6 * 3600;
        assert_eq!(
            intraday_chunk_start(1_699_999_000i64, Some(last_data), &cfg(), now),
            last_data
        );
    }

    #[test]
    fn intraday_chunk_start_no_gap() {
        // last data 1h ago — lookback floor (4h) wins
        let now = 1_700_000_000i64;
        let last_data = now - 3600;
        assert_eq!(
            intraday_chunk_start(1_699_999_000i64, Some(last_data), &cfg(), now),
            now - INTRADAY_LOOKBACK_SECS
        );
    }

    #[test]
    fn intraday_chunk_start_empty_table() {
        // NULL MAX (no rows) with cursor==0 — full backfill
        let now = 1_700_000_000i64;
        assert_eq!(intraday_chunk_start(0, None, &cfg(), now), now - 30 * 86400);
    }
}

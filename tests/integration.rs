use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};
/// Integration tests — require a live MySQL/MariaDB instance.
/// Set DATABASE_URL env var; tests are skipped if it's absent.
///
/// Run with:
///   DATABASE_URL="mysql://user:pass@127.0.0.1:3306/withings_test" cargo test --test integration
use withings_to_mysql::cmd::sync::run_sync;
use withings_to_mysql::config::Config;
use withings_to_mysql::db;
use withings_to_mysql::state::{self, Cursors};
use withings_to_mysql::withings::client::{Tokens, WithingsClient};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
}

fn test_cfg(database_url: &str) -> Config {
    Config {
        client_id: "CID".into(),
        client_secret: "SECRET".into(),
        database_url: database_url.into(),
        backfill_days: 30,
    }
}

/// Returns None if DATABASE_URL is not set (skip).
fn db_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

async fn clean_db(pool: &sqlx::MySqlPool) {
    for t in &[
        "devices",
        "intraday",
        "workouts",
        "sleep_sessions",
        "daily_activity",
        "measurements",
        "state",
    ] {
        sqlx::query(&format!("DELETE FROM {t}"))
            .execute(pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn sync_upserts_all_tables_and_advances_cursors() {
    let Some(url) = db_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let pool = db::connect(&url).await.unwrap();
    clean_db(&pool).await;

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/measure"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("getmeas.json")))
        .mount(&server)
        .await;

    // /v2/measure serves three actions; dispatch by body content
    let act_fix = fixture("getactivity.json");
    let wkt_fix = fixture("getworkouts.json");
    let intra_fix = fixture("getintraday.json");
    Mock::given(method("POST"))
        .and(path("/v2/measure"))
        .respond_with(move |req: &wiremock::Request| {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            if body.contains("getworkouts") {
                ResponseTemplate::new(200).set_body_string(wkt_fix.clone())
            } else if body.contains("getactivity") {
                ResponseTemplate::new(200).set_body_string(act_fix.clone())
            } else if body.contains("getintradayactivity") {
                ResponseTemplate::new(200).set_body_string(intra_fix.clone())
            } else {
                ResponseTemplate::new(404)
            }
        })
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v2/sleep"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("getsleep.json")))
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/v2/user"))
        .respond_with(ResponseTemplate::new(200).set_body_string(fixture("getdevice.json")))
        .mount(&server)
        .await;

    let tokens = Tokens {
        access_token: "atk".into(),
        refresh_token: "rtk".into(),
        expires_at: i64::MAX,
        userid: "12345".into(),
        scope: "user.metrics".into(),
    };
    let http = reqwest::Client::new();
    let cfg = test_cfg(&url);
    let client = WithingsClient::new(
        http,
        cfg.client_id.clone(),
        cfg.client_secret.clone(),
        tokens,
    )
    .with_base_url(server.uri());

    let mut cursors = Cursors::default();
    let now = 1_800_000_000i64;
    run_sync(&cfg, &client, &pool, &mut cursors, now)
        .await
        .unwrap();

    // All cursors advanced
    assert!(cursors.measure > 0, "measure cursor");
    assert!(cursors.activity > 0, "activity cursor");
    assert!(cursors.sleep > 0, "sleep cursor");
    assert!(cursors.workouts > 0, "workouts cursor");
    assert!(cursors.intraday > 0, "intraday cursor");

    // Row counts match fixture data
    let count = |t: &'static str| {
        let pool = pool.clone();
        async move {
            let (n,): (i64,) = sqlx::query_as(&format!("SELECT COUNT(*) FROM {t}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            n
        }
    };

    assert_eq!(count("measurements").await, 3);
    assert_eq!(count("daily_activity").await, 5);
    assert_eq!(count("sleep_sessions").await, 3);
    assert_eq!(count("workouts").await, 3);
    assert_eq!(count("intraday").await, 17);
    assert_eq!(count("devices").await, 2);

    // HRV columns persisted for the fixture's HRV sample (ts 1777669200).
    let (rmssd, sdnn1, quality): (Option<f64>, Option<f64>, Option<i64>) = sqlx::query_as(
        "SELECT rmssd_ms, sdnn1_ms, hrv_quality FROM intraday \
         WHERE UNIX_TIMESTAMP(event_time) = 1777669200",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rmssd, Some(42.5));
    assert_eq!(sdnn1, Some(58.3));
    assert_eq!(quality, Some(2));

    // ── idempotency: second sync same data → same row counts ──────────────────
    let mut cursors2 = Cursors::default();
    run_sync(&cfg, &client, &pool, &mut cursors2, now)
        .await
        .unwrap();

    assert_eq!(count("measurements").await, 3);
    assert_eq!(count("daily_activity").await, 5);
    assert_eq!(count("sleep_sessions").await, 3);
    assert_eq!(count("workouts").await, 3);
    assert_eq!(count("intraday").await, 17);
    assert_eq!(count("devices").await, 2);
}

#[tokio::test]
async fn sync_activity_follows_pagination() {
    let Some(url) = db_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let pool = db::connect(&url).await.unwrap();
    clean_db(&pool).await;

    let server = MockServer::start().await;

    // Non-activity endpoints return empty bodies so run_sync completes.
    Mock::given(method("POST"))
        .and(path("/measure"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"measuregrps":[]}}"#),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/sleep"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":[]}}"#),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"devices":[]}}"#),
        )
        .mount(&server)
        .await;

    // getactivity: first call (no offset) → page1 (more=true, offset=2);
    // second call (offset=2) → page2 (more=false). getworkouts/getintraday empty.
    let page1 = fixture("getactivity_page1.json");
    let page2 = fixture("getactivity_page2.json");
    Mock::given(method("POST"))
        .and(path("/v2/measure"))
        .respond_with(move |req: &wiremock::Request| {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            if body.contains("getactivity") {
                if body.contains("offset=2") {
                    ResponseTemplate::new(200).set_body_string(page2.clone())
                } else {
                    ResponseTemplate::new(200).set_body_string(page1.clone())
                }
            } else if body.contains("getworkouts") {
                ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":[]}}"#)
            } else if body.contains("getintradayactivity") {
                ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":{}}}"#)
            } else {
                ResponseTemplate::new(404)
            }
        })
        .mount(&server)
        .await;

    let tokens = Tokens {
        access_token: "atk".into(),
        refresh_token: "rtk".into(),
        expires_at: i64::MAX,
        userid: "12345".into(),
        scope: "user.metrics".into(),
    };
    let cfg = test_cfg(&url);
    let client = WithingsClient::new(
        reqwest::Client::new(),
        cfg.client_id.clone(),
        cfg.client_secret.clone(),
        tokens,
    )
    .with_base_url(server.uri());

    let mut cursors = Cursors::default();
    let now = 1_800_000_000i64;
    run_sync(&cfg, &client, &pool, &mut cursors, now)
        .await
        .unwrap();

    // Both pages' records upserted (2 + 2 distinct dates).
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM daily_activity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 4, "records from both pages should be present");

    // Cursor advances to the max `modified` across all pages (page2's value).
    assert_eq!(cursors.activity, 1_772_300_000);
}

#[tokio::test]
async fn sync_activity_aborts_when_more_without_offset() {
    let Some(url) = db_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let pool = db::connect(&url).await.unwrap();
    clean_db(&pool).await;

    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/measure"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"measuregrps":[]}}"#),
        )
        .mount(&server)
        .await;

    // getactivity claims `more: true` but omits `offset` — the loop must abort
    // rather than silently stop and advance the cursor over unread records.
    let bad = fixture("getactivity_more_no_offset.json");
    Mock::given(method("POST"))
        .and(path("/v2/measure"))
        .respond_with(move |req: &wiremock::Request| {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            if body.contains("getactivity") {
                ResponseTemplate::new(200).set_body_string(bad.clone())
            } else {
                ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":[]}}"#)
            }
        })
        .mount(&server)
        .await;

    let tokens = Tokens {
        access_token: "atk".into(),
        refresh_token: "rtk".into(),
        expires_at: i64::MAX,
        userid: "12345".into(),
        scope: "user.metrics".into(),
    };
    let cfg = test_cfg(&url);
    let client = WithingsClient::new(
        reqwest::Client::new(),
        cfg.client_id.clone(),
        cfg.client_secret.clone(),
        tokens,
    )
    .with_base_url(server.uri());

    let mut cursors = Cursors::default();
    let now = 1_800_000_000i64;
    let err = run_sync(&cfg, &client, &pool, &mut cursors, now)
        .await
        .expect_err("run_sync should abort on more-without-offset");
    assert!(
        err.to_string().contains("offset") || format!("{err:#}").contains("offset"),
        "error should mention the missing offset, got: {err:#}"
    );
    // Cursor must not advance past the unread records.
    assert_eq!(cursors.activity, 0, "cursor must not advance on abort");
}

#[tokio::test]
async fn sync_measurements_follows_pagination() {
    let Some(url) = db_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let pool = db::connect(&url).await.unwrap();
    clean_db(&pool).await;

    let server = MockServer::start().await;

    // getmeas: first call (no offset) → page1 (more=true, offset=2);
    // second call (offset=2) → page2 (more=false).
    let page1 = fixture("getmeas_page1.json");
    let page2 = fixture("getmeas_page2.json");
    Mock::given(method("POST"))
        .and(path("/measure"))
        .respond_with(move |req: &wiremock::Request| {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            if body.contains("offset=2") {
                ResponseTemplate::new(200).set_body_string(page2.clone())
            } else {
                ResponseTemplate::new(200).set_body_string(page1.clone())
            }
        })
        .mount(&server)
        .await;

    // Remaining /v2/measure actions return empty bodies (each with its own
    // shape) so run_sync completes without exercising their pagination.
    Mock::given(method("POST"))
        .and(path("/v2/measure"))
        .respond_with(move |req: &wiremock::Request| {
            let body = std::str::from_utf8(&req.body).unwrap_or("");
            if body.contains("getactivity") {
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"status":0,"body":{"activities":[]}}"#)
            } else if body.contains("getintradayactivity") {
                ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":{}}}"#)
            } else {
                // getworkouts
                ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":[]}}"#)
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/sleep"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"series":[]}}"#),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v2/user"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(r#"{"status":0,"body":{"devices":[]}}"#),
        )
        .mount(&server)
        .await;

    let tokens = Tokens {
        access_token: "atk".into(),
        refresh_token: "rtk".into(),
        expires_at: i64::MAX,
        userid: "12345".into(),
        scope: "user.metrics".into(),
    };
    let cfg = test_cfg(&url);
    let client = WithingsClient::new(
        reqwest::Client::new(),
        cfg.client_id.clone(),
        cfg.client_secret.clone(),
        tokens,
    )
    .with_base_url(server.uri());

    let mut cursors = Cursors::default();
    let now = 1_800_000_000i64;
    run_sync(&cfg, &client, &pool, &mut cursors, now)
        .await
        .unwrap();

    // Both pages' records upserted (2 + 1 distinct grpids).
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM measurements")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 3, "records from both pages should be present");

    // Cursor advances to the max `modified` across all pages (page2's value).
    assert_eq!(cursors.measure, 1_775_200_300);
}

#[tokio::test]
async fn tokens_round_trip() {
    let Some(url) = db_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let pool = db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM state")
        .execute(&pool)
        .await
        .unwrap();

    let t = Tokens {
        access_token: "abc123".into(),
        refresh_token: "def456".into(),
        expires_at: 1_900_000_000,
        userid: "42".into(),
        scope: "user.metrics".into(),
    };
    state::save_tokens(&pool, &t).await.unwrap();
    let loaded = state::load_tokens(&pool).await.unwrap().unwrap();
    assert_eq!(loaded.access_token, t.access_token);
    assert_eq!(loaded.refresh_token, t.refresh_token);
    assert_eq!(loaded.expires_at, t.expires_at);
    assert_eq!(loaded.userid, t.userid);
}

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
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("tests/fixtures/{name}")).unwrap()
}

fn test_cfg(database_url: &str) -> Config {
    Config {
        client_id:     "CID".into(),
        client_secret: "SECRET".into(),
        database_url:  database_url.into(),
        backfill_days: 30,
        user_tz:       "UTC".into(),
    }
}

/// Returns None if DATABASE_URL is not set (skip).
fn db_url() -> Option<String> {
    std::env::var("DATABASE_URL").ok()
}

async fn clean_db(pool: &sqlx::MySqlPool) {
    for t in &["intraday", "workouts", "sleep_sessions", "daily_activity", "measurements", "state"] {
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
    let act_fix   = fixture("getactivity.json");
    let wkt_fix   = fixture("getworkouts.json");
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

    let tokens = Tokens {
        access_token:  "atk".into(),
        refresh_token: "rtk".into(),
        expires_at:    i64::MAX,
        userid:        "12345".into(),
        scope:         "user.metrics".into(),
    };
    let http = reqwest::Client::new();
    let cfg = test_cfg(&url);
    let client =
        WithingsClient::new(http, cfg.client_id.clone(), cfg.client_secret.clone(), tokens)
            .with_base_url(server.uri());

    let mut cursors = Cursors::default();
    let now = 1_800_000_000i64;
    run_sync(&cfg, &client, &pool, &mut cursors, now).await.unwrap();

    // All cursors advanced
    assert!(cursors.measure  > 0, "measure cursor");
    assert!(cursors.activity > 0, "activity cursor");
    assert!(cursors.sleep    > 0, "sleep cursor");
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

    assert_eq!(count("measurements").await,   3);
    assert_eq!(count("daily_activity").await, 5);
    assert_eq!(count("sleep_sessions").await, 3);
    assert_eq!(count("workouts").await,       3);
    assert_eq!(count("intraday").await,       15);

    // ── idempotency: second sync same data → same row counts ──────────────────
    let mut cursors2 = Cursors::default();
    run_sync(&cfg, &client, &pool, &mut cursors2, now).await.unwrap();

    assert_eq!(count("measurements").await,   3);
    assert_eq!(count("daily_activity").await, 5);
    assert_eq!(count("sleep_sessions").await, 3);
    assert_eq!(count("workouts").await,       3);
    assert_eq!(count("intraday").await,       15);
}

#[tokio::test]
async fn tokens_round_trip() {
    let Some(url) = db_url() else {
        eprintln!("SKIP: DATABASE_URL not set");
        return;
    };

    let pool = db::connect(&url).await.unwrap();
    sqlx::query("DELETE FROM state").execute(&pool).await.unwrap();

    let t = Tokens {
        access_token:  "abc123".into(),
        refresh_token: "def456".into(),
        expires_at:    1_900_000_000,
        userid:        "42".into(),
        scope:         "user.metrics".into(),
    };
    state::save_tokens(&pool, &t).await.unwrap();
    let loaded = state::load_tokens(&pool).await.unwrap().unwrap();
    assert_eq!(loaded.access_token,  t.access_token);
    assert_eq!(loaded.refresh_token, t.refresh_token);
    assert_eq!(loaded.expires_at,    t.expires_at);
    assert_eq!(loaded.userid,        t.userid);
}

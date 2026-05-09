# Intraday Dynamic Lookback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix overnight data gaps by anchoring intraday sync start on the last actual data timestamp from the DB rather than a fixed 4-hour cursor offset.

**Architecture:** Extract `intraday_chunk_start` as a pure testable helper (alongside `since_or_backfill`). Query `MAX(event_time)` from the `intraday` table once before the loop and pass the result into the helper. No schema, state, or config changes.

**Tech Stack:** Rust, sqlx 0.8 (dynamic queries), MySQL/MariaDB

---

### Task 1: Extract and test `intraday_chunk_start` helper

**Files:**
- Modify: `src/cmd/sync.rs` (helpers section ~line 507, tests section ~line 530)

- [ ] **Step 1: Write the three failing unit tests**

Add to the `#[cfg(test)] mod tests` block in `src/cmd/sync.rs` (after the existing two tests):

```rust
#[test]
fn intraday_chunk_start_gap_larger_than_lookback() {
    // last data 6h ago, cursor advanced past gap — should start at last data
    let now = 1_700_000_000i64;
    let last_data = now - 6 * 3600;
    assert_eq!(
        intraday_chunk_start(now, Some(last_data), &cfg(), now),
        last_data
    );
}

#[test]
fn intraday_chunk_start_no_gap() {
    // last data 1h ago — lookback floor (4h) wins
    let now = 1_700_000_000i64;
    let last_data = now - 3600;
    assert_eq!(
        intraday_chunk_start(now, Some(last_data), &cfg(), now),
        now - INTRADAY_LOOKBACK_SECS
    );
}

#[test]
fn intraday_chunk_start_empty_table() {
    // NULL MAX (no rows) with cursor==0 — full backfill
    let now = 1_700_000_000i64;
    assert_eq!(
        intraday_chunk_start(0, None, &cfg(), now),
        now - 30 * 86400
    );
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --lib --bins intraday_chunk_start 2>&1
```

Expected: compile error — `intraday_chunk_start` not defined yet.

- [ ] **Step 3: Add the helper function**

Add to the `// ── helpers ───` section in `src/cmd/sync.rs`, after `since_or_backfill`:

```rust
pub fn intraday_chunk_start(cursor: i64, last_data_ts: Option<i64>, cfg: &Config, now: i64) -> i64 {
    match (cursor, last_data_ts) {
        (0, _) | (_, None) => now - cfg.backfill_days * 86400,
        (_, Some(last)) => last.min(now - INTRADAY_LOOKBACK_SECS),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cargo test --lib --bins intraday_chunk_start 2>&1
```

Expected output (3 tests passing):
```
test cmd::sync::tests::intraday_chunk_start_empty_table ... ok
test cmd::sync::tests::intraday_chunk_start_gap_larger_than_lookback ... ok
test cmd::sync::tests::intraday_chunk_start_no_gap ... ok

test result: ok. 3 passed; 0 failed; ...
```

- [ ] **Step 5: Run full test suite to check for regressions**

```bash
cargo test --lib --bins 2>&1 | tail -5
```

Expected: `test result: ok. 27 passed; 0 failed;`

- [ ] **Step 6: Commit**

```bash
git add src/cmd/sync.rs
git commit -m "feat: extract intraday_chunk_start helper with tests"
```

---

### Task 2: Wire DB query into `sync_intraday`

**Files:**
- Modify: `src/cmd/sync.rs` (function `sync_intraday` ~line 418)

- [ ] **Step 1: Replace the `chunk_start` calculation in `sync_intraday`**

Find this block (lines ~425–429):

```rust
    let mut chunk_start = if cursors.intraday > 0 {
        cursors.intraday.saturating_sub(INTRADAY_LOOKBACK_SECS)
    } else {
        now - cfg.backfill_days * 86400
    };
```

Replace it with:

```rust
    let last_data_ts: Option<i64> =
        sqlx::query_scalar("SELECT UNIX_TIMESTAMP(MAX(event_time)) FROM intraday")
            .fetch_one(pool)
            .await?;

    let mut chunk_start = intraday_chunk_start(cursors.intraday, last_data_ts, cfg, now);
```

- [ ] **Step 2: Verify it compiles cleanly**

```bash
cargo build 2>&1
```

Expected: no errors, no warnings.

- [ ] **Step 3: Run clippy**

```bash
cargo clippy --all-targets -- -D warnings 2>&1
```

Expected: no warnings.

- [ ] **Step 4: Run full test suite**

```bash
cargo test --lib --bins 2>&1 | tail -5
```

Expected: `test result: ok. 27 passed; 0 failed;`

- [ ] **Step 5: Commit**

```bash
git add src/cmd/sync.rs
git commit -m "fix: use last intraday data timestamp to recover overnight gaps"
```

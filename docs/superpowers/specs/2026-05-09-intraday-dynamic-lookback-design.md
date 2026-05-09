# Intraday Dynamic Lookback Design

**Date:** 2026-05-09  
**Status:** Approved

## Problem

`cursor.intraday` advances through empty time chunks (gaps in data), so the fixed
4-hour lookback (`cursor - INTRADAY_LOOKBACK_SECS`) misses overnight gaps longer
than 4 hours. When the watch stops syncing to the Withings cloud overnight, the
cursor moves past the gap and the next sync starts too late to recover the missing
samples.

## Solution

Before starting the intraday pagination loop, query the actual last data timestamp
from the `intraday` table. Use `min(last_data_ts, now - INTRADAY_LOOKBACK_SECS)`
as `chunk_start` instead of the cursor-relative fixed lookback.

Formula:
- Normal run (no gap, last data < 4h ago): `now - 4h` wins — behaviour unchanged
- Overnight gap (last data > 4h ago): `last_data_ts` wins — starts at actual last data

## Changes

### `src/cmd/sync.rs`

Replace the `chunk_start` calculation at the top of `sync_intraday`:

```rust
// query last actual data point
let last_data_ts: Option<i64> = sqlx::query_scalar(
    "SELECT UNIX_TIMESTAMP(MAX(event_time)) FROM intraday"
)
.fetch_one(pool)
.await?;

let chunk_start = match (cursors.intraday, last_data_ts) {
    (0, _) | (_, None) => now - cfg.backfill_days * 86400,
    (_, Some(last)) => last.min(now - INTRADAY_LOOKBACK_SECS),
};
```

`INTRADAY_LOOKBACK_SECS` (currently `4 * 3600`) is unchanged.

### No other changes

- No schema migrations
- No new state keys
- No config changes

## Edge Cases

| Case | Behaviour |
|---|---|
| `MAX(event_time)` = NULL (empty table) | Falls back to `now - backfill_days` |
| Last data < 4h ago (normal, no gap) | `now - 4h` lookback floor wins |
| Last data > 4h ago (overnight gap) | `last_data_ts` wins — gap covered |
| DB query fails | Error propagates via `?`; sync aborts and retries next run |
| `cursor == 0` with data in DB | Cursor-zero branch wins; full `backfill_days` used |

## Tests

Extract `chunk_start` calculation into a testable pure helper, then add unit tests:

1. **Gap > 4h:** `last_data = now - 6h`, cursor advanced past gap → `chunk_start = now - 6h`
2. **No gap:** `last_data = now - 1h` → `chunk_start = now - 4h` (floor wins)
3. **Empty table (NULL MAX):** cursor=0 → `chunk_start = now - backfill_days`

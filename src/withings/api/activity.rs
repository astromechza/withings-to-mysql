use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ActivityBody {
    pub activities: Vec<DailyActivity>,
    /// `true`/`1` when more pages remain; re-request with `offset`.
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub more: Option<i64>,
    /// Value to pass as the `offset` param to fetch the next page.
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DailyActivity {
    pub date: String,
    pub timezone: String,
    #[serde(deserialize_with = "super::de_bool_as_i64")]
    pub modified: i64,
    #[serde(default)]
    pub steps: Option<u64>,
    #[serde(default)]
    pub distance: Option<f64>,
    #[serde(default)]
    pub calories: Option<f64>,
    #[serde(default)]
    pub totalcalories: Option<f64>,
    #[serde(default)]
    pub deviceid: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub is_tracker: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::withings::api::unwrap_envelope;

    #[test]
    fn decodes_real_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/getactivity.json").unwrap();
        let body: ActivityBody = unwrap_envelope(&raw).unwrap();
        assert!(!body.activities.is_empty());
        let a = &body.activities[0];
        assert!(a.steps.is_some());
        assert!(!a.date.is_empty());
        // Real fixture omits pagination fields → default to None.
        assert_eq!(body.more, None);
        assert_eq!(body.offset, None);
    }

    #[test]
    fn decodes_paginated_page1() {
        // First page signals more data via `more: true` + `offset`.
        let raw = std::fs::read_to_string("tests/fixtures/getactivity_page1.json").unwrap();
        let body: ActivityBody = unwrap_envelope(&raw).unwrap();
        assert_eq!(body.activities.len(), 2);
        assert_eq!(body.more, Some(1));
        assert_eq!(body.offset, Some(2));
    }

    #[test]
    fn decodes_paginated_final_page() {
        // Final page: `more: false` (→ None) ends the loop.
        let raw = std::fs::read_to_string("tests/fixtures/getactivity_page2.json").unwrap();
        let body: ActivityBody = unwrap_envelope(&raw).unwrap();
        assert_eq!(body.activities.len(), 2);
        assert_eq!(body.more, None);
    }
}

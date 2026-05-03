use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ActivityBody {
    pub activities: Vec<DailyActivity>,
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub more: Option<i64>,
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
    }
}

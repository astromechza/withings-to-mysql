use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct SleepBody {
    pub series: Vec<SleepNight>,
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub more: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SleepNight {
    pub id: i64,
    pub timezone: String,
    pub startdate: i64,
    pub enddate: i64,
    pub date: String,
    #[serde(deserialize_with = "super::de_bool_as_i64")]
    pub created: i64,
    #[serde(deserialize_with = "super::de_bool_as_i64")]
    pub modified: i64,
    #[serde(default)]
    pub completed: Option<bool>,
    pub data: SleepData,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct SleepData {
    #[serde(default)]
    pub lightsleepduration: Option<i64>,
    #[serde(default)]
    pub deepsleepduration: Option<i64>,
    #[serde(default)]
    pub remsleepduration: Option<i64>,
    #[serde(default)]
    pub wakeupduration: Option<i64>,
    #[serde(default)]
    pub wakeupcount: Option<i64>,
    #[serde(default)]
    pub durationtosleep: Option<i64>,
    #[serde(default)]
    pub durationtowakeup: Option<i64>,
    #[serde(default)]
    pub hr_average: Option<f64>,
    #[serde(default)]
    pub hr_min: Option<f64>,
    #[serde(default)]
    pub hr_max: Option<f64>,
}

impl SleepNight {
    pub fn duration_seconds(&self) -> i64 {
        self.enddate - self.startdate
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::withings::api::unwrap_envelope;

    #[test]
    fn decodes_real_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/getsleep.json").unwrap();
        let body: SleepBody = unwrap_envelope(&raw).unwrap();
        assert!(!body.series.is_empty());
        assert!(body.series[0].duration_seconds() > 0);
    }
}

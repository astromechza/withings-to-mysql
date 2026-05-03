use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct WorkoutsBody {
    pub series: Vec<Workout>,
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub more: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Workout {
    pub id: i64,
    pub category: i64,
    pub attrib: i64,
    pub startdate: i64,
    pub enddate: i64,
    #[serde(deserialize_with = "super::de_bool_as_i64")]
    pub modified: i64,
    pub timezone: String,
    #[serde(default)]
    pub date: Option<String>,
    pub data: WorkoutData,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct WorkoutData {
    #[serde(default)]
    pub calories: Option<f64>,
    #[serde(default)]
    pub intensity: Option<f64>,
    #[serde(default)]
    pub manual_distance: Option<f64>,
    #[serde(default)]
    pub manual_calories: Option<f64>,
    #[serde(default)]
    pub hr_average: Option<f64>,
    #[serde(default)]
    pub hr_min: Option<f64>,
    #[serde(default)]
    pub hr_max: Option<f64>,
    #[serde(default)]
    pub hr_zone_0: Option<f64>,
    #[serde(default)]
    pub hr_zone_1: Option<f64>,
    #[serde(default)]
    pub hr_zone_2: Option<f64>,
    #[serde(default)]
    pub hr_zone_3: Option<f64>,
    #[serde(default)]
    pub pause_duration: Option<f64>,
    #[serde(default)]
    pub steps: Option<f64>,
    #[serde(default)]
    pub distance: Option<f64>,
    #[serde(default)]
    pub elevation: Option<f64>,
    #[serde(default)]
    pub spo2_average: Option<f64>,
}

impl Workout {
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
        let raw = std::fs::read_to_string("tests/fixtures/getworkouts.json").unwrap();
        let body: WorkoutsBody = unwrap_envelope(&raw).unwrap();
        assert!(!body.series.is_empty());
        assert!(body.series[0].duration_seconds() > 0);
    }
}

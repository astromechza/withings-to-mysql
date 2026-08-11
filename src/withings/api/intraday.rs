use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Clone)]
pub struct IntradayBody {
    /// Object keyed by unix-ts string (NOT an array).
    pub series: BTreeMap<String, IntradaySample>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IntradaySample {
    #[serde(default)]
    pub heart_rate: Option<f64>,
    #[serde(default)]
    pub steps: Option<u64>,
    #[serde(default)]
    pub elevation: Option<f64>,
    #[serde(default)]
    pub calories: Option<f64>,
    #[serde(default)]
    pub distance: Option<f64>,
    #[serde(default)]
    pub spo2_auto: Option<f64>,
    #[serde(default)]
    pub duration: Option<i64>,
    #[serde(default, rename = "core_body_temperature")]
    pub core_body_temperature_celsius: Option<f64>,
    #[serde(default, rename = "rmssd")]
    pub rmssd_ms: Option<f64>,
    #[serde(default, rename = "sdnn1")]
    pub sdnn1_ms: Option<f64>,
    #[serde(default)]
    pub hrv_quality: Option<i64>,
}

impl IntradayBody {
    pub fn samples_sorted(&self) -> Vec<(i64, &IntradaySample)> {
        let mut v: Vec<(i64, &IntradaySample)> = self
            .series
            .iter()
            .filter_map(|(k, v)| k.parse::<i64>().ok().map(|t| (t, v)))
            .collect();
        v.sort_by_key(|(t, _)| *t);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::withings::api::unwrap_envelope;

    #[test]
    fn decodes_real_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/getintraday.json").unwrap();
        let body: IntradayBody = unwrap_envelope(&raw).unwrap();
        let samples = body.samples_sorted();
        assert!(samples.len() >= 5);
        let times: Vec<i64> = samples.iter().map(|(t, _)| *t).collect();
        let mut sorted = times.clone();
        sorted.sort();
        assert_eq!(times, sorted);
        assert!(samples.iter().any(|(_, s)| s.heart_rate.is_some()));
        let temp_sample = samples
            .iter()
            .find(|(_, s)| s.core_body_temperature_celsius.is_some())
            .expect("fixture should contain a core_body_temperature sample");
        let v = temp_sample.1.core_body_temperature_celsius.unwrap();
        assert!((v - 37.243).abs() < 1e-6);

        let hrv_sample = samples
            .iter()
            .find(|(_, s)| s.rmssd_ms.is_some())
            .expect("fixture should contain an HRV sample");
        let s = hrv_sample.1;
        assert!((s.rmssd_ms.unwrap() - 42.5).abs() < 1e-6);
        assert!((s.sdnn1_ms.unwrap() - 58.3).abs() < 1e-6);
        assert_eq!(s.hrv_quality, Some(2));
    }
}

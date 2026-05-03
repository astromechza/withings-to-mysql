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
    }
}

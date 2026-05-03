use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct MeasureBody {
    #[serde(default)]
    pub updatetime: i64,
    #[serde(default)]
    pub timezone: String,
    pub measuregrps: Vec<MeasureGroup>,
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub more: Option<i64>,
    #[serde(default, deserialize_with = "super::de_bool_as_none_i64")]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MeasureGroup {
    pub grpid: i64,
    pub attrib: i64,
    pub date: i64,
    #[serde(deserialize_with = "super::de_bool_as_i64")]
    pub created: i64,
    #[serde(deserialize_with = "super::de_bool_as_i64")]
    pub modified: i64,
    pub category: i64,
    #[serde(default)]
    pub deviceid: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub measures: Vec<Measure>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Measure {
    pub value: i64,
    #[serde(rename = "type")]
    pub kind: i64,
    pub unit: i32,
}

impl Measure {
    pub fn real(&self) -> f64 {
        (self.value as f64) * 10f64.powi(self.unit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::withings::api::unwrap_envelope;

    #[test]
    fn decodes_real_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/getmeas.json").unwrap();
        let body: MeasureBody = unwrap_envelope(&raw).unwrap();
        assert!(!body.measuregrps.is_empty());
        let g = &body.measuregrps[0];
        assert!(!g.measures.is_empty());
    }

    #[test]
    fn real_value_unit_scaling() {
        let m = Measure {
            value: 82500,
            kind: 1,
            unit: -3,
        };
        let v = m.real();
        assert!((v - 82.5).abs() < 1e-9);
    }
}

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct DeviceBody {
    pub devices: Vec<Device>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Device {
    pub deviceid: String,
    // Required for a useful record — sync_devices skips the row if any are None.
    #[serde(rename = "type")]
    pub device_type: Option<String>,
    pub model: Option<String>,
    pub model_id: Option<i64>,
    // Genuinely optional fields
    pub battery: Option<String>,
    pub last_session_date: Option<i64>,
    pub timezone: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::withings::api::unwrap_envelope;

    #[test]
    fn decodes_real_fixture() {
        let raw = std::fs::read_to_string("tests/fixtures/getdevice.json").unwrap();
        let body: DeviceBody = unwrap_envelope(&raw).unwrap();
        assert!(!body.devices.is_empty());
        assert!(body.devices[0].model.is_some());
        assert!(body.devices[0].battery.is_some());
    }
}

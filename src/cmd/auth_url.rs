use anyhow::Result;
use crate::withings::client::authorize_url;

pub fn run(client_id: &str, redirect_uri: &str, scope: &str, state: Option<&str>) -> Result<()> {
    let st = state.map(str::to_string).unwrap_or_else(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        format!("state-{ts}")
    });
    println!("{}", authorize_url(client_id, redirect_uri, scope, &st));
    eprintln!("# state={st}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_does_not_panic() {
        run("CID", "https://example.com/cb", "user.metrics", Some("s1")).unwrap();
    }
}

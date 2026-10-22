//! Runs the `stravacz` binary against the mock canteen.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;
use stravacz_core::api::mock;

struct Cli {
    base_url: String,
    config_home: PathBuf,
}

impl Cli {
    fn new(base_url: String, name: &str) -> Self {
        let config_home =
            std::env::temp_dir().join(format!("stravacz-cli-test-{}-{name}", std::process::id()));
        Self {
            base_url,
            config_home,
        }
    }

    /// Run with the account passed through the environment, so neither the
    /// user's config nor the keyring is touched.
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_stravacz"))
            .args(args)
            .env("XDG_CONFIG_HOME", &self.config_home)
            .env("STRAVA_BASE_URL", &self.base_url)
            .env("STRAVA_CANTEEN", "1234")
            .env("STRAVA_USERNAME", "jan.novak")
            .env("STRAVA_PASSWORD", "secret")
            .output()
            .expect("stravacz runs")
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(&[args, &["--json"]].concat());
        assert!(
            output.status.success(),
            "{args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("JSON output")
    }

    /// Ids of the ordered meals.
    fn ordered(&self) -> Vec<Value> {
        meals(&self.json(&["menu", "--all", "--ordered"]))
            .map(|meal| meal["id"].clone())
            .collect()
    }
}

impl Drop for Cli {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.config_home);
    }
}

fn meals(menu: &Value) -> impl Iterator<Item = &Value> {
    menu["days"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|day| day["meals"].as_array().unwrap())
}

#[tokio::test(flavor = "multi_thread")]
async fn orders_and_cancels_a_meal() {
    let server = mock::start().await;
    let cli = Cli::new(server.uri(), "order");

    let menu = cli.json(&["menu", "--all"]);
    let meal = meals(&menu)
        .find(|meal| {
            meal["kind"] == "main"
                && meal["order_restriction"] == "none"
                && meal["ordered"] == false
        })
        .expect("an orderable lunch");
    let id = meal["id"].clone();
    let arg = id.to_string();

    let dry_run = cli.json(&["order", &arg, "--dry-run"]);
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(dry_run["changes"][0]["meal"], id);
    assert!(!cli.ordered().contains(&id));

    let order = cli.json(&["order", &arg]);
    assert_eq!(order["verified"], true);
    assert_eq!(order["changes"][0]["applied"], true);
    assert!(cli.ordered().contains(&id));

    let again = cli.json(&["order", &arg]);
    assert_eq!(again["changes"], Value::Array(Vec::new()));
    assert_eq!(again["unchanged"][0], id);

    let cancel = cli.json(&["cancel", &arg]);
    assert_eq!(cancel["changes"][0]["applied"], true);
    assert!(!cli.ordered().contains(&id));
}

#[tokio::test(flavor = "multi_thread")]
async fn shows_the_account_and_menu() {
    let server = mock::start().await;
    let cli = Cli::new(server.uri(), "account");

    let account = cli.json(&["account"]);
    assert_eq!(account["full_name"], "Jan Novák");
    assert_eq!(account["balance"], 512.5);

    let output = cli.run(&["menu", "--all"]);
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Oběd 1"), "{text}");
    assert!(!text.contains('\x1b'), "no colors when piped");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_unknown_meals() {
    let server = mock::start().await;
    let cli = Cli::new(server.uri(), "unknown");

    let output = cli.run(&["order", "999999"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("not on the menu"), "{stderr}");
}

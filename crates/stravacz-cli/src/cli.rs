use chrono::{Datelike, Local, NaiveDate, TimeDelta, Weekday};
use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

/// Order school canteen meals from Strava.cz.
#[derive(Debug, Parser)]
#[command(
    name = "stravacz",
    version,
    propagate_version = true,
    styles = crate::style::help(),
    after_help = "Run `stravacz login` once; the password is kept in the system keyring.\n\
                  For scripts, set STRAVA_PASSWORD (and STRAVA_CANTEEN, STRAVA_USERNAME)."
)]
pub struct Cli {
    #[command(flatten)]
    pub account: AccountArgs,

    /// Print JSON instead of text
    #[arg(long, global = true)]
    pub json: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// Which account to use; defaults to the one remembered by `stravacz login`.
#[derive(Debug, Args)]
pub struct AccountArgs {
    /// Canteen number
    #[arg(long, global = true, env = "STRAVA_CANTEEN")]
    pub canteen: Option<String>,

    /// Username
    #[arg(long, short, global = true, env = "STRAVA_USERNAME")]
    pub username: Option<String>,

    /// Talk to a different server, e.g. the mock server
    #[arg(long, global = true, env = "STRAVA_BASE_URL", hide = true)]
    pub base_url: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in and remember the account
    Login {
        /// Don't store the password in the system keyring
        #[arg(long)]
        no_save_password: bool,
    },

    /// Forget the remembered account and its password
    Logout,

    /// Show the menu
    #[command(visible_alias = "ls")]
    Menu(MenuArgs),

    /// Order meals
    Order(ChangeArgs),

    /// Cancel ordered meals
    Cancel(ChangeArgs),

    /// Show the account and its balance
    #[command(visible_alias = "balance")]
    Account,

    /// Print shell completions
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Args)]
pub struct MenuArgs {
    /// First day to show: today, tomorrow, a weekday (mon…sun) or YYYY-MM-DD
    #[arg(value_parser = parse_day, default_value = "today")]
    pub from: NaiveDate,

    /// Number of days to show
    #[arg(short = 'n', long, default_value_t = 5)]
    pub days: usize,

    /// Show every published day, including past ones and days without service
    #[arg(short, long, conflicts_with = "days")]
    pub all: bool,

    /// Only show ordered meals
    #[arg(short, long)]
    pub ordered: bool,
}

#[derive(Debug, Args)]
pub struct ChangeArgs {
    /// Meal ids, as listed by `stravacz menu`
    #[arg(required = true, value_name = "MEAL_ID")]
    pub meals: Vec<u32>,

    /// Show what would change without saving it
    #[arg(long)]
    pub dry_run: bool,
}

/// Parse `today`, `tomorrow`, a weekday (its next occurrence) or an ISO date.
fn parse_day(text: &str) -> Result<NaiveDate, String> {
    let today = Local::now().date_naive();
    match text.to_ascii_lowercase().as_str() {
        "today" => Ok(today),
        "tomorrow" => Ok(today + TimeDelta::days(1)),
        other => {
            if let Ok(weekday) = other.parse::<Weekday>() {
                let ahead = (weekday.num_days_from_monday() + 7
                    - today.weekday().num_days_from_monday())
                    % 7;
                Ok(today + TimeDelta::days(i64::from(ahead)))
            } else {
                other.parse().map_err(|_| {
                    format!("`{text}` is not a day; use today, tomorrow, mon…sun or YYYY-MM-DD")
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    #[test]
    fn cli_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn parses_days() {
        let today = Local::now().date_naive();
        assert_eq!(parse_day("today"), Ok(today));
        assert_eq!(parse_day("Tomorrow"), Ok(today + TimeDelta::days(1)));
        assert_eq!(
            parse_day("2026-09-02"),
            Ok(NaiveDate::from_ymd_opt(2026, 9, 2).unwrap())
        );

        let monday = parse_day("mon").unwrap();
        assert_eq!(monday.weekday(), Weekday::Mon);
        assert!((0..7).contains(&(monday - today).num_days()));

        assert!(parse_day("someday").is_err());
    }
}

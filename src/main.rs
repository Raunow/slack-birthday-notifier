use chrono::{DateTime, Datelike, Duration, Utc};
use colored::Colorize;
use csv::{DeserializeRecordsIter, Reader};
use reqwest::blocking;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;
use toml;

#[derive(Deserialize)]
struct Config {
    warning: Warning,
    csv: CSV,
    slack: Slack,
}

#[derive(Deserialize)]
struct Slack {
    enabled: bool,
    channel_id: Option<String>,
    webhook_url: Option<String>,
}
#[derive(Deserialize)]
struct Warning {
    enabled: bool,
    channel_id: Option<String>,
    webhook_url: Option<String>,
    number_of_days_warning: u8,
}

#[derive(Deserialize)]
struct CSV {
    path: PathBuf,
    date_separator: char,
    date_format: DateFormat,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum DateFormat {
    MonthDay,
    DayMonth,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
struct BirthdayRow {
    date: String,
    tag: String,
}

fn read_config() -> Config {
    let contents = fs::read_to_string("config.toml").expect("Failed to read config.toml");
    toml::from_str(&contents).expect("Unable to deserialize config.toml")
}

fn get_date_str(epoch_time: DateTime<Utc>, cfg: &CSV) -> String {
    let month = epoch_time.month();
    let day = epoch_time.day();

    if cfg.date_format == DateFormat::MonthDay {
        format!("{:0>2}{}{:0>2}", month, cfg.date_separator, day)
    } else {
        format!("{:0>2}{}{:0>2}", day, cfg.date_separator, month)
    }
}

fn match_dates(
    iter: DeserializeRecordsIter<fs::File, BirthdayRow>,
    warning_days: u8,
    cfg: &CSV,
) -> (Vec<BirthdayRow>, Vec<BirthdayRow>) {
    let cur_epoch_time = Utc::now();
    let current_date = get_date_str(cur_epoch_time, &cfg);
    let warning_date = get_date_str(cur_epoch_time + Duration::days(warning_days as i64), &cfg);

    let mut current_birthdays: Vec<BirthdayRow> = Vec::new();
    let mut upcoming_birthdays: Vec<BirthdayRow> = Vec::new();

    for result in iter {
        let record: BirthdayRow = result.expect("Deserialized CSV record");
        if record.date == current_date {
            current_birthdays.push(record);
        } else if record.date == warning_date {
            upcoming_birthdays.push(record);
        }
    }

    (current_birthdays, upcoming_birthdays)
}

fn slack_format(tags: Vec<BirthdayRow>, birthday_message: &str) -> String {
    format!(
        "{}<@{}>",
        birthday_message,
        tags.iter()
            .map(|b| b.tag.to_string())
            .collect::<Vec<String>>()
            .join(">, <@")
    )
}

fn slack_send(message: &str, webhook_url: String, channel_id: String) {
    let payload = serde_json::json!({
        "text": message,
        "channel": channel_id,
    });

    let client = blocking::Client::new();
    client.post(webhook_url).json(&payload).send().unwrap();
}

fn main() {
    let cfg = read_config();
    let mut reader = Reader::from_path(&cfg.csv.path).expect("Unable to read file");

    let iter = reader.deserialize();
    let (current_birthdays, upcoming_birthdays) =
        match_dates(iter, cfg.warning.number_of_days_warning, &cfg.csv);

    if !current_birthdays.is_empty() {
        let birthday_message = if current_birthdays.len() == 1 {
            "Happy birthday"
        } else if current_birthdays.len() == 2 {
            "Happy birthday to you both!"
        } else {
            "Happy birthday to you all!"
        };

        let message = slack_format(current_birthdays, birthday_message);
        println!("{}", &message.yellow());
        if cfg.slack.enabled {
            if let (Some(url), Some(channel_id)) = (cfg.slack.webhook_url, cfg.slack.channel_id) {
                slack_send(&message, url, channel_id);
            }
        };
    }

    if !upcoming_birthdays.is_empty() {
        let message = slack_format(
            upcoming_birthdays,
            format!(
                "Birthdays {} days from now: ",
                cfg.warning.number_of_days_warning
            )
            .as_str(),
        );
        println!("{}", &message.yellow());
        if cfg.warning.enabled {
            if let (Some(url), Some(channel_id)) = (cfg.warning.webhook_url, cfg.warning.channel_id)
            {
                slack_send(&message, url, channel_id);
            }
        };
    }
}

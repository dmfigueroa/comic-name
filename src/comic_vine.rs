use std::cmp::Ordering;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Deserializer};

const API_BASE: &str = "https://comicvine.gamespot.com/api";

#[derive(Clone, Debug, Deserialize)]
pub struct Publisher {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Volume {
    pub id: u64,
    pub name: String,
    #[serde(default, deserialize_with = "optional_year")]
    pub start_year: Option<i32>,
    pub publisher: Option<Publisher>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Issue {
    pub id: u64,
    pub issue_number: String,
    pub name: Option<String>,
    pub cover_date: Option<String>,
}

#[derive(Deserialize)]
struct VolumeResponse {
    status_code: u16,
    error: String,
    number_of_total_results: usize,
    results: Vec<Volume>,
}

#[derive(Deserialize)]
struct IssueResponse {
    status_code: u16,
    error: String,
    number_of_total_results: usize,
    results: Vec<Issue>,
}

pub fn search_volumes(api_key: &str, query: &str) -> Result<Vec<Volume>> {
    if api_key.trim().is_empty() {
        bail!("Add your ComicVine API key in Preferences first");
    }
    if query.trim().is_empty() {
        bail!("Enter a series name");
    }

    let agent = http_agent();
    let mut volumes = Vec::new();
    loop {
        let offset = volumes.len().to_string();
        let response: VolumeResponse = agent
            .get(&format!("{API_BASE}/search/"))
            .set("User-Agent", "ComicName/0.1 (comic metadata organizer)")
            .query("api_key", api_key.trim())
            .query("format", "json")
            .query("resources", "volume")
            .query("field_list", "id,name,start_year,publisher")
            .query("limit", "10")
            .query("offset", &offset)
            .query("query", query.trim())
            .call()
            .context("Could not contact ComicVine")?
            .into_json()
            .context("ComicVine returned an invalid response")?;
        validate(response.status_code, &response.error)?;
        let total = response.number_of_total_results;
        let received = response.results.len();
        volumes.extend(response.results);
        if received == 0 || volumes.len() >= total {
            break;
        }
    }
    Ok(volumes)
}

pub fn issues_for_volume(api_key: &str, volume_id: u64) -> Result<Vec<Issue>> {
    let agent = http_agent();
    let mut issues = Vec::new();
    loop {
        let offset = issues.len().to_string();
        let response: IssueResponse = agent
            .get(&format!("{API_BASE}/issues/"))
            .set("User-Agent", "ComicName/0.1 (comic metadata organizer)")
            .query("api_key", api_key.trim())
            .query("format", "json")
            .query("filter", &format!("volume:{volume_id}"))
            .query("field_list", "id,issue_number,name,cover_date")
            .query("limit", "100")
            .query("offset", &offset)
            .call()
            .context("Could not contact ComicVine")?
            .into_json()
            .context("ComicVine returned an invalid response")?;
        validate(response.status_code, &response.error)?;
        let total = response.number_of_total_results;
        let received = response.results.len();
        issues.extend(response.results);
        if received == 0 || issues.len() >= total {
            break;
        }
    }
    issues.sort_by(|left, right| compare_issue_numbers(&left.issue_number, &right.issue_number));
    Ok(issues)
}

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .build()
}

fn validate(status_code: u16, error: &str) -> Result<()> {
    if status_code == 1 {
        Ok(())
    } else {
        bail!("ComicVine: {error}")
    }
}

fn compare_issue_numbers(left: &str, right: &str) -> Ordering {
    match (numeric_prefix(left), numeric_prefix(right)) {
        (Some((left_number, left_suffix)), Some((right_number, right_suffix))) => left_number
            .total_cmp(&right_number)
            .then_with(|| {
                left_suffix
                    .to_ascii_lowercase()
                    .cmp(&right_suffix.to_ascii_lowercase())
            })
            .then_with(|| left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase()),
    }
}

fn numeric_prefix(value: &str) -> Option<(f64, &str)> {
    let value = value.trim();
    let (numerator, suffix) = decimal_prefix(value)?;
    if let Some(denominator) = suffix.strip_prefix('/') {
        let (denominator, suffix) = decimal_prefix(denominator)?;
        if denominator != 0.0 {
            return Some((numerator / denominator, suffix));
        }
    }
    Some((numerator, suffix))
}

fn decimal_prefix(value: &str) -> Option<(f64, &str)> {
    let mut end = 0;
    let mut has_digit = false;
    let mut has_decimal = false;
    for (index, character) in value.char_indices() {
        let accepted = if character.is_ascii_digit() {
            has_digit = true;
            true
        } else if (character == '+' || character == '-') && index == 0 {
            true
        } else if character == '.' && !has_decimal {
            has_decimal = true;
            true
        } else {
            false
        };
        if !accepted {
            break;
        }
        end = index + character.len_utf8();
    }
    has_digit
        .then(|| {
            value[..end]
                .parse()
                .ok()
                .map(|number| (number, &value[end..]))
        })
        .flatten()
}

fn optional_year<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Year {
        Number(i32),
        Text(String),
    }

    match Option::<Year>::deserialize(deserializer)? {
        Some(Year::Number(year)) => Ok(Some(year)),
        Some(Year::Text(year)) => year.parse().map(Some).map_err(serde::de::Error::custom),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comic_vine_string_year_is_supported() {
        let volume: Volume = serde_json::from_str(
            r#"{"id":1,"name":"Batman","start_year":"2014","publisher":null}"#,
        )
        .unwrap();

        assert_eq!(volume.start_year, Some(2014));
    }

    #[test]
    fn sorts_numbered_issues_naturally() {
        let mut numbers = ["10", "2", "1"];
        numbers.sort_by(|left, right| compare_issue_numbers(left, right));

        assert_eq!(numbers, ["1", "2", "10"]);
    }

    #[test]
    fn sorts_signed_and_decimal_issue_numbers_numerically() {
        let mut numbers = ["1.10", "2", "-1", "1.5", "1"];
        numbers.sort_by(|left, right| compare_issue_numbers(left, right));

        assert_eq!(numbers, ["-1", "1", "1.10", "1.5", "2"]);
    }

    #[test]
    fn sorts_fractional_issue_numbers_numerically() {
        let mut numbers = ["1", "1/2", "0.25"];
        numbers.sort_by(|left, right| compare_issue_numbers(left, right));

        assert_eq!(numbers, ["0.25", "1/2", "1"]);
    }
}

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, NaiveDate};
use gtk::gio;
use gtk::gio::prelude::*;

use crate::comic_vine::{Issue, Volume};

const COMIC_EXTENSIONS: &[&str] = &["cb7", "cba", "cbr", "cbt", "cbz", "pdf"];

#[derive(Clone, Debug)]
pub struct ComicMatch {
    pub volume: Volume,
    pub issue: Issue,
}

#[derive(Clone, Debug)]
pub struct ComicFile {
    pub path: PathBuf,
    pub comic_match: Option<ComicMatch>,
}

impl ComicFile {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            comic_match: None,
        }
    }

    pub fn display_name(&self) -> String {
        self.path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn target_path(&self) -> Result<PathBuf> {
        let comic_match = self
            .comic_match
            .as_ref()
            .ok_or_else(|| anyhow!("No issue has been selected"))?;
        let extension = self
            .path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| anyhow!("The source file has no extension"))?;
        let filename = format_filename(&comic_match.volume, &comic_match.issue, extension)?;
        Ok(self.path.with_file_name(filename))
    }
}

pub fn discover_comics(path: &Path) -> Result<Vec<ComicFile>> {
    let mut paths = Vec::new();
    if path.is_file() {
        if is_comic(path) {
            paths.push(path.to_path_buf());
        }
    } else {
        collect_comics(path, &mut paths)
            .with_context(|| format!("Could not read {}", path.display()))?;
    }
    paths.sort_by(|left, right| {
        left.to_string_lossy()
            .to_lowercase()
            .cmp(&right.to_string_lossy().to_lowercase())
            .then_with(|| left.cmp(right))
    });
    Ok(paths.into_iter().map(ComicFile::new).collect())
}

fn collect_comics(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        if file_type.is_dir() {
            collect_comics(&path, paths)?;
        } else if file_type.is_file() && is_comic(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_comic(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            COMIC_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
}

pub fn format_filename(volume: &Volume, issue: &Issue, extension: &str) -> Result<String> {
    let volume_year = volume
        .start_year
        .ok_or_else(|| anyhow!("ComicVine has no volume year for this series"))?;
    let cover_date = issue
        .cover_date
        .as_deref()
        .ok_or_else(|| anyhow!("ComicVine has no cover date for this issue"))?;
    let date = NaiveDate::parse_from_str(cover_date, "%Y-%m-%d")
        .with_context(|| format!("ComicVine returned an invalid cover date: {cover_date}"))?;
    let (series, annual) = annual_parts(&volume.name);
    let series = sanitize_component(series);
    let issue_number = sanitize_component(issue.issue_number.trim());
    if issue_number.is_empty() {
        bail!("ComicVine has no issue number for this issue");
    }
    let annual = if annual { " Annual" } else { "" };

    Ok(format!(
        "{series} ({volume_year}){annual} #{} ({} {}).{}",
        issue_number,
        date.format("%B"),
        date.year(),
        extension.to_ascii_lowercase()
    ))
}

fn annual_parts(name: &str) -> (&str, bool) {
    let name = name.trim();
    if let Some(suffix) = name.get(name.len().saturating_sub(7)..) {
        if suffix.eq_ignore_ascii_case(" annual") {
            return (&name[..name.len() - 7], true);
        }
    }
    (name, false)
}

fn sanitize_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character == '/' || character == '\0' {
                '-'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn rename_matched(files: &mut [ComicFile]) -> Result<usize> {
    let plans = files
        .iter()
        .enumerate()
        .filter(|(_, file)| file.comic_match.is_some())
        .map(|(index, file)| Ok((index, file.path.clone(), file.target_path()?)))
        .collect::<Result<Vec<_>>>()?;

    let mut targets = HashSet::new();
    let sources = plans
        .iter()
        .map(|(_, source, _)| source.clone())
        .collect::<HashSet<_>>();
    for (_, source, target) in &plans {
        if !gio::File::for_path(source).query_exists(None::<&gio::Cancellable>) {
            bail!("{} no longer exists", source.display());
        }
        if !targets.insert(target.clone()) {
            bail!(
                "More than one comic would be renamed to {}",
                target.display()
            );
        }
        if target.exists() && target != source && !sources.contains(target) {
            bail!("{} already exists", target.display());
        }
        if target != source && sources.contains(target) {
            bail!(
                "{} is also a source file; rename it separately first",
                target.display()
            );
        }
    }

    let mut completed: Vec<(usize, PathBuf, PathBuf)> = Vec::new();
    for (index, source, target) in &plans {
        if source == target {
            continue;
        }
        if let Err(error) = rename_no_replace(source, target) {
            let mut stranded = Vec::new();
            for (completed_index, original, completed_target) in completed.iter().rev() {
                if rename_no_replace(completed_target, original).is_err() {
                    files[*completed_index].path = completed_target.clone();
                    stranded.push(completed_target.display().to_string());
                }
            }
            if !stranded.is_empty() {
                bail!(
                    "Could not rename {}: {error}. Rollback also failed; files remain at {}",
                    source.display(),
                    stranded.join(", ")
                );
            }
            return Err(error).with_context(|| format!("Could not rename {}", source.display()));
        }
        completed.push((*index, source.clone(), target.clone()));
    }
    for (index, _, target) in &completed {
        files[*index].path = target.clone();
    }

    Ok(completed.len())
}

fn rename_no_replace(source: &Path, target: &Path) -> std::result::Result<(), gtk::glib::Error> {
    gio::File::for_path(source).move_(
        &gio::File::for_path(target),
        gio::FileCopyFlags::NONE,
        None::<&gio::Cancellable>,
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DIRECTORY_NUMBER: AtomicUsize = AtomicUsize::new(0);

    fn volume(name: &str, start_year: Option<i32>) -> Volume {
        Volume {
            id: 1,
            name: name.into(),
            start_year,
            publisher: None,
        }
    }

    fn issue(number: &str, cover_date: Option<&str>) -> Issue {
        Issue {
            id: 2,
            issue_number: number.into(),
            name: None,
            cover_date: cover_date.map(str::to_string),
        }
    }

    fn matched_file(path: PathBuf, number: &str) -> ComicFile {
        ComicFile {
            path,
            comic_match: Some(ComicMatch {
                volume: volume("Batman", Some(2014)),
                issue: issue(number, Some("2014-10-01")),
            }),
        }
    }

    fn temporary_directory() -> PathBuf {
        let number = DIRECTORY_NUMBER.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("comic-name-test-{}-{number}", std::process::id()));
        fs::create_dir(&path).unwrap();
        path
    }

    #[test]
    fn formats_a_regular_issue() {
        let result = format_filename(
            &volume("Batman", Some(2014)),
            &issue("1", Some("2014-10-01")),
            "CBZ",
        )
        .unwrap();

        assert_eq!(result, "Batman (2014) #1 (October 2014).cbz");
    }

    #[test]
    fn moves_annual_after_the_volume_year() {
        let result = format_filename(
            &volume("Batman Annual", Some(2014)),
            &issue("1", Some("2015-01-01")),
            "cbr",
        )
        .unwrap();

        assert_eq!(result, "Batman (2014) Annual #1 (January 2015).cbr");
    }

    #[test]
    fn removes_path_separators_from_issue_numbers() {
        let result = format_filename(
            &volume("Batman", Some(2014)),
            &issue("1/2", Some("2014-10-01")),
            "cbz",
        )
        .unwrap();

        assert_eq!(result, "Batman (2014) #1-2 (October 2014).cbz");
    }

    #[test]
    fn requires_the_metadata_used_by_the_format() {
        assert!(format_filename(&volume("Batman", None), &issue("1", None), "cbz").is_err());
    }

    #[test]
    fn renames_an_explicitly_matched_file() {
        let directory = temporary_directory();
        let source = directory.join("scan.cbz");
        fs::write(&source, b"comic").unwrap();
        let mut files = vec![matched_file(source, "1")];

        assert_eq!(rename_matched(&mut files).unwrap(), 1);
        assert_eq!(
            files[0].display_name(),
            "Batman (2014) #1 (October 2014).cbz"
        );
        assert!(files[0].path.exists());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_duplicate_targets_before_changing_files() {
        let directory = temporary_directory();
        let first = directory.join("scan-a.cbz");
        let second = directory.join("scan-b.cbz");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        let mut files = vec![
            matched_file(first.clone(), "1"),
            matched_file(second.clone(), "1"),
        ];

        assert!(rename_matched(&mut files).is_err());
        assert!(first.exists());
        assert!(second.exists());

        fs::remove_dir_all(directory).unwrap();
    }
}

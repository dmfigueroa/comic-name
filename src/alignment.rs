use std::ops::RangeInclusive;

use crate::comic::{ComicFile, ComicMatch};
use crate::comic_vine::{Issue, Volume};

#[derive(Debug)]
pub struct BatchAlignment {
    file_rows: Vec<Option<ComicFile>>,
    issues: Vec<Issue>,
    removed: Vec<RemovedFile>,
}

#[derive(Debug)]
struct RemovedFile {
    row: usize,
    file: ComicFile,
}

impl BatchAlignment {
    pub fn new(files: Vec<ComicFile>, issues: Vec<Issue>) -> Self {
        let row_count = files.len().max(issues.len());
        let mut file_rows = files.into_iter().map(Some).collect::<Vec<_>>();
        file_rows.resize_with(row_count, || None);
        Self {
            file_rows,
            issues,
            removed: Vec::new(),
        }
    }

    pub fn row_count(&self) -> usize {
        self.file_rows.len()
    }

    pub fn file(&self, index: usize) -> Option<&ComicFile> {
        self.file_rows.get(index).and_then(Option::as_ref)
    }

    pub fn issue(&self, index: usize) -> Option<&Issue> {
        self.issues.get(index)
    }

    pub fn removed_count(&self) -> usize {
        self.removed.len()
    }

    pub fn removed_file(&self, index: usize) -> Option<&ComicFile> {
        self.removed.get(index).map(|removed| &removed.file)
    }

    pub fn move_up(&mut self, first: usize, last: usize) -> Option<RangeInclusive<usize>> {
        if first == 0 || !self.valid_file_range(first, last) {
            return None;
        }
        self.file_rows[first - 1..=last].rotate_left(1);
        Some(first - 1..=last - 1)
    }

    pub fn move_down(&mut self, first: usize, last: usize) -> Option<RangeInclusive<usize>> {
        if !self.valid_file_range(first, last) {
            return None;
        }
        if last + 1 == self.file_rows.len() {
            self.file_rows.push(None);
        }
        self.file_rows[first..=last + 1].rotate_right(1);
        Some(first + 1..=last + 1)
    }

    pub fn remove(&mut self, first: usize, last: usize) -> bool {
        if !self.valid_file_range(first, last) {
            return false;
        }
        for index in first..=last {
            self.removed.push(RemovedFile {
                row: index,
                file: self.file_rows[index].take().expect("validated file row"),
            });
        }
        true
    }

    pub fn restore(&mut self, removed_indices: &[usize]) -> Vec<usize> {
        let mut indices = removed_indices
            .iter()
            .copied()
            .filter(|index| *index < self.removed.len())
            .collect::<Vec<_>>();
        indices.sort_unstable();
        indices.dedup();

        let mut removed = indices
            .iter()
            .rev()
            .map(|index| self.removed.remove(*index))
            .collect::<Vec<_>>();
        removed.reverse();

        let mut restored_rows = Vec::new();
        for removed in removed {
            let row = if self.file_rows.get(removed.row).is_some_and(Option::is_none) {
                removed.row
            } else {
                self.file_rows
                    .iter()
                    .position(Option::is_none)
                    .unwrap_or_else(|| {
                        self.file_rows.push(None);
                        self.file_rows.len() - 1
                    })
            };
            self.file_rows[row] = Some(removed.file);
            restored_rows.push(row);
        }
        restored_rows
    }

    pub fn matched_files(&self, volume: &Volume) -> Vec<ComicFile> {
        self.file_rows
            .iter()
            .zip(&self.issues)
            .filter_map(|(file, issue)| {
                let mut file = file.clone()?;
                file.comic_match = Some(ComicMatch {
                    volume: volume.clone(),
                    issue: issue.clone(),
                });
                Some(file)
            })
            .collect()
    }

    fn valid_file_range(&self, first: usize, last: usize) -> bool {
        first <= last
            && last < self.file_rows.len()
            && self.file_rows[first..=last].iter().all(Option::is_some)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn file(name: &str) -> ComicFile {
        ComicFile::new(PathBuf::from(name))
    }

    fn issue(id: u64, number: &str) -> Issue {
        Issue {
            id,
            issue_number: number.into(),
            name: None,
            cover_date: Some("2014-10-01".into()),
        }
    }

    fn volume() -> Volume {
        Volume {
            id: 10,
            name: "Batman".into(),
            start_year: Some(2014),
            publisher: None,
        }
    }

    fn names(alignment: &BatchAlignment) -> Vec<Option<String>> {
        (0..alignment.row_count())
            .map(|index| alignment.file(index).map(ComicFile::display_name))
            .collect()
    }

    #[test]
    fn shorter_side_is_padded_with_blank_rows() {
        let alignment =
            BatchAlignment::new(vec![file("a.cbz")], vec![issue(1, "1"), issue(2, "2")]);

        assert_eq!(alignment.row_count(), 2);
        assert_eq!(names(&alignment), vec![Some("a.cbz".into()), None]);
        assert_eq!(alignment.issue(1).unwrap().issue_number, "2");
    }

    #[test]
    fn moving_multiple_rows_preserves_their_order() {
        let mut alignment = BatchAlignment::new(
            vec![file("a.cbz"), file("b.cbz"), file("c.cbz")],
            vec![issue(1, "1"), issue(2, "2"), issue(3, "3")],
        );

        assert_eq!(alignment.move_up(1, 2), Some(0..=1));
        assert_eq!(
            names(&alignment),
            vec![
                Some("b.cbz".into()),
                Some("c.cbz".into()),
                Some("a.cbz".into())
            ]
        );
    }

    #[test]
    fn moving_the_last_rows_down_creates_an_offset_gap() {
        let mut alignment = BatchAlignment::new(
            vec![file("a.cbz"), file("b.cbz")],
            vec![issue(1, "1"), issue(2, "2")],
        );

        assert_eq!(alignment.move_down(0, 1), Some(1..=2));
        assert_eq!(
            names(&alignment),
            vec![None, Some("a.cbz".into()), Some("b.cbz".into())]
        );
        assert!(alignment.issue(2).is_none());
    }

    #[test]
    fn removed_files_can_be_restored_into_blank_rows() {
        let mut alignment = BatchAlignment::new(
            vec![file("a.cbz"), file("b.cbz")],
            vec![issue(1, "1"), issue(2, "2")],
        );

        assert!(alignment.remove(0, 0));
        assert_eq!(alignment.removed_file(0).unwrap().display_name(), "a.cbz");
        assert_eq!(names(&alignment), vec![None, Some("b.cbz".into())]);
        assert_eq!(alignment.restore(&[0]), vec![0]);
        assert_eq!(alignment.removed_count(), 0);
        assert_eq!(
            names(&alignment),
            vec![Some("a.cbz".into()), Some("b.cbz".into())]
        );
    }

    #[test]
    fn restore_prefers_the_row_the_file_was_removed_from() {
        let mut alignment = BatchAlignment::new(
            vec![file("a.cbz"), file("b.cbz"), file("c.cbz")],
            vec![issue(1, "1"), issue(2, "2"), issue(3, "3")],
        );
        alignment.remove(0, 0);
        alignment.remove(2, 2);

        assert_eq!(alignment.restore(&[1]), vec![2]);
        assert_eq!(
            names(&alignment),
            vec![None, Some("b.cbz".into()), Some("c.cbz".into())]
        );
    }

    #[test]
    fn only_rows_with_a_file_and_issue_become_matches() {
        let mut alignment = BatchAlignment::new(
            vec![file("a.cbz"), file("b.cbz")],
            vec![issue(1, "1"), issue(2, "2")],
        );
        alignment.move_down(1, 1);

        let matches = alignment.matched_files(&volume());

        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].display_name(), "a.cbz");
        assert_eq!(
            matches[0].comic_match.as_ref().unwrap().issue.issue_number,
            "1"
        );
    }
}

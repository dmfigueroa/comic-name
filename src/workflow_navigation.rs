use adw::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkflowPage {
    ChooseSeries,
    AlignFiles,
    ChooseIssue,
    ReviewRenames,
}

#[derive(Debug)]
pub struct WorkflowNavigation {
    pages: &'static [WorkflowPage],
    current: usize,
}

impl WorkflowNavigation {
    pub fn batch() -> Self {
        Self {
            pages: &[
                WorkflowPage::ChooseSeries,
                WorkflowPage::AlignFiles,
                WorkflowPage::ReviewRenames,
            ],
            current: 0,
        }
    }

    pub fn single() -> Self {
        Self {
            pages: &[WorkflowPage::ChooseSeries, WorkflowPage::ChooseIssue],
            current: 0,
        }
    }

    pub fn current(&self) -> WorkflowPage {
        self.pages[self.current]
    }

    pub fn advance(&mut self) -> Option<WorkflowPage> {
        if self.current + 1 == self.pages.len() {
            return None;
        }
        self.current += 1;
        Some(self.current())
    }

    pub fn back(&mut self) -> Option<WorkflowPage> {
        if self.current == 0 {
            return None;
        }
        self.current -= 1;
        Some(self.current())
    }
}

#[derive(Debug)]
pub struct WorkflowPageShell {
    page: adw::NavigationPage,
    back_button: gtk::Button,
    action_button: gtk::Button,
    #[cfg(test)]
    clamp: adw::Clamp,
}

impl WorkflowPageShell {
    pub fn new<W: IsA<gtk::Widget>>(title: &str, content: &W, action: &str) -> Self {
        let back_button = gtk::Button::builder()
            .icon_name("go-previous-symbolic")
            .tooltip_text("Back")
            .build();
        let action_button = gtk::Button::builder().label(action).build();
        let window_title = adw::WindowTitle::builder().title(title).build();
        let header = adw::HeaderBar::builder()
            .title_widget(&window_title)
            .show_back_button(false)
            .build();
        header.pack_start(&back_button);
        header.pack_end(&action_button);

        let clamp = adw::Clamp::builder()
            .maximum_size(960)
            .tightening_threshold(720)
            .child(content)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&clamp)
            .build();
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&scroller));
        let page = adw::NavigationPage::builder()
            .title(title)
            .child(&toolbar)
            .build();

        Self {
            page,
            back_button,
            action_button,
            #[cfg(test)]
            clamp,
        }
    }

    pub fn page(&self) -> adw::NavigationPage {
        self.page.clone()
    }

    pub fn back_button(&self) -> gtk::Button {
        self.back_button.clone()
    }

    pub fn action_button(&self) -> gtk::Button {
        self.action_button.clone()
    }

    #[cfg(test)]
    pub fn clamp(&self) -> adw::Clamp {
        self.clamp.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_navigation_moves_through_its_pages_and_back_to_entry() {
        let mut navigation = WorkflowNavigation::batch();

        assert_eq!(navigation.current(), WorkflowPage::ChooseSeries);
        assert_eq!(navigation.advance(), Some(WorkflowPage::AlignFiles));
        assert_eq!(navigation.advance(), Some(WorkflowPage::ReviewRenames));
        assert_eq!(navigation.back(), Some(WorkflowPage::AlignFiles));
        assert_eq!(navigation.back(), Some(WorkflowPage::ChooseSeries));
        assert_eq!(navigation.back(), None);
    }

    #[test]
    fn single_navigation_ends_at_issue_selection_and_returns_to_entry() {
        let mut navigation = WorkflowNavigation::single();

        assert_eq!(navigation.current(), WorkflowPage::ChooseSeries);
        assert_eq!(navigation.advance(), Some(WorkflowPage::ChooseIssue));
        assert_eq!(navigation.advance(), None);
        assert_eq!(navigation.back(), Some(WorkflowPage::ChooseSeries));
        assert_eq!(navigation.back(), None);
    }
}

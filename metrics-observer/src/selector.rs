use ratatui::widgets::ListState;

pub struct Selector(usize, ListState);

impl Selector {
    pub fn new() -> Selector {
        let mut state = ListState::default();
        state.select(Some(0));
        Selector(0, state)
    }

    pub fn set_length(&mut self, len: usize) {
        if len < self.0 {
            self.1.select(Some(0));
        }
        self.0 = len;
    }

    pub fn state(&mut self) -> &mut ListState {
        &mut self.1
    }

    pub fn top(&mut self) {
        self.1.select(Some(0));
    }

    pub fn bottom(&mut self) {
        self.1.select(Some(self.0 - 1));
    }

    pub fn next(&mut self) {
        let i = match self.1.selected() {
            Some(i) => {
                if i >= self.0 - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.1.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.1.selected() {
            Some(i) => {
                if i == 0 {
                    self.0 - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.1.select(Some(i));
    }
}

pub struct Spinner {
    frames: &'static [&'static str],
    current_frame: usize,
}

impl Spinner {
    pub fn new(frames: &'static [&'static str]) -> Self {
        Self {
            frames,
            current_frame: 0,
        }
    }

    pub fn default_dots() -> Self {
        Self::new(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
    }

    pub fn tick(&mut self) {
        if !self.frames.is_empty() {
            self.current_frame = (self.current_frame + 1) % self.frames.len();
        }
    }

    pub fn frame(&self) -> &'static str {
        if self.frames.is_empty() {
            ""
        } else {
            self.frames[self.current_frame]
        }
    }
}

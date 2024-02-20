use std::{
    collections::{HashMap, HashSet},
    io::{self, stdout, Stdout, Write},
    time::Instant,
};

use crossterm::{
    cursor::{MoveToNextLine, MoveToPreviousLine},
    event::{read, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Color, Print, SetBackgroundColor, SetForegroundColor},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};

fn str_index(string: &str, i: usize) -> char {
    (string.as_bytes()[i]) as char
}

struct Formatter {
    fg_color: Color,
    bg_color: Color,
}

impl Formatter {
    fn apply_fg(&mut self, stdout: &mut Stdout, color: Color) {
        if self.fg_color != color {
            self.fg_color = color;
            queue!(stdout, SetForegroundColor(self.fg_color)).unwrap();
        }
    }

    fn apply_bg(&mut self, stdout: &mut Stdout, color: Color) {
        if self.bg_color != color {
            self.bg_color = color;
            queue!(stdout, SetBackgroundColor(self.bg_color)).unwrap();
        }
    }
}

#[derive(Debug)]
struct State {
    start: Option<Instant>,
    text: String,
    i: usize,

    // are these structures overkill?
    mismatches: HashSet<usize>,
    extensions: HashMap<usize, String>,
    skips: HashMap<usize, usize>,
}

impl State {
    fn new(text: &str) -> Self {
        Self {
            start: None,
            text: text.to_string(),
            i: 0,
            mismatches: HashSet::new(),
            extensions: HashMap::new(),
            skips: HashMap::new(),
        }
    }

    fn handle_char(&mut self, c: char) {
        // situations:
        // - skippy
        // - currently in an extended word, and needs to add a new character
        // - mismatch and increment index
        // - mismatch and start extension
        // - match and increment index

        if self.start.is_none() {
            self.start = Some(Instant::now());
        }

        let target_c = str_index(&self.text, self.i);

        if c == ' ' && target_c != ' ' {
            let mut next_word_i = self.i;
            while str_index(&self.text, next_word_i) != ' ' && next_word_i < self.text.len() - 1 {
                next_word_i += 1;
            }
            self.skips.insert(next_word_i, self.i);
            self.i = next_word_i + 1;
            return;
        }

        if let Some(extension) = self.extensions.get_mut(&self.i) {
            extension.push(c);
            return;
        }

        if target_c == c {
            self.i += 1;
            return;
        }

        if target_c == ' ' {
            self.extensions.insert(self.i, c.to_string());
            return;
        }

        self.mismatches.insert(self.i);
        self.i += 1;
    }

    fn handle_backspace(&mut self) {
        // situations
        // - currently in an extended word, and needs to pop a char (and remove the extension if its now empty)
        // - decrement index and remove mismatch if present
        // - currently at start of word, with a previous skip, so undo the skip
        // - currently at start of word, with no previous skip, so do nothing

        if let Some(extension) = self.extensions.get_mut(&self.i) {
            extension.pop();
            if extension.is_empty() {
                self.extensions.remove(&self.i);
            }
            return;
        }

        if self.i == 0 {
            return;
        }

        if str_index(&self.text, self.i - 1) == ' ' {
            if let Some(start) = self.skips.get(&(self.i - 1)) {
                let temp_start = *start;
                self.skips.remove(&(self.i - 1));
                self.i = temp_start;
            }
            return;
        }

        self.i -= 1;
        self.mismatches.remove(&self.i);
    }

    fn should_exit(&self) -> bool {
        self.i >= self.text.len()
    }

    fn get_wpm(&self) -> Option<f64> {
        self.start.map(|start| {
            self.text[0..self.i].split_whitespace().count() as f64
                / Instant::now().duration_since(start).as_secs_f64()
                * 60.
        })
    }

    fn render(&self) {
        let mut stdout = stdout();
        let mut formatter = Formatter {
            fg_color: Color::Reset,
            bg_color: Color::Reset,
        };
        let skip_ranges: Vec<_> = self
            .skips
            .iter()
            .map(|(end, start)| *start..=*end)
            .collect();

        queue!(
            stdout,
            Clear(ClearType::CurrentLine),
            MoveToPreviousLine(0),
            Clear(ClearType::CurrentLine),
            Print(match self.get_wpm() {
                Some(wpm) => format!("{:.2} wpm", wpm),
                None => "Start typing".to_string(),
            }),
            MoveToNextLine(0)
        )
        .unwrap();
        formatter.apply_fg(&mut stdout, Color::Green);

        for (i, c) in self.text.chars().enumerate() {
            if let Some(extension) = self.extensions.get(&i) {
                formatter.apply_fg(&mut stdout, Color::Red);
                formatter.apply_bg(&mut stdout, Color::Reset);

                queue!(stdout, Print(extension), Print(c)).unwrap();
                continue;
            }

            if i < self.i {
                if self.mismatches.contains(&i)
                    || skip_ranges.iter().any(|range| range.contains(&i))
                {
                    formatter.apply_fg(&mut stdout, Color::Red);
                } else {
                    formatter.apply_fg(&mut stdout, Color::Green);
                }
                formatter.apply_bg(&mut stdout, Color::Reset);
                queue!(stdout, Print(c)).unwrap();
            } else if i == self.i {
                formatter.apply_fg(&mut stdout, Color::Black);
                formatter.apply_bg(&mut stdout, Color::White);
                queue!(stdout, Print(c)).unwrap();
            } else if i > self.i {
                formatter.apply_fg(&mut stdout, Color::Reset);
                formatter.apply_bg(&mut stdout, Color::Reset);
                queue!(stdout, Print(c)).unwrap();
            }
        }

        formatter.apply_fg(&mut stdout, Color::Reset);
        formatter.apply_bg(&mut stdout, Color::Reset);

        stdout.flush().unwrap();
    }

    fn debug_render(&self) {
        let mut stdout = stdout();
        execute!(stdout, Print(format!("{:?}", self)), MoveToNextLine(1)).unwrap();
    }
}

fn mainloop() -> io::Result<()> {
    let text = "The quick brown fox jumped over the lazy wolves.";
    let mut state = State::new(text);
    state.render();
    loop {
        match read()? {
            Event::Key(event) => {
                if event.kind != KeyEventKind::Press {
                    continue;
                }
                match event.code {
                    KeyCode::Backspace => {
                        state.handle_backspace();
                    }
                    KeyCode::Char(c) => {
                        state.handle_char(c);
                    }
                    KeyCode::Esc => {
                        break;
                    }
                    _ => {}
                }

                // state.debug_render();
                state.render();

                if state.should_exit() {
                    break;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let _ = mainloop();
    disable_raw_mode()?;
    Ok(())
}

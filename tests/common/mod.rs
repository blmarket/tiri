//! Server harness shared by integration tests that drive a real tiri server.

#![allow(dead_code)]

use std::fs::{self, File};
use std::io::{ErrorKind, Read as _, Write as _};
use std::os::fd::{AsRawFd as _, FromRawFd as _};
use std::os::unix::process::CommandExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct Server {
    directory: PathBuf,
    socket: PathBuf,
}

impl Server {
    pub fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("tiri-test-{}-{stamp}", std::process::id()));
        fs::create_dir(&directory).expect("create socket directory");
        let socket = directory.join("socket");
        Self { directory, socket }
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tiri"));
        command
            .args(["-f", "/dev/null", "-S"])
            .arg(&self.socket)
            .env("TERM", "xterm-256color")
            .env("SHELL", "/bin/sh")
            .env("TMUX", "")
            .env_remove("TMUX_PANE");
        command
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().expect("run tiri")
    }

    pub fn success(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).expect("UTF-8 command output")
    }

    pub fn display(&self, format: &str) -> String {
        self.success(&["display-message", "-p", format])
            .trim_end()
            .to_owned()
    }

    /// Start a manually sized window with `panes` in its strip.
    pub fn start(&self, sx: u32, sy: u32, panes: usize) {
        self.success(&[
            "new-session",
            "-d",
            "-x",
            &sx.to_string(),
            "-y",
            &sy.to_string(),
            "sleep 60",
        ]);
        self.success(&["set", "-g", "window-size", "manual"]);
        for _ in 1..panes {
            self.success(&["new-pane", "sleep 60"]);
        }
    }

    pub fn window_size(&self) -> (u32, u32) {
        let size = self.display("#{window_width}x#{window_height}");
        let (sx, sy) = size.split_once('x').expect("size format");
        (sx.parse().unwrap(), sy.parse().unwrap())
    }

    /// The layout root's size: the strip, or a lone pane's cell.
    pub fn root_size(&self) -> (u32, u32) {
        let layout = self.display("#{window_layout}");
        let root = &layout[layout.find("\"L\":{").expect("layout root")..];
        (field(root, "\"w\":"), field(root, "\"h\":"))
    }

    /// Each pane's id, first column, width, top row and height, in strip order.
    pub fn panes(&self) -> Vec<(String, u32, u32, u32, u32)> {
        self.success(&[
            "list-panes",
            "-F",
            "#{pane_id} #{pane_left} #{pane_width} #{pane_top} #{pane_height}",
        ])
        .lines()
        .map(|line| {
            let fields: Vec<_> = line.split(' ').collect();
            let number = |index: usize| fields[index].parse().expect("numeric pane field");
            (
                fields[0].to_owned(),
                number(1),
                number(2),
                number(3),
                number(4),
            )
        })
        .collect()
    }

    /// Count `window-layout-changed` and `window-resized` from here on.
    pub fn count_events(&self) {
        for (hook, option) in [
            ("window-layout-changed", "@layout_changed"),
            ("window-resized", "@resized"),
        ] {
            self.success(&["set", "-g", option, "0"]);
            let command = format!("set -gF {option} '#{{e|+:#{{{option}}},1}}'");
            self.success(&["set-hook", "-g", hook, &command]);
        }
    }

    pub fn events(&self) -> (u32, u32) {
        let events = self.display("#{@layout_changed} #{@resized}");
        let (layout, resized) = events.split_once(' ').expect("event counts");
        (layout.parse().unwrap(), resized.parse().unwrap())
    }

    pub fn wait_for_size(&self, expected: (u32, u32)) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.window_size() != expected {
            assert!(
                Instant::now() < deadline,
                "window stayed {:?}, expected {expected:?}",
                self.window_size()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// Attach a control client and size it; it stays attached until dropped.
    pub fn attach_control(&self, sx: u32, sy: u32) -> ControlClient {
        let mut child = self
            .command()
            .args(["-C", "attach"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn control client");
        writeln!(
            child.stdin.as_mut().expect("control stdin"),
            "refresh-client -C {sx}x{sy}"
        )
        .expect("size control client");
        ControlClient(child)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.run(&["kill-server"]);
        let _ = fs::remove_dir_all(&self.directory);
    }
}

pub struct ControlClient(Child);

impl Server {
    /// Attach a client on a pseudo-terminal of `width` by `height` cells; it
    /// stays attached until dropped.
    pub fn attach_terminal(&self, width: u16, height: u16) -> TerminalClient {
        TerminalClient::new(self, width, height)
    }
}

impl Drop for ControlClient {
    fn drop(&mut self) {
        drop(self.0.stdin.take());
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn field(text: &str, key: &str) -> u32 {
    let start = text.find(key).expect("layout field") + key.len();
    let digits: String = text[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().expect("numeric layout field")
}

/// One character cell and the foreground colour it was drawn in, `None` for
/// the terminal default or a colour the model does not track.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: Option<u8>,
}

const BLANK: Cell = Cell { ch: ' ', fg: None };

/// The DEC special graphics set, as UTF-8, so line drawing compares the same
/// whether the server sent it as ACS or as UTF-8.
fn line_drawing(ch: char) -> char {
    match ch {
        'x' => '│',
        'q' => '─',
        '~' => '·',
        'l' => '┌',
        'k' => '┐',
        'm' => '└',
        'j' => '┘',
        't' => '├',
        'u' => '┤',
        'v' => '┴',
        'w' => '┬',
        'n' => '┼',
        _ => ch,
    }
}

/// The cells a terminal client shows, kept by interpreting the subset of
/// escape sequences the server sends.
pub struct Screen {
    pub cells: Vec<Vec<Cell>>,
    pub x: usize,
    pub y: usize,
    pending: Vec<u8>,
    saved: (usize, usize),
    region: (usize, usize),
    fg: Option<u8>,
    /// Whether G0 holds the DEC special graphics set.
    graphics: bool,
}

impl Screen {
    fn new(width: usize, height: usize) -> Self {
        Self {
            cells: vec![vec![BLANK; width]; height],
            x: 0,
            y: 0,
            pending: Vec::new(),
            saved: (0, 0),
            region: (0, height - 1),
            fg: None,
            graphics: false,
        }
    }

    pub fn row(&self, y: usize) -> String {
        self.cells[y].iter().map(|cell| cell.ch).collect()
    }

    /// The characters of column `x`, top to bottom.
    pub fn column(&self, x: usize) -> String {
        self.cells.iter().map(|row| row[x].ch).collect()
    }

    /// The first column of `text` on row `y`.
    pub fn find(&self, y: usize, text: &str) -> Option<usize> {
        let row = self.row(y);
        row.find(text).map(|byte| row[..byte].chars().count())
    }

    fn newline(&mut self) {
        if self.y == self.region.1 {
            let width = self.cells[0].len();
            self.cells.remove(self.region.0);
            self.cells.insert(self.region.1, vec![BLANK; width]);
        } else {
            self.y = (self.y + 1).min(self.cells.len() - 1);
        }
    }

    fn csi(&mut self, bytes: &[u8], command: u8) {
        let private = bytes.first() == Some(&b'?');
        let params = String::from_utf8_lossy(bytes);
        let values: Vec<usize> = params
            .trim_start_matches(['?', '>', '='])
            .split(';')
            .map(|n| n.parse().unwrap_or(0))
            .collect();
        let get = |i: usize, default: usize| {
            values
                .get(i)
                .copied()
                .filter(|n| *n != 0)
                .unwrap_or(default)
        };
        let w = self.cells[0].len();
        let h = self.cells.len();
        let x = self.x.min(w - 1);
        match command {
            b'H' | b'f' => {
                self.y = get(0, 1).saturating_sub(1).min(h - 1);
                self.x = get(1, 1).saturating_sub(1).min(w - 1);
            }
            b'A' => self.y = self.y.saturating_sub(get(0, 1)),
            b'B' | b'e' => self.y = (self.y + get(0, 1)).min(h - 1),
            b'C' | b'a' => self.x = (self.x + get(0, 1)).min(w - 1),
            b'D' => self.x = self.x.saturating_sub(get(0, 1)),
            b'G' | b'`' => self.x = get(0, 1).saturating_sub(1).min(w - 1),
            b'd' => self.y = get(0, 1).saturating_sub(1).min(h - 1),
            b'J' if !private => {
                let rows = match values[0] {
                    0 => {
                        self.cells[self.y][x..].fill(BLANK);
                        self.y + 1..h
                    }
                    1 => {
                        self.cells[self.y][..=x].fill(BLANK);
                        0..self.y
                    }
                    _ => 0..h,
                };
                for row in &mut self.cells[rows] {
                    row.fill(BLANK);
                }
            }
            b'K' if !private => match values[0] {
                0 => self.cells[self.y][x..].fill(BLANK),
                1 => self.cells[self.y][..=x].fill(BLANK),
                _ => self.cells[self.y].fill(BLANK),
            },
            b'X' => {
                let end = (x + get(0, 1)).min(w);
                self.cells[self.y][x..end].fill(BLANK);
            }
            b'P' => {
                let n = get(0, 1).min(w - x);
                self.cells[self.y].drain(x..x + n);
                self.cells[self.y].resize(w, BLANK);
            }
            b'@' => {
                for _ in 0..get(0, 1).min(w - x) {
                    self.cells[self.y].insert(x, BLANK);
                    self.cells[self.y].pop();
                }
            }
            b'b' => {
                let cell = self.cells[self.y][x.saturating_sub(1)];
                for _ in 0..get(0, 1) {
                    self.put_cell(cell);
                }
            }
            b'r' if !private => {
                self.region = (
                    get(0, 1).saturating_sub(1).min(h - 1),
                    get(1, h).saturating_sub(1).min(h - 1),
                );
            }
            b'L' => {
                for _ in 0..get(0, 1).min(h - self.y) {
                    self.cells.remove(self.region.1);
                    self.cells.insert(self.y, vec![BLANK; w]);
                }
            }
            b'M' => {
                for _ in 0..get(0, 1).min(h - self.y) {
                    self.cells.remove(self.y);
                    self.cells.insert(self.region.1, vec![BLANK; w]);
                }
            }
            b'm' if !private => {
                let mut params = values.iter().copied();
                while let Some(param) = params.next() {
                    match param {
                        0 | 39 => self.fg = None,
                        30..=37 => self.fg = Some((param - 30) as u8),
                        90..=97 => self.fg = Some((param - 90 + 8) as u8),
                        38 | 48 => {
                            let colour = match params.next() {
                                Some(5) => params.next().map(|n| n as u8),
                                Some(2) => {
                                    params.by_ref().take(3).for_each(drop);
                                    None
                                }
                                _ => None,
                            };
                            if param == 38 {
                                self.fg = colour;
                            }
                        }
                        _ => (),
                    }
                }
            }
            _ => (),
        }
    }

    fn put(&mut self, ch: char) {
        let ch = if self.graphics { line_drawing(ch) } else { ch };
        self.put_cell(Cell { ch, fg: self.fg });
    }

    fn put_cell(&mut self, cell: Cell) {
        if self.x == self.cells[0].len() {
            self.x = 0;
            self.newline();
        }
        self.cells[self.y][self.x] = cell;
        self.x += 1;
    }

    fn feed(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
        let bytes = std::mem::take(&mut self.pending);
        let mut i = 0;
        while i < bytes.len() {
            let start = i;
            if bytes[i] == 0x1b {
                let Some(&kind) = bytes.get(i + 1) else {
                    break;
                };
                match kind {
                    b'[' => {
                        let Some(end) =
                            (i + 2..bytes.len()).find(|&j| (0x40..=0x7e).contains(&bytes[j]))
                        else {
                            break;
                        };
                        self.csi(&bytes[i + 2..end], bytes[end]);
                        i = end + 1;
                    }
                    b']' | b'P' | b'_' => {
                        let Some(end) = (i + 2..bytes.len()).find(|&j| {
                            bytes[j] == 7 || (bytes[j] == 0x1b && bytes.get(j + 1) == Some(&b'\\'))
                        }) else {
                            break;
                        };
                        i = end + if bytes[end] == 7 { 1 } else { 2 };
                    }
                    b'(' | b')' | b'%' => {
                        if i + 2 == bytes.len() {
                            break;
                        }
                        if kind == b'(' {
                            self.graphics = bytes[i + 2] == b'0';
                        }
                        i += 3;
                    }
                    b'7' => {
                        self.saved = (self.x, self.y);
                        i += 2;
                    }
                    b'8' => {
                        (self.x, self.y) = self.saved;
                        i += 2;
                    }
                    b'D' => {
                        self.newline();
                        i += 2;
                    }
                    b'M' => {
                        self.y = self.y.saturating_sub(1);
                        i += 2;
                    }
                    _ => i += 2,
                }
                continue;
            }
            let byte = bytes[i];
            i += 1;
            match byte {
                b'\r' => self.x = 0,
                b'\n' => self.newline(),
                0x08 => self.x = self.x.saturating_sub(1),
                b'\t' => self.x = ((self.x / 8 + 1) * 8).min(self.cells[0].len() - 1),
                0..=31 | 127 => (),
                32..=126 => self.put(byte as char),
                _ => {
                    let n = match byte {
                        0x80..0xe0 => 2,
                        0xe0..0xf0 => 3,
                        _ => 4,
                    };
                    if start + n > bytes.len() {
                        i = start;
                        break;
                    }
                    let ch = std::str::from_utf8(&bytes[start..start + n])
                        .ok()
                        .and_then(|text| text.chars().next())
                        .unwrap_or('?');
                    self.put(ch);
                    i = start + n;
                }
            }
        }
        self.pending.extend_from_slice(&bytes[i..]);
    }
}

/// A client attached on a pseudo-terminal, as a user's terminal would be.
pub struct TerminalClient {
    master: File,
    child: Child,
    pub screen: Screen,
    /// The client's terminal name, for `-c` and `-t` client targets.
    pub tty: String,
}

impl TerminalClient {
    fn new(server: &Server, width: u16, height: u16) -> Self {
        let mut size = libc::winsize {
            ws_row: height,
            ws_col: width,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let (mut master, mut slave) = (-1, -1);
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &raw mut size,
            )
        };
        assert_eq!(opened, 0, "openpty: {}", std::io::Error::last_os_error());
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        let mut command = server.command();
        command
            .arg("attach-session")
            .stdin(slave.try_clone().expect("clone terminal"))
            .stdout(slave.try_clone().expect("clone terminal"))
            .stderr(slave);
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().expect("spawn terminal client");
        let flags = unsafe { libc::fcntl(master.as_raw_fd(), libc::F_GETFL) };
        assert_ne!(
            unsafe { libc::fcntl(master.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK) },
            -1
        );
        let mut client = Self {
            master,
            child,
            screen: Screen::new(width.into(), height.into()),
            tty: String::new(),
        };
        let prefix = format!("{} ", client.child.id());
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let clients = server.success(&["list-clients", "-F", "#{client_pid} #{client_tty}"]);
            if let Some(line) = clients.lines().find(|line| line.starts_with(&prefix)) {
                client.tty = line[prefix.len()..].to_owned();
                break;
            }
            assert!(Instant::now() < deadline, "terminal client did not attach");
            client.drain();
            std::thread::sleep(Duration::from_millis(10));
        }
        client
    }

    /// Read whatever the server has drawn so far.
    pub fn drain(&mut self) {
        let mut bytes = [0; 65536];
        loop {
            match self.master.read(&mut bytes) {
                Ok(0) => return,
                Ok(n) => self.screen.feed(&bytes[..n]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return,
                Err(error) if error.raw_os_error() == Some(libc::EIO) => return,
                Err(error) => panic!("terminal read: {error}"),
            }
        }
    }

    /// Type `bytes` into the client's terminal.
    pub fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).expect("terminal write");
    }

    /// Press and release mouse button 1 at zero-based cell (`x`, `y`).
    pub fn click(&mut self, x: usize, y: usize) {
        let (x, y) = (x + 1, y + 1);
        self.send(format!("\x1b[<0;{x};{y}M\x1b[<0;{x};{y}m").as_bytes());
    }

    pub fn wait_screen(&mut self, condition: impl Fn(&Screen) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.drain();
            if condition(&self.screen) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "screen mismatch: cursor {},{}; rows {:#?}",
                self.screen.x,
                self.screen.y,
                (0..self.screen.cells.len())
                    .map(|y| self.screen.row(y))
                    .collect::<Vec<_>>()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Expand `format` for this client.
    pub fn display(&self, server: &Server, format: &str) -> String {
        server
            .success(&["display-message", "-p", "-c", &self.tty, format])
            .trim_end()
            .to_owned()
    }

    /// The first strip column this client shows.
    pub fn offset(&self, server: &Server) -> u32 {
        self.display(server, "#{?window_bigger,#{window_offset_x},0}")
            .parse()
            .expect("numeric offset")
    }

    pub fn wait_offset(&self, server: &Server, expected: u32) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.offset(server) != expected {
            assert!(
                Instant::now() < deadline,
                "offset stayed {}, expected {expected}",
                self.offset(server)
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for TerminalClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

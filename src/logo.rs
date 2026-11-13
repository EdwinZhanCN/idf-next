//! The Espressif symbol, drawn in the terminal for `idf`, `idf --help` and
//! `idf --version`.
//!
//! The logo is a 1-bit mask rasterized at runtime into Unicode Braille
//! (2×4 dots per cell) for the terminal's real cell shape, so it is square
//! whatever the font. Port of the press kit's `reference_render.py`; output
//! must stay identical to it (see the tests).

use std::io::IsTerminal;

/// 512×512 binary PBM (P4), 1 = logo. Espressif trademark: used per the
/// press kit's brand guidelines.
static MASK_PBM: &[u8] = include_bytes!("../assets/espressif_symbol_512.pbm");

/// Logo widths in cells, largest first.
const SIZES: [usize; 3] = [31, 27, 20];

/// Brand gradient; t = 0 is top-right (warm), t = 1 bottom-left (deep).
const STOPS: [(f64, [u8; 3]); 3] = [
    (0.0, [0xFF, 0x82, 0x46]),
    (0.5, [0xE8, 0x36, 0x2D]),
    (1.0, [0xB8, 0x12, 0x3C]),
];

/// Bit for the dot at (dx, dy) of a Braille cell: `BRAILLE[dy][dx]`.
const BRAILLE: [[u32; 2]; 4] = [[0x01, 0x08], [0x02, 0x10], [0x04, 0x20], [0x40, 0x80]];

/// ASCII fallback, indexed by quadrants TL<<3 | TR<<2 | BL<<1 | BR.
const QUAD: [char; 16] = [
    ' ', '.', ',', '_', '`', ']', '/', 'd', '\'', '\\', '[', 'b', '"', '#', 'P', '#',
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    None,
    Basic,
    Ansi256,
    TrueColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyphs {
    Braille,
    Ascii,
}

/// The mask as a summed-area table, so the coverage of any rectangle is O(1).
struct Mask {
    w: usize,
    h: usize,
    /// (w+1)×(h+1), row-major.
    sat: Vec<u32>,
}

impl Mask {
    fn parse(data: &[u8]) -> Option<Mask> {
        let mut i = 0;
        let mut tokens = Vec::new();
        while tokens.len() < 3 {
            while data.get(i)?.is_ascii_whitespace() {
                i += 1;
            }
            if data[i] == b'#' {
                while *data.get(i)? != b'\n' {
                    i += 1;
                }
                continue;
            }
            let start = i;
            while !data.get(i)?.is_ascii_whitespace() {
                i += 1;
            }
            tokens.push(&data[start..i]);
        }
        i += 1; // the single whitespace after the height
        if tokens[0] != b"P4" {
            return None;
        }
        let num = |t: &[u8]| std::str::from_utf8(t).ok()?.parse::<usize>().ok();
        let (w, h) = (num(tokens[1])?, num(tokens[2])?);
        let stride = w.div_ceil(8);
        let bits = data.get(i..i + stride * h)?;
        let mut sat = vec![0u32; (w + 1) * (h + 1)];
        for y in 0..h {
            let mut acc = 0;
            for x in 0..w {
                acc += u32::from((bits[y * stride + x / 8] >> (7 - x % 8)) & 1);
                sat[(y + 1) * (w + 1) + x + 1] = sat[y * (w + 1) + x + 1] + acc;
            }
        }
        Some(Mask { w, h, sat })
    }

    fn embedded() -> Mask {
        Mask::parse(MASK_PBM).expect("embedded logo mask is a valid P4 PBM")
    }

    /// Fraction of mask pixels set inside [x0,x1) × [y0,y1), in pixel units.
    fn coverage(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> f64 {
        let (w, h) = (self.w as i64, self.h as i64);
        let ix0 = (x0.floor() as i64).max(0);
        let iy0 = (y0.floor() as i64).max(0);
        let ix1 = (x1.ceil() as i64).max(ix0 + 1).min(w);
        let iy1 = (y1.ceil() as i64).max(iy0 + 1).min(h);
        if ix0 >= w || iy0 >= h || ix1 <= 0 || iy1 <= 0 {
            return 0.0;
        }
        let at = |x: i64, y: i64| i64::from(self.sat[y as usize * (self.w + 1) + x as usize]);
        let n = (ix1 - ix0) * (iy1 - iy0);
        (at(ix1, iy1) - at(ix1, iy0) - at(ix0, iy1) + at(ix0, iy0)) as f64 / n as f64
    }

    /// Sample the square logo into a (cols·sub_x) × (rows·sub_y) dot grid.
    ///
    /// `aspect` is cell height / cell width in pixels. The logo's height in
    /// dots is `cols · sub_y / aspect`; rows are rounded up and the logo is
    /// centered vertically in them, so rounding never distorts it.
    fn dots(&self, cols: usize, aspect: f64, sub: (usize, usize), threshold: f64) -> Grid {
        let (sub_x, sub_y) = sub;
        let dw = cols * sub_x;
        let logo_dh = (cols * sub_y) as f64 / aspect;
        let rows = ((logo_dh / sub_y as f64).ceil() as usize).max(1);
        let dh = rows * sub_y;
        let off = (dh as f64 - logo_dh) / 2.0;
        let (sx, sy) = (self.w as f64 / dw as f64, self.h as f64 / logo_dh);
        let mut on = vec![false; dw * dh];
        for y in 0..dh {
            let (my0, my1) = ((y as f64 - off) * sy, (y as f64 + 1.0 - off) * sy);
            if my1 <= 0.0 || my0 >= self.h as f64 {
                continue;
            }
            for x in 0..dw {
                let c = self.coverage(x as f64 * sx, my0, (x + 1) as f64 * sx, my1);
                on[y * dw + x] = c > threshold;
            }
        }
        Grid { w: dw, rows, on }
    }
}

struct Grid {
    w: usize,
    rows: usize,
    on: Vec<bool>,
}

impl Grid {
    fn at(&self, x: usize, y: usize) -> bool {
        self.on[y * self.w + x]
    }
}

fn gradient(t: f64) -> [u8; 3] {
    for pair in STOPS.windows(2) {
        let ((a, ca), (b, cb)) = (pair[0], pair[1]);
        if t <= b {
            let k = (t - a) / (b - a);
            // Python's round() is round-half-to-even; match it exactly.
            return std::array::from_fn(|i| {
                (f64::from(ca[i]) + (f64::from(cb[i]) - f64::from(ca[i])) * k).round_ties_even()
                    as u8
            });
        }
    }
    STOPS[STOPS.len() - 1].1
}

fn to_256([r, g, b]: [u8; 3]) -> u8 {
    let q = |v: u8| (f64::from(v) / 255.0 * 5.0).round_ties_even() as u8;
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

fn sgr(rgb: [u8; 3], mode: ColorMode) -> String {
    match mode {
        ColorMode::TrueColor => format!("\x1b[38;2;{};{};{}m", rgb[0], rgb[1], rgb[2]),
        ColorMode::Ansi256 => format!("\x1b[38;5;{}m", to_256(rgb)),
        ColorMode::Basic => "\x1b[31m".to_owned(),
        ColorMode::None => String::new(),
    }
}

/// The logo as lines (1-space indent, no trailing spaces, colors reset at
/// the end of each line).
pub fn render(cols: usize, aspect: f64, color: ColorMode, glyphs: Glyphs) -> Vec<String> {
    render_mask(&Mask::embedded(), cols, aspect, color, glyphs)
}

fn render_mask(
    mask: &Mask,
    cols: usize,
    aspect: f64,
    color: ColorMode,
    glyphs: Glyphs,
) -> Vec<String> {
    // The thin outer arc breaks up below ~22 cols at 0.5.
    let threshold = if cols > 22 { 0.5 } else { 0.42 };
    let sub = match glyphs {
        Glyphs::Braille => (2, 4),
        Glyphs::Ascii => (2, 2),
    };
    let grid = mask.dots(cols, aspect, sub, threshold);
    let rows = grid.rows;
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        let mut line = String::from(" ");
        let mut last = String::new();
        for c in 0..cols {
            let ch = match glyphs {
                Glyphs::Braille => {
                    let mut bits = 0;
                    for (dy, row) in BRAILLE.iter().enumerate() {
                        for (dx, bit) in row.iter().enumerate() {
                            if grid.at(2 * c + dx, 4 * r + dy) {
                                bits |= bit;
                            }
                        }
                    }
                    (bits != 0).then(|| char::from_u32(0x2800 + bits).expect("Braille block"))
                }
                Glyphs::Ascii => {
                    let q = |dx, dy| usize::from(grid.at(2 * c + dx, 2 * r + dy));
                    let i = q(0, 0) << 3 | q(1, 0) << 2 | q(0, 1) << 1 | q(1, 1);
                    (i != 0).then_some(QUAD[i])
                }
            };
            let Some(ch) = ch else {
                line.push(' ');
                continue;
            };
            let t = ((cols - 1 - c) as f64 / (cols.max(2) - 1) as f64
                + r as f64 / (rows.max(2) - 1) as f64)
                / 2.0;
            let code = sgr(gradient(t), color);
            if !code.is_empty() && code != last {
                line.push_str(&code);
                last = code;
            }
            line.push(ch);
        }
        let mut line = line.trim_end().to_owned();
        if !last.is_empty() {
            line.push_str("\x1b[0m");
        }
        out.push(line);
    }
    out
}

/// Rows the logo takes at `cols` wide.
fn rows_for(cols: usize, aspect: f64, glyphs: Glyphs) -> usize {
    let sub_y = if glyphs == Glyphs::Braille { 4.0 } else { 2.0 };
    ((cols as f64 * sub_y / aspect / sub_y).ceil() as usize).max(1)
}

/// What the terminal on stdout can show, or `None` if the logo should not be
/// printed at all (not a terminal, `TERM=dumb`, unknown size).
#[derive(Debug, Clone, Copy)]
pub struct Terminal {
    pub cols: usize,
    pub rows: usize,
    pub aspect: f64,
    pub color: ColorMode,
    pub glyphs: Glyphs,
}

impl Terminal {
    pub fn detect() -> Option<Terminal> {
        if !std::io::stdout().is_terminal() {
            return None;
        }
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        let term = var("TERM").unwrap_or_default();
        if term == "dumb" {
            return None;
        }
        let size = tty::size()?;
        let aspect = size
            .aspect()
            .or_else(|| {
                (std::io::stdin().is_terminal())
                    .then(tty::query_cell_size)
                    .flatten()
                    .map(|(h, w)| h / w)
            })
            .unwrap_or(2.0)
            .clamp(1.6, 2.6);
        let color = if std::env::var_os("NO_COLOR").is_some() {
            ColorMode::None
        } else if matches!(var("COLORTERM").as_deref(), Some("truecolor" | "24bit")) {
            ColorMode::TrueColor
        } else if term == "linux" || term == "ansi" || term.starts_with("vt") {
            ColorMode::Basic
        } else {
            ColorMode::Ansi256
        };
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"].into_iter().find_map(var);
        let utf8 = locale.is_some_and(|l| {
            let l = l.to_ascii_lowercase();
            l.contains("utf-8") || l.contains("utf8")
        });
        Some(Terminal {
            cols: size.cols,
            rows: size.rows,
            aspect,
            color,
            glyphs: if utf8 { Glyphs::Braille } else { Glyphs::Ascii },
        })
    }

    /// The largest logo that fits beside `side` text and within `max_rows`.
    pub fn pick(&self, side_width: usize, max_rows: usize) -> Option<usize> {
        // 1-space indent, logo, 3-space gap, text.
        SIZES.into_iter().find(|&cols| {
            1 + cols + 3 + side_width <= self.cols
                && rows_for(cols, self.aspect, self.glyphs) <= max_rows
        })
    }
}

/// The logo with `side` lines vertically centered to its right.
pub fn compose(term: &Terminal, cols: usize, side: &[String]) -> String {
    let logo = render(cols, term.aspect, term.color, term.glyphs);
    let top = logo.len().saturating_sub(side.len()) / 2;
    let mut out = String::new();
    for (i, line) in logo.iter().enumerate() {
        out.push_str(line);
        if let Some(text) = i.checked_sub(top).and_then(|j| side.get(j)) {
            out.push_str(&" ".repeat((1 + cols + 3).saturating_sub(line_width(line))));
            out.push_str(text);
        }
        out.push('\n');
    }
    out
}

/// Width in cells of a logo line: every char is one cell; SGR sequences none.
fn line_width(line: &str) -> usize {
    let mut n = 0;
    let mut in_esc = false;
    for ch in line.chars() {
        match (in_esc, ch) {
            (false, '\x1b') => in_esc = true,
            (true, 'm') => in_esc = false,
            (true, _) => {}
            (false, _) => n += 1,
        }
    }
    n
}

#[cfg(unix)]
mod tty {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    pub struct Size {
        pub cols: usize,
        pub rows: usize,
        xpixel: u16,
        ypixel: u16,
    }

    impl Size {
        /// Cell height / width from the pixel size the kernel reports, if any.
        pub fn aspect(&self) -> Option<f64> {
            (self.xpixel > 0 && self.ypixel > 0 && self.cols > 0 && self.rows > 0).then(|| {
                (f64::from(self.ypixel) / self.rows as f64)
                    / (f64::from(self.xpixel) / self.cols as f64)
            })
        }
    }

    pub fn size() -> Option<Size> {
        let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
        // SAFETY: TIOCGWINSZ writes a winsize into the pointer we pass.
        let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
        (ok && ws.ws_col > 0).then(|| Size {
            cols: ws.ws_col.into(),
            rows: ws.ws_row.into(),
            xpixel: ws.ws_xpixel,
            ypixel: ws.ws_ypixel,
        })
    }

    /// Ask the terminal for its cell size in pixels (`CSI 16 t`, answered
    /// with `CSI 6 ; height ; width t`). Returns (height, width).
    pub fn query_cell_size() -> Option<(f64, f64)> {
        let mut tty = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .ok()?;
        let fd = tty.as_raw_fd();
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        // SAFETY: fd is an open terminal; termios structs are plain data.
        if unsafe { libc::tcgetattr(fd, &mut saved) } != 0 {
            return None;
        }
        let mut raw = saved;
        raw.c_lflag &= !(libc::ICANON | libc::ECHO);
        raw.c_cc[libc::VMIN] = 0;
        raw.c_cc[libc::VTIME] = 0;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &raw) } != 0 {
            return None;
        }
        let reply = (|| {
            tty.write_all(b"\x1b[16t").ok()?;
            tty.flush().ok()?;
            let deadline = Instant::now() + Duration::from_millis(80);
            let mut buf = Vec::new();
            while !buf.ends_with(b"t") && buf.len() < 32 {
                let left = deadline.checked_duration_since(Instant::now())?;
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: one valid pollfd.
                if unsafe { libc::poll(&mut pfd, 1, left.as_millis() as libc::c_int) } <= 0 {
                    return None;
                }
                let mut byte = [0u8; 16];
                let n = tty.read(&mut byte).ok()?;
                buf.extend_from_slice(&byte[..n]);
            }
            Some(buf)
        })();
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
        parse_cell_size(&reply?)
    }

    pub fn parse_cell_size(reply: &[u8]) -> Option<(f64, f64)> {
        let s = std::str::from_utf8(reply).ok()?;
        let body = s.strip_prefix("\x1b[6;")?.strip_suffix('t')?;
        let (h, w) = body.split_once(';')?;
        let (h, w): (f64, f64) = (h.parse().ok()?, w.parse().ok()?);
        (h > 0.0 && w > 0.0).then_some((h, w))
    }
}

#[cfg(not(unix))]
mod tty {
    pub struct Size {
        pub cols: usize,
        pub rows: usize,
    }
    impl Size {
        pub fn aspect(&self) -> Option<f64> {
            None
        }
    }
    pub fn size() -> Option<Size> {
        None
    }
    pub fn query_cell_size() -> Option<(f64, f64)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_parses() {
        let m = Mask::embedded();
        assert_eq!((m.w, m.h), (512, 512));
        // Set-bit count of the press kit's file, counted independently.
        assert_eq!(m.sat[m.sat.len() - 1], 135_703);
    }

    // Expected output comes from reference_render.py in the press kit:
    // `--cols 27 --aspect 2.22 --color none`.
    #[test]
    fn braille_matches_reference() {
        let lines = render(27, 2.22, ColorMode::None, Glyphs::Braille);
        insta::assert_snapshot!(lines.join("\n"));
    }

    // `--cols 20 --aspect 2.0 --color none --ascii`
    #[test]
    fn ascii_matches_reference() {
        let lines = render(20, 2.0, ColorMode::None, Glyphs::Ascii);
        insta::assert_snapshot!(lines.join("\n"));
    }

    // `--cols 20 --aspect 1.93 --color truecolor`, first two lines.
    #[test]
    fn truecolor_matches_reference() {
        let lines = render(20, 1.93, ColorMode::TrueColor, Glyphs::Braille);
        insta::assert_snapshot!(format!("{:?}", &lines[..2]));
    }

    #[test]
    fn gradient_ends_and_middle() {
        assert_eq!(gradient(0.0), [0xFF, 0x82, 0x46]);
        assert_eq!(gradient(0.5), [0xE8, 0x36, 0x2D]);
        assert_eq!(gradient(1.0), [0xB8, 0x12, 0x3C]);
        assert_eq!(to_256([0xE8, 0x36, 0x2D]), 16 + 36 * 5 + 6 + 1);
    }

    #[test]
    fn logo_is_square_in_pixels() {
        for aspect in [1.93, 2.0, 2.22] {
            let rows = rows_for(27, aspect, Glyphs::Braille);
            // The logo's height (27/aspect rows) rounded up: never short.
            assert_eq!(rows, (27.0 / aspect).ceil() as usize);
        }
    }

    #[test]
    fn picks_the_largest_size_that_fits() {
        let t = Terminal {
            cols: 80,
            rows: 50,
            aspect: 2.0,
            color: ColorMode::None,
            glyphs: Glyphs::Braille,
        };
        assert_eq!(t.pick(30, 50), Some(31));
        assert_eq!(t.pick(48, 50), Some(27));
        assert_eq!(t.pick(30, 12), Some(20));
        assert_eq!(t.pick(60, 50), None);
    }

    #[cfg(unix)]
    #[test]
    fn parses_cell_size_reply() {
        assert_eq!(tty::parse_cell_size(b"\x1b[6;37;17t"), Some((37.0, 17.0)));
        assert_eq!(tty::parse_cell_size(b"\x1b[4;800;600t"), None);
    }
}

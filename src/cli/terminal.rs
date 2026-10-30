//! Small Unix terminal adapter. All terminal state is restored by the guard,
//! including on errors/unwind and handled termination signals.
use std::{
    io::{self, Write},
    mem::MaybeUninit,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::Relaxed);
}
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

pub struct Terminal {
    original: libc::termios,
    signals: Vec<(libc::c_int, libc::sigaction)>,
    compact: bool,
    rows: usize,
}

impl Terminal {
    pub fn enter(compact: bool) -> io::Result<Self> {
        let mut original = MaybeUninit::uninit();
        // SAFETY: stdin is a verified TTY; pointers refer to correctly sized structures.
        if unsafe { libc::tcgetattr(0, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let original = unsafe { original.assume_init() };
        let mut terminal = Self {
            original,
            signals: Vec::new(),
            compact,
            rows: 0,
        };
        INTERRUPTED.store(false, Ordering::Relaxed);
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = stop as *const () as usize;
            unsafe {
                libc::sigemptyset(&mut action.sa_mask);
            }
            let mut old = MaybeUninit::uninit();
            if unsafe { libc::sigaction(signal, &action, old.as_mut_ptr()) } != 0 {
                return Err(io::Error::last_os_error());
            }
            terminal
                .signals
                .push((signal, unsafe { old.assume_init() }));
        }
        let mut raw = original;
        unsafe {
            libc::cfmakeraw(&mut raw);
        }
        if unsafe { libc::tcsetattr(0, libc::TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut out = io::stdout().lock();
        if !compact {
            write!(out, "\x1b[?1049h\x1b[2J")?;
        }
        write!(out, "\x1b[?25l")?;
        out.flush()?;
        Ok(terminal)
    }

    pub fn size(&self) -> (usize, usize) {
        let mut size: libc::winsize = unsafe { std::mem::zeroed() };
        if unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut size) } == 0
            && size.ws_col > 0
            && size.ws_row > 0
        {
            (size.ws_col as usize, size.ws_row as usize)
        } else {
            (80, 24)
        }
    }

    pub fn draw(&mut self, lines: &[String], width: usize, height: usize) -> io::Result<()> {
        // Submit each frame together to avoid flicker between clearing and drawing.
        let mut out = Vec::new();
        let count = lines.len().min(height);
        if self.compact {
            if self.rows > 1 {
                write!(out, "\x1b[{}A", self.rows - 1)?;
            }
            write!(out, "\r")?;
        } else {
            write!(out, "\x1b[H\x1b[J")?;
            let top = height.saturating_sub(count) / 2;
            if top > 0 {
                write!(out, "\x1b[{}B", top)?;
            }
        }
        for (i, line) in lines.iter().take(count).enumerate() {
            if i > 0 {
                write!(out, "\r\n")?;
            }
            let line = &line[..line.len().min(width.saturating_sub(1))]; // renderer is ASCII
            let padding = if self.compact {
                0
            } else {
                width.saturating_sub(line.len()) / 2
            };
            write!(out, "\x1b[2K{:padding$}{line}", "")?;
        }
        self.rows = count;
        let mut stdout = io::stdout().lock();
        stdout.write_all(&out)?;
        stdout.flush()
    }

    pub fn key(&self, timeout: Duration) -> io::Result<Option<u8>> {
        let mut fd = libc::pollfd {
            fd: 0,
            events: libc::POLLIN,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut fd, 1, timeout.as_millis() as i32) };
        if result < 0 {
            let error = io::Error::last_os_error();
            return if error.kind() == io::ErrorKind::Interrupted {
                Ok(None)
            } else {
                Err(error)
            };
        }
        if fd.revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0 {
            return Ok(Some(4));
        }
        if fd.revents & libc::POLLIN != 0 {
            let mut key = 0u8;
            let n = unsafe { libc::read(0, (&mut key as *mut u8).cast(), 1) };
            if n == 1 {
                return Ok(Some(key));
            }
            if n == 0 {
                return Ok(Some(4));
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        Ok(None)
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let mut out = io::stdout().lock();
        let _ = write!(out, "\x1b[0m\x1b[?25h");
        if self.compact {
            let _ = write!(out, "\r\n");
        } else {
            let _ = write!(out, "\x1b[?1049l");
        }
        let _ = out.flush();
        unsafe {
            libc::tcsetattr(0, libc::TCSANOW, &self.original);
        }
        for (signal, action) in &self.signals {
            unsafe {
                libc::sigaction(*signal, action, std::ptr::null_mut());
            }
        }
    }
}

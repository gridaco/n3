//! Concrete Alacritty PTY ownership. No document, view, or application commands.
use std::io;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::event_loop::{EventLoop, EventLoopSender, Msg};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::Term;
use alacritty_terminal::tty;

#[derive(Clone)]
pub(super) struct EventProxy {
    sender: mpsc::Sender<Event>,
    context: Option<egui::Context>,
}

impl EventProxy {
    pub(super) fn channel(context: Option<egui::Context>) -> (Self, mpsc::Receiver<Event>) {
        let (sender, receiver) = mpsc::channel();
        (Self { sender, context }, receiver)
    }
}

impl EventListener for EventProxy {
    fn send_event(&self, event: Event) {
        let _ = self.sender.send(event);
        if let Some(context) = &self.context {
            context.request_repaint();
        }
    }
}

pub(super) struct NativePty {
    sender: EventLoopSender,
    cleanup: Option<JoinHandle<()>>,
    #[cfg(unix)]
    child_pid: i32,
    #[cfg(unix)]
    control: std::fs::File,
}

impl NativePty {
    pub(super) fn start(
        term: Arc<FairMutex<Term<EventProxy>>>,
        proxy: EventProxy,
        options: &tty::Options,
        size: WindowSize,
    ) -> io::Result<Self> {
        let pty = tty::new(options, size, 0)?;
        #[cfg(unix)]
        let child_pid = pty.child().id() as i32;
        #[cfg(unix)]
        let control = pty.file().try_clone()?;
        let event_loop = EventLoop::new(term, proxy, pty, true, false)?;
        let sender = event_loop.channel();
        let io = event_loop.spawn();
        // Alacritty returns its PTY from the I/O thread. Drop it on this joining
        // thread so its Child::wait cannot block the application during shutdown.
        let cleanup = thread::spawn(move || {
            drop(io.join());
        });
        Ok(Self {
            sender,
            cleanup: Some(cleanup),
            #[cfg(unix)]
            child_pid,
            #[cfg(unix)]
            control,
        })
    }

    pub(super) fn input(&self, bytes: Vec<u8>) -> bool {
        // Alacritty's writer requires nonempty buffers.
        bytes.is_empty() || self.sender.send(Msg::Input(bytes.into())).is_ok()
    }

    pub(super) fn resize(&self, size: WindowSize) -> bool {
        self.sender.send(Msg::Resize(size)).is_ok()
    }

    pub(super) fn is_finished(&self) -> bool {
        self.cleanup.as_ref().is_none_or(JoinHandle::is_finished)
    }

    #[cfg(unix)]
    fn foreground_group(&self) -> i32 {
        use std::os::fd::AsRawFd;
        unsafe { libc::tcgetpgrp(self.control.as_raw_fd()) }
    }

    #[cfg(unix)]
    fn signal_process_groups(&self, foreground: i32, signal: i32) {
        // tty::new establishes a new session/process group. Signal its current
        // foreground job too: interactive shells may have handed it the PTY.
        if foreground > 1 {
            unsafe {
                libc::kill(-foreground, signal);
            }
        }
        if self.child_pid > 1 && foreground != self.child_pid {
            unsafe {
                libc::kill(-self.child_pid, signal);
            }
        }
    }

    #[cfg(test)]
    #[cfg(unix)]
    pub(super) fn child_pid(&self) -> i32 {
        self.child_pid
    }
}

impl Drop for NativePty {
    fn drop(&mut self) {
        let Some(cleanup) = self.cleanup.take() else {
            return;
        };
        if !cleanup.is_finished() {
            #[cfg(unix)]
            let foreground = self.foreground_group();
            let _ = self.sender.send(Msg::Shutdown);
            #[cfg(unix)]
            self.signal_process_groups(foreground, libc::SIGHUP);
            let grace = Instant::now() + Duration::from_millis(150);
            while !cleanup.is_finished() && Instant::now() < grace {
                thread::sleep(Duration::from_millis(2));
            }
            #[cfg(unix)]
            {
                // The shell can exit before a foreground program that ignores
                // SIGHUP. Reap that owned job even if the I/O cleanup finished.
                if foreground > 1 && foreground != self.child_pid {
                    unsafe {
                        libc::kill(-foreground, libc::SIGKILL);
                    }
                }
                if !cleanup.is_finished() {
                    self.signal_process_groups(foreground, libc::SIGKILL);
                }
            }
            let deadline = Instant::now() + Duration::from_millis(500);
            while !cleanup.is_finished() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(2));
            }
        }
        if cleanup.is_finished() {
            let _ = cleanup.join();
        }
        // A kernel-stalled cleanup must not freeze UI shutdown. If still running,
        // it retains the Child and will reap it when the OS completes termination.
    }
}

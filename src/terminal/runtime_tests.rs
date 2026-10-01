use super::{SessionStatus, TerminalSession};
use egui::Context;

#[test]
fn child_environment_defaults_preserve_explicit_locale_and_overrides() {
    let defaults = super::child_environment(|_| None, &[]);
    assert_eq!(defaults["TERM_PROGRAM"], "N3");
    assert!(defaults["LANG"].ends_with("UTF-8"));
    assert_eq!(defaults["COLORTERM"], "truecolor");
    assert!(defaults.contains_key("PATH"));

    let inherited = |key: &str| match key {
        "LANG" => Some("".into()),
        "LC_CTYPE" => Some("ja_JP.UTF-8".into()),
        "PATH" => Some("/user/bin".into()),
        "TERM_PROGRAM" => Some("Apple_Terminal".into()),
        _ => None,
    };
    let existing = super::child_environment(inherited, &[]);
    assert!(
        !existing.contains_key("LANG"),
        "keep an explicit inherited locale"
    );
    assert!(
        !existing.contains_key("PATH"),
        "keep the inherited login PATH"
    );
    assert_eq!(existing["TERM_PROGRAM"], "N3");
    let explicit = super::child_environment(
        |_| Some("".into()),
        &[("LANG", "C"), ("TERM_PROGRAM", "fixture")],
    );
    assert_eq!(explicit["LANG"], "C");
    assert_eq!(explicit["TERM_PROGRAM"], "fixture");
}

#[test]
fn fixture_uses_real_terminal_replies_without_a_process() {
    let ctx = Context::default();
    let mut session = TerminalSession::interactive_fixture();
    session.resize(80, 24);
    session.set_default_colors([250, 250, 250], [23, 23, 23]);
    session.set_cell_size(8, 16);
    session.ingest(b"\x1b[6n\x1b]10;?\x07\x1b]11;?\x07\x1b[18t");
    session.poll(&ctx);
    let replies = String::from_utf8(session.take_fixture_input()).unwrap();
    assert!(replies.contains("\x1b[1;1R"), "{replies:?}");
    assert!(replies.contains("rgb:fafa/fafa/fafa"), "{replies:?}");
    assert!(replies.contains("rgb:1717/1717/1717"), "{replies:?}");
    assert!(replies.contains("\x1b[8;24;80t"), "{replies:?}");
    assert_eq!(session.status(), &SessionStatus::Fixture);
    assert!(session.runtime.is_none());
}

#[test]
fn unimplemented_enhanced_keyboard_modes_are_not_negotiated() {
    let ctx = Context::default();
    let mut session = TerminalSession::interactive_fixture();
    let mode = session.mode();
    session.ingest(b"\x1b[>1u\x1b[?u");
    session.poll(&ctx);
    assert_eq!(session.mode(), mode);
    assert!(session.take_fixture_input().is_empty());
}

#[test]
fn dormant_and_placeholder_sessions_never_launch_or_accept_input() {
    let mut dormant = TerminalSession::dormant_native();
    let mut placeholder = TerminalSession::placeholder();
    for session in [&mut dormant, &mut placeholder] {
        session.poll(&Context::default());
        assert!(!session.send_input(b"printf forbidden\r".to_vec()));
        assert!(session.runtime.is_none());
    }
}

#[cfg(unix)]
mod unix {
    use super::*;
    use std::time::{Duration, Instant};

    fn controlled(session: &mut TerminalSession, ctx: &Context, script: &str) {
        session
            .start_program(
                ctx,
                "/usr/bin/env",
                &[
                    "-i",
                    "PATH=/usr/bin:/bin",
                    "TERM=xterm-256color",
                    "COLORTERM=truecolor",
                    "LC_ALL=C",
                    "ENV=",
                    "BASH_ENV=",
                    "/bin/sh",
                    "-c",
                    script,
                ],
                None,
                &[],
            )
            .expect("controlled native PTY must start; sandbox denial is a test failure");
    }

    fn wait_for(
        session: &mut TerminalSession,
        ctx: &Context,
        label: &str,
        condition: impl Fn(&TerminalSession) -> bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            session.poll(ctx);
            if condition(session) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "{label}: status {:?}, output {:?}",
                session.status(),
                session.visible_text()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn child_status_does_not_publish_exit_before_output_is_drained() {
        use alacritty_terminal::event::{Event, EventListener};
        use std::os::unix::process::ExitStatusExt;
        let ctx = Context::default();
        let mut session = TerminalSession::dormant_native();
        let (proxy, receiver) = super::super::EventProxy::channel(None);
        session.events = receiver;
        session.status = SessionStatus::Running;
        proxy.send_event(Event::ChildExit(std::process::ExitStatus::from_raw(7 << 8)));
        session.poll(&ctx);
        assert_eq!(session.status(), &SessionStatus::Running);
        session.ingest(b"final output");
        proxy.send_event(Event::Exit);
        session.poll(&ctx);
        assert_eq!(session.status(), &SessionStatus::Exited { code: Some(7) });
        assert!(session.visible_text().contains("final output"));
    }

    #[test]
    fn native_pty_round_trips_input_drains_exit_output_and_restarts() {
        let ctx = Context::default();
        let mut session = TerminalSession::dormant_native();
        session.resize(80, 16);
        controlled(
            &mut session,
            &ctx,
            "printf '\\033[31mPTY-ready\\033[0m\\n'; IFS= read -r value; printf 'accepted:%s\\nlast-output\\n' \"$value\"; exit 7",
        );
        wait_for(&mut session, &ctx, "initial native output", |s| {
            s.visible_text().contains("PTY-ready")
        });
        assert!(session.is_interactive());
        assert!(session.send_input(b"hello from input\r".to_vec()));
        wait_for(&mut session, &ctx, "child exit", |s| {
            matches!(s.status(), SessionStatus::Exited { code: Some(7) })
        });
        assert!(session.visible_text().contains("accepted:hello from input"));
        assert!(session.visible_text().contains("last-output"));
        assert!(!session.is_interactive());
        assert!(!session.send_input(b"ignored".to_vec()));
        controlled(&mut session, &ctx, "printf 'restarted-clean\\n'");
        wait_for(&mut session, &ctx, "restart", |s| {
            matches!(s.status(), SessionStatus::Exited { code: Some(0) })
        });
        assert!(session.visible_text().contains("restarted-clean"));
        assert!(!session.visible_text().contains("PTY-ready"));
    }

    #[test]
    fn native_pty_resizes_and_reports_the_terminal_environment() {
        let ctx = Context::default();
        let mut session = TerminalSession::dormant_native();
        session.resize(80, 12);
        controlled(
            &mut session,
            &ctx,
            "printf '%s|%s\\n' \"$TERM\" \"$COLORTERM\"; stty size; IFS= read -r ignored; stty size; printf resize-complete",
        );
        wait_for(&mut session, &ctx, "first size", |s| {
            s.visible_text().contains("12 80")
        });
        assert!(session.visible_text().contains("xterm-256color|truecolor"));
        session.resize(91, 17);
        session.send_input(b"\r".to_vec());
        wait_for(&mut session, &ctx, "resized child", |s| {
            s.visible_text().contains("resize-complete")
        });
        assert!(session.visible_text().contains("17 91"));
    }

    #[test]
    fn real_tui_cursor_query_gets_a_reply_through_the_native_pty() {
        let ctx = Context::default();
        let mut session = TerminalSession::dormant_native();
        controlled(
            &mut session,
            &ctx,
            "stty -echo -icanon min 1 time 0; printf '\\033[6n'; dd bs=1 count=6 2>/dev/null | od -An -tx1",
        );
        wait_for(&mut session, &ctx, "native terminal reply", |s| {
            matches!(s.status(), SessionStatus::Exited { code: Some(0) })
        });
        assert_eq!(
            session
                .visible_text()
                .split_whitespace()
                .collect::<Vec<_>>(),
            ["1b", "5b", "31", "3b", "31", "52"]
        );
    }

    #[test]
    fn closing_a_session_terminates_and_reaps_a_hangup_ignoring_child() {
        let ctx = Context::default();
        let mut session = TerminalSession::dormant_native();
        controlled(
            &mut session,
            &ctx,
            "trap '' HUP; printf cleanup-ready; while :; do sleep 1; done",
        );
        wait_for(&mut session, &ctx, "cleanup child ready", |s| {
            s.visible_text().contains("cleanup-ready")
        });
        let pid = session.runtime.as_ref().unwrap().child_pid();
        let start = Instant::now();
        drop(session);
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "session shutdown must be bounded"
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while unsafe { libc::kill(pid, 0) } == 0 {
            assert!(
                Instant::now() < deadline,
                "PTY child {pid} survived session drop"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
    }

    #[test]
    fn launch_failure_is_explicit_and_can_be_retried() {
        let ctx = Context::default();
        let mut session = TerminalSession::dormant_native();
        assert!(
            session
                .start_program(&ctx, "/n3/missing-shell", &[], None, &[])
                .is_err()
        );
        assert!(matches!(session.status(), SessionStatus::Failed(_)));
        assert!(session.runtime.is_none());
        controlled(&mut session, &ctx, "printf recovered");
        wait_for(&mut session, &ctx, "recover from launch failure", |s| {
            matches!(s.status(), SessionStatus::Exited { code: Some(0) })
        });
        assert!(session.visible_text().contains("recovered"));
    }
}

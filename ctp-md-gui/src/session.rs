//! Background feed. The UI thread only reads a channel.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::config::{Mode, Settings};
use crate::ctp_feed::{self, LiveMd, SpiEvent};
use crate::mock::Book;
use crate::quote::Quote;

pub enum Cmd {
    Connect { broker: usize, server: usize },
    Subscribe(String),
    Unsubscribe(String),
}

pub enum Evt {
    Status {
        phase: String,
        broker: String,
        server: String,
        front: String,
        detail: String,
    },
    Quote(Quote),
}

enum State {
    Idle,
    Mock,
    Live(LiveMd),
}

pub fn start(settings: Settings) -> (Sender<Cmd>, Receiver<Evt>, JoinHandle<()>) {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("ctp-md".to_owned())
        .spawn(move || run(settings, cmd_rx, evt_tx))
        .expect("spawn market thread");
    (cmd_tx, evt_rx, handle)
}

fn run(settings: Settings, cmd_rx: Receiver<Cmd>, evt_tx: Sender<Evt>) {
    let mut instruments = settings.instruments.clone();
    let (mut book, replay_note) = Book::with_replay(settings.replay.as_deref());
    let mut state = if settings.mode == Mode::Mock {
        emit_mock(&evt_tx, &mut book, &instruments);
        State::Mock
    } else {
        State::Idle
    };
    send_status(
        &evt_tx,
        if settings.mode == Mode::Mock {
            "模拟行情"
        } else {
            "未连接"
        },
        "—",
        "—",
        if settings.mode == Mode::Mock {
            "模拟"
        } else {
            "—"
        },
        match (&settings.broker_note, &replay_note) {
            (Some(note), _) => note.clone(),
            (None, Some(note)) => note.clone(),
            (None, None) if settings.mode == Mode::Mock => "本地模拟，未连接柜台".to_owned(),
            (None, None) => "选择经纪商和服务器后连接".to_owned(),
        },
    );
    let mut tick_at = Instant::now();
    let mut retried = false;
    let mut last_target: Option<(usize, usize)> = None;
    loop {
        let events = match &state {
            State::Live(live) => live.poll(),
            _ => Vec::new(),
        };
        let mut lost = false;
        for event in events {
            match event {
                SpiEvent::Quote(quote) => {
                    if instruments.iter().any(|id| id == &quote.instrument) {
                        let _ = evt_tx.send(Evt::Quote(quote));
                    }
                }
                SpiEvent::Disconnected(code) => {
                    log::info!("md front disconnected {code}");
                    lost = true;
                }
                SpiEvent::Connected | SpiEvent::Login(_) => {}
            }
        }
        if lost {
            state = State::Idle;
            if let (Some((broker, server)), false) = (last_target, retried) {
                if settings.mode != Mode::Mock {
                    retried = true;
                    state = connect(&settings, broker, server, &instruments, &evt_tx, &mut book);
                }
            } else if settings.mode == Mode::Ctp {
                send_status(
                    &evt_tx,
                    "连接失败",
                    "—",
                    "—",
                    "—",
                    "行情前置已断开".to_owned(),
                );
            } else {
                emit_mock(&evt_tx, &mut book, &instruments);
                state = State::Mock;
                send_status(
                    &evt_tx,
                    "模拟行情",
                    "—",
                    "—",
                    "模拟",
                    "前置不可达，已改用模拟行情".to_owned(),
                );
            }
        }

        let wait = Duration::from_millis(50).min(settings.tick);
        match cmd_rx.recv_timeout(wait) {
            Ok(Cmd::Subscribe(id)) => {
                if !instruments.iter().any(|item| item == &id) {
                    instruments.push(id.clone());
                }
                if let State::Live(live) = &state {
                    live.subscribe(std::slice::from_ref(&id));
                }
                if matches!(state, State::Mock) {
                    let _ = evt_tx.send(Evt::Quote(book.quote(&id)));
                }
            }
            Ok(Cmd::Unsubscribe(id)) => {
                instruments.retain(|item| item != &id);
                if let State::Live(live) = &state {
                    live.unsubscribe(&id);
                }
            }
            Ok(Cmd::Connect { broker, server }) => {
                last_target = Some((broker, server));
                state = connect(&settings, broker, server, &instruments, &evt_tx, &mut book);
                retried = false;
            }
            Err(RecvTimeoutError::Timeout) => {
                if matches!(state, State::Mock) && tick_at.elapsed() >= settings.tick {
                    emit_mock(&evt_tx, &mut book, &instruments);
                    tick_at = Instant::now();
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn connect(
    settings: &Settings,
    broker_index: usize,
    server_index: usize,
    instruments: &[String],
    evt_tx: &Sender<Evt>,
    book: &mut Book,
) -> State {
    let Some(broker) = settings.brokers.get(broker_index) else {
        send_status(
            evt_tx,
            "连接失败",
            "—",
            "—",
            "—",
            "没有这个经纪商".to_owned(),
        );
        return State::Idle;
    };
    let Some(server) = broker.servers.get(server_index) else {
        send_status(
            evt_tx,
            "连接失败",
            &broker.name,
            "—",
            "—",
            "没有这个服务器".to_owned(),
        );
        return State::Idle;
    };
    if server.fronts.is_empty() {
        let detail = "该服务器没有行情前置";
        if settings.mode == Mode::Ctp {
            send_status(
                evt_tx,
                "连接失败",
                &broker.name,
                &server.name,
                "—",
                detail.to_owned(),
            );
            return State::Idle;
        }
        emit_mock(evt_tx, book, instruments);
        send_status(
            evt_tx,
            "模拟行情",
            &broker.name,
            &server.name,
            "模拟",
            format!("{detail}，已改用模拟行情"),
        );
        return State::Mock;
    }
    if settings.mode == Mode::Mock {
        emit_mock(evt_tx, book, instruments);
        send_status(
            evt_tx,
            "模拟行情",
            &broker.name,
            &server.name,
            &server.fronts[0],
            "模拟行情（未连接柜台）".to_owned(),
        );
        return State::Mock;
    }

    let mut last = String::from("没有可用的行情前置");
    for (index, front) in server.fronts.iter().enumerate() {
        send_status(
            evt_tx,
            "正在连接",
            &broker.name,
            &server.name,
            front,
            format!("前置 {}/{}", index + 1, server.fronts.len()),
        );
        log::info!("md front {} broker {}", front, broker.id);
        match ctp_feed::connect_front(
            &settings.dynlib,
            &settings.flow,
            front,
            &broker.id,
            settings.front_timeout,
            instruments,
        ) {
            Ok(live) => {
                send_status(
                    evt_tx,
                    "已连接",
                    &broker.name,
                    &server.name,
                    front,
                    "行情前置已连接".to_owned(),
                );
                return State::Live(live);
            }
            Err(err) => {
                log::info!("md front {front} failed: {err}");
                last = err;
            }
        }
    }
    if settings.mode == Mode::Auto {
        emit_mock(evt_tx, book, instruments);
        send_status(
            evt_tx,
            "模拟行情",
            &broker.name,
            &server.name,
            "模拟",
            format!("前置不可达，已改用模拟行情。{last}"),
        );
        State::Mock
    } else {
        send_status(evt_tx, "连接失败", &broker.name, &server.name, "—", last);
        State::Idle
    }
}

fn emit_mock(evt_tx: &Sender<Evt>, book: &mut Book, instruments: &[String]) {
    for instrument in instruments {
        let _ = evt_tx.send(Evt::Quote(book.quote(instrument)));
    }
}

fn send_status(
    evt_tx: &Sender<Evt>,
    phase: &str,
    broker: &str,
    server: &str,
    front: &str,
    detail: String,
) {
    let _ = evt_tx.send(Evt::Status {
        phase: phase.to_owned(),
        broker: broker.to_owned(),
        server: server.to_owned(),
        front: front.to_owned(),
        detail,
    });
}

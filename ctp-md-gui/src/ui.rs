//! Quote board. Broker list, server list, keyboard, subscribe.

use std::rc::Rc;
use std::time::Duration;

use nsh_core::element::input;
use nsh_core::prelude::*;

use crate::broker_xml::{filtered_brokers, Broker};
use crate::config::{self, Mode};
use crate::quote::{self, Quote};
use crate::session::{self, Cmd, Evt};

const BG: fn() -> Hsla = || hsl(222.0, 18.0, 11.0);
const PANEL: fn() -> Hsla = || hsl(222.0, 14.0, 15.0);
const PANEL_ON: fn() -> Hsla = || hsl(210.0, 24.0, 20.0);
const ROW_ON: fn() -> Hsla = || hsl(210.0, 28.0, 24.0);
const LINE: fn() -> Hsla = || hsl(220.0, 10.0, 24.0);
const TEXT: fn() -> Hsla = || hsl(40.0, 12.0, 92.0);
const MUTED: fn() -> Hsla = || hsl(220.0, 8.0, 68.0);
const UP: fn() -> Hsla = || hsl(0.0, 72.0, 58.0);
const DOWN: fn() -> Hsla = || hsl(145.0, 52.0, 42.0);
const BUTTON: fn() -> Hsla = || hsl(210.0, 72.0, 42.0);

#[component]
pub fn App() -> impl IntoElement {
    let settings = config::load_settings(&config::Env::from_process());
    let brokers = Rc::new(settings.brokers.clone());
    let mock = settings.mode == Mode::Mock;
    let seeded = settings
        .instruments
        .iter()
        .map(|id| Quote::placeholder(id))
        .collect::<Vec<_>>();
    let (cmd, events, thread) = session::start(settings);
    std::mem::forget(thread);

    let phase = signal(if mock {
        "模拟行情".to_owned()
    } else {
        "未连接".to_owned()
    });
    let broker_label = signal("—".to_owned());
    let server_label = signal("—".to_owned());
    let front_label = signal(if mock {
        "模拟".to_owned()
    } else {
        "—".to_owned()
    });
    let detail = signal(if mock {
        "本地模拟，未连接柜台".to_owned()
    } else {
        "选择经纪商和服务器后连接".to_owned()
    });
    let quotes = signal(seeded);
    let quote_cursor = signal(0usize);
    let broker_cursor = signal(0usize);
    let server_cursor = signal(0usize);
    let filter = signal(String::new());
    let draft = signal(String::new());
    // The quote board takes the keyboard until the user moves to another pane.
    let pane = signal(2usize);

    set_interval(Duration::from_millis(50), move || {
        while let Ok(event) = events.try_recv() {
            match event {
                Evt::Status {
                    phase: next_phase,
                    broker,
                    server,
                    front,
                    detail: next_detail,
                } => {
                    phase.set(next_phase);
                    broker_label.set(broker);
                    server_label.set(server);
                    front_label.set(front);
                    detail.set(next_detail);
                }
                Evt::Quote(quote) => {
                    quotes.update(|rows| {
                        if let Some(row) = rows
                            .iter_mut()
                            .find(|row| row.instrument == quote.instrument)
                        {
                            *row = quote;
                        }
                    });
                }
            }
        }
    });

    let nav = Nav {
        pane,
        broker_cursor,
        server_cursor,
        quote_cursor,
    };
    let brokers_keys = Rc::clone(&brokers);
    on_key("j", move |_| {
        step_pane(1, &brokers_keys, filter, quotes, nav)
    });
    let brokers_keys = Rc::clone(&brokers);
    on_key("down", move |_| {
        step_pane(1, &brokers_keys, filter, quotes, nav)
    });
    let brokers_keys = Rc::clone(&brokers);
    on_key("k", move |_| {
        step_pane(-1, &brokers_keys, filter, quotes, nav)
    });
    let brokers_keys = Rc::clone(&brokers);
    on_key("up", move |_| {
        step_pane(-1, &brokers_keys, filter, quotes, nav)
    });
    on_key("left", move |_| {
        pane.set(pane.get().saturating_sub(1));
    });
    on_key("right", move |_| {
        pane.set((pane.get() + 1).min(2));
    });
    let brokers_enter = Rc::clone(&brokers);
    let cmd_enter = cmd.clone();
    on_key("enter", move |_| {
        if pane.get() == 0 {
            pane.set(1);
            server_cursor.set(0);
        } else if pane.get() == 1 {
            connect_selected(
                &brokers_enter,
                filter,
                broker_cursor,
                server_cursor,
                &cmd_enter,
            );
        }
    });
    let cmd_unsub = cmd.clone();
    let unsubscribe = move || unsubscribe_selected(quotes, quote_cursor, &cmd_unsub);
    let cmd_unsub_key = cmd.clone();
    on_key("u", move |_| {
        unsubscribe_selected(quotes, quote_cursor, &cmd_unsub_key)
    });
    let cmd_del = cmd.clone();
    on_key("delete", move |_| {
        unsubscribe_selected(quotes, quote_cursor, &cmd_del)
    });

    let cmd_sub = cmd.clone();
    let subscribe = move || subscribe_draft(draft, quotes, quote_cursor, &cmd_sub);
    let cmd_conn = cmd.clone();
    let brokers_conn = Rc::clone(&brokers);
    let connect = move || {
        connect_selected(
            &brokers_conn,
            filter,
            broker_cursor,
            server_cursor,
            &cmd_conn,
        );
    };

    view()
        .flex()
        .flex_col()
        .size_full()
        .bg(BG())
        .text_color(TEXT())
        .child(header(
            phase,
            broker_label,
            server_label,
            front_label,
            detail,
        ))
        .child(toolbar(
            filter,
            broker_cursor,
            draft,
            subscribe.clone(),
            unsubscribe.clone(),
        ))
        .child(
            view()
                .flex()
                .flex_1()
                .min_h(px(240.0))
                .gap(px(8.0))
                .px(px(8.0))
                .child(broker_pane(
                    Rc::clone(&brokers),
                    filter,
                    broker_cursor,
                    server_cursor,
                    pane,
                ))
                .child(server_pane(
                    Rc::clone(&brokers),
                    filter,
                    broker_cursor,
                    server_cursor,
                    pane,
                    connect.clone(),
                ))
                .child(quote_pane(quotes, quote_cursor, pane)),
        )
        .child(
            view().px(px(12.0)).py(px(6.0)).child(
                text("j/k 上下  左右切换栏  Enter 选择或连接  u 退订  输入合约后点订阅")
                    .text_size(px(12.0))
                    .text_color(MUTED()),
            ),
        )
}

fn header(
    phase: Signal<String>,
    broker: Signal<String>,
    server: Signal<String>,
    front: Signal<String>,
    detail: Signal<String>,
) -> impl IntoElement {
    view()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(12.0))
        .py(px(8.0))
        .bg(PANEL())
        .child(
            view()
                .flex()
                .items_center()
                .gap(px(16.0))
                .child(
                    text("CTP 行情")
                        .font_weight(FontWeight::BOLD)
                        .text_size(px(18.0)),
                )
                .child(
                    text(move || phase.get())
                        .text_size(px(14.0))
                        .text_color(UP()),
                )
                .child(text(move || format!("经纪商 {}", broker.get())).text_size(px(13.0)))
                .child(text(move || format!("服务器 {}", server.get())).text_size(px(13.0)))
                .child(text(move || format!("前置 {}", front.get())).text_size(px(13.0))),
        )
        .child(
            text(move || detail.get())
                .text_size(px(12.0))
                .text_color(MUTED()),
        )
}

fn toolbar(
    filter: Signal<String>,
    broker_cursor: Signal<usize>,
    draft: Signal<String>,
    subscribe: impl Fn() + Clone + 'static,
    unsubscribe: impl Fn() + Clone + 'static,
) -> impl IntoElement {
    let submit = subscribe.clone();
    view()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(8.0))
        .child(
            input()
                .test_id("broker-search")
                .w(px(220.0))
                .placeholder("搜索经纪商")
                .on_change(move |text: &str| {
                    filter.set(text.to_owned());
                    broker_cursor.set(0);
                }),
        )
        .child(
            input()
                .test_id("instrument")
                .w(px(160.0))
                .placeholder("合约，如 rb2610")
                .on_change(move |text: &str| draft.set(text.to_owned()))
                .on_submit(move |_text: &str| submit()),
        )
        .child(action("订阅", subscribe))
        .child(action("退订", unsubscribe))
}

fn action(label: &'static str, press: impl Fn() + 'static) -> impl IntoElement {
    view()
        .flex()
        .items_center()
        .px(px(12.0))
        .py(px(6.0))
        .rounded(px(4.0))
        .bg(BUTTON())
        .on_press(press)
        .child(
            text(label)
                .text_color(hsl(0.0, 0.0, 100.0))
                .text_size(px(14.0)),
        )
}

fn broker_pane(
    brokers: Rc<Vec<Broker>>,
    filter: Signal<String>,
    broker_cursor: Signal<usize>,
    server_cursor: Signal<usize>,
    pane: Signal<usize>,
) -> impl IntoElement {
    view()
        .flex()
        .flex_col()
        .w(px(280.0))
        .bg(move || if pane.get() == 0 { PANEL_ON() } else { PANEL() })
        .child(pane_title("经纪商"))
        .child(scroll().flex_1().child(dynamic(move || {
            let ids = filtered_brokers(&brokers, &filter.get());
            let cursor = clamp_cursor(broker_cursor.get(), ids.len());
            let mut column = view().flex().flex_col();
            if ids.is_empty() {
                return column.child(text("无匹配经纪商").text_color(MUTED()).px(px(8.0)));
            }
            for (row, index) in ids.into_iter().enumerate() {
                let broker = &brokers[index];
                let name = broker.name.clone();
                let id = broker.id.clone();
                let on = row == cursor;
                column = column.child(
                    view()
                        .flex()
                        .items_center()
                        .h(px(32.0))
                        .px(px(8.0))
                        .bg(if on { ROW_ON() } else { PANEL() })
                        .on_press(move || {
                            broker_cursor.set(row);
                            server_cursor.set(0);
                            pane.set(0);
                        })
                        .child(text(name).flex_1().text_size(px(13.0)))
                        .child(text(id).text_size(px(12.0)).text_color(MUTED())),
                );
            }
            column
        })))
}

fn server_pane(
    brokers: Rc<Vec<Broker>>,
    filter: Signal<String>,
    broker_cursor: Signal<usize>,
    server_cursor: Signal<usize>,
    pane: Signal<usize>,
    connect: impl Fn() + Clone + 'static,
) -> impl IntoElement {
    let connect_button = connect.clone();
    view()
        .flex()
        .flex_col()
        .w(px(240.0))
        .bg(move || if pane.get() == 1 { PANEL_ON() } else { PANEL() })
        .child(pane_title("服务器"))
        .child(scroll().flex_1().child(dynamic(move || {
            let ids = filtered_brokers(&brokers, &filter.get());
            let broker_index = ids
                .get(clamp_cursor(broker_cursor.get(), ids.len()))
                .copied();
            let mut column = view().flex().flex_col();
            let Some(broker_index) = broker_index else {
                return column.child(text("无服务器").text_color(MUTED()).px(px(8.0)));
            };
            let servers = &brokers[broker_index].servers;
            let cursor = clamp_cursor(server_cursor.get(), servers.len());
            for (row, server) in servers.iter().enumerate() {
                let name = server.name.clone();
                let count = format!("{} 个前置", server.fronts.len());
                let on = row == cursor;
                let connect = connect.clone();
                column = column.child(
                    view()
                        .flex()
                        .flex_col()
                        .px(px(8.0))
                        .py(px(4.0))
                        .bg(if on { ROW_ON() } else { PANEL() })
                        .on_press(move || {
                            server_cursor.set(row);
                            pane.set(1);
                            connect();
                        })
                        .child(text(name).text_size(px(13.0)))
                        .child(text(count).text_size(px(12.0)).text_color(MUTED())),
                );
            }
            column
        })))
        .child(view().p(px(8.0)).child(action("连接", connect_button)))
}

fn quote_pane(
    quotes: Signal<Vec<Quote>>,
    quote_cursor: Signal<usize>,
    pane: Signal<usize>,
) -> impl IntoElement {
    view()
        .flex()
        .flex_col()
        .flex_1()
        .bg(move || if pane.get() == 2 { PANEL_ON() } else { PANEL() })
        .child(
            view()
                .flex()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .child(pane_title("行情"))
                .child(
                    text(move || {
                        let rows = quotes.get();
                        match rows.get(clamp_cursor(quote_cursor.get(), rows.len())) {
                            Some(quote) => format!("选中 {}", quote.instrument),
                            None => "选中 无".to_owned(),
                        }
                    })
                    .text_size(px(13.0))
                    .text_color(MUTED()),
                ),
        )
        .child(quote_header())
        .child(scroll().flex_1().child(dynamic(move || {
            let rows = quotes.get();
            let cursor = clamp_cursor(quote_cursor.get(), rows.len());
            let mut column = view().flex().flex_col();
            for (index, quote) in rows.iter().enumerate() {
                column = column.child(quote_row(quote, index == cursor, quote_cursor, index, pane));
            }
            column
        })))
}

fn quote_header() -> impl IntoElement {
    view()
        .flex()
        .items_center()
        .h(px(28.0))
        .px(px(4.0))
        .border_b_1()
        .border_b_color(LINE())
        .child(head_cell("合约", 110.0))
        .child(head_cell("最新", 90.0))
        .child(head_cell("买价", 90.0))
        .child(head_cell("卖价", 90.0))
        .child(head_cell("成交量", 80.0))
        .child(head_cell("持仓量", 90.0))
        .child(head_cell("涨跌", 80.0))
        .child(head_cell("时间", 110.0))
}

fn quote_row(
    quote: &Quote,
    selected: bool,
    cursor: Signal<usize>,
    index: usize,
    pane: Signal<usize>,
) -> impl IntoElement {
    let color = move_color(quote::move_of(quote.change()));
    let instrument = quote.instrument.clone();
    view()
        .flex()
        .items_center()
        .h(px(30.0))
        .px(px(4.0))
        .bg(if selected { ROW_ON() } else { PANEL() })
        .on_press(move || {
            cursor.set(index);
            pane.set(2);
        })
        .child(body_cell(instrument, 110.0, TEXT()))
        .child(body_cell(quote::format_price(quote.last), 90.0, color))
        .child(body_cell(quote::format_price(quote.bid), 90.0, UP()))
        .child(body_cell(quote::format_price(quote.ask), 90.0, DOWN()))
        .child(body_cell(quote.volume.to_string(), 80.0, TEXT()))
        .child(body_cell(
            quote::format_oi(quote.open_interest),
            90.0,
            TEXT(),
        ))
        .child(body_cell(quote::format_change(quote.change()), 80.0, color))
        .child(body_cell(quote.update_time.clone(), 110.0, MUTED()))
}

fn pane_title(label: &'static str) -> impl IntoElement {
    view().px(px(8.0)).py(px(6.0)).child(
        text(label)
            .font_weight(FontWeight::SEMIBOLD)
            .text_size(px(13.0)),
    )
}

fn head_cell(label: &'static str, width: f32) -> impl IntoElement {
    view()
        .w(px(width))
        .child(text(label).text_size(px(12.0)).text_color(MUTED()))
}

fn body_cell(label: String, width: f32, color: Hsla) -> impl IntoElement {
    view()
        .w(px(width))
        .child(text(label).text_size(px(13.0)).text_color(color))
}

fn move_color(direction: quote::Move) -> Hsla {
    match direction {
        quote::Move::Up => UP(),
        quote::Move::Down => DOWN(),
        quote::Move::Flat => TEXT(),
    }
}

fn clamp_cursor(cursor: usize, len: usize) -> usize {
    if len == 0 {
        0
    } else {
        cursor.min(len - 1)
    }
}

#[derive(Clone, Copy)]
struct Nav {
    pane: Signal<usize>,
    broker_cursor: Signal<usize>,
    server_cursor: Signal<usize>,
    quote_cursor: Signal<usize>,
}

fn step_pane(
    delta: i32,
    brokers: &[Broker],
    filter: Signal<String>,
    quotes: Signal<Vec<Quote>>,
    nav: Nav,
) {
    match nav.pane.get() {
        0 => {
            let len = filtered_brokers(brokers, &filter.get()).len();
            step_index(nav.broker_cursor, len, delta);
            nav.server_cursor.set(0);
        }
        1 => {
            let ids = filtered_brokers(brokers, &filter.get());
            let len = ids
                .get(clamp_cursor(nav.broker_cursor.get(), ids.len()))
                .map(|index| brokers[*index].servers.len())
                .unwrap_or(0);
            step_index(nav.server_cursor, len, delta);
        }
        _ => {
            let len = quotes.with(|rows| rows.len());
            step_index(nav.quote_cursor, len, delta);
        }
    }
}

fn step_index(cursor: Signal<usize>, len: usize, delta: i32) {
    if len == 0 {
        return;
    }
    let next = (cursor.get() as i32 + delta).rem_euclid(len as i32) as usize;
    cursor.set(next);
}

fn connect_selected(
    brokers: &[Broker],
    filter: Signal<String>,
    broker_cursor: Signal<usize>,
    server_cursor: Signal<usize>,
    cmd: &std::sync::mpsc::Sender<Cmd>,
) {
    let ids = filtered_brokers(brokers, &filter.get());
    let Some(broker) = ids
        .get(clamp_cursor(broker_cursor.get(), ids.len()))
        .copied()
    else {
        return;
    };
    let _ = cmd.send(Cmd::Connect {
        broker,
        server: server_cursor.get(),
    });
}

fn subscribe_draft(
    draft: Signal<String>,
    quotes: Signal<Vec<Quote>>,
    quote_cursor: Signal<usize>,
    cmd: &std::sync::mpsc::Sender<Cmd>,
) {
    let id = draft.get().trim().to_owned();
    if id.is_empty() {
        return;
    }
    let mut cursor = quotes.with(|rows| {
        rows.iter()
            .position(|row| row.instrument == id)
            .unwrap_or(rows.len())
    });
    quotes.update(|rows| {
        if let Some(index) = rows.iter().position(|row| row.instrument == id) {
            cursor = index;
        } else {
            rows.push(Quote::placeholder(&id));
            cursor = rows.len() - 1;
        }
    });
    quote_cursor.set(cursor);
    let _ = cmd.send(Cmd::Subscribe(id));
}

fn unsubscribe_selected(
    quotes: Signal<Vec<Quote>>,
    quote_cursor: Signal<usize>,
    cmd: &std::sync::mpsc::Sender<Cmd>,
) {
    let rows = quotes.get();
    let index = clamp_cursor(quote_cursor.get(), rows.len());
    let Some(id) = rows.get(index).map(|row| row.instrument.clone()) else {
        return;
    };
    quotes.update(|rows| {
        if rows.get(index).is_some_and(|row| row.instrument == id) {
            rows.remove(index);
        }
    });
    let len = quotes.with(|rows| rows.len());
    if len > 0 {
        quote_cursor.set(index.min(len - 1));
    }
    let _ = cmd.send(Cmd::Unsubscribe(id));
}

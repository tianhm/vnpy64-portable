//! Headless mock board: subscribe, keyboard, broker search.

use std::time::Duration;

use nsh_engine::{EngineConfig, Idiom, Os, Points, Root, Size};
use nsh_testing::Headless;

fn set_env(key: &str, value: &str) {
    unsafe { std::env::set_var(key, value) }
}

#[test]
fn mock_board_subscribes_searches_and_moves() {
    set_env("CTP_MD_MODE", "mock");
    let mut config = EngineConfig::new(Root::of::<ctp_md_gui::App>());
    config.viewport = Size::new(Points(1400.0), Points(900.0));
    config.environment.window = config.viewport;
    config.environment.os = Os::Linux;
    config.environment.idiom = Idiom::Desktop;
    let mut app = Headless::start(config).expect("start");
    app.wait_for("模拟行情").expect("phase");
    app.wait_for("rb2610").expect("rb");
    app.wait_for("ag2612").expect("ag");
    app.wait_for("选中 rb2610").expect("selection");
    app.press_key("j").expect("j");
    app.wait_for("选中 ag2612").expect("moved");
    app.type_text("id:instrument", "cu2610").expect("type");
    app.press("订阅").expect("subscribe");
    app.clear("id:instrument").expect("clear");
    app.wait_for("cu2610").expect("cu");
    app.press("退订").expect("unsubscribe");
    app.wait_for_gone("cu2610", Duration::from_secs(5))
        .expect("gone");
    app.type_text("id:broker-search", "渤海").expect("search");
    app.wait_for_within("B渤海期货", Duration::from_secs(5))
        .expect("bohai");
    app.wait_for_gone("A安粮期货", Duration::from_secs(5))
        .expect("filtered");
}

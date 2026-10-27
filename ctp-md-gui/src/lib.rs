//! CTP market-data board. Mock quotes drive the same UI when a front is unreachable.

#[cfg(not(any(
    feature = "ctp_v6_7_2",
    feature = "ctp_v6_7_7",
    feature = "ctp_v6_7_11",
    feature = "ctp_v6_7_13"
)))]
compile_error!("enable one CTP version feature, for example ctp_v6_7_7");

mod broker_xml;
mod config;
mod ctp_feed;
mod mock;
mod quote;
mod session;
mod ui;

pub use config::apply_cli;
pub use ui::App;

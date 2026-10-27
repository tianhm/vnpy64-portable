//! ctp2rs market-data API. UserID and Password stay empty.
//! BrokerID comes from the selected broker.xml entry.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::time::{Duration, Instant};

use ctp2rs::ffi::{AssignFromString, DecodeString};
use ctp2rs::v1alpha1::{
    CThostFtdcDepthMarketDataField, CThostFtdcReqUserLoginField, CThostFtdcRspInfoField,
    CThostFtdcRspUserLoginField, MdApi, MdSpi,
};

use crate::config::dynlib_filename;
use crate::quote::{valid_price, Quote};

pub struct LiveMd {
    api: MdApi,
    events: Receiver<SpiEvent>,
}

pub enum SpiEvent {
    Connected,
    Disconnected(i32),
    Login(Result<(), String>),
    Quote(Quote),
}

struct SpiBridge {
    tx: SyncSender<SpiEvent>,
}

impl MdSpi for SpiBridge {
    fn on_front_connected(&mut self) {
        let _ = self.tx.try_send(SpiEvent::Connected);
    }

    fn on_front_disconnected(&mut self, reason: i32) {
        let _ = self.tx.try_send(SpiEvent::Disconnected(reason));
    }

    fn on_rsp_user_login(
        &mut self,
        _login: Option<&CThostFtdcRspUserLoginField>,
        info: Option<&CThostFtdcRspInfoField>,
        _request_id: i32,
        is_last: bool,
    ) {
        if let Err(err) = rsp_ok(info) {
            let _ = self.tx.try_send(SpiEvent::Login(Err(err)));
            return;
        }
        if is_last {
            let _ = self.tx.try_send(SpiEvent::Login(Ok(())));
        }
    }

    fn on_rtn_depth_market_data(&mut self, depth: Option<&CThostFtdcDepthMarketDataField>) {
        if let Some(depth) = depth {
            let _ = self.tx.try_send(SpiEvent::Quote(quote_from_depth(depth)));
        }
    }
}

impl LiveMd {
    pub fn subscribe(&self, ids: &[String]) {
        if ids.is_empty() {
            return;
        }
        let _ = self.api.subscribe_market_data(ids);
    }

    pub fn unsubscribe(&self, id: &str) {
        let _ = self.api.unsubscribe_market_data(&[id]);
    }

    pub fn poll(&self) -> Vec<SpiEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }
}

pub fn connect_front(
    dynlib: &Path,
    flow: &Path,
    front: &str,
    broker_id: &str,
    timeout: Duration,
    instruments: &[String],
) -> Result<LiveMd, String> {
    if !dynlib.is_file() {
        return Err(format!(
            "找不到行情库 {}。将 {} 放在当前目录或程序旁边，或设置 CTP_MD_DYNLIB。不要使用本仓库的 Win64/vnctpmd.pyd 或 Linux64/thostmduserapi.so（约 6.3.5，ABI 不同）",
            dynlib.display(),
            dynlib_filename()
        ));
    }
    if let Err(err) = std::fs::create_dir_all(flow) {
        return Err(format!("无法创建流文件目录 {}: {err}", flow.display()));
    }
    let flow_path = flow_arg(flow);
    let dynlib_owned = dynlib.to_path_buf();
    let api = catch_unwind(AssertUnwindSafe(|| create_api(&dynlib_owned, &flow_path))).map_err(
        |payload| {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "动态库加载失败".to_owned());
            format!("加载行情库失败: {detail}")
        },
    )?;

    let (tx, rx) = mpsc::sync_channel(1024);
    let spi = Box::into_raw(Box::new(SpiBridge { tx })) as *mut dyn MdSpi;
    api.register_front(front);
    api.register_spi(spi);
    api.init();

    wait_until(&rx, timeout, WaitFor::Connected)?;
    let mut req = CThostFtdcReqUserLoginField::default();
    req.BrokerID.assign_from_str(broker_id);
    req.UserID.assign_from_str("");
    req.Password.assign_from_str("");
    let code = api.req_user_login(&mut req, 1);
    if code != 0 {
        return Err(format!("ReqUserLogin 返回 {code}"));
    }
    wait_until(&rx, timeout, WaitFor::Login)?;
    if !instruments.is_empty() {
        let _ = api.subscribe_market_data(instruments);
    }
    Ok(LiveMd { api, events: rx })
}

#[cfg(not(any(feature = "ctp_v6_7_11", feature = "ctp_v6_7_13")))]
fn create_api(dynlib: &Path, flow: &str) -> MdApi {
    MdApi::create_api(dynlib, flow, false, false)
}

#[cfg(any(feature = "ctp_v6_7_11", feature = "ctp_v6_7_13"))]
fn create_api(dynlib: &Path, flow: &str) -> MdApi {
    // ctp2rs passes production mode for the union CreateFtdcMdApi.
    MdApi::create_api(dynlib, flow, false, false, true)
}

enum WaitFor {
    Connected,
    Login,
}

fn wait_until(rx: &Receiver<SpiEvent>, timeout: Duration, want: WaitFor) -> Result<(), String> {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(match want {
                WaitFor::Connected => "等待前置连接超时".to_owned(),
                WaitFor::Login => "等待行情登录响应超时".to_owned(),
            });
        }
        match rx.recv_timeout(left.min(Duration::from_millis(200))) {
            Ok(SpiEvent::Connected) => {
                if matches!(want, WaitFor::Connected) {
                    return Ok(());
                }
            }
            Ok(SpiEvent::Login(result)) => {
                if matches!(want, WaitFor::Login) {
                    return result;
                }
            }
            Ok(SpiEvent::Disconnected(code)) => {
                return Err(format!("前置断开 ({code})"));
            }
            Ok(SpiEvent::Quote(_)) => {}
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err("行情回调已结束".to_owned());
            }
        }
    }
}

fn rsp_ok(info: Option<&CThostFtdcRspInfoField>) -> Result<(), String> {
    let Some(info) = info else {
        return Ok(());
    };
    if info.ErrorID == 0 {
        return Ok(());
    }
    let message = info
        .ErrorMsg
        .try_decode()
        .unwrap_or_else(|_| format!("错误 {}", info.ErrorID));
    Err(format!("{message} ({})", info.ErrorID))
}

pub fn quote_from_depth(depth: &CThostFtdcDepthMarketDataField) -> Quote {
    let instrument = depth
        .InstrumentID
        .try_decode()
        .unwrap_or_default()
        .trim()
        .to_owned();
    let update = depth.UpdateTime.try_decode().unwrap_or_default();
    let update_time = if depth.UpdateMillisec > 0 {
        format!("{update}.{:03}", depth.UpdateMillisec)
    } else if update.is_empty() {
        "--".to_owned()
    } else {
        update
    };
    Quote {
        instrument,
        last: valid_price(depth.LastPrice),
        bid: valid_price(depth.BidPrice1),
        ask: valid_price(depth.AskPrice1),
        bid_vol: depth.BidVolume1,
        ask_vol: depth.AskVolume1,
        volume: depth.Volume,
        open_interest: depth.OpenInterest,
        pre_settlement: valid_price(depth.PreSettlementPrice),
        update_time,
    }
}

fn flow_arg(flow: &Path) -> String {
    let mut text = flow.to_string_lossy().into_owned();
    if !text.ends_with(['/', '\\']) {
        text.push(if cfg!(windows) { '\\' } else { '/' });
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use ctp2rs::ffi::AssignFromString;

    #[test]
    fn depth_field_maps_onto_the_board() {
        let mut depth: CThostFtdcDepthMarketDataField = unsafe { std::mem::zeroed() };
        depth.InstrumentID.assign_from_str("rb2610");
        depth.LastPrice = 3510.0;
        depth.PreSettlementPrice = 3500.0;
        depth.BidPrice1 = 3509.0;
        depth.AskPrice1 = 3511.0;
        depth.BidVolume1 = 3;
        depth.AskVolume1 = 4;
        depth.Volume = 100;
        depth.OpenInterest = 200.0;
        depth.UpdateTime.assign_from_str("09:01:02");
        depth.UpdateMillisec = 5;
        depth.HighestPrice = f64::MAX;
        let quote = quote_from_depth(&depth);
        assert_eq!(quote.instrument, "rb2610");
        assert_eq!(quote.last, Some(3510.0));
        assert_eq!(quote.change(), Some(10.0));
        assert_eq!(quote.bid_vol, 3);
        assert_eq!(quote.update_time, "09:01:02.005");
    }
}

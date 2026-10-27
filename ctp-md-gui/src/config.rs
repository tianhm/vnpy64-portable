//! Process environment and an optional toml file. No secrets.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::broker_xml::{self, Broker};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Mock,
    Ctp,
    Auto,
}

impl Mode {
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mock" => Self::Mock,
            "ctp" => Self::Ctp,
            "auto" => Self::Auto,
            _ => Self::Auto,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Env {
    pub mode: Option<String>,
    pub broker_xml: Option<String>,
    pub dynlib: Option<String>,
    pub flow: Option<String>,
    pub instruments: Option<String>,
    pub tick_ms: Option<String>,
    pub front_timeout_secs: Option<String>,
    pub replay: Option<String>,
    pub config: Option<String>,
}

impl Env {
    pub fn from_process() -> Self {
        Self {
            mode: var("CTP_MD_MODE"),
            broker_xml: var("BROKER_XML"),
            dynlib: var("CTP_MD_DYNLIB"),
            flow: var("CTP_MD_FLOW"),
            instruments: var("CTP_MD_INSTRUMENTS"),
            tick_ms: var("CTP_MD_TICK_MS"),
            front_timeout_secs: var("CTP_MD_FRONT_TIMEOUT_SECS"),
            replay: var("CTP_MD_REPLAY"),
            config: var("CTP_MD_CONFIG"),
        }
    }
}

fn var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub mode: Mode,
    pub brokers: Vec<Broker>,
    pub broker_note: Option<String>,
    pub dynlib: PathBuf,
    pub flow: PathBuf,
    pub instruments: Vec<String>,
    pub tick: Duration,
    pub front_timeout: Duration,
    pub replay: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Default)]
struct FileConfig {
    mode: Option<String>,
    broker_xml: Option<String>,
    dynlib: Option<String>,
    flow: Option<String>,
    instruments: Option<Vec<String>>,
    tick_ms: Option<u64>,
    front_timeout_secs: Option<u64>,
    replay: Option<String>,
}

pub fn load_settings(env: &Env) -> Settings {
    let file = env
        .config
        .as_deref()
        .map(load_file)
        .transpose()
        .unwrap_or_else(|err| {
            log::warn!("{err}");
            None
        })
        .unwrap_or_default();

    let mode = Mode::parse(
        env.mode
            .as_deref()
            .or(file.mode.as_deref())
            .unwrap_or("auto"),
    );
    let broker_path = env
        .broker_xml
        .clone()
        .or(file.broker_xml)
        .map(PathBuf::from);
    let (list, broker_note) = broker_xml::load_broker_xml(broker_path.as_deref());
    let dynlib = resolve_dynlib(
        env.dynlib
            .clone()
            .or(file.dynlib)
            .map(PathBuf::from)
            .as_deref(),
    );
    let flow = env
        .flow
        .clone()
        .or(file.flow)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("md_flow"));
    let instruments = env
        .instruments
        .as_deref()
        .map(|value| {
            value
                .split([',', ' ', ';'])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|items| !items.is_empty())
        .or(file.instruments)
        .unwrap_or_else(|| vec!["rb2610".to_owned(), "ag2612".to_owned()]);
    let tick_ms = env
        .tick_ms
        .as_deref()
        .and_then(|value| value.parse().ok())
        .or(file.tick_ms)
        .unwrap_or(500);
    let timeout_secs = env
        .front_timeout_secs
        .as_deref()
        .and_then(|value| value.parse().ok())
        .or(file.front_timeout_secs)
        .unwrap_or(8);
    let replay = env
        .replay
        .clone()
        .or(file.replay)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    Settings {
        mode,
        brokers: list.brokers,
        broker_note,
        dynlib,
        flow,
        instruments,
        tick: Duration::from_millis(tick_ms.max(50)),
        front_timeout: Duration::from_secs(timeout_secs.max(1)),
        replay,
    }
}

fn load_file(path: &str) -> Result<FileConfig, String> {
    let text =
        std::fs::read_to_string(path).map_err(|err| format!("读取配置 {path} 失败: {err}"))?;
    toml::from_str(&text).map_err(|err| format!("解析配置 {path} 失败: {err}"))
}

pub fn dynlib_filename() -> &'static str {
    if cfg!(windows) {
        "thostmduserapi_se.dll"
    } else if cfg!(target_os = "macos") {
        "thostmduserapi_se.dylib"
    } else {
        "thostmduserapi_se.so"
    }
}

pub fn resolve_dynlib(configured: Option<&Path>) -> PathBuf {
    if let Some(path) = configured.filter(|path| !path.as_os_str().is_empty()) {
        return path.to_path_buf();
    }
    let name = dynlib_filename();
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let in_cwd = cwd.join(name);
    if in_cwd.is_file() {
        return in_cwd;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join(name);
            if beside.is_file() {
                return beside;
            }
        }
    }
    in_cwd
}

pub fn apply_cli() {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mock" => set_env("CTP_MD_MODE", "mock"),
            "--ctp" => set_env("CTP_MD_MODE", "ctp"),
            "--mode" => set_env("CTP_MD_MODE", &need(&mut args, "--mode")),
            "--broker-xml" => set_env("BROKER_XML", &need(&mut args, "--broker-xml")),
            "--config" => set_env("CTP_MD_CONFIG", &need(&mut args, "--config")),
            "--dynlib" => set_env("CTP_MD_DYNLIB", &need(&mut args, "--dynlib")),
            "--help" | "-h" => {
                println!(
                    "\
ctp-md-gui — CTP 行情看板

  cargo run --release -- --mock
  cargo run --release
  cargo run --release -- --ctp --dynlib <thostmduserapi_se.dll>

不需要账号、密码。BrokerID 只取自所选经纪商。
--mock  本地模拟，不访问前置
--ctp   只连所选服务器的行情前置
默认    前置不通时改用模拟行情
"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("未知参数 {other}，可用 --help");
                std::process::exit(2);
            }
        }
    }
}

fn need(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("{flag} 需要一个参数");
        std::process::exit(2);
    })
}

fn set_env(key: &str, value: &str) {
    // `std::env::set_var` is unsafe on recent toolchains. The CLI runs before
    // any other thread starts.
    unsafe { std::env::set_var(key, value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overrides_file_and_defaults_instruments() {
        let env = Env {
            mode: Some("mock".to_owned()),
            instruments: Some("cu2610".to_owned()),
            ..Env::default()
        };
        let settings = load_settings(&env);
        assert_eq!(settings.mode, Mode::Mock);
        assert_eq!(settings.instruments, vec!["cu2610".to_owned()]);
        assert!(!settings.brokers.is_empty());
        assert!(settings.broker_note.is_none());
    }

    #[test]
    fn missing_broker_xml_falls_back() {
        let env = Env {
            broker_xml: Some("this-file-does-not-exist.xml".to_owned()),
            ..Env::default()
        };
        let settings = load_settings(&env);
        assert!(settings.broker_note.unwrap().contains("已改用内置"));
        assert_eq!(settings.brokers.len(), 151);
    }
}

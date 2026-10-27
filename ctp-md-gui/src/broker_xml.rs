//! Shinny / openctp `broker.xml`.
//!
//! Expected shape (UTF-8 or GB2312/GBK):
//!
//! ```xml
//! <root>
//!   <broker BrokerID="0052" BrokerName="A安粮期货" BrokerEName="alqh">
//!     <Servers>
//!       <Server>
//!         <Name>次席</Name>
//!         <MarketData>
//!           <item>101.226.249.90:33213</item>
//!         </MarketData>
//!         <Trading>
//!           <item>ignored</item>
//!         </Trading>
//!       </Server>
//!     </Servers>
//!   </broker>
//! </root>
//! ```
//!
//! `ApiType="UFX"` (and any other non-empty type that is not `CTP`, including
//! `XONE`) is skipped. A missing `ApiType` is CTP. The tag `MarKetData` is
//! accepted because 金瑞期货 spells it that way. `Trading/item` is never used.
//! `AuthCode2` / `AuthCode3` are ignored and are not copied into the model.
//! Market-data items are `host:port` or already `tcp://host:port`.

use std::path::Path;

use encoding_rs::GBK;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    pub fronts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Broker {
    pub id: String,
    pub name: String,
    pub ename: String,
    pub servers: Vec<Server>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrokerList {
    pub brokers: Vec<Broker>,
    pub skipped_non_ctp: usize,
}

pub fn bundled_xml() -> &'static str {
    include_str!("../broker.xml")
}

pub fn bundled_brokers() -> BrokerList {
    parse_broker_xml(bundled_xml()).expect("bundled broker.xml")
}

pub fn load_broker_xml(path: Option<&Path>) -> (BrokerList, Option<String>) {
    let Some(path) = path.filter(|path| !path.as_os_str().is_empty()) else {
        return (bundled_brokers(), None);
    };
    match std::fs::read(path) {
        Ok(bytes) => match parse_broker_xml_bytes(&bytes) {
            Ok(list) => (list, None),
            Err(err) => (
                bundled_brokers(),
                Some(format!(
                    "无法解析 {} ({err})，已改用内置经纪商列表",
                    path.display()
                )),
            ),
        },
        Err(err) => (
            bundled_brokers(),
            Some(format!(
                "无法读取 {} ({err})，已改用内置经纪商列表",
                path.display()
            )),
        ),
    }
}

pub fn parse_broker_xml_bytes(bytes: &[u8]) -> Result<BrokerList, String> {
    let text = decode_xml(bytes)?;
    parse_broker_xml(&text)
}

pub fn parse_broker_xml(text: &str) -> Result<BrokerList, String> {
    let doc = roxmltree::Document::parse(text).map_err(|err| err.to_string())?;
    let mut brokers = Vec::new();
    let mut skipped_non_ctp = 0usize;
    for node in doc.descendants().filter(|node| tag_is(node, "broker")) {
        if !is_ctp_broker(node) {
            skipped_non_ctp += 1;
            continue;
        }
        let mut servers = Vec::new();
        for server in node.descendants().filter(|node| tag_is(node, "server")) {
            if !is_under(server, node) {
                continue;
            }
            let name = child_text(server, "name").unwrap_or_else(|| "服务器".to_owned());
            let mut fronts = Vec::new();
            for block in server.children().filter(|child| {
                child.is_element() && child.tag_name().name().eq_ignore_ascii_case("marketdata")
            }) {
                for item in block.children().filter(|child| tag_is(child, "item")) {
                    if let Some(front) = normalize_front(item.text().unwrap_or("")) {
                        fronts.push(front);
                    }
                }
            }
            servers.push(Server { name, fronts });
        }
        brokers.push(Broker {
            id: attr(node, "BrokerID"),
            name: attr(node, "BrokerName"),
            ename: attr(node, "BrokerEName"),
            servers,
        });
    }
    Ok(BrokerList {
        brokers,
        skipped_non_ctp,
    })
}

pub fn broker_matches(broker: &Broker, query: &str) -> bool {
    let query = query.trim();
    if query.is_empty() {
        return true;
    }
    let folded = query.to_ascii_lowercase();
    broker.name.contains(query)
        || broker.id.to_ascii_lowercase().contains(&folded)
        || broker.ename.to_ascii_lowercase().contains(&folded)
}

pub fn filtered_brokers(brokers: &[Broker], query: &str) -> Vec<usize> {
    brokers
        .iter()
        .enumerate()
        .filter(|(_, broker)| broker_matches(broker, query))
        .map(|(index, _)| index)
        .collect()
}

fn is_ctp_broker(node: roxmltree::Node<'_, '_>) -> bool {
    let api = node.attribute("ApiType").unwrap_or("").trim();
    api.is_empty() || api.eq_ignore_ascii_case("ctp")
}

fn is_under(node: roxmltree::Node<'_, '_>, ancestor: roxmltree::Node<'_, '_>) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        if parent == ancestor {
            return true;
        }
        if tag_is(&parent, "broker") {
            return false;
        }
        current = parent.parent();
    }
    false
}

fn tag_is(node: &roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name().eq_ignore_ascii_case(name)
}

fn attr(node: roxmltree::Node<'_, '_>, name: &str) -> String {
    node.attribute(name).unwrap_or("").trim().to_owned()
}

fn child_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| tag_is(child, name))
        .and_then(|child| {
            let text = child.text().unwrap_or("").trim().to_owned();
            if text.is_empty() {
                None
            } else {
                Some(text)
            }
        })
}

pub fn normalize_front(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (scheme, rest) = match raw.find("://") {
        Some(index) => (Some(&raw[..index]), &raw[index + 3..]),
        None => (None, raw),
    };
    let (host, port) = rest.rsplit_once(':')?;
    if host.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    let scheme = scheme.unwrap_or("tcp").to_ascii_lowercase();
    Some(format!("{scheme}://{host}:{port}"))
}

fn decode_xml(bytes: &[u8]) -> Result<String, String> {
    let declared = xml_encoding(bytes);
    if matches!(
        declared.as_deref(),
        Some("gb2312") | Some("gbk") | Some("gb18030")
    ) {
        let (text, _, _) = GBK.decode(bytes);
        return Ok(text.into_owned());
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => Ok(text.to_owned()),
        Err(_) => {
            let (text, _, had_errors) = GBK.decode(bytes);
            if had_errors {
                Err("broker.xml 不是 UTF-8 或 GB2312/GBK".to_owned())
            } else {
                Ok(text.into_owned())
            }
        }
    }
}

fn xml_encoding(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(240)];
    let head = String::from_utf8_lossy(head);
    let lower = head.to_ascii_lowercase();
    let key = "encoding=\"";
    let start = lower.find(key)? + key.len();
    let rest = head.get(start..)?;
    let end = rest.find('"')?;
    Some(rest[..end].trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gb2312_skips_ufx_and_trading() {
        let xml = r#"<?xml version="1.0" encoding="GB2312"?>
<root>
  <broker BrokerID="0052" BrokerName="测试期货" BrokerEName="test">
    <Servers>
      <Server>
        <Name>主席</Name>
        <MarketData>
          <item>1.2.3.4:1234</item>
          <item>tcp://5.6.7.8:9</item>
          <item>not a front</item>
        </MarketData>
        <Trading><item>9.9.9.9:1</item></Trading>
      </Server>
    </Servers>
    <AuthCode2>SECRET-AUTH-CODE</AuthCode2>
  </broker>
  <broker BrokerID="2" BrokerName="UFX商" ApiType="UFX">
    <Servers><Server><Name>s</Name><MarketData><item>8.8.8.8:8</item></MarketData></Server></Servers>
  </broker>
  <broker BrokerID="8888" BrokerName="金瑞期货" ApiType="">
    <Servers><Server><Name>主席</Name><MarKetData><item>10.0.0.1:1</item></MarKetData></Server></Servers>
  </broker>
</root>"#;
        let (bytes, _, _) = GBK.encode(xml);
        let list = parse_broker_xml_bytes(&bytes).unwrap();
        assert_eq!(list.skipped_non_ctp, 1);
        assert_eq!(list.brokers.len(), 2);
        assert_eq!(
            list.brokers[0].servers[0].fronts,
            vec![
                "tcp://1.2.3.4:1234".to_owned(),
                "tcp://5.6.7.8:9".to_owned()
            ]
        );
        assert_eq!(list.brokers[1].name, "金瑞期货");
        assert_eq!(
            list.brokers[1].servers[0].fronts,
            vec!["tcp://10.0.0.1:1".to_owned()]
        );
        let blob = format!("{list:?}");
        assert!(!blob.contains("SECRET-AUTH-CODE"));
        assert!(!blob.contains("9.9.9.9"));
    }

    #[test]
    fn bundled_list_matches_the_cleaned_file() {
        let text = bundled_xml();
        assert!(text.contains("A安粮期货"));
        assert!(text.contains("J金瑞期货"));
        assert!(!text.to_ascii_lowercase().contains("authcode"));
        let list = bundled_brokers();
        assert_eq!(list.brokers.len() + list.skipped_non_ctp, 158);
        assert_eq!(list.skipped_non_ctp, 7);
        assert_eq!(list.brokers.len(), 151);
        let fronts: usize = list
            .brokers
            .iter()
            .flat_map(|broker| &broker.servers)
            .map(|server| server.fronts.len())
            .sum();
        assert_eq!(fronts, 841);
        let jinrui = list
            .brokers
            .iter()
            .find(|broker| broker.name.contains("金瑞"))
            .unwrap();
        let chair = jinrui
            .servers
            .iter()
            .find(|server| server.name == "主席")
            .unwrap();
        assert_eq!(chair.fronts.len(), 14);
        assert!(broker_matches(&list.brokers[0], "安粮"));
        assert!(filtered_brokers(&list.brokers, "渤海")
            .iter()
            .any(|index| list.brokers[*index].name.contains("渤海")));
    }
}

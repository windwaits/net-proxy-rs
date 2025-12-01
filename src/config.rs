use std::{collections::HashMap, fs::File, io::Read, net::SocketAddr, path::Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::{
    rules::{Matcher, Rule, RuleSet},
    types::{ProxyKind, RouteAction},
};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse yaml: {0}")]
    Parse(#[from] serde_yaml::Error),
    #[error("unknown upstream '{0}' referenced by rule '{1}'")]
    UnknownUpstream(String, String),
    #[error("invalid upstream '{name}': {reason}")]
    InvalidUpstream { name: String, reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub upstreams: HashMap<String, UpstreamConfig>,
    pub rules: Vec<RuleConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamConfig {
    pub kind: UpstreamKind,
    pub addr: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpstreamKind {
    Direct,
    Tcp,
    Socks5,
    Block,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleConfig {
    pub name: String,
    pub matcher: Matcher,
    pub action: ActionConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionConfig {
    pub block: Option<bool>,
    pub upstream: Option<String>,
    pub direct: Option<bool>,
}

impl Config {
    pub fn from_reader(mut reader: impl Read) -> Result<Self, ConfigError> {
        let mut buf = String::new();
        reader.read_to_string(&mut buf)?;
        Ok(serde_yaml::from_str(&buf)?)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let file = File::open(path)?;
        Self::from_reader(file)
    }

    pub fn build_rule_set(&self) -> Result<(RuleSet, HashMap<String, ProxyKind>), ConfigError> {
        let upstreams = self.build_upstreams()?;
        let mut rules = Vec::new();
        for rule_cfg in &self.rules {
            let action = match (
                &rule_cfg.action.upstream,
                rule_cfg.action.block,
                rule_cfg.action.direct,
            ) {
                (_, Some(true), _) => RouteAction::Block,
                (_, _, Some(true)) => RouteAction::Direct,
                (Some(up), _, _) => {
                    if !upstreams.contains_key(up) {
                        return Err(ConfigError::UnknownUpstream(
                            up.clone(),
                            rule_cfg.name.clone(),
                        ));
                    }
                    RouteAction::Upstream(up.clone())
                }
                _ => RouteAction::Direct,
            };
            rules.push(Rule {
                name: rule_cfg.name.clone(),
                matcher: rule_cfg.matcher.clone(),
                action,
            });
        }
        Ok((RuleSet::new(rules), upstreams))
    }

    pub fn build_upstreams(&self) -> Result<HashMap<String, ProxyKind>, ConfigError> {
        let mut upstreams = HashMap::new();
        for (name, cfg) in &self.upstreams {
            let proxy_kind = match cfg.kind {
                UpstreamKind::Direct => ProxyKind::Direct,
                UpstreamKind::Block => ProxyKind::Block,
                UpstreamKind::Tcp => {
                    let addr = cfg
                        .addr
                        .as_ref()
                        .ok_or_else(|| ConfigError::InvalidUpstream {
                            name: name.clone(),
                            reason: "tcp upstream requires addr".into(),
                        })?
                        .parse::<SocketAddr>()
                        .map_err(|e| ConfigError::InvalidUpstream {
                            name: name.clone(),
                            reason: e.to_string(),
                        })?;
                    ProxyKind::TcpForward(addr)
                }
                UpstreamKind::Socks5 => {
                    let addr = cfg
                        .addr
                        .as_ref()
                        .ok_or_else(|| ConfigError::InvalidUpstream {
                            name: name.clone(),
                            reason: "socks5 upstream requires addr".into(),
                        })?
                        .parse::<SocketAddr>()
                        .map_err(|e| ConfigError::InvalidUpstream {
                            name: name.clone(),
                            reason: e.to_string(),
                        })?;
                    ProxyKind::Socks5 {
                        addr,
                        username: cfg.username.clone(),
                        password: cfg.password.clone(),
                    }
                }
            };
            upstreams.insert(name.clone(), proxy_kind);
        }
        Ok(upstreams)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;

    #[test]
    fn parses_sample_config() {
        let yaml = r#"
upstreams:
  direct: { kind: direct }
  dc: { kind: tcp, addr: "10.0.0.2:9000" }
  socks: { kind: socks5, addr: "127.0.0.1:1080", username: "u", password: "p" }
rules:
  - name: block_malware
    matcher: { domains: ["*.evil.com"] }
    action: { block: true }
  - name: internal
    matcher: { ip_cidrs: ["10.0.0.0/8"] }
    action: { upstream: "dc" }
  - name: default
    matcher: { any: true }
    action: { upstream: "socks" }
"#;

        let cfg = Config::from_reader(yaml.as_bytes()).unwrap();
        let (rules, upstreams) = cfg.build_rule_set().unwrap();
        assert_eq!(rules.rules.len(), 3);
        assert!(
            matches!(upstreams.get("dc"), Some(ProxyKind::TcpForward(addr)) if *addr == "10.0.0.2:9000".parse::<SocketAddr>().unwrap())
        );
        assert!(upstreams.contains_key("socks"));
    }
}

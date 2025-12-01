use std::{collections::HashMap, net::IpAddr};

use ipnet::IpNet;
use serde::{Deserialize, Serialize};

use super::types::{ConnectionCtx, RouteAction};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSet {
    pub rules: Vec<Rule>,
}

impl RuleSet {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    pub fn decide_route(
        &self,
        ctx: &ConnectionCtx,
        upstreams: &HashMap<String, ProxyRef>,
    ) -> RouteAction {
        self.rules
            .iter()
            .find(|rule| rule.matcher.matches(ctx))
            .map(|rule| rule.action.clone())
            .unwrap_or(RouteAction::Direct)
            .resolve(upstreams)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub matcher: Matcher,
    pub action: RouteAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Matcher {
    #[serde(default)]
    pub any: bool,
    #[serde(default)]
    pub domains: Vec<String>,
    #[serde(default)]
    pub ip_cidrs: Vec<IpNet>,
    #[serde(default)]
    pub process_paths: Vec<String>,
}

impl Matcher {
    pub fn matches(&self, ctx: &ConnectionCtx) -> bool {
        if self.any {
            return true;
        }

        let ip = ctx.destination.ip();
        let ip_match = self.ip_cidrs.iter().any(|cidr| cidr.contains(&ip))
            || match ip {
                IpAddr::V6(v6) => v6
                    .to_ipv4()
                    .map(|v4| {
                        self.ip_cidrs
                            .iter()
                            .any(|net| net.contains(&IpAddr::V4(v4)))
                    })
                    .unwrap_or(false),
                IpAddr::V4(_) => false,
            };

        let domain_match = ctx
            .domain
            .as_ref()
            .map(|d| self.domains.iter().any(|pat| domain_match(pat, d)))
            .unwrap_or(false);

        let process_match = ctx
            .process_path
            .as_ref()
            .map(|p| self.process_paths.iter().any(|pat| path_match(pat, p)))
            .unwrap_or(false);

        ip_match || domain_match || process_match
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProxyRef {
    pub name: String,
}

impl RouteAction {
    fn resolve(self, upstreams: &HashMap<String, ProxyRef>) -> RouteAction {
        match self {
            RouteAction::Upstream(name) if upstreams.contains_key(&name) => {
                RouteAction::Upstream(name)
            }
            RouteAction::Upstream(_) => RouteAction::Direct,
            other => other,
        }
    }
}

fn domain_match(pattern: &str, domain: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if pattern.starts_with("*.") {
        let suffix = &pattern[2..];
        return domain.ends_with(suffix);
    }
    pattern.eq_ignore_ascii_case(domain)
}

fn path_match(pattern: &str, process_path: &str) -> bool {
    if cfg!(windows) {
        process_path.eq_ignore_ascii_case(pattern)
    } else {
        process_path == pattern
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    #[test]
    fn matches_domain_and_ip() {
        let rule = Rule {
            name: "test".into(),
            matcher: Matcher {
                any: false,
                domains: vec!["*.example.com".into()],
                ip_cidrs: vec![IpNet::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 0)), 8).unwrap()],
                process_paths: vec![],
            },
            action: RouteAction::Upstream("socks".into()),
        };

        let ctx = ConnectionCtx {
            source: SocketAddr::from(([192, 168, 0, 1], 1000)),
            destination: SocketAddr::from(([10, 1, 2, 3], 443)),
            domain: Some("api.example.com".into()),
            process_path: None,
        };

        assert!(rule.matcher.matches(&ctx));
    }

    #[test]
    fn falls_back_on_miss() {
        let rules = RuleSet::new(vec![Rule {
            name: "block".into(),
            matcher: Matcher {
                any: false,
                domains: vec!["bad.com".into()],
                ip_cidrs: vec![],
                process_paths: vec![],
            },
            action: RouteAction::Block,
        }]);

        let ctx = ConnectionCtx {
            source: "192.168.0.2:1111".parse().unwrap(),
            destination: "8.8.8.8:53".parse().unwrap(),
            domain: None,
            process_path: None,
        };

        let route = rules.decide_route(&ctx, &HashMap::new());
        assert_eq!(route, RouteAction::Direct);
    }
}

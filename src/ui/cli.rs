use std::{collections::HashMap, net::SocketAddr, path::PathBuf};

use clap::{Parser, Subcommand};
use futures::StreamExt;

use crate::{
    config::Config,
    core::dispatcher::ProxyError,
    core::{
        rules::RuleSet,
        types::{ConnectionCtx, ProxyKind},
    },
    platform::{mock::MockPacketSource, PacketSource},
};

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Functional split proxy controller",
    propagate_version = true
)]
pub struct Cli {
    /// Path to the YAML configuration file
    #[arg(short, long, default_value = "proxifier.yaml")]
    pub config: PathBuf,
    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the proxy engine
    Run {},
    /// Evaluate a rule against a synthetic connection
    TestRule {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        ip: Option<String>,
        #[arg(long)]
        process: Option<String>,
    },
    /// Stub that would enumerate processes in a GUI-enabled build
    ListProcesses {},
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cfg = Config::from_file(&cli.config)?;
    let (rules, upstreams) = cfg.build_rule_set()?;

    match cli.command.unwrap_or(Commands::Run {}) {
        Commands::Run {} => run_loop(rules, upstreams).await?,
        Commands::TestRule {
            domain,
            ip,
            process,
        } => test_rule(&rules, &upstreams, domain, ip, process).await?,
        Commands::ListProcesses {} => {
            println!("process listing requires platform integration; stubbed for now");
        }
    }
    Ok(())
}

async fn run_loop(rules: RuleSet, upstreams: HashMap<String, ProxyKind>) -> Result<(), ProxyError> {
    // For now, use a mock packet source. Platform-specific implementations plug into the same loop.
    let ctx = ConnectionCtx {
        source: "127.0.0.1:50000".parse().unwrap(),
        destination: "93.184.216.34:80".parse().unwrap(),
        domain: Some("example.com".into()),
        process_path: None,
    };
    let source = MockPacketSource::new(vec![ctx]);
    let mut stream = source.events().await;
    while let Some(ctx) = stream.next().await {
        let action = rules.decide_route(&ctx, &to_refs(&upstreams));
        // Real-world builds would receive the inbound TcpStream from the platform layer; here we only log.
        println!("route decision for {:?}: {:?}", ctx, action);
    }
    Ok(())
}

async fn test_rule(
    rules: &RuleSet,
    upstreams: &HashMap<String, ProxyKind>,
    domain: Option<String>,
    ip: Option<String>,
    process: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let dest: SocketAddr = ip.unwrap_or_else(|| "1.1.1.1:443".into()).parse()?;
    let ctx = ConnectionCtx {
        source: "127.0.0.1:50001".parse()?,
        destination: dest,
        domain,
        process_path: process,
    };
    let decision = rules.decide_route(&ctx, &to_refs(upstreams));
    println!("Decision: {:?}", decision);
    Ok(())
}

fn to_refs(
    upstreams: &HashMap<String, ProxyKind>,
) -> HashMap<String, crate::core::rules::ProxyRef> {
    upstreams
        .iter()
        .map(|(name, _)| {
            (
                name.clone(),
                crate::core::rules::ProxyRef { name: name.clone() },
            )
        })
        .collect()
}

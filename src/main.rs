mod configuration;
mod database;
mod route;

use crate::configuration::Configuration;
use crate::route::{setup_routes, RouterState};
use axum::routing::get;
use axum::Router;
use clap::Parser;
use clap_verbosity_flag::InfoLevel;
use std::fs::read_to_string;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::{TcpListener, UnixListener};
use tracing::{error, info};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short, long, default_value = "config.yaml")]
    configuration_file: String,

    #[clap(flatten)]
    verbosity: clap_verbosity_flag::Verbosity<InfoLevel>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(args.verbosity.tracing_level())
        .init();

    let config = match read_to_string(&args.configuration_file) {
        Ok(config) => match yaml_serde::from_str::<Configuration>(&config) {
            Ok(config) => config,
            Err(e) => {
                error!("Failed to parse configuration file: {}", e);
                return;
            }
        },
        Err(e) => {
            error!("Failed to read configuration file: {}", e);
            return;
        }
    };

    let config = Arc::new(config);

    let router_state = RouterState {
        config: config.clone(),
    };
    let app = setup_routes().with_state(router_state);

    info!("bin-chicken");

    let mut handles = Vec::new();
    for listener in &config.listeners {
        if let Ok(listener) = TcpListener::bind(listener.clone()).await {
            let addr = listener.local_addr().unwrap();
            let app = app.clone();
            handles.push(tokio::task::spawn(async move {
                if let Err(e) = axum::serve(listener, app).await {
                    error!("Failed to start server: {}", e);
                }
            }));
            info!("Listening on {addr}");
            continue;
        };

        if let Ok(port) = listener.parse::<u16>() {
            let Ok(listener) = TcpListener::bind(&format!("0.0.0.0:{}", port)).await else {
                error!("Failed to bind to port: {}", port);
                continue;
            };

            let addr = listener.local_addr().unwrap();
            let app = app.clone();

            handles.push(tokio::task::spawn(async move {
                if let Err(e) = axum::serve(listener, app).await {
                    error!("Failed to start server: {}", e);
                }
            }));
            info!("Listening on {addr}");
            continue;
        }

        if let Ok(path) = PathBuf::try_from(listener.clone()) {
            let Ok(unix_listener) = UnixListener::bind(&*path) else {
                error!("Failed to bind to unix socket: {}", listener);
                continue;
            };

            let app = app.clone();

            handles.push(tokio::task::spawn(async move {
                if let Err(e) = axum::serve(unix_listener, app).await {
                    error!("Failed to start server: {}", e);
                }
            }));
            info!("Listening on {}", listener);
            continue;
        }

        error!("Failed to bind to listener: {}", listener);
    }

    for handle in handles {
        let _ = handle.await;
    }
}

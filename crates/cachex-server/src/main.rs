

use cachex_core::config::Config;
use cachex_core::hashing::ConsistentHasher;
use cachex_server::heartbeat::Heartbeat;
use cachex_server::internal::{
    internal_address, internal_map_for_cluster, run_internal, INTERNAL_PORT_OFFSET,
};
use cachex_server::replication::Replicator;
use cachex_server::{aof, metrics::Metrics, metrics_http, node::NodeContext, server, storage::CacheStore};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::time::{interval, Duration};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "configs/cachex.toml".to_string());

    let config = Config::load(&path).unwrap_or_else(|error| {
        eprintln!("failed to load config from {path}: {error}");
        std::process::exit(1);
    });

    let store = Arc::new(CacheStore::new(config.cache.max_memory_bytes));
    let metrics = Arc::new(Metrics::new());

    
    let aof = aof::Aof::new(&config.aof).await?;
    if config.aof.enabled {
        let report = aof::replay(&config.aof.path, &store).unwrap_or_else(|error| {
            eprintln!("AOF replay failed: {error}");
            std::process::exit(1);
        });
        metrics.set_recovery_ms(report.elapsed.as_millis() as u64);
        println!(
            "AOF replay: {} commands, {} applied, {} skipped in {}ms",
            report.commands,
            report.applied,
            report.skipped,
            report.elapsed.as_millis()
        );
    }

    
    let self_address = config.node.address();
    let router = Arc::new(ConsistentHasher::new(
        config.cluster.nodes.clone(),
        config.hashing.vnodes,
    ));
    let internal_map = internal_map_for_cluster(&config.cluster.nodes);
    println!(
        "node {} public {} internal {} cluster {} nodes",
        config.node.id,
        self_address,
        internal_address(&self_address),
        config.cluster.nodes.len()
    );

    
    let replicator = if config.replication.enabled {
        Some(Arc::new(Replicator::new(
            router.clone(),
            internal_map.clone(),
            config.replication.factor,
        )))
    } else {
        None
    };

    
    let heartbeat = Arc::new(Heartbeat::new(
        &self_address,
        &config.cluster.nodes,
        internal_map.clone(),
        config.heartbeat.clone(),
    ));
    {
        let heartbeat = heartbeat.clone();
        tokio::spawn(async move { heartbeat.run().await });
    }

    let ctx = Arc::new(NodeContext {
        store: store.clone(),
        metrics: metrics.clone(),
        aof: aof.clone(),
        router: router.clone(),
        self_address,
        replicator: replicator.clone(),
        heartbeat: Some(heartbeat.clone()),
        replication_factor: config.replication.factor,
    });

    
    let reaper_store = store.clone();
    let maintenance_aof = aof.clone();
    let rewrite_threshold = config.aof.rewrite_threshold_bytes;
    let purge_interval_secs = config.cache.ttl_purge_interval_secs.max(1);
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(purge_interval_secs));
        loop {
            ticker.tick().await;

            let removed = reaper_store.purge_expired();
            if removed > 0 {
                println!("TTL reaper removed {removed} expired keys");
            }

            if let Some(aof) = maintenance_aof.as_ref() {
                if let Err(error) = aof.sync().await {
                    eprintln!("AOF fsync error: {error}");
                }
                if rewrite_threshold > 0 && aof.bytes_written() > rewrite_threshold {
                    let entries = reaper_store.snapshot();
                    match aof.rewrite(entries).await {
                        Ok(()) => {
                            println!("AOF rewrote log to {} entries", reaper_store.key_count())
                        }
                        Err(error) => eprintln!("AOF rewrite error: {error}"),
                    }
                }
            }
        }
    });

    
    {
        let metrics = metrics.clone();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(1));
            loop {
                ticker.tick().await;
                metrics.sample_rates();
            }
        });
    }

    
    let public_listener =
        TcpListener::bind((config.node.host.as_str(), config.node.port)).await?;
    let internal_listener = TcpListener::bind((
        config.node.host.as_str(),
        config.node.port + INTERNAL_PORT_OFFSET,
    ))
    .await?;
    println!("CacheX node {} listening on {}", config.node.id, config.node.address());

    
    if config.metrics.enabled {
        let metrics_listener = TcpListener::bind((config.metrics.host.as_str(), config.metrics.port))
            .await?;
        println!(
            "metrics API listening on {}:{}",
            config.metrics.host, config.metrics.port
        );
        tokio::spawn(metrics_http::serve(metrics_listener, ctx.clone()));
    }

    tokio::spawn(run_internal(internal_listener, ctx.clone()));
    server::run(public_listener, ctx).await
}
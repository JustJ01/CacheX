

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;

use crate::events::Event;
use crate::state::AppState;

async fn kill_pid(pid: u32) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    let status = Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .status()
        .await;

    #[cfg(not(target_os = "windows"))]
    let status = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .await;

    status?;
    Ok(())
}

pub fn aof_path(control_dir: &Path, node_id: u16) -> std::path::PathBuf {
    control_dir.join(format!("node{node_id}.aof"))
}

pub fn clear_aofs(state: &Arc<AppState>) -> std::io::Result<()> {
    let spec = state.spec.lock().unwrap().clone();
    for node_id in 1..=spec.node_count {
        let path = aof_path(&state.control_dir, node_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
    }
    Ok(())
}

pub async fn start_node(state: &Arc<AppState>, node_id: u16) -> std::io::Result<u32> {
    let spec = state.spec.lock().unwrap().clone();
    let config_path = crate::configgen::config_path(&state.control_dir, node_id);
    let out_log = state.control_dir.join(format!("node{node_id}.out.log"));
    let err_log = state.control_dir.join(format!("node{node_id}.err.log"));

    let child = Command::new(&state.server_exe)
        .arg(&config_path)
        .current_dir(&state.control_dir)
        .stdout(std::process::Stdio::from(
            std::fs::File::create(&out_log)?,
        ))
        .stderr(std::process::Stdio::from(
            std::fs::File::create(&err_log)?,
        ))
        .kill_on_drop(false)
        .spawn()?;

    let pid = child.id().unwrap_or(0);

    {
        let mut pids = state.pids.lock().unwrap();
        pids.insert(spec.public_port(node_id), pid);
    }
    state.emit(Event::NodeStarted {
        id: node_id,
        address: spec.public_address(node_id),
    });
    Ok(pid)
}

pub async fn start_cluster(state: &Arc<AppState>) -> std::io::Result<()> {
    let spec = state.spec.lock().unwrap().clone();
    crate::configgen::write_all_configs(&state.control_dir, &spec)?;

    for node_id in 1..=spec.node_count {
        if state.node_alive(node_id).await {
            continue;
        }
        let port = spec.public_port(node_id);
        if let Some(pid) = pid_on_port(port).await {
            let _ = kill_pid(pid).await;
        }
        start_node(state, node_id).await?;
    }

    
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        let mut up = true;
        for node_id in 1..=spec.node_count {
            if !state.node_alive(node_id).await {
                up = false;
                break;
            }
        }
        if up {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            return Err(std::io::Error::other("cluster did not become healthy in time"));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    state.emit(Event::ClusterStarted {
        node_count: spec.node_count,
    });
    Ok(())
}

pub async fn kill_node(state: &Arc<AppState>, node_id: u16) -> std::io::Result<u32> {
    let spec = state.spec.lock().unwrap().clone();
    let port = spec.public_port(node_id);
    let pid = pid_on_port(port)
        .await
        .or_else(|| state.pids.lock().unwrap().get(&port).copied());
    let pid = match pid {
        Some(pid) => pid,
        None => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("no process found on port {port}"),
            ));
        }
    };

    kill_pid(pid).await?;
    state.pids.lock().unwrap().remove(&port);
    state.emit(Event::NodeKilled {
        id: node_id,
        address: spec.public_address(node_id),
    });
    Ok(pid)
}

pub async fn restart_node(state: &Arc<AppState>, node_id: u16) -> std::io::Result<()> {
    let spec = state.spec.lock().unwrap().clone();
    if state.node_alive(node_id).await {
        let _ = kill_node(state, node_id).await;
    }
    start_node(state, node_id).await?;

    
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if state.node_alive(node_id).await {
            break;
        }
        if tokio::time::Instant::now() > deadline {
            return Err(std::io::Error::other(format!(
                "node {node_id} did not come back up in time"
            )));
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    let snapshot = crate::metrics::metrics_of(state, node_id).await.ok();
    if let Some(snapshot) = snapshot {
        state.emit(Event::AofReplayed {
            id: node_id,
            ms: snapshot.recovery_ms,
        });
        state.emit(Event::NodeHealthy {
            id: node_id,
            address: spec.public_address(node_id),
        });
    } else {
        state.emit(Event::NodeRestarted {
            id: node_id,
            address: spec.public_address(node_id),
        });
    }
    Ok(())
}

pub async fn stop_cluster(state: &Arc<AppState>) -> std::io::Result<()> {
    let spec = state.spec.lock().unwrap().clone();
    let known: Vec<u32> = {
        let mut pids = state.pids.lock().unwrap();
        pids.drain().map(|(_, pid)| pid).collect()
    };

    for pid in known {
        let _ = kill_pid(pid).await;
    }

    
    for node_id in 1..=spec.node_count {
        let port = spec.public_port(node_id);
        if let Some(pid) = pid_on_port(port).await {
            let _ = kill_pid(pid).await;
        }
    }

    state.emit(Event::ClusterStopped);
    Ok(())
}

pub async fn pid_on_port(port: u16) -> Option<u32> {
    let output = Command::new("netstat").arg("-ano").output().await.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let needle = format!(":{port}");
    for line in text.lines() {
        if !line.contains("LISTENING") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        
        if let Some(local) = tokens.get(1) {
            if local.ends_with(&needle) {
                if let Some(pid) = tokens.last().and_then(|s| s.parse::<u32>().ok()) {
                    return Some(pid);
                }
            }
        }
    }
    None
}
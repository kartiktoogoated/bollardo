use bollard::Docker;
use bollard::errors::Error;

use bollard::query_parameters::{
    CreateContainerOptions, InspectContainerOptions, ListContainersOptions, RemoveContainerOptions,
    StartContainerOptions, StopContainerOptions,
};

use bollard::models::{ContainerCreateBody, ContainerSummaryStateEnum, HostConfig};

use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

const SERVICE_NAME: &str = "demo-nginx";
const IMAGE: &str = "nginx:alpine";
const DESIRED_REPLICAS: usize = 3;

const MAX_CONSECUTIVE_FAILURES: u32 = 5;
const BACKOFF_DURATION_SECS: u64 = 30;
const FAILURE_RESET_AFTER_SECS: u64 = 300;

#[derive(Debug, Default)]
struct BackoffState {
    consecutive_failures: u32,
    last_failure: Option<Instant>,
}

impl BackoffState {
    fn register_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure = Some(Instant::now());
    }

    fn maybe_reset(&mut self) {
        if let Some(last) = self.last_failure
            && last.elapsed() > Duration::from_secs(FAILURE_RESET_AFTER_SECS)
        {
            self.consecutive_failures = 0;
            self.last_failure = None;
        }
    }

    fn in_backoff(&self) -> bool {
        if let Some(last) = self.last_failure
            && self.consecutive_failures >= MAX_CONSECUTIVE_FAILURES
            && last.elapsed() < Duration::from_secs(BACKOFF_DURATION_SECS)
        {
            return true;
        }

        false
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let docker = connect_with_retry().await;
    let mut backoff = BackoffState::default();
    println!("Connected to docker, starting orchestrataaa");
    println!("Service: {SERVICE_NAME}, image: {IMAGE}, desired replicas: {DESIRED_REPLICAS}");

    loop {
        if let Err(e) = reconcile(&docker, &mut backoff).await {
            eprintln!("reconcile error: {:?}", e);
        }

        sleep(Duration::from_secs(5)).await;
    }
}

async fn reconcile(docker: &Docker, backoff: &mut BackoffState) -> Result<(), Error> {
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_string(),
        vec![format!("service={}", SERVICE_NAME)],
    );

    let options = ListContainersOptions {
        all: true,
        limit: None,
        size: false,
        filters: Some(filters),
    };

    let containers = docker.list_containers(Some(options)).await?;

    let mut running = Vec::new();
    let mut dead = Vec::new();

    for c in containers {
        let id = c.id.unwrap_or_default();
        let state = c.state.unwrap_or(ContainerSummaryStateEnum::EMPTY);
        let status = c.status.unwrap_or_default().to_lowercase();

        let is_running = matches!(state, ContainerSummaryStateEnum::RUNNING)
            || status.contains("up")
            || status.contains("running");

        if is_running {
            running.push(id);
        } else {
            dead.push(id);
        }
    }

    println!(
        "[reconcile] running: {}, dead: {}, backoff: {:?}",
        running.len(),
        dead.len(),
        backoff
    );

    if !dead.is_empty() {
        backoff.register_failure();
    } else {
        backoff.maybe_reset();
    }

    for id in &dead {
        println!("Removing dead container: {id}");
        graceful_remove_container(docker, id).await?;
    }

    let running_count = running.len();

    let mut outdated = Vec::new();

    for id in &running {
        let inspect = docker
            .inspect_container(id, None::<InspectContainerOptions>)
            .await?;

        let version = inspect
            .config
            .as_ref()
            .and_then(|cfg| cfg.labels.as_ref())
            .and_then(|labels| labels.get("version"))
            .map(|v| v.as_str());

        if version != Some(IMAGE) {
            outdated.push(id.clone());
        }
    }

    if !outdated.is_empty() {
        println!("Found outdated containers, performing rolling update");
        return perform_rolling_update(docker, &running).await;
    }

    if running_count < DESIRED_REPLICAS {
        let to_spawn = DESIRED_REPLICAS - running_count;

        if backoff.in_backoff() {
            println!(
                "Backoff active ({} failures). Skipping respawn this cycle",
                backoff.consecutive_failures
            );
            return Ok(());
        }

        println!("Need {to_spawn} more replicas");

        for _ in 0..to_spawn {
            spawn_replica_and_get_id(docker).await?;
        }
        return Ok(());
    }

    if running_count > DESIRED_REPLICAS {
        let to_kill = running_count - DESIRED_REPLICAS;
        println!("Removing {to_kill} extra replicas");

        for id in running.iter().take(to_kill) {
            graceful_remove_container(docker, id).await?;
        }
        return Ok(());
    }

    println!("Desired state satisfied.");
    Ok(())
}

async fn spawn_replica_and_get_id(docker: &Docker) -> Result<String, Error> {
    let container_name = format!("{}-{}", SERVICE_NAME, Uuid::new_v4());

    let mut labels = HashMap::new();
    labels.insert("service".to_string(), SERVICE_NAME.to_string());
    labels.insert("managed-by".to_string(), "bollard-orchestrator".to_string());
    labels.insert("version".to_string(), IMAGE.to_string());

    let body = ContainerCreateBody {
        image: Some(IMAGE.to_string()),
        labels: Some(labels),
        host_config: Some(HostConfig {
            ..Default::default()
        }),
        ..Default::default()
    };

    let options = CreateContainerOptions {
        name: Some(container_name.clone()),
        platform: "".into(),
    };

    docker.create_container(Some(options), body).await?;

    println!("Starting container: {container_name}");
    docker
        .start_container(&container_name, Some(StartContainerOptions::default()))
        .await?;

    Ok(container_name)
}

async fn connect_with_retry() -> Docker {
    loop {
        match Docker::connect_with_local_defaults() {
            Ok(docker) => {
                println!("Connected to Docker DAEMMONN successfully");
                return docker;
            }
            Err(e) => {
                eprintln!("Docker not available: {e:?}");
                eprintln!("Retrying in 3 seconds");
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
        }
    }
}

async fn graceful_remove_container(docker: &Docker, id: &str) -> Result<(), Error> {
    println!("Gracefully stopping container: {id}");

    let stop_options = StopContainerOptions {
        signal: None,
        t: Some(5),
    };

    let _ = docker.stop_container(id, Some(stop_options)).await;

    println!("Removing container: {id}");
    docker
        .remove_container(
            id,
            Some(RemoveContainerOptions {
                force: false,
                v: false,
                link: false,
            }),
        )
        .await?;

    Ok(())
}

async fn perform_rolling_update(docker: &Docker, running: &Vec<String>) -> Result<(), Error> {
    println!("Starting rolling update for image = {}", IMAGE);

    for old_id in running {
        println!("Spawning new replica before removing: {old_id}");
        let new_id = spawn_replica_and_get_id(docker).await?;

        tokio::time::sleep(Duration::from_secs(3)).await;

        graceful_remove_container(docker, old_id).await?;

        println!("Replaced {old_id} with {new_id}");
    }

    println!("Rolling update done");
    Ok(())
}

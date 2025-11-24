use bollard::Docker;
use bollard::errors::Error;

use bollard::query_parameters::{
    CreateContainerOptions, ListContainersOptions, RemoveContainerOptions, StartContainerOptions,
};

use bollard::models::{ContainerCreateBody, ContainerSummaryStateEnum, HostConfig};

use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;
use uuid::Uuid;

const SERVICE_NAME: &str = "demo-nginx";
const IMAGE: &str = "nginx:alpine";
const DESIRED_REPLICAS: usize = 3;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let docker = Docker::connect_with_local_defaults()?;
    println!("Connected to docker, starting orchestrator");
    println!("Service: {SERVICE_NAME}, image: {IMAGE}, desired replicas: {DESIRED_REPLICAS}");

    loop {
        if let Err(e) = reconcile(&docker).await {
            eprintln!("reconcile error: {:?}", e);
        }

        sleep(Duration::from_secs(5)).await;
    }
}

async fn reconcile(docker: &Docker) -> Result<(), Error> {
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
        "[reconcile] running: {}, dead: {}",
        running.len(),
        dead.len()
    );

    for id in dead {
        println!("Removing dead container: {id}");
        docker
            .remove_container(
                &id,
                Some(RemoveContainerOptions {
                    force: true,
                    v: false,
                    link: false,
                }),
            )
            .await?;
    }

    let running_count = running.len();

    if running_count < DESIRED_REPLICAS {
        let to_spawn = DESIRED_REPLICAS - running_count;
        println!("Need {to_spawn} more replicas");

        for _ in 0..to_spawn {
            spawn_replica(docker).await?;
        }
    } else if running_count > DESIRED_REPLICAS {
        let to_kill = running_count - DESIRED_REPLICAS;
        println!("Removing {to_kill} extra replicas");

        for id in running.iter().take(to_kill) {
            docker
                .remove_container(
                    id,
                    Some(RemoveContainerOptions {
                        force: true,
                        v: false,
                        link: false,
                    }),
                )
                .await?;
        }
    } else {
        println!("Desired state satisfied.");
    }

    Ok(())
}

async fn spawn_replica(docker: &Docker) -> Result<(), Error> {
    let container_name = format!("{}-{}", SERVICE_NAME, Uuid::new_v4());

    let mut labels = HashMap::new();
    labels.insert("service".to_string(), SERVICE_NAME.to_string());
    labels.insert("managed-by".to_string(), "bollard-orchestrator".to_string());

    let body = ContainerCreateBody {
        image: Some(IMAGE.to_string()),
        labels: Some(labels),
        host_config: Some(HostConfig {
            ..Default::default()
        }),
        ..Default::default()
    };

    let opts = CreateContainerOptions {
        name: Some(container_name.clone()),
        platform: "".into(),
    };

    println!("Creating container: {container_name}");
    docker.create_container(Some(opts), body).await?;

    println!("Starting container: {container_name}");
    docker
        .start_container(&container_name, Some(StartContainerOptions::default()))
        .await?;

    Ok(())
}

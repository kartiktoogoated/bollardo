use bollard::{Docker, query_parameters::ListContainersOptions};

#[tokio::main]
async fn main() {
    // Initialize docker to connect
    let docker = Docker::connect_with_local_defaults().expect("Failed to connect to docker");

    println!("Connected to Docker");

    // List all containerdinho
    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true, // will show stopped one too
            ..Default::default()
        }))
        .await
        .expect("Failed to list containers");

    if containers.is_empty() {
        println!("No containers found");
        return;
    }

    for c in containers {
        println!("ID: {:?}", c.id);
        println!("Name: {:?}", c.names);
        println!("Image: {:?}", c.image);
        println!("State: {:?}", c.state);
        println!("Status: {:?}", c.status);
    }
}

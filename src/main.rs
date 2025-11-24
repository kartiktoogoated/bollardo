use bollard::Docker;

#[tokio::main]
async fn main() {
    // Initialize docker to connect
    let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker");

    println!("Connected to Docker");

    // Dockers service infoo
    let info = docker.info().await.unwrap();
    println!("{:?}", info.server_version);
}

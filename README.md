Mini Docker Orchestrator (Rust + Bollard)
=========================================

This is a small container orchestrator written in Rust using the **Bollard** Docker SDK.\
It keeps a fixed number of containers running, replaces dead ones, and supports basic rolling updates and backoff handling.

What It Does
------------

-   Connects to the local Docker engine

-   Monitors containers with a specific service label

-   Ensures a set number of replicas (`DESIRED_REPLICAS`) are always running

-   Removes dead or stopped containers

-   Creates new containers when needed

-   Gracefully stops containers during scale-down or updates

-   Adds a simple backoff mechanism to avoid constant respawning

-   Supports rolling update logic using a version label

Requirements
------------

-   Rust stable toolchain

-   Docker Desktop or Docker Engine running

-   `cargo` to build/run the project

Running
-------

`cargo run`

If Docker isn't running, the program will keep retrying until it becomes available.

How to Test
-----------

-   Stop a container manually → the orchestrator will replace it

-   Change the `IMAGE` constant → it will roll out new containers

-   Kill Docker → orchestrator waits and reconnects when available

Configuration
-------------

Modify these constants in `main.rs`:

`const SERVICE_NAME: &str = "...";
const IMAGE: &str = "nginx:alpine";
const DESIRED_REPLICAS: usize = 3;`

Files
-----

Everything is inside:

`src/main.rs`

No other external files are needed.

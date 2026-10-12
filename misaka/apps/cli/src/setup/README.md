# MISAKA setup module boundary

This directory contains the guided setup Web UI and the host mutation helpers it calls.

Keep this feature removable:

- `main.rs` should only contain the `mod setup;`, `Command::Setup`, and dispatch entry points.
- Setup-specific host mutations should stay in this directory.
- Setup UI assets should stay under `setup/ui/`.
- Shared operator commands that are useful without the Web UI should live outside this directory.

If the setup Web UI is removed, delete this directory and remove the setup entry points from `main.rs`.

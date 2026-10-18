# tiri-sdbus

Owned, blocking wrappers over the parts of libsystemd's sd-bus that tiri uses:
connecting to the user bus, building and calling method messages with
container arguments, and matching signals. Every handle releases its sd-bus
reference on drop.

The `systemd` crate covers the rest of libsystemd, but its bus module cannot
build container arguments or match signals.

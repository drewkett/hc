This is a simple utility that wraps terminal commands, sending a start and end
notification to healthchecks.io. It will report failures if the command exits
with a non-zero return. It will also include the output from stdout and stderr
from the command to healthchecks

```
hc [--hc-id ID] [--hc-tee] cmd [arg]...

--hc-id ID   This is used to pass in the healthcheck id. This can also be done using the environment variable HC_ID. The command line value takes precedence
--hc-tee     This enables the output from the command to be printed to stdout and stderr in addition to forwarding it to healthchecks. This can also be specified with the environment variable HC_TEE. It just checks for the existence of the environment variable without checking the value
```

# Build Instructions

This utility is written in Rust. The normal build procedure is used

```
cargo build --release
```

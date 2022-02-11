`hcp` (short for healthcheck ping) is a simple utility that wraps terminal commands, sending a start and end
notification to healthchecks.io. It will report failures if the command exits
with a non-zero return. It will also include the output from stdout and stderr
from the command to healthchecks

```
hcp [--hcp-id HCP_ID] [--hcp-tee] [--hcp-ignore-code] [cmd [args...]]
    
    HCP_ID can be set using an environment variable
    --hcp-id HCP_ID    Sets the healthchecks id. This can also be set using the
                     environment variable HCP_ID
    --hcp-ignore-code Ignore the return code from cmd. Also available using HCP_IGNORE_CODE
    --hcp-tee         Controls whether to also output the cmd stdout/stderr to the local
                     stdout/stderr. By default the output from the cmd will only get
                     passed as text to healthchecks. This option can also be enabled
                     using the environment variable HCP_TEE. Only the existance of the
                     variable is checked
    [cmd [args...]]  If no command is passed, the healthcheck will be notified as a 
                     success with the text 'No command given'
```

# Build Instructions

This utility is written in Rust. The normal build procedure is used

```
cargo build --release
```

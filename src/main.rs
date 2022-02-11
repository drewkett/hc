use bstr::ByteSlice;
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Write,
    process::{exit, Command, Stdio},
};

const EXIT_CODE: i32 = 963;

/// This reads the rdr to the end and returns the data as a Vec. If a wrtr is passed
/// in, also copy all data to wrtr
fn read_to_end_tee(
    mut rdr: impl std::io::Read,
    mut wrtr: Option<impl std::io::Write>,
) -> std::io::Result<Vec<u8>> {
    // This tracks the position in the out buffer that has been already forwarded to
    // wrtr, when a wrtr is passed
    let mut out_position = 0;
    // Buffer to return which captures all the data from rdr
    let mut out = Vec::new();
    // Temporary buffer used for read data
    let mut buf = [0; 16 * 1024];
    loop {
        match rdr.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                let read_contents = &buf[..n];
                out.extend(read_contents);
                if let Some(ref mut wrtr) = wrtr {
                    // Only write contents up to last new line. Since both stdout and
                    // stderr can be writing at the same time, attempt to line buffer
                    // to make output look nicer
                    // remaining is all the data that has been read to out but not yet
                    // written to wrtr
                    let remaining = &out[out_position..];
                    match remaining
                        .iter()
                        .rev()
                        .position(|&c| c == b'\n' || c == b'\r')
                    {
                        Some(j) => {
                            let to_write = &remaining[..remaining.len() - j];
                            if let Err(e) = wrtr.write_all(to_write) {
                                eprintln!("Error writing to output stream: {}", e)
                            }
                            out_position += to_write.len();
                        }
                        None => break,
                    }
                }
            }
            Err(e) => return Err(e),
        }
    }
    if let Some(ref mut wrtr) = wrtr {
        // The read has finished, so write all remaining data to wrtr if exists
        if out_position < out.len() {
            let remaining = &out[out_position..];
            if let Err(e) = wrtr.write_all(remaining) {
                eprintln!("Error writing to output stream: {}", e)
            }
        }
    }
    Ok(out)
}

fn print_help() {
    eprintln!(
        "\
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
"
    )
}

mod internal {
    use std::process::exit;
    const EXIT_CODE: i32 = 963;

    /// Check if buf is only valid hex characters
    fn is_hex(buf: &[u8]) -> bool {
        buf.iter()
            .all(|b| matches!(b, b'0'..=b'9'|b'a'..=b'z'|b'A'..=b'Z'))
    }

    #[derive(Clone, Copy)]
    pub struct Uuid([u8; 36]);

    impl Uuid {
        pub fn from_str(s: &str) -> Option<Self> {
            if s.len() != 36 {
                return None;
            }
            let mut uuid = [0; 36];
            uuid.copy_from_slice(s.as_bytes());
            if is_hex(&uuid[..8])
                && uuid[8] == b'-'
                && is_hex(&uuid[9..13])
                && uuid[13] == b'-'
                && is_hex(&uuid[14..18])
                && uuid[18] == b'-'
                && is_hex(&uuid[19..23])
                && uuid[23] == b'-'
                && is_hex(&uuid[24..])
            {
                Some(Self(uuid))
            } else {
                None
            }
        }

        fn as_str(&self) -> &str {
            // SAFETY: Uuid can only be created with from_str and it checks for
            // valid utf-8 characters
            unsafe { std::str::from_utf8_unchecked(&self.0) }
        }
    }

    #[derive(Clone, Copy)]
    pub struct HealthCheck(Uuid);

    impl HealthCheck {
        pub fn from_str(s: &str) -> Option<Self> {
            Uuid::from_str(s).map(Self)
        }

        fn base_url(&self) -> String {
            let mut url = "https://hc-ping.com/".to_string();
            url.push_str(self.0.as_str());
            url
        }

        fn start_url(&self) -> String {
            let mut url = self.base_url();
            url.push_str("/start");
            url
        }

        fn finish_url(&self) -> String {
            self.base_url()
        }

        fn fail_url(&self) -> String {
            let mut url = self.base_url();
            url.push_str("/fail");
            url
        }

        pub fn start(&self) {
            if let Err(e) = ureq::get(&self.start_url()).call() {
                eprintln!("Error on healthchecks /start call: {}", e);
            }
        }

        pub fn finish_and_exit(&self, msg: &str, code: i32, log: bool) -> ! {
            let url = if code == 0 {
                self.finish_url()
            } else {
                self.fail_url()
            };
            if log {
                eprintln!("{}", msg);
            }
            if let Err(e) = ureq::post(&url).send_string(msg) {
                eprintln!("Error sending finishing request to healthchecks: {}", e);
                exit(EXIT_CODE)
            }
            exit(code)
        }
    }
}

use internal::HealthCheck;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let mut hcp_id = std::env::var_os("HCP_ID");
    let mut ignore_code = std::env::var_os("HCP_IGNORE_CODE").is_some();
    let mut tee = std::env::var_os("HCP_TEE").is_some();
    let filtered_env: HashMap<OsString, OsString> = std::env::vars_os()
        .filter(|&(ref k, _)| k != "HCP_ID" && k != "HCP_TEE")
        .collect();
    let cmd = loop {
        match args.next() {
            Some(arg) => match arg.to_str() {
                Some("--hcp-id") => hcp_id = args.next(),
                Some("--hcp-tee") => tee = true,
                Some("--hcp-ignore-code") => ignore_code = true,
                _ => break Some(arg),
            },
            None => break None,
        }
    };
    let hc = match hcp_id.as_ref() {
        Some(hcp_id) => match hcp_id.to_str().and_then(HealthCheck::from_str) {
            Some(hcp_id) => hcp_id,
            None => {
                let hcp_id: &std::path::Path = hcp_id.as_ref();
                eprintln!("Healthcheck Id isn't a valid uuid '{}'", hcp_id.display());
                exit(1);
            }
        },
        None => {
            eprintln!("No Healthcheck Id given");
            print_help();
            exit(1);
        }
    };
    let cmd = cmd.or_else(|| args.next());
    let cmd = match cmd {
        Some(cmd) => cmd,
        None => hc.finish_and_exit("No command given", 0, true),
    };
    hc.start();
    let mut proc = match Command::new(cmd)
        .args(args)
        .env_clear()
        .envs(filtered_env)
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(p) => p,
        Err(e) => hc.finish_and_exit(&format!("Failed to spawn process: {}", e), EXIT_CODE, true),
    };

    let child_stdout = proc.stdout.take().unwrap();
    let child_stderr = proc.stderr.take().unwrap();

    let pipe_stdout = if tee { Some(std::io::stdout()) } else { None };
    let pipe_stderr = if tee { Some(std::io::stderr()) } else { None };

    // Spawn threads for continuously reading from the child process's stdout and stderr. If
    // tee is enabled forward the output to the processes pipes
    let stdout_thread = std::thread::spawn(move || read_to_end_tee(child_stdout, pipe_stdout));
    let stderr_thread = std::thread::spawn(move || read_to_end_tee(child_stderr, pipe_stderr));

    match proc.wait() {
        Ok(status) => {
            let out = match stdout_thread.join() {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => hc.finish_and_exit(
                    &format!("Error reading stdout from child: {}", e),
                    693,
                    false,
                ),
                Err(e) => std::panic::resume_unwind(e),
            };
            let err = match stderr_thread.join() {
                Ok(Ok(err)) => err,
                Ok(Err(e)) => hc.finish_and_exit(
                    &format!("Error reading stderr from child: {}", e),
                    693,
                    false,
                ),
                Err(e) => std::panic::resume_unwind(e),
            };
            let mut msg = String::new();
            let mut code = match status.code() {
                Some(code) => {
                    if let Err(e) = writeln!(msg, "Command exited with exit code {}", code) {
                        eprintln!("Write to message buffer failed: {}", e)
                    }
                    code
                }
                None => {
                    msg.push_str("Command exited without an exit code\n");
                    EXIT_CODE
                }
            };
            if !out.is_empty() {
                let _ = writeln!(msg, "stdout:");
                let _ = writeln!(msg, "{}", out.as_bstr());
            }
            if !err.is_empty() {
                if !out.is_empty() {
                    let _ = writeln!(msg);
                }
                let _ = writeln!(msg, "stderr:");
                let _ = writeln!(msg, "{}", err.as_bstr());
            }
            if ignore_code {
                // 0 would indicate success
                code = 0;
            }
            hc.finish_and_exit(&msg, code, false)
        }
        Err(e) => {
            let msg = format!("Failed waiting for process: {}", e);
            hc.finish_and_exit(&msg, EXIT_CODE, true)
        }
    }
}

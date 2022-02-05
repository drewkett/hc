use bstr::ByteSlice;
use std::{
    collections::HashMap,
    ffi::OsString,
    fmt::Write,
    process::{exit, Command, Stdio},
};

const EXIT_CODE: i32 = 963;

fn pipe(
    mut in_pipe: impl std::io::Read,
    mut out_pipe: Option<impl std::io::Write>,
) -> std::io::Result<Vec<u8>> {
    let mut i = 0;
    let mut out = Vec::new();
    let mut buf = [0; 16 * 1024];
    loop {
        match in_pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend(&buf[..n]);
                if let Some(ref mut out_pipe) = out_pipe {
                    match out[i..].iter().rev().position(|&c| c == b'\n') {
                        Some(j) => {
                            let i_end = out.len() - j;
                            if let Err(e) = out_pipe.write_all(&out[i..i_end]) {
                                eprintln!("Error writing to output stream: {}", e)
                            }
                            i = i_end;
                        }
                        None => break,
                    }
                }
            }
            Err(e) => return Err(e),
        }
    }
    if let Some(ref mut out_pipe) = out_pipe {
        if i < out.len() {
            if let Err(e) = out_pipe.write_all(&out[i..]) {
                eprintln!("Error writing to output stream: {}", e)
            }
        }
    }
    Ok(out)
}

fn hex_range(rng: &[u8]) -> bool {
    rng.iter()
        .all(|b| matches!(b, b'0'..=b'9'|b'a'..=b'z'|b'A'..=b'Z'))
}

fn valid_uuid(uuid: &str) -> bool {
    let uuid = uuid.as_bytes();
    if uuid.len() != 36 {
        return false;
    }
    if !hex_range(&uuid[..8]) {
        return false;
    }
    if uuid[8] != b'-' {
        return false;
    }
    if !hex_range(&uuid[9..13]) {
        return false;
    }
    if uuid[13] != b'-' {
        return false;
    }
    if !hex_range(&uuid[14..18]) {
        return false;
    }
    if uuid[18] != b'-' {
        return false;
    }
    if !hex_range(&uuid[19..23]) {
        return false;
    }
    if uuid[23] != b'-' {
        return false;
    }
    hex_range(&uuid[24..])
}

fn print_help() {
    eprintln!(
        "\
hc [--hc-id HC_ID] [--hc-tee] [--hc-ignore-code] [cmd [args...]]

    HC_ID can be set using an environment variable
    --hc-id HC_ID    Sets the healthchecks id. This can also be set using the
                     environment variable HC_ID
    --hc-ignore-code Ignore the return code from cmd. Also available using HC_IGNORE_CODE
    --hc-tee         Controls whether to also output the cmd stdout/stderr to the local
                     stdout/stderr. By default the output from the cmd will only get
                     passed as text to healthchecks. This option can also be enabled
                     using the environment variable HC_TEE. Only the existance of the
                     variable is checked
    [cmd [args...]]  If no command is passed, the healthcheck will be notified as a
                     success with the text 'No command given'
"
    )
}
fn main() {
    let mut args = std::env::args_os();
    let _ = args.next();
    let mut hc_id = std::env::var_os("HC_ID");
    let mut ignore_code = std::env::var_os("HC_IGNORE_CODE").is_some();
    let mut tee = std::env::var_os("HC_TEE").is_some();
    let filtered_env: HashMap<OsString, OsString> = std::env::vars_os()
        .filter(|&(ref k, _)| k != "HC_ID" && k != "HC_TEE")
        .collect();
    let mut cmd = loop {
        match args.next() {
            Some(arg) => match arg.to_str() {
                Some("--hc-id") => hc_id = args.next(),
                Some("--hc-tee") => tee = true,
                Some("--hc-ignore-code") => ignore_code = true,
                _ => break Some(arg),
            },
            None => break None,
        }
    };
    let hc_id = match hc_id.as_ref() {
        Some(hc_id) => match hc_id.to_str() {
            Some(hc_id) if valid_uuid(hc_id) => hc_id,
            Some(hc_id) => {
                eprintln!("Healthcheck Id isn't a valid uuid '{}'", hc_id);
                print_help();
                exit(1);
            }
            None => {
                let hc_id: &std::path::Path = hc_id.as_ref();
                eprintln!("Healthcheck Id isn't valid utf-8 '{}'", hc_id.display());
                exit(1);
            }
        },
        None => {
            eprintln!("No Healthcheck Id given");
            print_help();
            exit(1);
        }
    };
    let base_url = {
        let mut url = "https://hc-ping.com/".to_string();
        url.push_str(hc_id);
        url
    };
    let start_url = {
        let mut url = base_url.clone();
        url.push_str("/start");
        url
    };
    let finish_url = base_url.clone();
    let error_url = {
        let mut url = base_url;
        url.push_str("/fail");
        url
    };
    let post_and_exit = |msg: &str, code: i32| -> ! {
        let url = if code == 0 { &finish_url } else { &error_url };
        if let Err(e) = ureq::post(url).send_string(msg) {
            eprintln!("Error sending finishing request to healthchecks: {}", e);
            exit(EXIT_CODE)
        }
        exit(code);
    };
    let log_post_and_exit = |msg: &str, code: i32| -> ! {
        eprintln!("{}", msg);
        post_and_exit(msg, code)
    };
    if cmd.is_none() {
        cmd = args.next()
    }
    let cmd = match cmd {
        Some(cmd) => cmd,
        None => log_post_and_exit("No command given", 0),
    };
    if let Err(e) = ureq::get(&start_url).call() {
        eprintln!("Error on healthchecks /start call: {}", e);
    }
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
        Err(e) => log_post_and_exit(&format!("Failed to spawn process: {}", e), EXIT_CODE),
    };

    let child_stdout = proc.stdout.take().unwrap();
    let child_stderr = proc.stderr.take().unwrap();

    let pipe_stdout = if tee { Some(std::io::stdout()) } else { None };
    let pipe_stderr = if tee { Some(std::io::stderr()) } else { None };

    // Spawn threads for continuously reading from the child process's stdout and stderr. If
    // tee is enabled forward the output to the processes pipes
    let stdout_thread = std::thread::spawn(move || pipe(child_stdout, pipe_stdout));
    let stderr_thread = std::thread::spawn(move || pipe(child_stderr, pipe_stderr));

    match proc.wait() {
        Ok(status) => {
            let out = match stdout_thread.join() {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => {
                    post_and_exit(&format!("Error reading stdout from child: {}", e), 693)
                }
                Err(e) => std::panic::resume_unwind(e),
            };
            let err = match stderr_thread.join() {
                Ok(Ok(err)) => err,
                Ok(Err(e)) => {
                    post_and_exit(&format!("Error reading stderr from child: {}", e), 693)
                }
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
            post_and_exit(&msg, code)
        }
        Err(e) => log_post_and_exit(&format!("Failed waiting for process: {}", e), EXIT_CODE),
    }
}

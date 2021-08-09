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
    mut out_pipe: impl std::io::Write,
    tee: bool,
) -> std::io::Result<Vec<u8>> {
    let mut i = 0;
    let mut out = Vec::new();
    let mut buf = [0; 16 * 1024];
    loop {
        match in_pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend(&buf[..n]);
                if tee {
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
    if tee {
        if i < out.len() {
            if let Err(e) = out_pipe.write_all(&out[i..]) {
                eprintln!("Error writing to output stream: {}", e)
            }
        }
    }
    Ok(out)
}

fn main() {
    let mut args = std::env::args_os();
    let _ = args.next();
    let mut hc_id = std::env::var_os("HC_ID");
    let mut tee = std::env::var_os("HC_TEE").is_some();
    let filtered_env: HashMap<OsString, OsString> = std::env::vars_os()
        .filter(|&(ref k, _)| k != "HC_ID" && k != "HC_TEE")
        .collect();
    let mut cmd = loop {
        match args.next() {
            Some(arg) => match arg.to_str() {
                Some("--hc-id") => hc_id = args.next(),
                Some("--hc-tee") => tee = true,
                _ => break Some(arg),
            },
            None => break None,
        }
    };
    let hc_id = match hc_id.as_ref().map(|s| s.to_str()) {
        Some(Some(hc_id)) => hc_id,
        Some(None) => {
            eprintln!("Invalid HealthCheck ID given");
            exit(1);
        }
        None => {
            eprintln!("No HealthCheck ID given");
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
        let mut url = base_url.clone();
        url.push_str("/fail");
        url
    };
    let post_and_exit = |msg: &str, code: i32| -> ! {
        let url = if code == 0 { &finish_url } else { &error_url };
        if let Err(e) = ureq::post(&url).send_string(&msg) {
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
        None => log_post_and_exit("No command given to run", EXIT_CODE),
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
    let stdout_thread = std::thread::spawn(move || pipe(child_stdout, std::io::stdout(), tee));
    let stderr_thread = std::thread::spawn(move || pipe(child_stderr, std::io::stderr(), tee));
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
            let code = match status.code() {
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
                    let _ = writeln!(msg, "");
                }
                let _ = writeln!(msg, "stderr:");
                let _ = writeln!(msg, "{}", err.as_bstr());
            }
            post_and_exit(&msg, code)
        }
        Err(e) => log_post_and_exit(&format!("Failed waiting for process: {}", e), EXIT_CODE),
    }
}

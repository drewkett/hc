use bstr::ByteSlice;
use std::{
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
                            if let Err(_e) = out_pipe.write_all(&out[i..i_end]) {
                                // Not Sure if I should print this error
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
            if let Err(_e) = out_pipe.write_all(&out[i..]) {
                // Not Sure if I should print this error
            }
        }
    }
    Ok(out)
}

fn main() {
    // TODO forward stdin
    // TODO remote environment variables that are found
    let mut args = std::env::args_os();
    let _ = args.next();
    let mut hc_id = std::env::var_os("HC_ID");
    let mut tee = std::env::var_os("HC_TEE").is_some();
    let mut cmd = loop {
        match args.next() {
            Some(arg) => match arg.to_str() {
                Some("--hc-id") => hc_id = args.next(),
                Some("--hc-tee") => tee = true,
                Some(_cmd) => break Some(arg),
                // TODO Handle more elegantly
                None => break None,
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
    let finish = |msg: &str, code: i32| -> ! {
        let url = if code == 0 { &finish_url } else { &error_url };
        if let Err(_e) = ureq::post(&url).send_string(&msg) {
            // Not much to do here
            // Could check return code
        }
        exit(code);
    };
    let log_and_finish = |msg: &str, code: i32| -> ! {
        eprintln!("{}", msg);
        finish(msg, code)
    };
    if cmd.is_none() {
        cmd = args.next()
    }
    let cmd = match cmd {
        Some(cmd) => cmd,
        None => log_and_finish("No command given to run", EXIT_CODE),
    };
    if let Err(_e) = ureq::get(&start_url).call() {
        // This should log or something
    }
    let mut proc = match Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(p) => p,
        Err(e) => log_and_finish(&format!("Failed to spawn process: {}", e), EXIT_CODE),
    };
    let child_stdout = proc.stdout.take().unwrap();
    let child_stderr = proc.stderr.take().unwrap();
    let stdout_thread = std::thread::spawn(move || pipe(child_stdout, std::io::stdout(), tee));
    let stderr_thread = std::thread::spawn(move || pipe(child_stderr, std::io::stderr(), tee));
    match proc.wait() {
        Ok(status) => {
            let out = match stdout_thread.join() {
                Ok(Ok(out)) => out,
                Ok(Err(e)) => finish(&format!("Error reading stdout from child: {}", e), 693),
                Err(e) => std::panic::resume_unwind(e),
            };
            let err = match stderr_thread.join() {
                Ok(Ok(err)) => err,
                Ok(Err(e)) => finish(&format!("Error reading stderr from child: {}", e), 693),
                Err(e) => std::panic::resume_unwind(e),
            };
            let mut msg = String::new();
            let code = match status.code() {
                Some(code) => {
                    if let Err(_e) = writeln!(msg, "Command exited with exit code {}", code) {
                        // Not sure what to do here, but should only fail on out of memory i assume
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
            finish(&msg, code)
        }
        Err(e) => log_and_finish(&format!("Failed waiting for process: {}", e), EXIT_CODE),
    }
}

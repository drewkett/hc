use std::process::{exit, Command, Stdio};

fn pipe(mut in_pipe: impl std::io::Read, mut out_pipe: impl std::io::Write) -> Vec<u8> {
    let mut i = 0;
    let mut out = Vec::new();
    let mut buf = [0; 16 * 1024];
    loop {
        match in_pipe.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                out.extend(&buf[..n]);
                loop {
                    match out[i..].iter().position(|&c| c == b'\n') {
                        Some(j) => {
                            i += j + 1;
                            if let Err(_e) = out_pipe.write_all(&out[i..i + j + 1]) {
                                // Not Sure if I should print this error
                            }
                        }
                        None => break,
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading stdout: {}", e);
                break;
            }
        }
        if i < out.len() {
            if let Err(_e) = out_pipe.write_all(&out[i..]) {
                // Not Sure if I should print this error
            }
        }
    }
    out
}

fn main() {
    let mut args = std::env::args_os();
    let cmd = match args.next() {
        Some(cmd) => cmd,
        None => {
            eprintln!("No arguments given");
            exit(1);
        }
    };
    let mut proc = match Command::new(cmd)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to spawn process: {}", e);
            exit(2);
        }
    };
    let child_stdout = proc.stdout.take().unwrap();
    let child_stderr = proc.stderr.take().unwrap();
    let stdout_thread = std::thread::spawn(move || pipe(child_stdout, std::io::stdout()));
    let stderr_thread = std::thread::spawn(move || pipe(child_stderr, std::io::stderr()));
    match proc.wait() {
        Ok(status) => {
            let code = status.code().unwrap_or(963);
            let _out = match stdout_thread.join() {
                Ok(out) => out,
                Err(e) => std::panic::resume_unwind(e),
            };
            let _err = match stderr_thread.join() {
                Ok(err) => err,
                Err(e) => std::panic::resume_unwind(e),
            };
            exit(code)
        }
        Err(e) => {
            eprintln!("Failed waiting for process: {}", e);
            exit(3)
        }
    }
}

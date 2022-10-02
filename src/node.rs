use color_eyre::eyre;
use color_eyre::eyre::eyre;
use color_eyre::eyre::Context;
use std::io::Write;
use std::process::Command;
use std::process::Stdio;

pub fn js_eval(code: String) -> eyre::Result<String> {
    let mut child = Command::new("node")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .wrap_err("Failed to execute `node`")?;

    // If the child process fills its stdout buffer, it may end up
    // waiting until the parent reads the stdout, and not be able to
    // read stdin in the meantime, causing a deadlock.
    // Writing from another thread ensures that stdout is being read
    // at the same time, avoiding the problem.
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| eyre!("Failed to open `node`'s stdin"))?;
    let thread = std::thread::spawn(move || {
        stdin
            .write_all(code.as_bytes())
            .wrap_err("Failed to write JavaScript to `node`'s stdin")
    });

    let output = child
        .wait_with_output()
        .wrap_err("failed to wait on child")?;

    thread
        .join()
        .map_err(|_err| eyre!("Uh oh!"))?
        .wrap_err("Failed to join `node`-stdin-writer thread")?;

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

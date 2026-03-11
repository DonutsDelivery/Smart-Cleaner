use std::process::Command;

use gtk::gio;
use gtk::glib;

use crate::model::removal_plan::RemovalCommand;

/// Execute removal commands in a background thread, callback on main thread
pub fn execute_removal(
    commands: Vec<RemovalCommand>,
    on_complete: impl FnOnce(bool, Vec<String>) + 'static,
) {
    glib::spawn_future_local(async move {
        let (all_success, output_lines) = gio::spawn_blocking(move || {
            let mut output_lines = Vec::new();
            let mut all_success = true;

            for cmd in &commands {
                output_lines.push(format!(
                    ">>> [{}] Removing: {}",
                    cmd.source,
                    cmd.packages.join(", ")
                ));
                output_lines.push(format!("$ {}", cmd.display));

                let result = Command::new(&cmd.program).args(&cmd.args).output();

                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);

                        if !stdout.is_empty() {
                            for line in stdout.lines() {
                                output_lines.push(line.to_string());
                            }
                        }
                        if !stderr.is_empty() {
                            for line in stderr.lines() {
                                output_lines.push(format!("STDERR: {line}"));
                            }
                        }

                        if !output.status.success() {
                            output_lines.push(format!(
                                "ERROR: Command exited with status {}",
                                output.status
                            ));
                            all_success = false;
                        } else {
                            output_lines.push("OK".to_string());
                        }
                    }
                    Err(e) => {
                        output_lines.push(format!("ERROR: Failed to execute: {e}"));
                        all_success = false;
                    }
                }

                output_lines.push(String::new());
            }

            (all_success, output_lines)
        })
        .await
        .expect("Removal thread panicked");

        on_complete(all_success, output_lines);
    });
}

use std::process::{Command, Output};
use crate::error::{NetError, Result};

pub struct NetnsController;

impl NetnsController {
    /// Executes an arbitrary command inside a designated Linux network namespace
    pub fn exec_in_netns(netns: &str, binary: &str, args: &[&str]) -> Result<Output> {
        let mut cmd = Command::new("ip");
        cmd.args(["netns", "exec", netns, binary]);
        cmd.args(args);

        cmd.output().map_err(|e| {
            NetError::SocketError(format!("Failed to execute command in netns '{}': {}", netns, e))
        })
    }

    /// Modulates tc-netem parameters programmatically from Rust
    pub fn set_link_params(
        netns: &str,
        dev: &str,
        delay_ms: f64,
        loss_pct: f64,
        rate_kbps: u32,
    ) -> Result<()> {
        let delay_arg = format!("{:.2}ms", delay_ms);
        let loss_arg = format!("{:.2}%", loss_pct);
        let rate_arg = format!("{}kbit", rate_kbps);

        let output = Command::new("ip")
            .args([
                "netns", "exec", netns, "tc", "qdisc", "change", "dev", dev, "root", "netem",
                "delay", &delay_arg, "loss", &loss_arg, "rate", &rate_arg, "limit", "1000",
            ])
            .output()
            .map_err(|e| NetError::SocketError(format!("tc execution failed: {}", e)))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(NetError::SocketError(format!("tc qdisc update failed: {}", err_msg)));
        }

        Ok(())
    }
}
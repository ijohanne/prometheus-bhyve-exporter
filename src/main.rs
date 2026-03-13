use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Router};
use clap::Parser;
use prometheus_client::encoding::text::encode;
use prometheus_client::encoding::EncodeLabelSet;
use prometheus_client::metrics::counter::Counter;
use prometheus_client::metrics::family::Family;
use prometheus_client::metrics::gauge::Gauge;
use prometheus_client::registry::Registry;

#[derive(Parser)]
#[command(name = "prometheus-bhyve-exporter")]
#[command(about = "Prometheus exporter for FreeBSD bhyve VMs managed by vm-bhyve")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:9288")]
    listen_address: String,

    #[arg(long, default_value = "/usr/local/sbin/vm")]
    vm_command: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, EncodeLabelSet)]
struct VmLabels {
    vm: String,
}

struct Metrics {
    vm_up: Family<VmLabels, Gauge>,
    cpu_allocated: Family<VmLabels, Gauge>,
    memory_allocated_bytes: Family<VmLabels, Gauge>,
    cpu_usage_percent: Family<VmLabels, Gauge<f64, AtomicU64>>,
    memory_rss_bytes: Family<VmLabels, Gauge>,
    memory_vsz_bytes: Family<VmLabels, Gauge>,
    vm_pid: Family<VmLabels, Gauge>,
    scrape_duration_seconds: Gauge<f64, AtomicU64>,
    scrape_errors_total: Counter,
}

use std::sync::atomic::AtomicU64;

impl Metrics {
    fn new(registry: &mut Registry) -> Self {
        let vm_up = Family::default();
        registry.register("bhyve_vm_up", "Whether the VM is running", vm_up.clone());

        let cpu_allocated = Family::default();
        registry.register(
            "bhyve_vm_cpu_allocated",
            "Number of CPUs allocated to VM",
            cpu_allocated.clone(),
        );

        let memory_allocated_bytes = Family::default();
        registry.register(
            "bhyve_vm_memory_allocated_bytes",
            "Memory allocated to VM in bytes",
            memory_allocated_bytes.clone(),
        );

        let cpu_usage_percent: Family<VmLabels, Gauge<f64, AtomicU64>> = Family::default();
        registry.register(
            "bhyve_vm_cpu_usage_percent",
            "Current CPU usage percentage from ps",
            cpu_usage_percent.clone(),
        );

        let memory_rss_bytes = Family::default();
        registry.register(
            "bhyve_vm_memory_rss_bytes",
            "Resident set size in bytes",
            memory_rss_bytes.clone(),
        );

        let memory_vsz_bytes = Family::default();
        registry.register(
            "bhyve_vm_memory_vsz_bytes",
            "Virtual memory size in bytes",
            memory_vsz_bytes.clone(),
        );

        let vm_pid = Family::default();
        registry.register("bhyve_vm_pid", "PID of the bhyve process", vm_pid.clone());

        let scrape_duration_seconds: Gauge<f64, AtomicU64> = Gauge::default();
        registry.register(
            "bhyve_exporter_scrape_duration_seconds",
            "Duration of the last scrape in seconds",
            scrape_duration_seconds.clone(),
        );

        let scrape_errors_total = Counter::default();
        registry.register(
            "bhyve_exporter_scrape_errors_total",
            "Total number of scrape errors",
            scrape_errors_total.clone(),
        );

        Self {
            vm_up,
            cpu_allocated,
            memory_allocated_bytes,
            cpu_usage_percent,
            memory_rss_bytes,
            memory_vsz_bytes,
            vm_pid,
            scrape_duration_seconds,
            scrape_errors_total,
        }
    }

    fn clear_families(&self) {
        self.vm_up.clear();
        self.cpu_allocated.clear();
        self.memory_allocated_bytes.clear();
        self.cpu_usage_percent.clear();
        self.memory_rss_bytes.clear();
        self.memory_vsz_bytes.clear();
        self.vm_pid.clear();
    }
}

#[derive(Debug)]
struct VmInfo {
    name: String,
    cpu: i64,
    memory_bytes: i64,
    state: VmState,
}

#[derive(Debug)]
enum VmState {
    Running(i64),
    Stopped,
}

struct PsInfo {
    cpu_percent: f64,
    rss_kb: i64,
    vsz_kb: i64,
}

fn parse_memory(mem_str: &str) -> Result<i64> {
    let mem_str = mem_str.trim();
    let (num_str, multiplier) = if let Some(n) = mem_str.strip_suffix('G') {
        (n, 1024 * 1024 * 1024_i64)
    } else if let Some(n) = mem_str.strip_suffix('M') {
        (n, 1024 * 1024_i64)
    } else if let Some(n) = mem_str.strip_suffix('K') {
        (n, 1024_i64)
    } else {
        (mem_str, 1_i64)
    };
    let num: i64 = num_str
        .parse()
        .with_context(|| format!("failed to parse memory value: {mem_str}"))?;
    Ok(num * multiplier)
}

fn parse_vm_list(output: &str) -> Result<Vec<VmInfo>> {
    let mut vms = Vec::new();

    for line in output.lines().skip(1) {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }

        let name = parts[0].to_string();
        let cpu: i64 = parts[3].parse().unwrap_or(0);
        let memory_bytes = parse_memory(parts[4]).unwrap_or(0);

        let state_idx = parts.len() - 1;
        let state_str = parts[state_idx];

        let state = if state_str.starts_with('(') && state_str.ends_with(')') {
            let pid_str = &state_str[1..state_str.len() - 1];
            match pid_str.parse::<i64>() {
                Ok(pid) => VmState::Running(pid),
                Err(_) => VmState::Stopped,
            }
        } else {
            VmState::Stopped
        };

        vms.push(VmInfo {
            name,
            cpu,
            memory_bytes,
            state,
        });
    }

    Ok(vms)
}

fn get_ps_info(pid: i64) -> Result<PsInfo> {
    let output = Command::new("ps")
        .args(["-o", "pid,pcpu,rss,vsz", "-p", &pid.to_string()])
        .output()
        .context("failed to execute ps")?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 {
            let cpu_percent: f64 = parts[1].parse().unwrap_or(0.0);
            let rss_kb: i64 = parts[2].parse().unwrap_or(0);
            let vsz_kb: i64 = parts[3].parse().unwrap_or(0);
            return Ok(PsInfo {
                cpu_percent,
                rss_kb,
                vsz_kb,
            });
        }
    }

    anyhow::bail!("no ps output for pid {pid}")
}

fn collect(vm_command: &str, metrics: &Metrics) {
    metrics.clear_families();

    let output = match Command::new(vm_command).arg("list").output() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("failed to run vm list: {e}");
            metrics.scrape_errors_total.inc();
            return;
        }
    };

    if !output.status.success() {
        eprintln!(
            "vm list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        metrics.scrape_errors_total.inc();
        return;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let vms = match parse_vm_list(&stdout) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("failed to parse vm list: {e}");
            metrics.scrape_errors_total.inc();
            return;
        }
    };

    for vm in &vms {
        let labels = VmLabels {
            vm: vm.name.clone(),
        };

        metrics.cpu_allocated.get_or_create(&labels).set(vm.cpu);
        metrics
            .memory_allocated_bytes
            .get_or_create(&labels)
            .set(vm.memory_bytes);

        match &vm.state {
            VmState::Running(pid) => {
                metrics.vm_up.get_or_create(&labels).set(1);
                metrics.vm_pid.get_or_create(&labels).set(*pid);

                match get_ps_info(*pid) {
                    Ok(ps) => {
                        metrics
                            .cpu_usage_percent
                            .get_or_create(&labels)
                            .set(ps.cpu_percent);
                        metrics
                            .memory_rss_bytes
                            .get_or_create(&labels)
                            .set(ps.rss_kb * 1024);
                        metrics
                            .memory_vsz_bytes
                            .get_or_create(&labels)
                            .set(ps.vsz_kb * 1024);
                    }
                    Err(e) => {
                        eprintln!("failed to get ps info for {} (pid {}): {e}", vm.name, pid);
                        metrics.scrape_errors_total.inc();
                    }
                }
            }
            VmState::Stopped => {
                metrics.vm_up.get_or_create(&labels).set(0);
                metrics.vm_pid.get_or_create(&labels).set(0);
            }
        }
    }
}

struct AppState {
    registry: Registry,
    metrics: Metrics,
    vm_command: String,
}

async fn metrics_handler(
    State(state): State<Arc<Mutex<AppState>>>,
) -> impl IntoResponse {
    let state = state.lock().unwrap();
    let start = Instant::now();

    collect(&state.vm_command, &state.metrics);

    let duration = start.elapsed().as_secs_f64();
    state.metrics.scrape_duration_seconds.set(duration);

    let mut buf = String::new();
    match encode(&mut buf, &state.registry) {
        Ok(()) => (
            StatusCode::OK,
            [("content-type", "application/openmetrics-text; version=1.0.0; charset=utf-8")],
            buf,
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [("content-type", "text/plain; charset=utf-8")],
            format!("encoding error: {e}"),
        ),
    }
}

async fn root_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        "<html><head><title>bhyve Exporter</title></head><body><h1>bhyve Exporter</h1><p><a href=\"/metrics\">Metrics</a></p></body></html>",
    )
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut registry = Registry::default();
    let metrics = Metrics::new(&mut registry);

    let state = Arc::new(Mutex::new(AppState {
        registry,
        metrics,
        vm_command: args.vm_command,
    }));

    let app = Router::new()
        .route("/", get(root_handler))
        .route("/metrics", get(metrics_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&args.listen_address)
        .await
        .with_context(|| format!("failed to bind to {}", args.listen_address))?;

    println!("listening on {}", args.listen_address);

    axum::serve(listener, app)
        .await
        .context("server error")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory() {
        assert_eq!(parse_memory("32G").unwrap(), 32 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory("96G").unwrap(), 96 * 1024 * 1024 * 1024);
        assert_eq!(parse_memory("512M").unwrap(), 512 * 1024 * 1024);
        assert_eq!(parse_memory("8G").unwrap(), 8 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_vm_list() {
        let output = "\
NAME         DATASTORE  LOADER  CPU  MEMORY  VNC           AUTO     STATE
ccdmonkey    default    grub    4    32G     -             Yes [3]  Running (55121)
pakhet       ssddata    grub    12   96G     -             No       Stopped
thoth        ssddata    grub    4    8G      0.0.0.0:5901  Yes [1]  Running (1234)
";
        let vms = parse_vm_list(output).unwrap();
        assert_eq!(vms.len(), 3);

        assert_eq!(vms[0].name, "ccdmonkey");
        assert_eq!(vms[0].cpu, 4);
        assert_eq!(vms[0].memory_bytes, 32 * 1024 * 1024 * 1024);
        match vms[0].state {
            VmState::Running(pid) => assert_eq!(pid, 55121),
            _ => panic!("expected Running"),
        }

        assert_eq!(vms[1].name, "pakhet");
        assert_eq!(vms[1].cpu, 12);
        assert_eq!(vms[1].memory_bytes, 96 * 1024 * 1024 * 1024);
        assert!(matches!(vms[1].state, VmState::Stopped));

        assert_eq!(vms[2].name, "thoth");
        assert_eq!(vms[2].cpu, 4);
        match vms[2].state {
            VmState::Running(pid) => assert_eq!(pid, 1234),
            _ => panic!("expected Running"),
        }
    }
}

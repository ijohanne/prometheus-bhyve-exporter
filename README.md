# prometheus-bhyve-exporter

Prometheus exporter for FreeBSD bhyve virtual machines managed by [vm-bhyve](https://github.com/churchers/vm-bhyve).

Collects VM state, resource allocation, and runtime process metrics by parsing `vm list` output and querying `ps` for running VMs.

## Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `bhyve_vm_up` | gauge | Whether the VM is running (1) or stopped (0) |
| `bhyve_vm_cpu_allocated` | gauge | Number of CPUs allocated to VM |
| `bhyve_vm_memory_allocated_bytes` | gauge | Memory allocated to VM in bytes |
| `bhyve_vm_cpu_usage_percent` | gauge | Current CPU usage percentage from ps |
| `bhyve_vm_memory_rss_bytes` | gauge | Resident set size in bytes |
| `bhyve_vm_memory_vsz_bytes` | gauge | Virtual memory size in bytes |
| `bhyve_vm_pid` | gauge | PID of the bhyve process (0 if stopped) |
| `bhyve_exporter_scrape_duration_seconds` | gauge | Duration of the last scrape |
| `bhyve_exporter_scrape_errors_total` | counter | Total number of scrape errors |

All per-VM metrics have a `vm` label with the VM name.

## Installation

### From source

Requires Rust toolchain.

```bash
git clone https://github.com/ijohanne/prometheus-bhyve-exporter.git
cd prometheus-bhyve-exporter
sudo make install
```

This installs the binary to `/usr/local/bin/` and the rc.d script to `/usr/local/etc/rc.d/`.

### Enable and start

```bash
sudo sysrc bhyve_exporter_enable=YES
sudo service bhyve_exporter start
```

### Configuration

rc.conf variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `bhyve_exporter_enable` | `NO` | Enable the service |
| `bhyve_exporter_listen_address` | `0.0.0.0:9288` | Address and port to listen on |
| `bhyve_exporter_vm_command` | `/usr/local/sbin/vm` | Path to the vm-bhyve command |

## Prometheus

```yaml
scrape_configs:
  - job_name: bhyve
    static_configs:
      - targets: ['your-host:9288']
```

## Grafana

An example dashboard is included in [`grafana/dashboard.json`](grafana/dashboard.json).

Import it via Grafana UI (Dashboards > Import > Upload JSON file) or provision it directly.

## License

MIT

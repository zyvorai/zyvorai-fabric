// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package doctor

import (
	"bufio"
	"context"
	"crypto/tls"
	"fmt"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"time"
)

const gib = uint64(1024 * 1024 * 1024)

// Run executes all production-preflight checks. A failed check never aborts the
// rest of the run; operators get a complete view in one invocation.
func Run(ctx context.Context, cfg Config, version string) Report {
	host, _ := os.Hostname()
	results := make([]CheckResult, 0, 20)

	checks := []func(context.Context, Config) CheckResult{
		checkLinux,
		checkRoot,
		checkCPUHardwareVirtualization,
		checkKVMDevice,
		checkKVMModule,
		checkTunDevice,
		checkVhostNet,
		checkCgroupV2,
		checkNetworkCommands,
		checkQEMU,
		checkDataDirectory,
		checkDataDirectoryFreeSpace,
		checkTimeSync,
		checkSecurityLSM,
	}
	for _, check := range checks {
		results = append(results, check(ctx, cfg))
	}

	if !cfg.SkipServicePing {
		results = append(results,
			checkFabricHealth(ctx, cfg),
			checkFabricReadyz(ctx, cfg),
			checkFluxVMTCP(ctx, cfg),
			checkFluxVMReadyz(ctx, cfg),
		)
	}

	return Report{
		SchemaVersion: "v1",
		ToolVersion:   version,
		GeneratedAt:   time.Now().UTC(),
		Hostname:      host,
		OS:            runtime.GOOS,
		Architecture:  runtime.GOARCH,
		Summary:       Summarize(results),
		Checks:        results,
	}
}

func timed(id, category string, fn func() (Status, string, string)) CheckResult {
	started := time.Now()
	status, message, remediation := fn()
	return CheckResult{
		ID:          id,
		Category:    category,
		Status:      status,
		Message:     message,
		Remediation: remediation,
		DurationMS:  time.Since(started).Milliseconds(),
	}
}

func checkLinux(_ context.Context, _ Config) CheckResult {
	return timed("host.linux", "host", func() (Status, string, string) {
		if runtime.GOOS != "linux" {
			return StatusFail, "Fabric VM hosts require Linux; detected " + runtime.GOOS, "Run Fabric Doctor on the target Linux host."
		}
		return StatusPass, "Linux host detected", ""
	})
}

func checkRoot(_ context.Context, _ Config) CheckResult {
	return timed("host.privilege", "host", func() (Status, string, string) {
		if os.Geteuid() != 0 {
			return StatusWarn, fmt.Sprintf("doctor is running as uid %d; some host checks may be incomplete", os.Geteuid()), "Re-run with sudo for the same privilege view Fabric uses on a VM host."
		}
		return StatusPass, "running with root privileges", ""
	})
}

func checkCPUHardwareVirtualization(_ context.Context, _ Config) CheckResult {
	return timed("compute.cpu_virtualization", "compute", func() (Status, string, string) {
		data, err := os.ReadFile("/proc/cpuinfo")
		if err != nil {
			return StatusWarn, "unable to read /proc/cpuinfo: " + err.Error(), "Verify Intel VT-x (vmx) or AMD-V (svm) is enabled in firmware."
		}
		text := string(data)
		if strings.Contains(text, " vmx ") || strings.Contains(text, " svm ") || strings.Contains(text, "\tvmx ") || strings.Contains(text, "\tsvm ") {
			return StatusPass, "hardware virtualization flag detected (vmx/svm)", ""
		}
		return StatusFail, "no vmx/svm CPU flag detected", "Enable Intel VT-x or AMD-V in BIOS/UEFI and verify virtualization is exposed to this host."
	})
}

func checkKVMDevice(_ context.Context, _ Config) CheckResult {
	return timed("compute.kvm_device", "compute", func() (Status, string, string) {
		st, err := os.Stat("/dev/kvm")
		if err != nil {
			return StatusFail, "/dev/kvm is unavailable", "Load the KVM modules and ensure the host exposes /dev/kvm."
		}
		if st.Mode()&os.ModeDevice == 0 {
			return StatusFail, "/dev/kvm exists but is not a device node", "Repair the KVM device node before starting Fabric."
		}
		f, err := os.OpenFile("/dev/kvm", os.O_RDWR, 0)
		if err != nil {
			return StatusFail, "/dev/kvm exists but cannot be opened read/write: " + err.Error(), "Run Fabric with sufficient privileges or fix /dev/kvm ownership/permissions."
		}
		_ = f.Close()
		return StatusPass, "/dev/kvm exists and is accessible read/write", ""
	})
}

func checkKVMModule(_ context.Context, _ Config) CheckResult {
	return timed("compute.kvm_module", "compute", func() (Status, string, string) {
		if pathExists("/sys/module/kvm") {
			if pathExists("/sys/module/kvm_intel") || pathExists("/sys/module/kvm_amd") {
				return StatusPass, "KVM core and vendor module are loaded", ""
			}
			return StatusWarn, "KVM core is loaded but kvm_intel/kvm_amd was not detected", "Load the matching vendor KVM module."
		}
		return StatusFail, "KVM kernel module is not loaded", "Load kvm plus kvm_intel or kvm_amd."
	})
}

func checkTunDevice(_ context.Context, _ Config) CheckResult {
	return timed("network.tun", "network", func() (Status, string, string) {
		if !pathExists("/dev/net/tun") {
			return StatusFail, "/dev/net/tun is unavailable", "Load the tun module and expose /dev/net/tun to the Fabric host/container."
		}
		return StatusPass, "TUN/TAP device is available", ""
	})
}

func checkVhostNet(_ context.Context, _ Config) CheckResult {
	return timed("network.vhost_net", "network", func() (Status, string, string) {
		if pathExists("/dev/vhost-net") || pathExists("/sys/module/vhost_net") {
			return StatusPass, "vhost-net acceleration is available", ""
		}
		return StatusWarn, "vhost-net acceleration was not detected", "Load vhost_net for better virtio networking performance."
	})
}

func checkCgroupV2(_ context.Context, _ Config) CheckResult {
	return timed("host.cgroup_v2", "host", func() (Status, string, string) {
		data, err := os.ReadFile("/sys/fs/cgroup/cgroup.controllers")
		if err != nil {
			return StatusFail, "cgroup v2 unified hierarchy was not detected", "Boot with cgroup v2 enabled; Fabric/FluxVM use modern cgroup controls."
		}
		controllers := strings.Fields(string(data))
		return StatusPass, fmt.Sprintf("cgroup v2 detected (%d controllers)", len(controllers)), ""
	})
}

func checkNetworkCommands(_ context.Context, _ Config) CheckResult {
	return timed("network.tools", "network", func() (Status, string, string) {
		required := []string{"ip", "nft"}
		optional := []string{"bridge", "tc"}
		var missingRequired, missingOptional []string
		for _, name := range required {
			if _, err := exec.LookPath(name); err != nil {
				missingRequired = append(missingRequired, name)
			}
		}
		for _, name := range optional {
			if _, err := exec.LookPath(name); err != nil {
				missingOptional = append(missingOptional, name)
			}
		}
		if len(missingRequired) > 0 {
			return StatusFail, "missing required networking commands: " + strings.Join(missingRequired, ", "), "Install iproute2 and nftables on the host."
		}
		if len(missingOptional) > 0 {
			return StatusWarn, "core networking tools found; optional commands missing: " + strings.Join(missingOptional, ", "), "Install full iproute2 tooling for advanced bridge/QoS operations."
		}
		return StatusPass, "ip, nft, bridge and tc are available", ""
	})
}

func checkQEMU(_ context.Context, _ Config) CheckResult {
	return timed("compute.qemu", "compute", func() (Status, string, string) {
		candidates := []string{"qemu-system-x86_64", "qemu-system-aarch64", "qemu-kvm"}
		for _, name := range candidates {
			if path, err := exec.LookPath(name); err == nil {
				return StatusPass, "QEMU executable found at " + path, ""
			}
		}
		return StatusWarn, "QEMU executable was not found in PATH", "Install QEMU on hosts where FluxVM uses the QEMU backend; ignore this warning for non-QEMU backends."
	})
}

func checkDataDirectory(_ context.Context, cfg Config) CheckResult {
	return timed("storage.data_dir", "storage", func() (Status, string, string) {
		if cfg.DataDir == "" {
			return StatusFail, "data directory is empty", "Set --data-dir to Fabric's state directory."
		}
		st, err := os.Stat(cfg.DataDir)
		if os.IsNotExist(err) {
			parent := filepath.Dir(cfg.DataDir)
			if !pathExists(parent) {
				return StatusFail, "data directory and parent do not exist: " + cfg.DataDir, "Create the Fabric state path before first production start."
			}
			return StatusWarn, "data directory does not exist yet: " + cfg.DataDir, "Fabric may create it on first start; verify ownership and persistent storage first."
		}
		if err != nil {
			return StatusFail, "cannot stat data directory: " + err.Error(), "Verify the Fabric data directory path and permissions."
		}
		if !st.IsDir() {
			return StatusFail, cfg.DataDir + " is not a directory", "Point --data-dir at Fabric's state directory."
		}
		probe := filepath.Join(cfg.DataDir, ".fabric-doctor-write-probe-"+strconv.Itoa(os.Getpid()))
		if err := os.WriteFile(probe, []byte("doctor\n"), 0600); err != nil {
			return StatusFail, "data directory is not writable: " + err.Error(), "Fix ownership/permissions for the Fabric daemon user."
		}
		_ = os.Remove(probe)
		return StatusPass, "Fabric data directory exists and is writable: " + cfg.DataDir, ""
	})
}

func checkDataDirectoryFreeSpace(_ context.Context, cfg Config) CheckResult {
	return timed("storage.free_space", "storage", func() (Status, string, string) {
		path := cfg.DataDir
		for !pathExists(path) {
			parent := filepath.Dir(path)
			if parent == path {
				path = "/"
				break
			}
			path = parent
		}
		var stat syscall.Statfs_t
		if err := syscall.Statfs(path, &stat); err != nil {
			return StatusWarn, "unable to read filesystem capacity: " + err.Error(), "Check free space manually for the Fabric state filesystem."
		}
		free := stat.Bavail * uint64(stat.Bsize)
		freeGiB := free / gib
		if freeGiB < cfg.MinimumFreeGiB {
			return StatusFail, fmt.Sprintf("only %d GiB free on %s; minimum is %d GiB", freeGiB, path, cfg.MinimumFreeGiB), "Expand/clean the filesystem or lower --min-free-gib only for intentional lab use."
		}
		if freeGiB < cfg.MinimumFreeGiB*2 {
			return StatusWarn, fmt.Sprintf("%d GiB free on %s; headroom is limited", freeGiB, path), "Plan additional capacity before storing VM images, snapshots or backups."
		}
		return StatusPass, fmt.Sprintf("%d GiB free on %s", freeGiB, path), ""
	})
}

func checkTimeSync(_ context.Context, _ Config) CheckResult {
	return timed("host.time_sync", "host", func() (Status, string, string) {
		if data, err := os.ReadFile("/run/systemd/timesync/synchronized"); err == nil {
			if strings.TrimSpace(string(data)) == "yes" {
				return StatusPass, "systemd-timesyncd reports synchronized time", ""
			}
		}
		if _, err := exec.LookPath("timedatectl"); err == nil {
			cmd := exec.Command("timedatectl", "show", "-p", "NTPSynchronized", "--value")
			out, err := cmd.Output()
			if err == nil && strings.EqualFold(strings.TrimSpace(string(out)), "yes") {
				return StatusPass, "NTP synchronization is active", ""
			}
		}
		return StatusWarn, "time synchronization could not be confirmed", "Enable chronyd/systemd-timesyncd/NTP; cluster auth, TLS and HA depend on sane clocks."
	})
}

func checkSecurityLSM(_ context.Context, _ Config) CheckResult {
	return timed("security.lsm", "security", func() (Status, string, string) {
		data, err := os.ReadFile("/sys/kernel/security/lsm")
		if err != nil {
			return StatusInfo, "kernel LSM list is not readable", ""
		}
		lsm := strings.TrimSpace(string(data))
		if lsm == "" {
			return StatusWarn, "no active Linux Security Module was reported", "Consider SELinux or AppArmor according to your host security baseline."
		}
		return StatusPass, "active Linux Security Modules: " + lsm, ""
	})
}

func checkFabricHealth(ctx context.Context, cfg Config) CheckResult {
	return timed("service.fabric_api", "service", func() (Status, string, string) {
		client := &http.Client{Timeout: cfg.HTTPTimeout}
		req, err := http.NewRequestWithContext(ctx, http.MethodGet, cfg.FabricURL, nil)
		if err != nil {
			return StatusFail, "invalid Fabric health URL: " + err.Error(), "Set --fabric-url to a valid http(s) health endpoint."
		}
		resp, err := client.Do(req)
		if err != nil {
			status := StatusWarn
			if cfg.StrictServices {
				status = StatusFail
			}
			return status, "Fabric API is not reachable: " + err.Error(), "Start zyvor-fabricd or use --skip-service-ping while validating a host before installation."
		}
		defer resp.Body.Close()
		if resp.StatusCode < 200 || resp.StatusCode >= 300 {
			status := StatusWarn
			if cfg.StrictServices {
				status = StatusFail
			}
			return status, fmt.Sprintf("Fabric health endpoint returned HTTP %d", resp.StatusCode), "Inspect zyvor-fabricd logs and health dependencies."
		}
		msg := fmt.Sprintf("Fabric health endpoint is reachable (HTTP %d)", resp.StatusCode)
		if u, err := url.Parse(cfg.FabricURL); err == nil && strings.EqualFold(u.Scheme, "https") {
			if certMsg := tlsCertificateHealth(u, cfg.TCPTimeout); certMsg != "" {
				msg += "; " + certMsg
			}
		}
		return StatusPass, msg, ""
	})
}

func checkFluxVMTCP(ctx context.Context, cfg Config) CheckResult {
	return timed("service.fluxvm", "service", func() (Status, string, string) {
		d := net.Dialer{Timeout: cfg.TCPTimeout}
		conn, err := d.DialContext(ctx, "tcp", cfg.FluxVMAddress)
		if err != nil {
			status := StatusWarn
			if cfg.StrictServices {
				status = StatusFail
			}
			return status, "FluxVM is not reachable at " + cfg.FluxVMAddress + ": " + err.Error(), "Start FluxVM or use --skip-service-ping when checking a pre-install host."
		}
		_ = conn.Close()
		return StatusPass, "FluxVM TCP endpoint is reachable at " + cfg.FluxVMAddress, ""
	})
}

func checkFabricReadyz(ctx context.Context, cfg Config) CheckResult {
	return timed("service.fabric_readyz", "service", func() (Status, string, string) {
		readyURL := cfg.FabricReadyURL
		if readyURL == "" {
			readyURL = strings.TrimSuffix(cfg.FabricURL, "/health") + "/readyz"
		}
		return httpProbe(ctx, cfg, readyURL, "Fabric readiness", "Inspect zyvor-fabricd /readyz (store + FluxVM).")
	})
}

func checkFluxVMReadyz(ctx context.Context, cfg Config) CheckResult {
	return timed("service.fluxvm_readyz", "service", func() (Status, string, string) {
		readyURL := cfg.FluxVMReadyURL
		if readyURL == "" {
			readyURL = "http://" + cfg.FluxVMAddress + "/readyz"
		}
		return httpProbe(ctx, cfg, readyURL, "FluxVM readiness", "Inspect FluxVM /readyz (state dir + dataplane when required).")
	})
}

func httpProbe(ctx context.Context, cfg Config, endpoint, label, remediation string) (Status, string, string) {
	client := &http.Client{Timeout: cfg.HTTPTimeout}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, endpoint, nil)
	if err != nil {
		return StatusFail, "invalid " + label + " URL: " + err.Error(), "Set a valid http(s) " + label + " endpoint."
	}
	resp, err := client.Do(req)
	if err != nil {
		status := StatusWarn
		if cfg.StrictServices {
			status = StatusFail
		}
		return status, label + " is not reachable: " + err.Error(), remediation
	}
	defer resp.Body.Close()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		status := StatusWarn
		if cfg.StrictServices {
			status = StatusFail
		}
		return status, fmt.Sprintf("%s returned HTTP %d", label, resp.StatusCode), remediation
	}
	return StatusPass, fmt.Sprintf("%s is reachable (HTTP %d)", label, resp.StatusCode), ""
}

func tlsCertificateHealth(u *url.URL, timeout time.Duration) string {
	host := u.Host
	if !strings.Contains(host, ":") {
		host += ":443"
	}
	serverName := u.Hostname()
	d := &net.Dialer{Timeout: timeout}
	conn, err := tls.DialWithDialer(d, "tcp", host, &tls.Config{ServerName: serverName, MinVersion: tls.VersionTLS12})
	if err != nil {
		return "TLS certificate inspection failed"
	}
	defer conn.Close()
	certs := conn.ConnectionState().PeerCertificates
	if len(certs) == 0 {
		return "no peer certificate presented"
	}
	days := int(time.Until(certs[0].NotAfter).Hours() / 24)
	return fmt.Sprintf("TLS certificate expires in %d days", days)
}

func pathExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

// ReadFirstMatchingField returns a single key from a key:value text file. It is
// used by support-bundle code and intentionally kept small and deterministic.
func ReadFirstMatchingField(path, key string) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	scanner := bufio.NewScanner(f)
	prefix := key + ":"
	for scanner.Scan() {
		line := scanner.Text()
		if strings.HasPrefix(line, prefix) {
			return strings.TrimSpace(strings.TrimPrefix(line, prefix))
		}
	}
	return ""
}

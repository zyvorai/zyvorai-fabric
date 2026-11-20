// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

package bundle

import (
	"archive/tar"
	"compress/gzip"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/zyvorai/fabric/tools/fabric-doctor/internal/doctor"
)

const maxCollectedBytes = 2 * 1024 * 1024

// Options controls support-bundle collection. Sensitive file contents are not
// included unless IncludeConfig is explicitly requested, and are redacted even then.
type Options struct {
	OutputPath    string
	ConfigPath    string
	IncludeConfig bool
	IncludeLogs   bool
}

type collector struct {
	tw *tar.Writer
}

func Create(report doctor.Report, opts Options) (string, error) {
	path := opts.OutputPath
	if path == "" {
		stamp := time.Now().UTC().Format("20060102T150405Z")
		path = "fabric-support-" + stamp + ".tar.gz"
	}
	f, err := os.OpenFile(path, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0600)
	if err != nil {
		return "", fmt.Errorf("create bundle: %w", err)
	}
	defer f.Close()
	gz := gzip.NewWriter(f)
	defer gz.Close()
	c := &collector{tw: tar.NewWriter(gz)}
	defer c.tw.Close()

	if err := c.addJSON("report.json", report); err != nil {
		return "", err
	}
	_ = c.addText("system/runtime.txt", fmt.Sprintf("goos=%s\ngoarch=%s\nhostname=%s\n", runtime.GOOS, runtime.GOARCH, report.Hostname))
	_ = c.addCommand("system/uname.txt", "uname", "-a")
	_ = c.addCommand("system/memory.txt", "free", "-h")
	_ = c.addCommand("system/storage.txt", "df", "-hT")
	_ = c.addCommand("network/ip-address.txt", "ip", "-brief", "address")
	_ = c.addCommand("network/ip-route.txt", "ip", "route", "show")
	_ = c.addCommand("network/nftables.txt", "nft", "list", "ruleset")
	_ = c.addCommand("compute/lsmod.txt", "lsmod")
	_ = c.addCommand("compute/lscpu.txt", "lscpu")

	cpuModel := doctor.ReadFirstMatchingField("/proc/cpuinfo", "model name")
	if cpuModel == "" {
		cpuModel = doctor.ReadFirstMatchingField("/proc/cpuinfo", "Processor")
	}
	_ = c.addText("compute/cpu-summary.txt", "model="+cpuModel+"\n")

	if opts.ConfigPath != "" {
		_ = c.addConfigMetadata(opts.ConfigPath)
		if opts.IncludeConfig {
			_ = c.addRedactedFile("config/zyvor-fabricd.toml.redacted", opts.ConfigPath)
		}
	}
	if opts.IncludeLogs {
		_ = c.addCommand("logs/zyvor-fabricd-journal.txt", "journalctl", "--no-pager", "-u", "zyvor-fabricd", "-n", "1000")
		_ = c.addCommand("logs/fluxvm-journal.txt", "journalctl", "--no-pager", "-u", "fluxvm", "-n", "1000")
	}

	if err := c.tw.Close(); err != nil {
		return "", fmt.Errorf("finalize tar: %w", err)
	}
	if err := gz.Close(); err != nil {
		return "", fmt.Errorf("finalize gzip: %w", err)
	}
	if err := f.Close(); err != nil {
		return "", fmt.Errorf("finalize bundle file: %w", err)
	}
	return path, nil
}

func (c *collector) addJSON(name string, v any) error {
	data, err := json.MarshalIndent(v, "", "  ")
	if err != nil {
		return err
	}
	return c.addBytes(name, append(data, '\n'))
}

func (c *collector) addText(name, text string) error {
	return c.addBytes(name, []byte(Redact(text)))
}

func (c *collector) addBytes(name string, data []byte) error {
	if len(data) > maxCollectedBytes {
		data = append(data[:maxCollectedBytes], []byte("\n[TRUNCATED BY FABRIC DOCTOR]\n")...)
	}
	h := &tar.Header{
		Name:    filepath.ToSlash(name),
		Mode:    0600,
		Size:    int64(len(data)),
		ModTime: time.Now().UTC(),
	}
	if err := c.tw.WriteHeader(h); err != nil {
		return err
	}
	_, err := c.tw.Write(data)
	return err
}

func (c *collector) addCommand(name, command string, args ...string) error {
	path, err := exec.LookPath(command)
	if err != nil {
		return c.addText(name, "command not available: "+command+"\n")
	}
	cmd := exec.Command(path, args...)
	out, err := cmd.CombinedOutput()
	if err != nil {
		out = append(out, []byte("\ncommand error: "+err.Error()+"\n")...)
	}
	return c.addText(name, string(out))
}

func (c *collector) addConfigMetadata(path string) error {
	st, err := os.Stat(path)
	if err != nil {
		return c.addText("config/metadata.txt", "config unavailable: "+err.Error()+"\n")
	}
	f, err := os.Open(path)
	if err != nil {
		return c.addText("config/metadata.txt", "config unavailable: "+err.Error()+"\n")
	}
	defer f.Close()
	h := sha256.New()
	_, _ = io.Copy(h, f)
	meta := fmt.Sprintf("path=%s\nsize=%d\nmode=%s\nmodified=%s\nsha256=%s\n",
		path, st.Size(), st.Mode().Perm(), st.ModTime().UTC().Format(time.RFC3339), hex.EncodeToString(h.Sum(nil)))
	return c.addText("config/metadata.txt", meta)
}

func (c *collector) addRedactedFile(name, path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return c.addText(name, "unable to read config: "+err.Error()+"\n")
	}
	if len(data) > maxCollectedBytes {
		data = data[:maxCollectedBytes]
	}
	text := Redact(string(data))
	if !strings.HasSuffix(text, "\n") {
		text += "\n"
	}
	return c.addText(name, text)
}

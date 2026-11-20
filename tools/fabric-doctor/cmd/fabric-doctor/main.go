// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"time"

	"github.com/zyvorai/fabric/tools/fabric-doctor/internal/bundle"
	"github.com/zyvorai/fabric/tools/fabric-doctor/internal/doctor"
)

var version = "dev"

func main() {
	os.Exit(run(os.Args[1:]))
}

func run(args []string) int {
	if len(args) == 0 {
		args = []string{"check"}
	}
	switch args[0] {
	case "check":
		return runCheck(args[1:])
	case "bundle":
		return runBundle(args[1:])
	case "version", "--version", "-version":
		fmt.Println(version)
		return 0
	case "help", "--help", "-h":
		usage()
		return 0
	default:
		fmt.Fprintf(os.Stderr, "unknown command %q\n\n", args[0])
		usage()
		return 2
	}
}

func commonFlags(fs *flag.FlagSet, cfg *doctor.Config, format *string) {
	fs.StringVar(&cfg.FabricURL, "fabric-url", cfg.FabricURL, "Fabric liveness endpoint URL (/health)")
	fs.StringVar(&cfg.FabricReadyURL, "fabric-ready-url", cfg.FabricReadyURL, "Fabric readiness endpoint URL (/readyz)")
	fs.StringVar(&cfg.FluxVMAddress, "fluxvm-address", cfg.FluxVMAddress, "FluxVM TCP address")
	fs.StringVar(&cfg.FluxVMReadyURL, "fluxvm-ready-url", cfg.FluxVMReadyURL, "FluxVM readiness endpoint URL (/readyz)")
	fs.StringVar(&cfg.DataDir, "data-dir", cfg.DataDir, "Fabric state/data directory")
	fs.Uint64Var(&cfg.MinimumFreeGiB, "min-free-gib", cfg.MinimumFreeGiB, "minimum free space required")
	fs.DurationVar(&cfg.HTTPTimeout, "http-timeout", cfg.HTTPTimeout, "HTTP check timeout")
	fs.DurationVar(&cfg.TCPTimeout, "tcp-timeout", cfg.TCPTimeout, "FluxVM TCP check timeout")
	fs.BoolVar(&cfg.StrictServices, "strict-services", false, "treat Fabric/FluxVM unreachability as failure instead of warning")
	fs.BoolVar(&cfg.SkipServicePing, "skip-service-ping", false, "skip Fabric and FluxVM reachability checks")
	fs.StringVar(format, "output", "table", "output format: table or json")
}

func runCheck(args []string) int {
	cfg := doctor.DefaultConfig()
	var format string
	fs := flag.NewFlagSet("check", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	commonFlags(fs, &cfg, &format)
	if err := fs.Parse(args); err != nil {
		return 2
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	report := doctor.Run(ctx, cfg, version)
	if format == "json" {
		if err := doctor.WriteJSON(os.Stdout, report); err != nil {
			fmt.Fprintln(os.Stderr, err)
			return 2
		}
	} else if format == "table" {
		doctor.WriteTable(os.Stdout, report)
	} else {
		fmt.Fprintf(os.Stderr, "unsupported output format %q (use table or json)\n", format)
		return 2
	}
	if report.Summary.Failed > 0 {
		return 1
	}
	return 0
}

func runBundle(args []string) int {
	cfg := doctor.DefaultConfig()
	var format, output, configPath string
	var includeConfig, includeLogs bool
	fs := flag.NewFlagSet("bundle", flag.ContinueOnError)
	fs.SetOutput(os.Stderr)
	commonFlags(fs, &cfg, &format)
	fs.StringVar(&output, "file", "", "output .tar.gz path")
	fs.StringVar(&configPath, "config", "/etc/zyvor-fabricd/zyvor-fabricd.toml", "Fabric config path; only metadata is included by default")
	fs.BoolVar(&includeConfig, "include-config", false, "include a redacted copy of the Fabric config")
	fs.BoolVar(&includeLogs, "include-logs", false, "include redacted recent service journal output")
	if err := fs.Parse(args); err != nil {
		return 2
	}

	ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
	defer cancel()
	report := doctor.Run(ctx, cfg, version)
	path, err := bundle.Create(report, bundle.Options{
		OutputPath: output, ConfigPath: configPath,
		IncludeConfig: includeConfig, IncludeLogs: includeLogs,
	})
	if err != nil {
		fmt.Fprintln(os.Stderr, "bundle:", err)
		return 2
	}
	fmt.Printf("Support bundle written to %s\n", path)
	fmt.Println("Secrets are redacted best-effort; review the archive before sharing it outside your organization.")
	if report.Summary.Failed > 0 {
		return 1
	}
	return 0
}

func usage() {
	fmt.Print(`fabric-doctor - Zyvor Fabric production preflight and support bundle

Usage:
  fabric-doctor check [flags]
  fabric-doctor bundle [flags]
  fabric-doctor version

Examples:
  fabric-doctor check
  fabric-doctor check --output json --skip-service-ping
  fabric-doctor check --strict-services --min-free-gib 50
  sudo fabric-doctor bundle --include-logs
`)
}

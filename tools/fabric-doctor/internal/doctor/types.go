// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package doctor

import "time"

// Status is the outcome of a diagnostic check.
type Status string

const (
	StatusPass Status = "pass"
	StatusWarn Status = "warn"
	StatusFail Status = "fail"
	StatusInfo Status = "info"
)

// CheckResult is one operator-facing diagnostic result.
type CheckResult struct {
	ID          string `json:"id"`
	Category    string `json:"category"`
	Status      Status `json:"status"`
	Message     string `json:"message"`
	Remediation string `json:"remediation,omitempty"`
	DurationMS  int64  `json:"duration_ms"`
}

// Summary aggregates diagnostic outcomes.
type Summary struct {
	Passed int `json:"passed"`
	Warned int `json:"warned"`
	Failed int `json:"failed"`
	Info   int `json:"info"`
}

// Report is the stable machine-readable doctor output.
type Report struct {
	SchemaVersion string        `json:"schema_version"`
	ToolVersion   string        `json:"tool_version"`
	GeneratedAt   time.Time     `json:"generated_at"`
	Hostname      string        `json:"hostname"`
	OS            string        `json:"os"`
	Architecture  string        `json:"architecture"`
	Summary       Summary       `json:"summary"`
	Checks        []CheckResult `json:"checks"`
}

// Config controls local and service checks.
type Config struct {
	FabricURL       string
	FabricReadyURL  string
	FluxVMAddress   string
	FluxVMReadyURL  string
	DataDir         string
	MinimumFreeGiB  uint64
	HTTPTimeout     time.Duration
	TCPTimeout      time.Duration
	StrictServices  bool
	SkipServicePing bool
}

// DefaultConfig matches the current Fabric defaults documented by the project.
func DefaultConfig() Config {
	return Config{
		FabricURL:      "http://127.0.0.1:9095/health",
		FabricReadyURL: "http://127.0.0.1:9095/readyz",
		FluxVMAddress:  "127.0.0.1:7788",
		FluxVMReadyURL: "http://127.0.0.1:7788/readyz",
		DataDir:        "/var/lib/zyvor-fabricd",
		MinimumFreeGiB: 10,
		HTTPTimeout:    2 * time.Second,
		TCPTimeout:     2 * time.Second,
	}
}

func Summarize(results []CheckResult) Summary {
	var s Summary
	for _, r := range results {
		switch r.Status {
		case StatusPass:
			s.Passed++
		case StatusWarn:
			s.Warned++
		case StatusFail:
			s.Failed++
		default:
			s.Info++
		}
	}
	return s
}

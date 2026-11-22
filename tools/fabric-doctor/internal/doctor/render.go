// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package doctor

import (
	"encoding/json"
	"fmt"
	"io"
	"strings"
)

func WriteJSON(w io.Writer, report Report) error {
	enc := json.NewEncoder(w)
	enc.SetIndent("", "  ")
	return enc.Encode(report)
}

func WriteTable(w io.Writer, report Report) {
	fmt.Fprintf(w, "Zyvor Fabric Doctor %s\n", report.ToolVersion)
	fmt.Fprintf(w, "Host: %s  OS: %s/%s\n", report.Hostname, report.OS, report.Architecture)
	fmt.Fprintln(w, strings.Repeat("-", 92))
	fmt.Fprintf(w, "%-7s %-10s %-31s %s\n", "STATUS", "CATEGORY", "CHECK", "MESSAGE")
	fmt.Fprintln(w, strings.Repeat("-", 92))
	for _, r := range report.Checks {
		fmt.Fprintf(w, "%-7s %-10s %-31s %s\n", strings.ToUpper(string(r.Status)), r.Category, r.ID, r.Message)
		if r.Remediation != "" && (r.Status == StatusWarn || r.Status == StatusFail) {
			fmt.Fprintf(w, "        %-10s %-31s -> %s\n", "", "", r.Remediation)
		}
	}
	fmt.Fprintln(w, strings.Repeat("-", 92))
	fmt.Fprintf(w, "Summary: %d passed, %d warnings, %d failed, %d info\n",
		report.Summary.Passed, report.Summary.Warned, report.Summary.Failed, report.Summary.Info)
}

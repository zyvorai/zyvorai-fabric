// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package bundle

import (
	"regexp"
	"strings"
)

var secretAssignmentLine = regexp.MustCompile(`(?im)^(\s*(?:password|passwd|secret|token|api[_-]?key|jwt[_-]?secret|client[_-]?secret|private[_-]?key)\s*[:=]\s*).*$`)
var jsonSecret = regexp.MustCompile(`(?i)("(?:password|passwd|secret|token|api[_-]?key|jwt[_-]?secret|client[_-]?secret|private[_-]?key)"\s*:\s*)"(?:\\.|[^"\\])*"`)
var bearer = regexp.MustCompile(`(?i)bearer\s+[A-Za-z0-9._~+/-]+=*`)
var authHeader = regexp.MustCompile(`(?im)^(authorization\s*:\s*).+$`)

// Redact removes common secret forms from diagnostic text before it enters a
// support bundle. It deliberately favors over-redaction over convenience.
func Redact(input string) string {
	out := secretAssignmentLine.ReplaceAllString(input, `${1}[REDACTED]`)
	out = jsonSecret.ReplaceAllString(out, `${1}"[REDACTED]"`)
	out = bearer.ReplaceAllString(out, "Bearer [REDACTED]")
	out = authHeader.ReplaceAllString(out, `${1}[REDACTED]`)
	// Environment-style credential URLs are common in proxy settings.
	for _, scheme := range []string{"http://", "https://"} {
		idx := 0
		for {
			pos := strings.Index(out[idx:], scheme)
			if pos < 0 {
				break
			}
			start := idx + pos + len(scheme)
			at := strings.Index(out[start:], "@")
			if at < 0 {
				break
			}
			at += start
			segment := out[start:at]
			if strings.Contains(segment, ":") && !strings.ContainsAny(segment, "/ \t\n") {
				out = out[:start] + "[REDACTED]@" + out[at+1:]
				idx = start + len("[REDACTED]@")
				continue
			}
			idx = at + 1
		}
	}
	return out
}

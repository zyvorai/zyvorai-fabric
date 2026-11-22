// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package bundle

import (
	"strings"
	"testing"
)

func TestRedactSecrets(t *testing.T) {
	input := `password = "super secret with spaces"
api_key: abc123
Authorization: Bearer eyJhbGciOiJ...
proxy=https://user:pass@example.com:8443
client_secret=myclientsecret
{"token":"json-secret-value","mode":"production"}`
	out := Redact(input)
	for _, forbidden := range []string{"super secret", "abc123", "eyJhbGciOiJ", "user:pass", "myclientsecret", "json-secret-value"} {
		if strings.Contains(out, forbidden) {
			t.Fatalf("secret %q leaked in %q", forbidden, out)
		}
	}
	if !strings.Contains(out, "[REDACTED]") {
		t.Fatalf("expected redaction marker: %q", out)
	}
}

func TestRedactPreservesNormalText(t *testing.T) {
	input := "listen_address = 127.0.0.1:9095\nmode = production\n"
	if got := Redact(input); got != input {
		t.Fatalf("unexpected change: %q", got)
	}
}

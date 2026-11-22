// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package bundle

import (
	"archive/tar"
	"compress/gzip"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/zyvorai/fabric/tools/fabric-doctor/internal/doctor"
)

func TestCreateBundleDoesNotIncludeConfigByDefault(t *testing.T) {
	dir := t.TempDir()
	config := filepath.Join(dir, "zyvor-fabricd.toml")
	if err := os.WriteFile(config, []byte("password = should-not-leak\n"), 0600); err != nil {
		t.Fatal(err)
	}
	out := filepath.Join(dir, "bundle.tar.gz")
	_, err := Create(doctor.Report{ToolVersion: "test"}, Options{OutputPath: out, ConfigPath: config})
	if err != nil {
		t.Fatal(err)
	}
	contents := readBundle(t, out)
	if strings.Contains(contents, "should-not-leak") {
		t.Fatal("raw config secret leaked into default support bundle")
	}
	if !strings.Contains(contents, "sha256=") {
		t.Fatal("expected config metadata hash")
	}
}

func TestCreateBundleRedactsIncludedConfig(t *testing.T) {
	dir := t.TempDir()
	config := filepath.Join(dir, "zyvor-fabricd.toml")
	if err := os.WriteFile(config, []byte("jwt_secret = topsecret\n"), 0600); err != nil {
		t.Fatal(err)
	}
	out := filepath.Join(dir, "bundle.tar.gz")
	_, err := Create(doctor.Report{ToolVersion: "test"}, Options{OutputPath: out, ConfigPath: config, IncludeConfig: true})
	if err != nil {
		t.Fatal(err)
	}
	contents := readBundle(t, out)
	if strings.Contains(contents, "topsecret") {
		t.Fatal("included config secret was not redacted")
	}
	if !strings.Contains(contents, "[REDACTED]") {
		t.Fatal("redaction marker missing")
	}
}

func readBundle(t *testing.T, path string) string {
	t.Helper()
	f, err := os.Open(path)
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	gz, err := gzip.NewReader(f)
	if err != nil {
		t.Fatal(err)
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	var b strings.Builder
	for {
		_, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatal(err)
		}
		data, err := io.ReadAll(tr)
		if err != nil {
			t.Fatal(err)
		}
		b.Write(data)
		b.WriteByte('\n')
	}
	return b.String()
}

// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package doctor

import (
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestSummarize(t *testing.T) {
	results := []CheckResult{
		{Status: StatusPass},
		{Status: StatusPass},
		{Status: StatusWarn},
		{Status: StatusFail},
		{Status: StatusInfo},
	}
	s := Summarize(results)
	if s.Passed != 2 || s.Warned != 1 || s.Failed != 1 || s.Info != 1 {
		t.Fatalf("unexpected summary: %+v", s)
	}
}

func TestFabricHealthPass(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/health" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
	}))
	defer ts.Close()

	cfg := DefaultConfig()
	cfg.FabricURL = ts.URL + "/health"
	cfg.HTTPTimeout = time.Second
	result := checkFabricHealth(context.Background(), cfg)
	if result.Status != StatusPass {
		t.Fatalf("expected pass, got %+v", result)
	}
}

func TestFabricHealthStrictFailure(t *testing.T) {
	cfg := DefaultConfig()
	cfg.FabricURL = "http://127.0.0.1:1/health"
	cfg.HTTPTimeout = 50 * time.Millisecond
	cfg.StrictServices = true
	result := checkFabricHealth(context.Background(), cfg)
	if result.Status != StatusFail {
		t.Fatalf("expected fail, got %+v", result)
	}
}

func TestFabricReadyzPass(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/readyz" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer ts.Close()

	cfg := DefaultConfig()
	cfg.FabricReadyURL = ts.URL + "/readyz"
	cfg.HTTPTimeout = time.Second
	result := checkFabricReadyz(context.Background(), cfg)
	if result.Status != StatusPass {
		t.Fatalf("expected pass, got %+v", result)
	}
}

func TestFluxVMReadyzPass(t *testing.T) {
	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/readyz" {
			t.Fatalf("unexpected path: %s", r.URL.Path)
		}
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer ts.Close()

	cfg := DefaultConfig()
	cfg.FluxVMReadyURL = ts.URL + "/readyz"
	cfg.HTTPTimeout = time.Second
	result := checkFluxVMReadyz(context.Background(), cfg)
	if result.Status != StatusPass {
		t.Fatalf("expected pass, got %+v", result)
	}
}

func TestTableIncludesRemediation(t *testing.T) {
	report := Report{
		ToolVersion:  "test",
		Hostname:     "host",
		OS:           "linux",
		Architecture: "amd64",
		Summary:      Summary{Warned: 1},
		Checks: []CheckResult{{
			ID: "x", Category: "host", Status: StatusWarn,
			Message: "warning", Remediation: "fix it",
		}},
	}
	var b bytes.Buffer
	WriteTable(&b, report)
	if !strings.Contains(b.String(), "fix it") {
		t.Fatalf("table missing remediation: %s", b.String())
	}
}

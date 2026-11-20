// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

// Package main runs client smoke checks against a live zyvor-fabricd (see scripts/acceptance-smoke.sh).
package main

import (
	"context"
	"fmt"
	"os"

	"github.com/zyvorai/terraform-provider-zyvor-fabricd/internal/provider"
)

func main() {
	endpoint := os.Getenv("ZYVOR_FABRICD_ENDPOINT")
	token := os.Getenv("ZYVOR_FABRICD_TOKEN")
	if endpoint == "" || token == "" {
		fmt.Fprintln(os.Stderr, "ZYVOR_FABRICD_ENDPOINT and ZYVOR_FABRICD_TOKEN required")
		os.Exit(1)
	}
	client := provider.NewClient(endpoint, token)
	ctx := context.Background()

	// VM read path
	if _, err := client.GetVM(ctx, "__tf_smoke_missing__"); err == nil {
		fmt.Fprintln(os.Stderr, "expected missing VM error")
		os.Exit(1)
	}

	// Network policy create/delete
	policy, err := client.CreateNetworkPolicy(ctx, "tf-smoke-policy", "terraform acceptance", true, map[string]string{})
	if err != nil {
		fmt.Fprintf(os.Stderr, "CreateNetworkPolicy: %v\n", err)
		os.Exit(1)
	}
	if err := client.DeleteNetworkPolicy(ctx, policy.ID); err != nil {
		fmt.Fprintf(os.Stderr, "DeleteNetworkPolicy: %v\n", err)
		os.Exit(1)
	}

	fmt.Println("ok")
}

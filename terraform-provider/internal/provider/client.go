// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package provider

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

type Client struct {
	baseURL    string
	token      string
	httpClient *http.Client
}

func NewClient(endpoint, token string) *Client {
	return &Client{
		baseURL: strings.TrimRight(endpoint, "/"),
		token:   token,
		httpClient: &http.Client{
			Timeout: 60 * time.Second,
		},
	}
}

func (c *Client) do(ctx context.Context, method, path string, body any, out any) error {
	var reader io.Reader
	if body != nil {
		b, err := json.Marshal(body)
		if err != nil {
			return err
		}
		reader = bytes.NewReader(b)
	}

	req, err := http.NewRequestWithContext(ctx, method, c.baseURL+path, reader)
	if err != nil {
		return err
	}
	if body != nil {
		req.Header.Set("Content-Type", "application/json")
	}
	if c.token != "" {
		req.Header.Set("Authorization", "Bearer "+c.token)
	}

	res, err := c.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer res.Body.Close()

	data, err := io.ReadAll(res.Body)
	if err != nil {
		return err
	}
	if res.StatusCode == http.StatusNotFound {
		return &APIError{Status: res.StatusCode, Method: method, Path: path, Body: strings.TrimSpace(string(data))}
	}
	if res.StatusCode >= 400 {
		return &APIError{Status: res.StatusCode, Method: method, Path: path, Body: strings.TrimSpace(string(data))}
	}
	if out == nil || len(data) == 0 {
		return nil
	}
	return json.Unmarshal(data, out)
}

// APIError is an HTTP failure from fabricd. Status 404 means the object is gone.
type APIError struct {
	Status int
	Method string
	Path   string
	Body   string
}

func (e *APIError) Error() string {
	return fmt.Sprintf("API %s %s: %s", e.Method, e.Path, e.Body)
}

func IsNotFound(err error) bool {
	var api *APIError
	if !errors.As(err, &api) {
		return false
	}
	return api.Status == http.StatusNotFound
}

type vmRecord struct {
	Name   string  `json:"name"`
	State  string  `json:"state"`
	CPUs   int64   `json:"cpus"`
	Memory int64   `json:"memory"`
	Image  string  `json:"image"`
	IP     *string `json:"ip"`
}

type createVMRequest struct {
	Name   string `json:"name"`
	Image  string `json:"image"`
	CPUs   int64  `json:"cpus"`
	Memory int64  `json:"memory"`
}

func (c *Client) GetVM(ctx context.Context, name string) (*vmRecord, error) {
	var vm vmRecord
	if err := c.do(ctx, http.MethodGet, "/api/vms/"+name, nil, &vm); err != nil {
		return nil, err
	}
	return &vm, nil
}

func (c *Client) CreateVM(ctx context.Context, req createVMRequest) (*vmRecord, error) {
	var vm vmRecord
	if err := c.do(ctx, http.MethodPost, "/api/vms", req, &vm); err != nil {
		return nil, err
	}
	return &vm, nil
}

func (c *Client) DeleteVM(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, "/api/vms/"+name, nil, nil)
}

func (c *Client) StartVM(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodPost, "/api/vms/"+name+"/start", map[string]any{}, nil)
}

type storagePoolRecord struct {
	Name string `json:"name"`
	Path string `json:"path"`
	Type string `json:"type"`
}

type createLocalPoolRequest struct {
	Name      string `json:"name"`
	Path      string `json:"path"`
	AutoStart bool   `json:"auto_start"`
}

func (c *Client) GetStoragePool(ctx context.Context, name string) (*storagePoolRecord, error) {
	var pool storagePoolRecord
	if err := c.do(ctx, http.MethodGet, "/api/storage/pools/"+name, nil, &pool); err != nil {
		return nil, err
	}
	return &pool, nil
}

func (c *Client) CreateLocalStoragePool(ctx context.Context, name, path string, autoStart bool) (*storagePoolRecord, error) {
	var pool storagePoolRecord
	req := createLocalPoolRequest{Name: name, Path: path, AutoStart: autoStart}
	if err := c.do(ctx, http.MethodPost, "/api/storage/pools/local", req, &pool); err != nil {
		return nil, err
	}
	return &pool, nil
}

func (c *Client) DeleteStoragePool(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, "/api/storage/pools/"+name, nil, nil)
}

type networkPolicyRecord struct {
	ID      string `json:"id"`
	Name    string `json:"name"`
	Enabled bool   `json:"enabled"`
}

type createNetworkPolicyRequest struct {
	Name             string        `json:"name"`
	Description      string        `json:"description"`
	EndpointSelector labelSelector `json:"endpoint_selector"`
	Ingress          []any         `json:"ingress"`
	Egress           []any         `json:"egress"`
	Enabled          bool          `json:"enabled"`
}

type labelSelector struct {
	MatchLabels map[string]string `json:"match_labels"`
}

func (c *Client) CreateNetworkPolicy(ctx context.Context, name, description string, enabled bool, labels map[string]string) (*networkPolicyRecord, error) {
	var policy networkPolicyRecord
	req := createNetworkPolicyRequest{
		Name:             name,
		Description:      description,
		Enabled:          enabled,
		EndpointSelector: labelSelector{MatchLabels: labels},
		Ingress:          []any{},
		Egress:           []any{},
	}
	if err := c.do(ctx, http.MethodPost, "/api/network-policies", req, &policy); err != nil {
		return nil, err
	}
	return &policy, nil
}

func (c *Client) DeleteNetworkPolicy(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, "/api/network-policies/"+id, nil, nil)
}

type vmSnapshotRecord struct {
	ID     string `json:"id"`
	VMName string `json:"vm_name"`
	Name   string `json:"name"`
}

type createSnapshotRequest struct {
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
}

func (c *Client) CreateVMSnapshot(ctx context.Context, vmName, snapshotName, description string) (*vmSnapshotRecord, error) {
	var snap vmSnapshotRecord
	req := createSnapshotRequest{Name: snapshotName, Description: description}
	if err := c.do(ctx, http.MethodPost, "/api/vms/"+vmName+"/snapshots", req, &snap); err != nil {
		return nil, err
	}
	return &snap, nil
}

func (c *Client) DeleteVMSnapshot(ctx context.Context, vmName, snapshotID string) error {
	return c.do(ctx, http.MethodDelete, "/api/vms/"+vmName+"/snapshots/"+snapshotID, nil, nil)
}

// --- AI Workloads -----------------------------------------------------------

type modelArtifactRecord struct {
	Name      string  `json:"name"`
	Source    string  `json:"source"`
	Format    string  `json:"format"`
	Revision  *string `json:"revision"`
	Checksum  *string `json:"checksum"`
	Tenant    *string `json:"tenant"`
	LocalPath *string `json:"local_path"`
}

type createModelArtifactRequest struct {
	Name     string  `json:"name"`
	Source   string  `json:"source"`
	Format   string  `json:"format"`
	Revision *string `json:"revision,omitempty"`
	Checksum *string `json:"checksum,omitempty"`
	Tenant   *string `json:"tenant,omitempty"`
}

func (c *Client) GetModelArtifact(ctx context.Context, name string) (*modelArtifactRecord, error) {
	var m modelArtifactRecord
	if err := c.do(ctx, http.MethodGet, "/api/ai/models/"+name, nil, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

func (c *Client) CreateModelArtifact(ctx context.Context, req createModelArtifactRequest) (*modelArtifactRecord, error) {
	var m modelArtifactRecord
	if err := c.do(ctx, http.MethodPost, "/api/ai/models", req, &m); err != nil {
		return nil, err
	}
	return &m, nil
}

func (c *Client) DeleteModelArtifact(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, "/api/ai/models/"+name, nil, nil)
}

type inferenceProfileRecord struct {
	Name      string `json:"name"`
	Runtime   string `json:"runtime"`
	GPU       gpuReq `json:"gpu"`
	CPU       int64  `json:"cpu"`
	MemoryGiB int64  `json:"memory_gib"`
}

type gpuReq struct {
	Vendor         string `json:"vendor"`
	Count          int64  `json:"count"`
	MinimumVramGiB int64  `json:"minimum_vram_gib"`
}

type createInferenceProfileRequest struct {
	Name      string `json:"name"`
	Runtime   string `json:"runtime"`
	GPU       gpuReq `json:"gpu"`
	CPU       int64  `json:"cpu"`
	MemoryGiB int64  `json:"memory_gib"`
}

func (c *Client) GetInferenceProfile(ctx context.Context, name string) (*inferenceProfileRecord, error) {
	var p inferenceProfileRecord
	if err := c.do(ctx, http.MethodGet, "/api/ai/profiles/"+name, nil, &p); err != nil {
		return nil, err
	}
	return &p, nil
}

func (c *Client) CreateInferenceProfile(ctx context.Context, req createInferenceProfileRequest) (*inferenceProfileRecord, error) {
	var p inferenceProfileRecord
	if err := c.do(ctx, http.MethodPost, "/api/ai/profiles", req, &p); err != nil {
		return nil, err
	}
	return &p, nil
}

func (c *Client) DeleteInferenceProfile(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, "/api/ai/profiles/"+name, nil, nil)
}

type inferenceDeploymentRecord struct {
	Name     string                        `json:"name"`
	Model    string                        `json:"model"`
	Profile  string                        `json:"profile"`
	Replicas int64                         `json:"replicas"`
	Tenant   *string                       `json:"tenant"`
	Status   inferenceDeploymentStatusJSON `json:"status"`
}

type inferenceDeploymentStatusJSON struct {
	Phase   string `json:"phase"`
	Message string `json:"message"`
}

type createInferenceDeploymentRequest struct {
	Name     string  `json:"name"`
	Model    string  `json:"model"`
	Profile  string  `json:"profile"`
	Replicas int64   `json:"replicas"`
	Tenant   *string `json:"tenant,omitempty"`
}

type scaleInferenceDeploymentRequest struct {
	Replicas int64 `json:"replicas"`
}

func (c *Client) GetInferenceDeployment(ctx context.Context, name string) (*inferenceDeploymentRecord, error) {
	var d inferenceDeploymentRecord
	if err := c.do(ctx, http.MethodGet, "/api/ai/deployments/"+name, nil, &d); err != nil {
		return nil, err
	}
	return &d, nil
}

func (c *Client) CreateInferenceDeployment(ctx context.Context, req createInferenceDeploymentRequest) (*inferenceDeploymentRecord, error) {
	var d inferenceDeploymentRecord
	if err := c.do(ctx, http.MethodPost, "/api/ai/deployments", req, &d); err != nil {
		return nil, err
	}
	return &d, nil
}

func (c *Client) ScaleInferenceDeployment(ctx context.Context, name string, replicas int64) (*inferenceDeploymentRecord, error) {
	var d inferenceDeploymentRecord
	if err := c.do(ctx, http.MethodPost, "/api/ai/deployments/"+name+"/scale", scaleInferenceDeploymentRequest{Replicas: replicas}, &d); err != nil {
		return nil, err
	}
	return &d, nil
}

func (c *Client) DeleteInferenceDeployment(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, "/api/ai/deployments/"+name, nil, nil)
}

type inferenceEndpointRecord struct {
	Name            string  `json:"name"`
	Deployment      string  `json:"deployment"`
	Protocol        string  `json:"protocol"`
	Port            int64   `json:"port"`
	VIP             *string `json:"vip"`
	RoutingStrategy string  `json:"routing_strategy"`
}

type createInferenceEndpointRequest struct {
	Name            string  `json:"name"`
	Deployment      string  `json:"deployment"`
	Protocol        string  `json:"protocol"`
	Port            int64   `json:"port"`
	VIP             *string `json:"vip,omitempty"`
	RoutingStrategy string  `json:"routing_strategy"`
}

func (c *Client) GetInferenceEndpoint(ctx context.Context, name string) (*inferenceEndpointRecord, error) {
	var e inferenceEndpointRecord
	if err := c.do(ctx, http.MethodGet, "/api/ai/endpoints/"+name, nil, &e); err != nil {
		return nil, err
	}
	return &e, nil
}

func (c *Client) CreateInferenceEndpoint(ctx context.Context, req createInferenceEndpointRequest) (*inferenceEndpointRecord, error) {
	var e inferenceEndpointRecord
	if err := c.do(ctx, http.MethodPost, "/api/ai/endpoints", req, &e); err != nil {
		return nil, err
	}
	return &e, nil
}

func (c *Client) DeleteInferenceEndpoint(ctx context.Context, name string) error {
	return c.do(ctx, http.MethodDelete, "/api/ai/endpoints/"+name, nil, nil)
}

type aiSiteRecord struct {
	ID        string `json:"id"`
	Residency string `json:"residency"`
	LatencyMs int64  `json:"latency_ms"`
	CostClass int64  `json:"cost_class"`
}

func (c *Client) GetAiSite(ctx context.Context, id string) (*aiSiteRecord, error) {
	var site aiSiteRecord
	if err := c.do(ctx, http.MethodGet, "/api/ai/sites/"+id, nil, &site); err != nil {
		return nil, err
	}
	return &site, nil
}

func (c *Client) PutAiSite(ctx context.Context, site aiSiteRecord) (*aiSiteRecord, error) {
	var saved aiSiteRecord
	if err := c.do(ctx, http.MethodPost, "/api/ai/sites", site, &saved); err != nil {
		return nil, err
	}
	return &saved, nil
}

func (c *Client) DeleteAiSite(ctx context.Context, id string) error {
	return c.do(ctx, http.MethodDelete, "/api/ai/sites/"+id, nil, nil)
}

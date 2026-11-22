// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/int64planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &inferenceProfileResource{}

type inferenceProfileResource struct {
	client *Client
}

type inferenceProfileModel struct {
	Name           types.String `tfsdk:"name"`
	Runtime        types.String `tfsdk:"runtime"`
	Vendor         types.String `tfsdk:"vendor"`
	GPUCount       types.Int64  `tfsdk:"gpu_count"`
	MinimumVramGiB types.Int64  `tfsdk:"minimum_vram_gib"`
	CPU            types.Int64  `tfsdk:"cpu"`
	MemoryGiB      types.Int64  `tfsdk:"memory_gib"`
}

func NewInferenceProfileResource() resource.Resource {
	return &inferenceProfileResource{}
}

func (r *inferenceProfileResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_inference_profile"
}

func (r *inferenceProfileResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Fabric AI InferenceProfile (runtime + GPU/CPU shape).",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"runtime": schema.StringAttribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.UseStateForUnknown(),
				},
			},
			"vendor": schema.StringAttribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.UseStateForUnknown(),
				},
			},
			"gpu_count": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"minimum_vram_gib": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"cpu": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"memory_gib": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
		},
	}
}

func (r *inferenceProfileResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
	if req.ProviderData == nil {
		return
	}
	client, ok := req.ProviderData.(*Client)
	if !ok {
		resp.Diagnostics.AddError("Invalid provider data", "expected *Client")
		return
	}
	r.client = client
}

func (r *inferenceProfileResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan inferenceProfileModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	runtime := plan.Runtime.ValueString()
	if runtime == "" {
		runtime = "vllm"
	}
	vendor := plan.Vendor.ValueString()
	if vendor == "" {
		vendor = "nvidia"
	}
	gpu := plan.GPUCount.ValueInt64()
	if gpu == 0 {
		gpu = 1
	}
	vram := plan.MinimumVramGiB.ValueInt64()
	if vram == 0 {
		vram = 24
	}
	cpu := plan.CPU.ValueInt64()
	if cpu == 0 {
		cpu = 8
	}
	mem := plan.MemoryGiB.ValueInt64()
	if mem == 0 {
		mem = 32
	}
	p, err := r.client.CreateInferenceProfile(ctx, createInferenceProfileRequest{
		Name:    plan.Name.ValueString(),
		Runtime: runtime,
		GPU: gpuReq{
			Vendor:         vendor,
			Count:          gpu,
			MinimumVramGiB: vram,
		},
		CPU:       cpu,
		MemoryGiB: mem,
	})
	if err != nil {
		resp.Diagnostics.AddError("Create InferenceProfile failed", err.Error())
		return
	}
	applyProfile(p, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *inferenceProfileResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state inferenceProfileModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	p, err := r.client.GetInferenceProfile(ctx, state.Name.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read failed", err.Error())
		return
	}
	applyProfile(p, &state)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *inferenceProfileResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Recreate the InferenceProfile to change shape")
}

func (r *inferenceProfileResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state inferenceProfileModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteInferenceProfile(ctx, state.Name.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete InferenceProfile failed", err.Error())
	}
}

func (r *inferenceProfileResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("name"), req, resp)
}

func applyProfile(p *inferenceProfileRecord, plan *inferenceProfileModel) {
	plan.Name = types.StringValue(p.Name)
	plan.Runtime = types.StringValue(p.Runtime)
	plan.Vendor = types.StringValue(p.GPU.Vendor)
	plan.GPUCount = types.Int64Value(p.GPU.Count)
	plan.MinimumVramGiB = types.Int64Value(p.GPU.MinimumVramGiB)
	plan.CPU = types.Int64Value(p.CPU)
	plan.MemoryGiB = types.Int64Value(p.MemoryGiB)
}

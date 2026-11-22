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

var _ resource.Resource = &inferenceEndpointResource{}

type inferenceEndpointResource struct {
	client *Client
}

type inferenceEndpointModel struct {
	Name            types.String `tfsdk:"name"`
	Deployment      types.String `tfsdk:"deployment"`
	Protocol        types.String `tfsdk:"protocol"`
	Port            types.Int64  `tfsdk:"port"`
	VIP             types.String `tfsdk:"vip"`
	RoutingStrategy types.String `tfsdk:"routing_strategy"`
	GatewayPath     types.String `tfsdk:"gateway_path"`
}

func NewInferenceEndpointResource() resource.Resource {
	return &inferenceEndpointResource{}
}

func (r *inferenceEndpointResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_inference_endpoint"
}

func (r *inferenceEndpointResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Fabric AI InferenceEndpoint (OpenAI-compatible Maglev frontend).",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"deployment": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"protocol": schema.StringAttribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.UseStateForUnknown(),
				},
			},
			"port": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"vip": schema.StringAttribute{Optional: true},
			"routing_strategy": schema.StringAttribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.UseStateForUnknown(),
				},
			},
			"gateway_path": schema.StringAttribute{
				Computed:    true,
				Description: "API-key OpenAI gateway path under fabricd",
			},
		},
	}
}

func (r *inferenceEndpointResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
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

func (r *inferenceEndpointResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan inferenceEndpointModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	proto := plan.Protocol.ValueString()
	if proto == "" {
		proto = "openai"
	}
	port := plan.Port.ValueInt64()
	if port == 0 {
		port = 8000
	}
	routing := plan.RoutingStrategy.ValueString()
	if routing == "" {
		routing = "least_queue"
	}
	creq := createInferenceEndpointRequest{
		Name:            plan.Name.ValueString(),
		Deployment:      plan.Deployment.ValueString(),
		Protocol:        proto,
		Port:            port,
		RoutingStrategy: routing,
	}
	if !plan.VIP.IsNull() && plan.VIP.ValueString() != "" {
		v := plan.VIP.ValueString()
		creq.VIP = &v
	}
	e, err := r.client.CreateInferenceEndpoint(ctx, creq)
	if err != nil {
		resp.Diagnostics.AddError("Create InferenceEndpoint failed", err.Error())
		return
	}
	applyEndpoint(e, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *inferenceEndpointResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state inferenceEndpointModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	e, err := r.client.GetInferenceEndpoint(ctx, state.Name.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read failed", err.Error())
		return
	}
	applyEndpoint(e, &state)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *inferenceEndpointResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Recreate the InferenceEndpoint to change routing or VIP")
}

func (r *inferenceEndpointResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state inferenceEndpointModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteInferenceEndpoint(ctx, state.Name.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete InferenceEndpoint failed", err.Error())
	}
}

func (r *inferenceEndpointResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("name"), req, resp)
}

func applyEndpoint(e *inferenceEndpointRecord, plan *inferenceEndpointModel) {
	plan.Name = types.StringValue(e.Name)
	plan.Deployment = types.StringValue(e.Deployment)
	plan.Protocol = types.StringValue(e.Protocol)
	plan.Port = types.Int64Value(e.Port)
	plan.RoutingStrategy = types.StringValue(e.RoutingStrategy)
	if e.VIP != nil {
		plan.VIP = types.StringValue(*e.VIP)
	}
	plan.GatewayPath = types.StringValue("/api/ai/openai/" + e.Name)
}

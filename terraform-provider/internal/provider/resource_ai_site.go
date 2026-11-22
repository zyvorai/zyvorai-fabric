// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/int64planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &aiSiteResource{}

type aiSiteResource struct {
	client *Client
}

type aiSiteModel struct {
	ID        types.String `tfsdk:"id"`
	Residency types.String `tfsdk:"residency"`
	LatencyMs types.Int64  `tfsdk:"latency_ms"`
	CostClass types.Int64  `tfsdk:"cost_class"`
}

func NewAiSiteResource() resource.Resource {
	return &aiSiteResource{}
}

func (r *aiSiteResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_ai_site"
}

func (r *aiSiteResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Fabric AI site record used for residency and failover.",
		Attributes: map[string]schema.Attribute{
			"id": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"residency": schema.StringAttribute{
				Optional: true,
			},
			"latency_ms": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
			"cost_class": schema.Int64Attribute{
				Optional: true,
				Computed: true,
				PlanModifiers: []planmodifier.Int64{
					int64planmodifier.UseStateForUnknown(),
				},
			},
		},
	}
}

func (r *aiSiteResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
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

func siteFromModel(plan aiSiteModel) aiSiteRecord {
	return aiSiteRecord{
		ID:        plan.ID.ValueString(),
		Residency: plan.Residency.ValueString(),
		LatencyMs: plan.LatencyMs.ValueInt64(),
		CostClass: plan.CostClass.ValueInt64(),
	}
}

func applySite(site *aiSiteRecord, model *aiSiteModel) {
	model.ID = types.StringValue(site.ID)
	model.Residency = types.StringValue(site.Residency)
	model.LatencyMs = types.Int64Value(site.LatencyMs)
	model.CostClass = types.Int64Value(site.CostClass)
}

func (r *aiSiteResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan aiSiteModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	saved, err := r.client.PutAiSite(ctx, siteFromModel(plan))
	if err != nil {
		resp.Diagnostics.AddError("Create AI site failed", err.Error())
		return
	}
	applySite(saved, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *aiSiteResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state aiSiteModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	site, err := r.client.GetAiSite(ctx, state.ID.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read AI site failed", err.Error())
		return
	}
	applySite(site, &state)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *aiSiteResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	var plan aiSiteModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	saved, err := r.client.PutAiSite(ctx, siteFromModel(plan))
	if err != nil {
		resp.Diagnostics.AddError("Update AI site failed", err.Error())
		return
	}
	applySite(saved, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *aiSiteResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state aiSiteModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteAiSite(ctx, state.ID.ValueString()); err != nil && !IsNotFound(err) {
		resp.Diagnostics.AddError("Delete AI site failed", err.Error())
	}
}

// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &networkPolicyResource{}

type networkPolicyResource struct {
	client *Client
}

type networkPolicyResourceModel struct {
	ID          types.String `tfsdk:"id"`
	Name        types.String `tfsdk:"name"`
	Description types.String `tfsdk:"description"`
	Enabled     types.Bool   `tfsdk:"enabled"`
}

func NewNetworkPolicyResource() resource.Resource {
	return &networkPolicyResource{}
}

func (r *networkPolicyResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_network_policy"
}

func (r *networkPolicyResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Manages a Zyvor Fabric network policy (allow/deny rules scaffold).",
		Attributes: map[string]schema.Attribute{
			"id": schema.StringAttribute{
				Computed: true,
			},
			"name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"description": schema.StringAttribute{
				Optional: true,
			},
			"enabled": schema.BoolAttribute{
				Optional: true,
			},
		},
	}
}

func (r *networkPolicyResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
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

func (r *networkPolicyResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan networkPolicyResourceModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	enabled := true
	if !plan.Enabled.IsNull() {
		enabled = plan.Enabled.ValueBool()
	}
	desc := ""
	if !plan.Description.IsNull() {
		desc = plan.Description.ValueString()
	}
	policy, err := r.client.CreateNetworkPolicy(ctx, plan.Name.ValueString(), desc, enabled, map[string]string{})
	if err != nil {
		resp.Diagnostics.AddError("Create network policy failed", err.Error())
		return
	}
	plan.ID = types.StringValue(policy.ID)
	plan.Enabled = types.BoolValue(policy.Enabled)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *networkPolicyResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	resp.Diagnostics.Append(req.State.Get(ctx, &networkPolicyResourceModel{})...)
}

func (r *networkPolicyResource) Update(_ context.Context, _ resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Recreate the network policy to change rules")
}

func (r *networkPolicyResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state networkPolicyResourceModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteNetworkPolicy(ctx, state.ID.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete network policy failed", err.Error())
	}
}

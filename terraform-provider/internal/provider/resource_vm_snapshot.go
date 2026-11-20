// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-1.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &vmSnapshotResource{}

type vmSnapshotResource struct {
	client *Client
}

type vmSnapshotResourceModel struct {
	ID          types.String `tfsdk:"id"`
	VMName      types.String `tfsdk:"vm_name"`
	Name        types.String `tfsdk:"name"`
	Description types.String `tfsdk:"description"`
}

func NewVMSnapshotResource() resource.Resource {
	return &vmSnapshotResource{}
}

func (r *vmSnapshotResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_vm_snapshot"
}

func (r *vmSnapshotResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Manages a Zyvor Fabric VM disk snapshot.",
		Attributes: map[string]schema.Attribute{
			"id": schema.StringAttribute{
				Computed: true,
			},
			"vm_name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
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
		},
	}
}

func (r *vmSnapshotResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
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

func (r *vmSnapshotResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan vmSnapshotResourceModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	desc := ""
	if !plan.Description.IsNull() {
		desc = plan.Description.ValueString()
	}
	snap, err := r.client.CreateVMSnapshot(ctx, plan.VMName.ValueString(), plan.Name.ValueString(), desc)
	if err != nil {
		resp.Diagnostics.AddError("Create VM snapshot failed", err.Error())
		return
	}
	plan.ID = types.StringValue(snap.ID)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *vmSnapshotResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	resp.Diagnostics.Append(req.State.Get(ctx, &vmSnapshotResourceModel{})...)
}

func (r *vmSnapshotResource) Update(_ context.Context, _ resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Snapshots are immutable")
}

func (r *vmSnapshotResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state vmSnapshotResourceModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteVMSnapshot(ctx, state.VMName.ValueString(), state.ID.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete VM snapshot failed", err.Error())
	}
}

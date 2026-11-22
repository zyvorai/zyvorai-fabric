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

var _ resource.Resource = &storagePoolResource{}

type storagePoolResource struct {
	client *Client
}

type storagePoolResourceModel struct {
	Name      types.String `tfsdk:"name"`
	Path      types.String `tfsdk:"path"`
	AutoStart types.Bool   `tfsdk:"auto_start"`
	PoolType  types.String `tfsdk:"pool_type"`
}

func NewStoragePoolResource() resource.Resource {
	return &storagePoolResource{}
}

func (r *storagePoolResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_storage_pool"
}

func (r *storagePoolResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Manages a local Zyvor Fabric storage pool.",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"path": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"auto_start": schema.BoolAttribute{
				Optional: true,
			},
			"pool_type": schema.StringAttribute{
				Computed: true,
			},
		},
	}
}

func (r *storagePoolResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
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

func (r *storagePoolResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan storagePoolResourceModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	autoStart := true
	if !plan.AutoStart.IsNull() {
		autoStart = plan.AutoStart.ValueBool()
	}
	pool, err := r.client.CreateLocalStoragePool(ctx, plan.Name.ValueString(), plan.Path.ValueString(), autoStart)
	if err != nil {
		resp.Diagnostics.AddError("Create storage pool failed", err.Error())
		return
	}
	plan.AutoStart = types.BoolValue(autoStart)
	plan.PoolType = types.StringValue(pool.Type)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *storagePoolResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state storagePoolResourceModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	pool, err := r.client.GetStoragePool(ctx, state.Name.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read failed", err.Error())
		return
	}
	state.PoolType = types.StringValue(pool.Type)
	if pool.Path != "" {
		state.Path = types.StringValue(pool.Path)
	}
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *storagePoolResource) Update(_ context.Context, _ resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Recreate the storage pool to change path or auto_start")
}

func (r *storagePoolResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state storagePoolResourceModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteStoragePool(ctx, state.Name.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete storage pool failed", err.Error())
	}
}

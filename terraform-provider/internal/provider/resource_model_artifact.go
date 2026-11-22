// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0

package provider

import (
	"context"

	"github.com/hashicorp/terraform-plugin-framework/path"
	"github.com/hashicorp/terraform-plugin-framework/resource"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/planmodifier"
	"github.com/hashicorp/terraform-plugin-framework/resource/schema/stringplanmodifier"
	"github.com/hashicorp/terraform-plugin-framework/types"
)

var _ resource.Resource = &modelArtifactResource{}

type modelArtifactResource struct {
	client *Client
}

type modelArtifactModel struct {
	Name      types.String `tfsdk:"name"`
	Source    types.String `tfsdk:"source"`
	Format    types.String `tfsdk:"format"`
	Revision  types.String `tfsdk:"revision"`
	Checksum  types.String `tfsdk:"checksum"`
	Tenant    types.String `tfsdk:"tenant"`
	LocalPath types.String `tfsdk:"local_path"`
}

func NewModelArtifactResource() resource.Resource {
	return &modelArtifactResource{}
}

func (r *modelArtifactResource) Metadata(_ context.Context, req resource.MetadataRequest, resp *resource.MetadataResponse) {
	resp.TypeName = req.ProviderTypeName + "_model_artifact"
}

func (r *modelArtifactResource) Schema(_ context.Context, _ resource.SchemaRequest, resp *resource.SchemaResponse) {
	resp.Schema = schema.Schema{
		Description: "Registers a Fabric AI ModelArtifact (hf:// or local path).",
		Attributes: map[string]schema.Attribute{
			"name": schema.StringAttribute{
				Required: true,
				PlanModifiers: []planmodifier.String{
					stringplanmodifier.RequiresReplace(),
				},
			},
			"source":   schema.StringAttribute{Required: true},
			"format":   schema.StringAttribute{Required: true},
			"revision": schema.StringAttribute{Optional: true},
			"checksum": schema.StringAttribute{Optional: true},
			"tenant":   schema.StringAttribute{Optional: true},
			"local_path": schema.StringAttribute{
				Computed:    true,
				Description: "Host path after materialization",
			},
		},
	}
}

func (r *modelArtifactResource) Configure(_ context.Context, req resource.ConfigureRequest, resp *resource.ConfigureResponse) {
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

func (r *modelArtifactResource) Create(ctx context.Context, req resource.CreateRequest, resp *resource.CreateResponse) {
	var plan modelArtifactModel
	resp.Diagnostics.Append(req.Plan.Get(ctx, &plan)...)
	if resp.Diagnostics.HasError() {
		return
	}
	creq := createModelArtifactRequest{
		Name:   plan.Name.ValueString(),
		Source: plan.Source.ValueString(),
		Format: plan.Format.ValueString(),
	}
	if !plan.Revision.IsNull() && plan.Revision.ValueString() != "" {
		v := plan.Revision.ValueString()
		creq.Revision = &v
	}
	if !plan.Checksum.IsNull() && plan.Checksum.ValueString() != "" {
		v := plan.Checksum.ValueString()
		creq.Checksum = &v
	}
	if !plan.Tenant.IsNull() && plan.Tenant.ValueString() != "" {
		v := plan.Tenant.ValueString()
		creq.Tenant = &v
	}
	m, err := r.client.CreateModelArtifact(ctx, creq)
	if err != nil {
		resp.Diagnostics.AddError("Create ModelArtifact failed", err.Error())
		return
	}
	applyModelArtifact(m, &plan)
	resp.Diagnostics.Append(resp.State.Set(ctx, &plan)...)
}

func (r *modelArtifactResource) Read(ctx context.Context, req resource.ReadRequest, resp *resource.ReadResponse) {
	var state modelArtifactModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	m, err := r.client.GetModelArtifact(ctx, state.Name.ValueString())
	if err != nil {
		if IsNotFound(err) {
			resp.State.RemoveResource(ctx)
			return
		}
		resp.Diagnostics.AddError("Read failed", err.Error())
		return
	}
	applyModelArtifact(m, &state)
	resp.Diagnostics.Append(resp.State.Set(ctx, &state)...)
}

func (r *modelArtifactResource) Update(ctx context.Context, req resource.UpdateRequest, resp *resource.UpdateResponse) {
	resp.Diagnostics.AddError("Update not supported", "Recreate the ModelArtifact to change source or format")
}

func (r *modelArtifactResource) Delete(ctx context.Context, req resource.DeleteRequest, resp *resource.DeleteResponse) {
	var state modelArtifactModel
	resp.Diagnostics.Append(req.State.Get(ctx, &state)...)
	if resp.Diagnostics.HasError() {
		return
	}
	if err := r.client.DeleteModelArtifact(ctx, state.Name.ValueString()); err != nil {
		resp.Diagnostics.AddError("Delete ModelArtifact failed", err.Error())
	}
}

func (r *modelArtifactResource) ImportState(ctx context.Context, req resource.ImportStateRequest, resp *resource.ImportStateResponse) {
	resource.ImportStatePassthroughID(ctx, path.Root("name"), req, resp)
}

func applyModelArtifact(m *modelArtifactRecord, plan *modelArtifactModel) {
	plan.Name = types.StringValue(m.Name)
	plan.Source = types.StringValue(m.Source)
	plan.Format = types.StringValue(m.Format)
	if m.LocalPath != nil {
		plan.LocalPath = types.StringValue(*m.LocalPath)
	} else {
		plan.LocalPath = types.StringNull()
	}
}
